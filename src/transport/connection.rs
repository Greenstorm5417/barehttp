use crate::error::Error;
use crate::headers::Headers;
use crate::parser::framing::FramingDetector;
use crate::parser::{BodyReadStrategy, Response};
use crate::socket::BlockingSocket;
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
}

impl<'a, S: BlockingSocket> Connection<'a, S> {
  pub const fn new(socket: &'a mut S, max_header_size: usize) -> Self {
    Self {
      socket,
      max_header_size,
    }
  }

  /// Send HTTP request bytes to the socket
  pub fn send_request(&mut self, request_bytes: &[u8]) -> Result<(), Error> {
    self.socket.write(request_bytes).map_err(Error::Socket)?;
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
      let n = self.socket.read(&mut buffer).map_err(Error::Socket)?;
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
      }
    };

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
              let n = self.socket.read(buf_slice).map_err(Error::Socket)?;

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
      }
      BodyReadStrategy::Chunked => {
        let mut raw_bytes = Vec::from(initial_bytes);
        let mut chunk_buffer = alloc::vec![0u8; 8192];

        loop {
          if FramingDetector::has_chunked_terminator(&raw_bytes) {
            break;
          }

          let n = self.socket.read(&mut chunk_buffer).map_err(Error::Socket)?;
          if n == 0 {
            return Err(Error::Socket(crate::error::SocketError::NotConnected));
          }
          if let Some(slice) = chunk_buffer.get(..n) {
            raw_bytes.extend_from_slice(slice);
          }
        }

        Ok(raw_bytes)
      }
      BodyReadStrategy::UntilClose => {
        let mut body_bytes = Vec::from(initial_bytes);
        let mut read_buffer = alloc::vec![0u8; 8192];

        loop {
          let n = self.socket.read(&mut read_buffer).map_err(Error::Socket)?;
          if n == 0 {
            break;
          }
          if let Some(slice) = read_buffer.get(..n) {
            body_bytes.extend_from_slice(slice);
          }
        }

        Ok(body_bytes)
      }
    }
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
  use super::*;
  use crate::error::SocketError;
  use crate::socket::{BlockingSocket, SocketAddr, SocketFlags};
  use alloc::format;
  use alloc::string::{String, ToString};
  use alloc::vec;

  struct MockSocket {
    read_data: Vec<u8>,
    read_pos: usize,
    written: Vec<u8>,
  }

  impl MockSocket {
    fn new(response: &str) -> Self {
      Self {
        read_data: response.as_bytes().to_vec(),
        read_pos: 0,
        written: Vec::new(),
      }
    }

    fn get_written(&self) -> String {
      String::from_utf8_lossy(&self.written).to_string()
    }
  }

  impl BlockingSocket for MockSocket {
    fn connect(&mut self, _addr: &SocketAddr<'_>) -> Result<(), SocketError> {
      Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, SocketError> {
      if self.read_pos >= self.read_data.len() {
        return Ok(0);
      }
      let remaining = &self.read_data[self.read_pos..];
      let to_read = remaining.len().min(buf.len());
      buf[..to_read].copy_from_slice(&remaining[..to_read]);
      self.read_pos += to_read;
      Ok(to_read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, SocketError> {
      self.written.extend_from_slice(buf);
      Ok(buf.len())
    }

    fn shutdown(&mut self) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_flags(&mut self, _flags: SocketFlags) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_read_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_write_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
      Ok(())
    }
  }

  #[test]
  fn send_request_writes_to_socket() {
    let mut socket = MockSocket::new("");
    let mut conn = Connection::new(&mut socket, 8192);

    let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let result = conn.send_request(request);

    assert!(result.is_ok());
    assert_eq!(socket.get_written(), "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n");
  }

  #[test]
  fn read_response_with_content_length() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 200);
    assert_eq!(raw.reason, "OK");
    assert_eq!(raw.body_bytes, b"Hello");
  }

  #[test]
  fn read_response_no_body_expectation_ignores_content() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::NoBody);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 200);
    assert!(raw.body_bytes.is_empty(), "NoBody expectation should skip reading body");
  }

  #[test]
  fn read_response_chunked_encoding() {
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 200);
    assert_eq!(raw.body_bytes, b"5\r\nHello\r\n0\r\n\r\n");
  }

  #[test]
  fn read_response_204_no_content() {
    let response = "HTTP/1.1 204 No Content\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 204);
    assert!(raw.body_bytes.is_empty());
  }

  #[test]
  fn read_response_304_not_modified() {
    let response = "HTTP/1.1 304 Not Modified\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 304);
    assert!(raw.body_bytes.is_empty());
  }

  #[test]
  fn header_size_limit_enforced() {
    let large_header = "HTTP/1.1 200 OK\r\n".to_string() + "X-Large: " + &"A".repeat(10000) + "\r\n\r\n";
    let mut socket = MockSocket::new(&large_header);
    let mut conn = Connection::new(&mut socket, 1024);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::ResponseHeaderTooLarge));
  }

  #[test]
  fn read_response_with_multiple_headers() {
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nOK";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 200);
    assert_eq!(raw.headers.get("Content-Type"), Some("text/plain"));
    assert_eq!(raw.headers.get("Content-Length"), Some("2"));
    assert_eq!(raw.body_bytes, b"OK");
  }

  #[test]
  fn read_response_empty_body_with_content_length_zero() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 200);
    assert!(raw.body_bytes.is_empty());
  }

  #[test]
  fn read_response_handles_body_in_header_buffer() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nHello World";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.body_bytes, b"Hello World");
  }

  #[test]
  fn response_body_expectation_enum_equality() {
    assert_eq!(ResponseBodyExpectation::NoBody, ResponseBodyExpectation::NoBody);
    assert_eq!(ResponseBodyExpectation::Normal, ResponseBodyExpectation::Normal);
    assert_ne!(ResponseBodyExpectation::NoBody, ResponseBodyExpectation::Normal);
  }

  #[test]
  fn raw_response_can_be_cloned() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "text/plain");
    
    let response = RawResponse {
      status_code: 200,
      reason: String::from("OK"),
      headers,
      body_bytes: vec![1, 2, 3],
    };

    let cloned = response.clone();
    assert_eq!(response.status_code, 200);
    assert_eq!(cloned.status_code, 200);
    assert_eq!(cloned.reason, "OK");
    assert_eq!(cloned.body_bytes, vec![1, 2, 3]);
  }

  #[test]
  fn read_response_1xx_informational() {
    let response = "HTTP/1.1 100 Continue\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 100);
    assert!(raw.body_bytes.is_empty());
  }

  #[test]
  fn read_response_redirect_with_location() {
    let response = "HTTP/1.1 302 Found\r\nLocation: /new-url\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.status_code, 302);
    assert_eq!(raw.headers.get("Location"), Some("/new-url"));
  }

  #[test]
  fn connection_new_sets_max_header_size() {
    let mut socket = MockSocket::new("");
    let conn = Connection::new(&mut socket, 4096);

    assert_eq!(conn.max_header_size, 4096);
  }

  #[test]
  fn read_response_large_body_content_length() {
    let body = "A".repeat(10000);
    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
    let mut socket = MockSocket::new(&response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert_eq!(raw.body_bytes.len(), 10000);
  }

  #[test]
  fn read_response_chunked_multiple_chunks() {
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nTest\r\n5\r\nChunk\r\n0\r\n\r\n";
    let mut socket = MockSocket::new(response);
    let mut conn = Connection::new(&mut socket, 8192);

    let result = conn.read_raw_response(ResponseBodyExpectation::Normal);

    assert!(result.is_ok());
    let raw = result.unwrap();
    assert!(!raw.body_bytes.is_empty());
  }
}
