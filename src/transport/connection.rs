use crate::error::Error;
use crate::headers::{HeaderName, Headers};
use crate::parser::framing::FramingDetector;
use crate::parser::{BodyReadStrategy, Response};
use crate::socket::BlockingSocket;
use crate::transport::connection_state::ConnectionState;
use alloc::string::String;
use alloc::vec::Vec;

/// Indicates whether the response should have a body based on HTTP protocol rules
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseBodyExpectation {
  /// Response should not have a body (HEAD requests, 204/304 responses, etc.)
  NoBody,
  /// Normal response that may have a body
  Normal,
}

/// Raw HTTP response without policy interpretation
#[derive(Debug, Clone)]
pub struct RawResponse {
  pub status_code: u16,
  pub reason: String,
  pub headers: Headers,
  pub body_bytes: Vec<u8>,
}

/// A single live HTTP connection (policy-free I/O operations)
pub struct Connection<'a, S> {
  socket: &'a mut S,
  max_header_size: usize,
  state: ConnectionState,
}

impl<'a, S: BlockingSocket> Connection<'a, S> {
  pub const fn new(
    socket: &'a mut S,
    max_header_size: usize,
  ) -> Self {
    Self {
      socket,
      max_header_size,
      state: ConnectionState::new(),
    }
  }

  /// Send HTTP request bytes to the socket
  ///
  pub fn send_request(
    &mut self,
    request_bytes: &[u8],
  ) -> Result<(), Error> {
    self.socket.write(request_bytes).map_err(Error::Socket)?;

    // RFC 9112 Section 9.6: If the client sends "Connection: close", it MUST NOT
    // send further requests on that connection.
    //
    // We only mark this state if the actual request bytes contain the
    // "Connection: close" header field.
    if request_bytes
      .windows("connection: close".len())
      .any(|w| w.eq_ignore_ascii_case(b"connection: close"))
    {
      self.state.mark_sent_close();
    }

    Ok(())
  }

  /// Read complete HTTP response (headers + body) with HTTP protocol awareness
  ///
  /// The `expectation` parameter handles protocol-level body semantics:
  /// - `NoBody`: For HEAD requests, 204/304 responses, CONNECT, etc.
  /// - Normal: Standard responses that may have bodies
  ///
  /// This is wire-protocol behavior, not a policy decision.
  pub fn read_raw_response(
    &mut self,
    expectation: ResponseBodyExpectation,
  ) -> Result<RawResponse, Error> {
    let max_header_size = self.max_header_size;
    let mut buffer = alloc::vec![0u8; max_header_size.min(8192)];
    let mut total_read = 0usize;
    let mut header_buffer = Vec::new();

    loop {
      let n = match self.socket.read(&mut buffer) {
        Ok(n) => n,
        Err(e) => {
          // RFC 9112 Section 9.5: If timing out, implementation SHOULD issue a graceful close
          if e == crate::error::SocketError::TimedOut {
            let _ = self.socket.shutdown();
          }
          return Err(Error::Socket(e));
        },
      };
      if n == 0 {
        break;
      }

      if let Some(slice) = buffer.get(..n) {
        header_buffer.extend_from_slice(slice);
      }
      total_read += n;

      if total_read > max_header_size {
        return Err(Error::ResponseHeaderTooLarge);
      }

      if FramingDetector::has_complete_headers(&header_buffer) {
        break;
      }
    }

    let (status_code, reason, headers, remaining_after_headers) =
      Response::parse_headers_only(&header_buffer).map_err(Error::Parse)?;

    let body_bytes = match expectation {
      ResponseBodyExpectation::NoBody => Vec::new(),
      ResponseBodyExpectation::Normal => {
        let body_strategy = Response::body_read_strategy(&headers, status_code);
        self.read_body(body_strategy, remaining_after_headers)?
      },
    };

    // RFC 9112 Section 9.6: Check if server sent Connection: close
    if let Some(conn_value) = headers.get(HeaderName::CONNECTION)
      && conn_value.eq_ignore_ascii_case("close")
    {
      self.state.mark_received_close();
    }

    Ok(RawResponse {
      status_code,
      reason,
      headers,
      body_bytes,
    })
  }

  fn read_body(
    &mut self,
    strategy: BodyReadStrategy,
    initial_bytes: &[u8],
  ) -> Result<Vec<u8>, Error> {
    match strategy {
      BodyReadStrategy::NoBody => Ok(Vec::new()),
      BodyReadStrategy::ContentLength(len) => {
        let mut body_bytes = Vec::from(initial_bytes);
        let bytes_needed = len.saturating_sub(body_bytes.len());

        if bytes_needed > 0 {
          let mut read_buffer = alloc::vec![0u8; bytes_needed.min(8192)];
          let mut bytes_read = 0usize;

          while bytes_read < bytes_needed {
            let to_read = (bytes_needed - bytes_read).min(read_buffer.len());
            if let Some(buf_slice) = read_buffer.get_mut(..to_read) {
              let n = match self.socket.read(buf_slice) {
                Ok(n) => n,
                Err(e) => {
                  if e == crate::error::SocketError::TimedOut {
                    let _ = self.socket.shutdown();
                  }
                  return Err(Error::Socket(e));
                },
              };

              if n == 0 {
                return Err(Error::Socket(crate::error::SocketError::NotConnected));
              }

              if let Some(slice) = read_buffer.get(..n) {
                body_bytes.extend_from_slice(slice);
              }
              bytes_read += n;
            }
          }
        }

        Ok(body_bytes)
      },
      BodyReadStrategy::Chunked => {
        let mut raw_bytes = Vec::from(initial_bytes);
        let mut chunk_buffer = alloc::vec![0u8; 8192];

        loop {
          if FramingDetector::has_chunked_terminator(&raw_bytes) {
            break;
          }

          let n = match self.socket.read(&mut chunk_buffer) {
            Ok(n) => n,
            Err(e) => {
              if e == crate::error::SocketError::TimedOut {
                let _ = self.socket.shutdown();
              }
              return Err(Error::Socket(e));
            },
          };
          if n == 0 {
            return Err(Error::Socket(crate::error::SocketError::NotConnected));
          }
          if let Some(slice) = chunk_buffer.get(..n) {
            raw_bytes.extend_from_slice(slice);
          }
        }

        Ok(raw_bytes)
      },
      BodyReadStrategy::UntilClose => {
        let mut body_bytes = Vec::from(initial_bytes);
        let mut read_buffer = alloc::vec![0u8; 8192];

        loop {
          let n = match self.socket.read(&mut read_buffer) {
            Ok(n) => n,
            Err(e) => {
              if e == crate::error::SocketError::TimedOut {
                let _ = self.socket.shutdown();
              }
              return Err(Error::Socket(e));
            },
          };
          if n == 0 {
            break;
          }
          if let Some(slice) = read_buffer.get(..n) {
            body_bytes.extend_from_slice(slice);
          }
        }

        Ok(body_bytes)
      },
    }
  }

  /// Check if the connection can be reused for another request
  ///
  /// RFC 9112 Section 9.6: Connection cannot be reused if either side sent Connection: close
  pub const fn is_reusable(&self) -> bool {
    self.state.can_be_reused()
  }
}
