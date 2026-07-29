use crate::client::HttpClient;
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::socket::BlockingSocket;
use crate::util::{form_url_encode, percent_encode};
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

/// Request builder for [`HttpClient`].
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

impl<S, D> ClientRequestBuilder<S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  pub(crate) fn new(
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
      c.timeout_connect = timeout;
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
      c.timeout_read = timeout;
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
      c.timeout_write = timeout;
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
      c.max_response_body_size = limit;
    }
    self
  }

  /// Append a header (does not replace existing values for the same name).
  #[must_use]
  pub fn header(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.headers.insert(name, value);
    self
  }

  /// Replace all values for a header name.
  #[must_use]
  pub fn set_header(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.headers.set(name, value);
    self
  }

  /// Set `Content-Type`, replacing any prior value.
  #[must_use]
  pub fn content_type(
    self,
    value: impl Into<String>,
  ) -> Self {
    self.set_header(Headers::CONTENT_TYPE, value)
  }

  /// Add a URL-encoded query parameter (space as `%20`).
  #[must_use]
  pub fn query(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.query_params.push((key.into(), value.into()));
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
    K: Into<String>,
    V: Into<String>,
  {
    self
      .query_params
      .extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
    self
  }

  /// Add a cookie to the request.
  ///
  /// Appends `name=value` to the Cookie header (`; `-joined) when the request is sent.
  /// Invalid names/values (`;` or control characters) make [`Self::call`] / [`Self::send`]
  /// return [`Error::InvalidRequest`].
  #[must_use]
  pub fn cookie(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.cookies.push((name.into(), value.into()));
    self
  }

  /// Add a form data field (`application/x-www-form-urlencoded`).
  #[must_use]
  pub fn form(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.form_data.push((key.into(), value.into()));
    self
  }

  /// Set the request body.
  #[must_use]
  pub fn body(
    mut self,
    data: Vec<u8>,
  ) -> Self {
    self.body = Some(data);
    self
  }

  /// Send the request.
  ///
  /// # Errors
  /// [`Error::InvalidRequest`] if form fields and a body are both set, or a cookie is invalid.
  /// Otherwise the same failures as [`HttpClient::request_with_config`].
  pub fn call(self) -> Result<Response, Error> {
    if !self.form_data.is_empty() && self.body.is_some() {
      return Err(Error::InvalidRequest);
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
        return Err(Error::InvalidRequest);
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
    let body = match (self.method, prepared_body) {
      (Method::Post | Method::Put | Method::Patch, None) => Some(Vec::new()),
      (_, other) => other,
    };

    let config = self
      .config_override
      .unwrap_or_else(|| self.client.config().clone());
    self
      .client
      .request_with_config(&config, self.method, &url, &headers, body)
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
mod tests {
  use super::{encode_form_pairs, encode_query_pairs};

  #[test]
  fn query_encodes_space_as_percent20() {
    assert_eq!(encode_query_pairs([("a", "b c")].into_iter()), "a=b%20c");
  }

  #[test]
  fn form_encodes_space_as_plus() {
    assert_eq!(encode_form_pairs([("a", "b c")].into_iter()), "a=b+c");
  }
}
