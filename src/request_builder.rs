use crate::client::HttpClient;
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, InvalidRequest};
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::socket::{BlockingSocket, BlockingSocketFactory};
use crate::util::{form_url_encode, percent_encode};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;

/// Request builder for [`HttpClient`].
///
/// Finish with [`Self::call`] (no body / body already set) or [`Self::send`].
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::HttpClient::new()
///     .get("http://example.com")
///     .header("Accept", "text/plain")
///     .call()?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use = "builders do nothing unless you call `.call()` or `.send()`"]
pub struct ClientRequestBuilder<S, D> {
  client: HttpClient<S, D>,
  method: Method,
  url: String,
  headers: Headers,
  query_params: Vec<(String, String)>,
  form_data: Vec<(String, String)>,
  cookies: Vec<(String, String)>,
  body: Option<Vec<u8>>,
  /// Optional per-request config (cloned from the client, then patched by timeout setters).
  config_override: Option<Config>,
}

impl<S, D> fmt::Debug for ClientRequestBuilder<S, D> {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    f.debug_struct("ClientRequestBuilder")
      .field("method", &self.method)
      .field("url", &self.url)
      .field("headers", &self.headers)
      .field("has_body", &self.body.is_some())
      .finish_non_exhaustive()
  }
}

impl<S, D> ClientRequestBuilder<S, D>
where
  S: BlockingSocket + BlockingSocketFactory,
  D: DnsResolver,
{
  pub(crate) fn new(
    client: HttpClient<S, D>,
    method: Method,
    url: impl AsRef<str>,
  ) -> Self {
    Self {
      client,
      method,
      url: String::from(url.as_ref()),
      headers: Headers::new(),
      query_params: Vec::new(),
      form_data: Vec::new(),
      cookies: Vec::new(),
      body: None,
      config_override: None,
    }
  }

  fn ensure_config_override(&mut self) {
    if self.config_override.is_none() {
      self.config_override = Some(self.client.config().clone());
    }
  }

  /// Override [`Config::timeout_connect`] for this request.
  #[must_use]
  pub fn timeout_connect(
    mut self,
    timeout: Option<Duration>,
  ) -> Self {
    self.ensure_config_override();
    if let Some(ref mut c) = self.config_override {
      c.set_timeout_connect(timeout);
    }
    self
  }

  /// Override [`Config::timeout_read`] for this request.
  #[must_use]
  pub fn timeout_read(
    mut self,
    timeout: Option<Duration>,
  ) -> Self {
    self.ensure_config_override();
    if let Some(ref mut c) = self.config_override {
      c.set_timeout_read(timeout);
    }
    self
  }

  /// Override [`Config::timeout_write`] for this request.
  #[must_use]
  pub fn timeout_write(
    mut self,
    timeout: Option<Duration>,
  ) -> Self {
    self.ensure_config_override();
    if let Some(ref mut c) = self.config_override {
      c.set_timeout_write(timeout);
    }
    self
  }

  /// Override [`Config::max_response_body_size`] for this request.
  #[must_use]
  pub fn max_response_body_size(
    mut self,
    limit: usize,
  ) -> Self {
    self.ensure_config_override();
    if let Some(ref mut c) = self.config_override {
      c.set_max_response_body_size(limit);
    }
    self
  }

  /// Append a header (does not replace existing values for the same name).
  #[must_use]
  pub fn header(
    mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Self {
    self.headers.insert(name, value);
    self
  }

  /// Replace all values for a header name.
  #[must_use]
  pub fn set_header(
    mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Self {
    self.headers.set(name, value);
    self
  }

  /// Set `Content-Type`, replacing any prior value.
  #[must_use]
  pub fn content_type(
    self,
    value: impl AsRef<str>,
  ) -> Self {
    self.set_header(Headers::CONTENT_TYPE, value)
  }

  /// Add a URL-encoded query parameter (space as `%20`).
  #[must_use]
  pub fn query(
    mut self,
    key: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Self {
    self
      .query_params
      .push((String::from(key.as_ref()), String::from(value.as_ref())));
    self
  }

  /// Add multiple URL-encoded query parameters (space as `%20`).
  #[must_use]
  pub fn query_pairs<I, K, V>(
    mut self,
    iter: I,
  ) -> Self
  where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
  {
    self.query_params.extend(
      iter
        .into_iter()
        .map(|(k, v)| (String::from(k.as_ref()), String::from(v.as_ref()))),
    );
    self
  }

  /// Append `name=value` to the Cookie header (`; `-joined) when the request is sent.
  ///
  /// Invalid names/values (`;` or control characters) make [`Self::call`] / [`Self::send`]
  /// return [`Error::InvalidRequest`] with [`InvalidRequest::CookieOctet`].
  #[must_use]
  pub fn cookie(
    mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Self {
    self
      .cookies
      .push((String::from(name.as_ref()), String::from(value.as_ref())));
    self
  }

  /// Add a form data field (`application/x-www-form-urlencoded`).
  #[must_use]
  pub fn form(
    mut self,
    key: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Self {
    self
      .form_data
      .push((String::from(key.as_ref()), String::from(value.as_ref())));
    self
  }

  /// Set the request body.
  #[must_use]
  pub fn body(
    mut self,
    data: impl AsRef<[u8]>,
  ) -> Self {
    self.body = Some(data.as_ref().to_vec());
    self
  }

  /// Send the request.
  ///
  /// # Errors
  /// [`Error::InvalidRequest`] for [`Method::Connect`] (no tunnel API; RFC 9112
  /// authority-form / ignore CL/TE on success), if form fields and a body are both set,
  /// or a cookie is invalid. Otherwise the same failures as
  /// [`HttpClient::request_with_config`].
  ///
  /// # Examples
  ///
  /// ```no_run
  /// let response = barehttp::get("http://example.com")
  ///     .header("Accept", "text/plain")
  ///     .call()?;
  /// assert!(response.status_code() > 0);
  /// # Ok::<(), barehttp::Error>(())
  /// ```
  pub fn call(self) -> Result<Response, Error> {
    if matches!(self.method, Method::Connect) {
      return Err(Error::InvalidRequest(InvalidRequest::ConnectUnsupported));
    }
    if !self.form_data.is_empty() && self.body.is_some() {
      return Err(Error::InvalidRequest(InvalidRequest::FormAndBody));
    }

    let url = append_encoded_pairs(
      &self.url,
      self
        .query_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str())),
    );

    let mut headers = self.headers;
    for (name, value) in &self.cookies {
      if !cookie_pair_ok(name) || !cookie_pair_ok(value) {
        return Err(Error::InvalidRequest(InvalidRequest::CookieOctet));
      }
      headers.merge_cookie(&alloc::format!("{name}={value}"));
    }

    let prepared_body = if self.form_data.is_empty() {
      self.body
    } else {
      if !headers.contains(Headers::CONTENT_TYPE) {
        headers.set(Headers::CONTENT_TYPE, "application/x-www-form-urlencoded");
      }
      Some(encode_form_pairs(self.form_data.iter().map(|(k, v)| (k.as_str(), v.as_str()))).into_bytes())
    };

    // POST/PUT/PATCH with no body still send Content-Length: 0 (via empty Some).
    let body = match (&self.method, prepared_body) {
      (Method::Post | Method::Put | Method::Patch, None) => Some(Vec::new()),
      (_, other) => other,
    };

    // Borrow client config when no per-request override (avoid cloning Strings).
    // Move headers in so the client can mutate Host/defaults in place (no map clone).
    let config_owned = self.config_override;
    let config = config_owned.as_ref().unwrap_or_else(|| self.client.config());
    self
      .client
      .request_with_config_owned(config, self.method, &url, &headers, body)
  }

  /// Set the body and send ([`Self::call`]).
  ///
  /// # Errors
  /// Same as [`Self::call`].
  pub fn send(
    mut self,
    body: impl AsRef<[u8]>,
  ) -> Result<Response, Error> {
    self.body = Some(body.as_ref().to_vec());
    self.call()
  }

  /// Encode `iter` as `application/x-www-form-urlencoded` (space as `+`) and send.
  ///
  /// Sets `Content-Type` if not already present.
  ///
  /// # Errors
  /// Same as [`Self::call`].
  pub fn send_form<I, K, V>(
    mut self,
    iter: I,
  ) -> Result<Response, Error>
  where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let encoded = encode_form_pairs(iter);
    if !self.headers.contains(Headers::CONTENT_TYPE) {
      self
        .headers
        .set(Headers::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    self.body = Some(encoded.into_bytes());
    self.form_data.clear();
    self.call()
  }
}

fn encode_query_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> String {
  let mut out = String::new();
  for (i, (key, value)) in pairs.enumerate() {
    if i > 0 {
      out.push('&');
    }
    out.push_str(&percent_encode(key));
    out.push('=');
    out.push_str(&percent_encode(value));
  }
  out
}

fn encode_form_pairs<I, K, V>(pairs: I) -> String
where
  I: IntoIterator<Item = (K, V)>,
  K: AsRef<str>,
  V: AsRef<str>,
{
  let mut out = String::new();
  for (i, (key, value)) in pairs.into_iter().enumerate() {
    if i > 0 {
      out.push('&');
    }
    out.push_str(&form_url_encode(key.as_ref()));
    out.push('=');
    out.push_str(&form_url_encode(value.as_ref()));
  }
  out
}

fn append_encoded_pairs<'a>(
  base: &str,
  pairs: impl Iterator<Item = (&'a str, &'a str)>,
) -> String {
  let encoded = encode_query_pairs(pairs);
  if encoded.is_empty() {
    return String::from(base);
  }
  let mut url = String::from(base);
  url.push(if url.contains('?') {
    '&'
  } else {
    '?'
  });
  url.push_str(&encoded);
  url
}

fn cookie_pair_ok(s: &str) -> bool {
  !s.bytes().any(|b| b == b';' || b.is_ascii_control())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::{encode_form_pairs, encode_query_pairs};
  use crate::HttpClient;
  use crate::error::{Error, InvalidRequest};

  #[test]
  fn query_encodes_space_as_percent20() {
    assert_eq!(encode_query_pairs([("a", "b c")].into_iter()), "a=b%20c");
  }

  #[test]
  fn form_encodes_space_as_plus() {
    assert_eq!(encode_form_pairs([("a", "b c")].into_iter()), "a=b+c");
  }

  #[test]
  fn form_and_body_is_invalid_request() {
    let err = HttpClient::new()
      .post("http://example.com/")
      .form("a", "1")
      .body(b"x")
      .call()
      .unwrap_err();
    assert_eq!(err, Error::InvalidRequest(InvalidRequest::FormAndBody));
  }

  #[test]
  fn cookie_control_octet_is_invalid_request() {
    let err = HttpClient::new()
      .get("http://example.com/")
      .cookie("a", "b\nc")
      .call()
      .unwrap_err();
    assert_eq!(err, Error::InvalidRequest(InvalidRequest::CookieOctet));
  }

  #[test]
  fn connect_method_rejected_at_call() {
    use crate::method::Method;
    let err = HttpClient::new()
      .method(Method::Connect, "http://example.com:443")
      .call()
      .unwrap_err();
    assert_eq!(err, Error::InvalidRequest(InvalidRequest::ConnectUnsupported));
  }

  #[test]
  fn connect_method_rejected_on_request_api() {
    use crate::headers::Headers;
    use crate::method::Method;
    let client = HttpClient::new();
    let err = client
      .request(Method::Connect, "http://example.com:443", &Headers::new(), None::<&[u8]>)
      .unwrap_err();
    assert_eq!(err, Error::InvalidRequest(InvalidRequest::ConnectUnsupported));
  }

  #[test]
  fn connect_with_body_still_rejected() {
    use crate::method::Method;
    let err = HttpClient::new()
      .method(Method::Connect, "http://example.com:443")
      .body(b"nope")
      .call()
      .unwrap_err();
    assert_eq!(err, Error::InvalidRequest(InvalidRequest::ConnectUnsupported));
  }
}
