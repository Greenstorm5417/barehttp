use crate::body::Body;
use crate::client::HttpClient;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::socket::BlockingSocket;
use alloc::string::String;

/// A pure HTTP request data structure
///
/// This struct holds request data without creating a client.
/// Use `send()` for convenience or `send_with()` for custom clients.
pub struct Request {
  method: Method,
  url: String,
  headers: Headers,
  body: Option<Body>,
}

impl Request {
  /// Create a GET request
  #[must_use]
  pub fn get(url: impl Into<String>) -> Self {
    Self::new(Method::Get, url)
  }

  /// Create a POST request
  #[must_use]
  pub fn post(url: impl Into<String>) -> Self {
    Self::new(Method::Post, url)
  }

  /// Create a PUT request
  #[must_use]
  pub fn put(url: impl Into<String>) -> Self {
    Self::new(Method::Put, url)
  }

  /// Create a DELETE request
  #[must_use]
  pub fn delete(url: impl Into<String>) -> Self {
    Self::new(Method::Delete, url)
  }

  /// Create a HEAD request
  #[must_use]
  pub fn head(url: impl Into<String>) -> Self {
    Self::new(Method::Head, url)
  }

  /// Create a PATCH request
  #[must_use]
  pub fn patch(url: impl Into<String>) -> Self {
    Self::new(Method::Patch, url)
  }

  /// Create a OPTIONS request
  #[must_use]
  pub fn options(url: impl Into<String>) -> Self {
    Self::new(Method::Options, url)
  }

  /// Create a new request with the given method and URL
  #[must_use]
  pub fn new(
    method: Method,
    url: impl Into<String>,
  ) -> Self {
    Self {
      method,
      url: url.into(),
      headers: Headers::new(),
      body: None,
    }
  }

  /// Add a header to the request
  #[must_use]
  pub fn header(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.headers.insert(name, value);
    self
  }

  /// Set the request body
  #[must_use]
  pub fn body(
    mut self,
    data: impl Into<Body>,
  ) -> Self {
    self.body = Some(data.into());
    self
  }

  /// Decompose the request into its parts
  #[must_use]
  pub fn into_parts(self) -> (Method, String, Headers, Option<Body>) {
    (self.method, self.url, self.headers, self.body)
  }

  /// Send the request using a custom client
  ///
  /// This allows you to control the socket and DNS adapters used.
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub fn send_with<S: BlockingSocket, D: DnsResolver>(
    self,
    client: &mut HttpClient<S, D>,
  ) -> Result<crate::parser::Response, Error> {
    client.run(self)
  }

  /// Send the request using the default OS socket and DNS resolver
  ///
  /// This is a convenience method that creates a new client with default adapters.
  /// For better performance or custom configuration, use `send_with()` with a reusable client.
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub fn send(self) -> Result<crate::parser::Response, Error> {
    let mut client = crate::HttpClient::new()?;
    self.send_with(&mut client)
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::*;

  #[test]
  fn request_get_creates_get_request() {
    let request = Request::get("http://example.com");
    let (method, url, _, _) = request.into_parts();

    assert_eq!(method, Method::Get);
    assert_eq!(url, "http://example.com");
  }

  #[test]
  fn request_post_creates_post_request() {
    let request = Request::post("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Post);
  }

  #[test]
  fn request_put_creates_put_request() {
    let request = Request::put("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Put);
  }

  #[test]
  fn request_delete_creates_delete_request() {
    let request = Request::delete("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Delete);
  }

  #[test]
  fn request_head_creates_head_request() {
    let request = Request::head("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Head);
  }

  #[test]
  fn request_patch_creates_patch_request() {
    let request = Request::patch("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Patch);
  }

  #[test]
  fn request_options_creates_options_request() {
    let request = Request::options("http://example.com");
    let (method, _, _, _) = request.into_parts();

    assert_eq!(method, Method::Options);
  }

  #[test]
  fn request_header_adds_header() {
    let request = Request::get("http://example.com").header("X-Custom", "value");

    let (_, _, headers, _) = request.into_parts();
    assert_eq!(headers.get("X-Custom"), Some("value"));
  }

  #[test]
  fn request_header_chaining() {
    let request = Request::get("http://example.com")
      .header("X-First", "one")
      .header("X-Second", "two");

    let (_, _, headers, _) = request.into_parts();
    assert_eq!(headers.get("X-First"), Some("one"));
    assert_eq!(headers.get("X-Second"), Some("two"));
  }

  #[test]
  fn request_body_sets_body() {
    let body_data = Body::from_bytes(b"test data".to_vec());
    let request = Request::post("http://example.com").body(body_data);

    let (_, _, _, body) = request.into_parts();
    assert!(body.is_some());
    assert_eq!(body.unwrap().as_bytes(), b"test data");
  }

  #[test]
  fn request_into_parts_decomposition() {
    let request = Request::post("http://example.com/api")
      .header("Content-Type", "application/json")
      .body(Body::from_bytes(b"{}".to_vec()));

    let (method, url, headers, body) = request.into_parts();

    assert_eq!(method, Method::Post);
    assert_eq!(url, "http://example.com/api");
    assert_eq!(headers.get("Content-Type"), Some("application/json"));
    assert!(body.is_some());
  }

  #[test]
  fn request_new_with_method() {
    let request = Request::new(Method::Get, "http://example.com");
    let (method, url, headers, body) = request.into_parts();

    assert_eq!(method, Method::Get);
    assert_eq!(url, "http://example.com");
    assert!(headers.is_empty());
    assert!(body.is_none());
  }
}
