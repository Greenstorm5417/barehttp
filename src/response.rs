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
