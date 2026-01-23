use crate::client::HttpClient;
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::{HeaderName, Headers};
use crate::method::Method;
use crate::parser::Response;
use crate::parser::version::Version;
use crate::socket::BlockingSocket;
use crate::util::percent_encode;
use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;

/// Trait for types that can be converted into an HTTP body
pub trait IntoBody {
  /// Convert this type into a byte vector
  fn into_body(self) -> Vec<u8>;
}

impl IntoBody for Vec<u8> {
  fn into_body(self) -> Vec<u8> {
    self
  }
}

impl IntoBody for String {
  fn into_body(self) -> Vec<u8> {
    self.into_bytes()
  }
}

impl IntoBody for &str {
  fn into_body(self) -> Vec<u8> {
    self.as_bytes().to_vec()
  }
}

impl IntoBody for &[u8] {
  fn into_body(self) -> Vec<u8> {
    self.to_vec()
  }
}

/// Typestate marker indicating a request without a body
///
/// Used for HTTP methods like GET, HEAD, DELETE, OPTIONS.
pub struct WithoutBody;

/// Typestate marker indicating a request with a body
///
/// Used for HTTP methods like POST, PUT, PATCH.
pub struct WithBody;

/// Builder for constructing HTTP requests with compile-time body safety
///
/// Uses the typestate pattern to enforce body semantics at compile time.
/// Methods that require a body (POST, PUT, PATCH) return `ClientRequestBuilder<WithBody>`,
/// while methods without a body (GET, HEAD, etc.) return `ClientRequestBuilder<WithoutBody>`.
pub struct ClientRequestBuilder<S, D, B = WithoutBody> {
  client: HttpClient<S, D>,
  method: Method,
  url: String,
  headers: Headers,
  query_params: Vec<(String, String)>,
  form_data: Vec<(String, String)>,
  body: Option<Vec<u8>>,
  version: Version,
  request_config: Option<Config>,
  _phantom: PhantomData<B>,
}

impl<S, D, B> ClientRequestBuilder<S, D, B>
where
  S: BlockingSocket,
  D: DnsResolver,
{
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

  /// Add a URL-encoded query parameter
  #[must_use]
  pub fn query(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.query_params.push((key.into(), value.into()));
    self
  }

  /// Add multiple URL-encoded query parameters from an iterator
  #[must_use]
  pub fn query_pairs<I, K, V>(
    mut self,
    iter: I,
  ) -> Self
  where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
  {
    self
      .query_params
      .extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
    self
  }

  /// Add a raw (non-encoded) query parameter
  #[must_use]
  pub fn query_raw(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    let key_str = key.into();
    let value_str = value.into();
    self.query_params.push((key_str, value_str));
    self
  }

  /// Add multiple raw (non-encoded) query parameters from an iterator
  #[must_use]
  pub fn query_pairs_raw<I, K, V>(
    mut self,
    iter: I,
  ) -> Self
  where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
  {
    self
      .query_params
      .extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
    self
  }

  /// Add a form data field (application/x-www-form-urlencoded)
  #[must_use]
  pub fn form(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.form_data.push((key.into(), value.into()));
    self
  }

  /// Set the Content-Type header
  #[must_use]
  pub fn content_type(
    self,
    content_type: impl Into<String>,
  ) -> Self {
    self.header(HeaderName::CONTENT_TYPE, content_type)
  }

  /// Add a cookie to the request
  ///
  /// Cookies are automatically combined into a single Cookie header with semicolon separators.
  /// Multiple calls to this method will append cookies.
  ///
  /// # Example
  /// ```no_run
  /// # use barehttp::HttpClient;
  /// let mut client = HttpClient::new()?;
  /// client.get("http://example.com")
  ///     .cookie("session", "abc123")
  ///     .cookie("user", "john")
  ///     .call()?;
  /// # Ok::<(), barehttp::Error>(())
  /// ```
  #[must_use]
  pub fn cookie(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    use alloc::format;
    let name_str = name.into();
    let value_str = value.into();
    let cookie_value = format!("{name_str}={value_str}");

    // Check if Cookie header already exists
    if let Some(existing) = self.headers.get(HeaderName::COOKIE) {
      // Append to existing cookies with semicolon separator
      let combined = format!("{existing}; {cookie_value}");
      self.headers.remove(HeaderName::COOKIE);
      self.headers.insert(HeaderName::COOKIE, combined);
    } else {
      // Create new Cookie header
      self.headers.insert(HeaderName::COOKIE, cookie_value);
    }

    self
  }

  /// Override the request URL
  #[must_use]
  pub fn uri(
    mut self,
    url: impl Into<String>,
  ) -> Self {
    self.url = url.into();
    self
  }

  /// Get the HTTP method
  #[must_use]
  pub const fn method(&self) -> Method {
    self.method
  }

  /// Get the request URL
  #[must_use]
  pub fn url(&self) -> &str {
    &self.url
  }

  /// Get immutable reference to request headers
  #[must_use]
  pub const fn headers_ref(&self) -> &Headers {
    &self.headers
  }

  /// Get mutable reference to request headers
  #[must_use]
  pub const fn headers_mut(&mut self) -> &mut Headers {
    &mut self.headers
  }

  /// Set the HTTP protocol version
  #[must_use]
  pub const fn version(
    mut self,
    version: Version,
  ) -> Self {
    self.version = version;
    self
  }

  /// Get the HTTP protocol version
  #[must_use]
  pub const fn version_ref(&self) -> Version {
    self.version
  }

  /// Override the client configuration for this request
  #[must_use]
  pub fn with_config(
    mut self,
    config: Config,
  ) -> Self {
    self.request_config = Some(config);
    self
  }

  /// Get the request-specific configuration if set
  #[must_use]
  pub const fn config_ref(&self) -> Option<&Config> {
    self.request_config.as_ref()
  }

  fn build_url(&self) -> String {
    if self.query_params.is_empty() {
      return self.url.clone();
    }

    let mut url = self.url.clone();
    let separator = if url.contains('?') {
      '&'
    } else {
      '?'
    };
    url.push(separator);

    for (i, (key, value)) in self.query_params.iter().enumerate() {
      if i > 0 {
        url.push('&');
      }
      url.push_str(&percent_encode(key));
      url.push('=');
      url.push_str(&percent_encode(value));
    }

    url
  }

  fn build_form_url_encoded<I, K, V>(iter: I) -> Vec<u8>
  where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let mut body = String::new();
    for (i, (key, value)) in iter.into_iter().enumerate() {
      if i > 0 {
        body.push('&');
      }
      body.push_str(&percent_encode(key.as_ref()));
      body.push('=');
      body.push_str(&percent_encode(value.as_ref()));
    }
    body.into_bytes()
  }

  fn build_form_body(&self) -> Vec<u8> {
    let mut body = String::new();
    for (i, (key, value)) in self.form_data.iter().enumerate() {
      if i > 0 {
        body.push('&');
      }
      body.push_str(&percent_encode(key));
      body.push('=');
      body.push_str(&percent_encode(value));
    }
    body.into_bytes()
  }
}

