use crate::parser::Response as ParsedResponse;

/// Extension trait for HTTP response convenience methods
///
/// Provides helper methods for checking status codes and accessing the response body.
pub trait ResponseExt {
  /// Check if the response has a 2xx status code
  fn is_success(&self) -> bool;
  /// Check if the response has a 3xx status code
  fn is_redirect(&self) -> bool;
  /// Check if the response has a 4xx status code
  fn is_client_error(&self) -> bool;
  /// Check if the response has a 5xx status code
  fn is_server_error(&self) -> bool;
  /// Get the HTTP status code
  fn status(&self) -> u16;
  /// Get all Set-Cookie header values from the response
  fn cookies(&self) -> alloc::vec::Vec<&str>;
  /// Convert the response body to a UTF-8 string
  ///
  /// # Errors
  /// Returns an error if the response body contains invalid UTF-8.
  fn text(&self) -> Result<alloc::string::String, alloc::string::FromUtf8Error>;
  /// Get the response body as a byte slice
  fn bytes(&self) -> &[u8];
  /// Convert the response into its body bytes
  fn into_bytes(self) -> alloc::vec::Vec<u8>;
}

impl ResponseExt for ParsedResponse {
  fn status(&self) -> u16 {
    self.status_code
  }

  fn cookies(&self) -> alloc::vec::Vec<&str> {
    self.headers.get_all("Set-Cookie")
  }

  fn is_success(&self) -> bool {
    (200..300).contains(&self.status_code)
  }

  fn is_redirect(&self) -> bool {
    (300..400).contains(&self.status_code)
  }

  fn is_client_error(&self) -> bool {
    (400..500).contains(&self.status_code)
  }

  fn is_server_error(&self) -> bool {
    (500..600).contains(&self.status_code)
  }

  fn text(&self) -> Result<alloc::string::String, alloc::string::FromUtf8Error> {
    self.body.to_string()
  }

  fn bytes(&self) -> &[u8] {
    self.body.as_bytes()
  }

  fn into_bytes(self) -> alloc::vec::Vec<u8> {
    self.body.into_bytes()
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::*;
  use crate::body::Body;
  use crate::headers::Headers;
  use alloc::string::String;

  fn make_response(status_code: u16, body: &[u8]) -> ParsedResponse {
    ParsedResponse {
      status_code,
      reason: String::from("Test"),
      headers: Headers::new(),
      body: Body::from_bytes(body.to_vec()),
    }
  }

  #[test]
  fn is_success_true_for_2xx() {
    assert!(make_response(200, b"").is_success());
    assert!(make_response(201, b"").is_success());
    assert!(make_response(204, b"").is_success());
    assert!(make_response(299, b"").is_success());
  }

  #[test]
  fn is_success_false_for_non_2xx() {
    assert!(!make_response(199, b"").is_success());
    assert!(!make_response(300, b"").is_success());
    assert!(!make_response(404, b"").is_success());
    assert!(!make_response(500, b"").is_success());
  }

  #[test]
  fn is_redirect_true_for_3xx() {
    assert!(make_response(300, b"").is_redirect());
    assert!(make_response(301, b"").is_redirect());
    assert!(make_response(302, b"").is_redirect());
    assert!(make_response(307, b"").is_redirect());
    assert!(make_response(399, b"").is_redirect());
  }

  #[test]
  fn is_redirect_false_for_non_3xx() {
    assert!(!make_response(299, b"").is_redirect());
    assert!(!make_response(400, b"").is_redirect());
  }

  #[test]
  fn is_client_error_true_for_4xx() {
    assert!(make_response(400, b"").is_client_error());
    assert!(make_response(404, b"").is_client_error());
    assert!(make_response(403, b"").is_client_error());
    assert!(make_response(499, b"").is_client_error());
  }

  #[test]
  fn is_client_error_false_for_non_4xx() {
    assert!(!make_response(399, b"").is_client_error());
    assert!(!make_response(500, b"").is_client_error());
  }

  #[test]
  fn is_server_error_true_for_5xx() {
    assert!(make_response(500, b"").is_server_error());
    assert!(make_response(502, b"").is_server_error());
    assert!(make_response(503, b"").is_server_error());
    assert!(make_response(599, b"").is_server_error());
  }

  #[test]
  fn is_server_error_false_for_non_5xx() {
    assert!(!make_response(499, b"").is_server_error());
    assert!(!make_response(600, b"").is_server_error());
  }

  #[test]
  fn status_returns_status_code() {
    assert_eq!(make_response(200, b"").status(), 200);
    assert_eq!(make_response(404, b"").status(), 404);
    assert_eq!(make_response(500, b"").status(), 500);
  }

  #[test]
  fn cookies_returns_set_cookie_headers() {
    let mut headers = Headers::new();
    headers.insert("Set-Cookie", "session=abc");
    headers.insert("Set-Cookie", "user=john");
    
    let response = ParsedResponse {
      status_code: 200,
      reason: String::from("OK"),
      headers,
      body: Body::empty(),
    };
    
    let cookies = response.cookies();
    assert_eq!(cookies.len(), 2);
    assert!(cookies.contains(&"session=abc"));
    assert!(cookies.contains(&"user=john"));
  }

  #[test]
  fn text_converts_utf8_body() {
    let response = make_response(200, b"Hello, World!");
    assert_eq!(response.text().unwrap(), "Hello, World!");
  }

  #[test]
  fn bytes_returns_body_slice() {
    let response = make_response(200, b"test data");
    assert_eq!(response.bytes(), b"test data");
  }

  #[test]
  fn into_bytes_consumes_and_returns_body() {
    let response = make_response(200, b"data");
    let bytes = response.into_bytes();
    assert_eq!(bytes, b"data");
  }
}
