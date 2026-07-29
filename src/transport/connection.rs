use crate::error::Error;
use crate::headers::Headers;
use crate::parser::framing::{has_chunked_terminator, has_complete_headers};
use crate::parser::version::Version;
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
  pub version: Version,
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
    let mut offset = 0usize;
    while offset < request_bytes.len() {
      let chunk = request_bytes.get(offset..).ok_or(Error::Socket(crate::error::SocketError::NotConnected))?;
      let n = self.socket.write(chunk).map_err(Error::Socket)?;
      if n == 0 {
        return Err(Error::Socket(crate::error::SocketError::NotConnected));
      }
      offset = offset.saturating_add(n);
    }

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
  /// 1xx interim responses are discarded; reading continues until a non-1xx
  /// final response (RFC 9112 §15).
  ///
  /// This is wire-protocol behavior, not a policy decision.
  pub fn read_raw_response(
    &mut self,
    expectation: ResponseBodyExpectation,
  ) -> Result<RawResponse, Error> {
    let max_header_size = self.max_header_size;
    let mut buffer = alloc::vec![0u8; max_header_size.min(8192)];
    let mut header_buffer = Vec::new();

    loop {
      while !has_complete_headers(&header_buffer) {
        if header_buffer.len() > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }
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
      }

      let (status_code, reason, headers, version, remaining_after_headers) =
        Response::parse_headers_only(&header_buffer).map_err(Error::Parse)?;

      // RFC 9112: discard 1xx and keep reading the final response
      if (100..200).contains(&status_code) {
        header_buffer = remaining_after_headers.to_vec();
        continue;
      }

      let body_bytes = match expectation {
        // HEAD / no-body: no entity body on the wire (RFC 9112); don't poison the pool.
        ResponseBodyExpectation::NoBody => Vec::new(),
        ResponseBodyExpectation::Normal => {
          let body_strategy = match Response::body_read_strategy(&headers, status_code) {
            Ok(s) => s,
            Err(e) => {
              self.state.mark_received_close();
              return Err(Error::Parse(e));
            },
          };
          if matches!(body_strategy, BodyReadStrategy::UntilClose) {
            // UntilClose ends the connection; never pool it.
            self.state.mark_received_close();
          }
          self.read_body(body_strategy, remaining_after_headers)?
        },
      };

      // RFC 9112 Section 9.3 / 9.6: HTTP/1.0 defaults to close unless keep-alive
      if version == Version::HTTP_10 {
        let keep_alive = headers.get(Headers::CONNECTION).is_some_and(|v| {
          v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("keep-alive"))
        });
        if !keep_alive {
          self.state.mark_received_close();
        }
      }

      // RFC 9112 Section 9.6: Connection header is a comma-separated token list
      if let Some(conn_value) = headers.get(Headers::CONNECTION)
        && conn_value
          .split(',')
          .any(|t| t.trim().eq_ignore_ascii_case("close"))
      {
        self.state.mark_received_close();
      }

      return Ok(RawResponse {
        status_code,
        reason,
        headers,
        version,
        body_bytes,
      });
    }
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
          if has_chunked_terminator(&raw_bytes) {
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