impl<S, D> ClientRequestBuilder<S, D, WithoutBody>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  /// Create a new request builder for methods without a body
  pub fn new(
    client: HttpClient<S, D>,
    method: Method,
    url: impl Into<String>,
  ) -> Self {
    Self {
      client,
      method,
      url: url.into(),
      headers: Headers::new(),
      query_params: Vec::new(),
      form_data: Vec::new(),
      body: None,
      version: Version::HTTP_11,
      request_config: None,
      _phantom: PhantomData,
    }
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn call(self) -> Result<Response, Error> {
    let url = self.build_url();

    let body = if self.form_data.is_empty() {
      self.body
    } else {
      Some(self.build_form_body())
    };

    self
      .client
      .request(self.method, &url, &self.headers, body, self.request_config)
  }

  /// Force this request to allow a body (e.g., for DELETE with body)
  #[must_use]
  pub fn force_send_body(self) -> ClientRequestBuilder<S, D, WithBody> {
    ClientRequestBuilder {
      client: self.client,
      method: self.method,
      url: self.url,
      headers: self.headers,
      query_params: self.query_params,
      form_data: self.form_data,
      body: self.body,
      version: self.version,
      request_config: self.request_config,
      _phantom: PhantomData,
    }
  }
}

impl<S, D> ClientRequestBuilder<S, D, WithBody>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  /// Create a new request builder for methods with a body
  pub fn new(
    client: HttpClient<S, D>,
    method: Method,
    url: impl Into<String>,
  ) -> Self {
    Self {
      client,
      method,
      url: url.into(),
      headers: Headers::new(),
      query_params: Vec::new(),
      form_data: Vec::new(),
      body: None,
      version: Version::HTTP_11,
      request_config: None,
      _phantom: PhantomData,
    }
  }

  /// Set the request body
  #[must_use]
  pub fn body(
    mut self,
    data: Vec<u8>,
  ) -> Self {
    self.body = Some(data);
    self
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn call(self) -> Result<Response, Error> {
    let url = self.build_url();

    let body = if self.form_data.is_empty() {
      self.body
    } else {
      Some(self.build_form_body())
    };

    self
      .client
      .request(self.method, &url, &self.headers, body, self.request_config)
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn send_string(
    mut self,
    content: impl Into<String>,
  ) -> Result<Response, Error> {
    self.body = Some(content.into().into_bytes());
    self.call()
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn send_bytes(
    mut self,
    bytes: Vec<u8>,
  ) -> Result<Response, Error> {
    self.body = Some(bytes);
    self.call()
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn send(
    mut self,
    body: impl IntoBody,
  ) -> Result<Response, Error> {
    self.body = Some(body.into_body());
    self.call()
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn send_form<I, K, V>(
    mut self,
    iter: I,
  ) -> Result<Response, Error>
  where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let form_body = Self::build_form_url_encoded(iter);
    self
      .headers
      .insert(HeaderName::CONTENT_TYPE, "application/x-www-form-urlencoded");
    self.body = Some(form_body);
    self.call()
  }

  /// # Errors
  /// Returns an error if the request fails
  pub fn send_empty(self) -> Result<Response, Error> {
    self.call()
  }
}
