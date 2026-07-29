use crate::client::HttpClient;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::socket::BlockingSocket;
use crate::util::percent_encode;
use alloc::string::String;
use alloc::vec::Vec;

/// Request builder for [`HttpClient`].
pub struct ClientRequestBuilder<'a, S, D> {
  client: &'a HttpClient<S, D>,
  method: Method,
  url: String,
  headers: Headers,
  query_params: Vec<(String, String)>,
  form_data: Vec<(String, String)>,
  body: Option<Vec<u8>>,
}

impl<'a, S, D> ClientRequestBuilder<'a, S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  pub(crate) fn new(
    client: &'a HttpClient<S, D>,
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

  /// Add a cookie to the request.
  ///
  /// Appends `name=value` to the Cookie header (`; `-joined).
  ///
  /// # Errors
  /// Returns [`Error::InvalidRequest`] if name or value contains `;` or a control character.
  pub fn cookie(
    mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) -> Result<Self, Error> {
    let name_str = name.into();
    let value_str = value.into();
    if !cookie_pair_ok(&name_str) || !cookie_pair_ok(&value_str) {
      return Err(Error::InvalidRequest);
    }
    self
      .headers
      .merge_cookie(&alloc::format!("{name_str}={value_str}"));
    Ok(self)
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
  /// [`Error::InvalidRequest`] if form fields and a body are both set; otherwise URL parse,
  /// DNS, connect, HTTP, or policy failure from the client.
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
    let body = if self.form_data.is_empty() {
      self.body
    } else {
      if !headers.contains(Headers::CONTENT_TYPE) {
        headers.insert(Headers::CONTENT_TYPE, "application/x-www-form-urlencoded");
      }
      Some(encode_pairs(self.form_data.iter().map(|(k, v)| (k.as_str(), v.as_str()))).into_bytes())
    };

    self.client.request(self.method, &url, &headers, body)
  }

  /// # Errors
  /// Same as [`Self::call`].
  pub fn send(
    mut self,
    body: impl AsRef<[u8]>,
  ) -> Result<Response, Error> {
    self.body = Some(body.as_ref().to_vec());
    self.call()
  }
}

fn encode_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> String {
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

fn append_encoded_pairs<'a>(
  base: &str,
  pairs: impl Iterator<Item = (&'a str, &'a str)>,
) -> String {
  let encoded = encode_pairs(pairs);
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
