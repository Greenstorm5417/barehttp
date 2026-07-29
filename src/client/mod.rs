#[cfg(test)]
pub mod tests;

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::parser::serialize_request;
use crate::parser::uri::{Host, Uri};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::{BlockingSocket, BlockingSocketFactory};
use crate::transport::{ConnectionPool, PoolKey, RawResponse};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "cookie-jar")]
use crate::cookie_jar::CookieStore;

/// HTTP client. `S` = socket ([`BlockingSocketFactory`]), `D` = DNS (`DnsResolver`).
///
/// [`Clone`] shares the connection pool (and cookie store when enabled) via `Arc`.
///
/// ```no_run
/// use barehttp::HttpClient;
///
/// let client = HttpClient::new();
/// let response = client.get("http://example.com").call()?;
/// # Ok::<(), barehttp::Error>(())
/// ```
pub struct HttpClient<S, D> {
  pool: Arc<ConnectionPool<S>>,
  dns: Arc<D>,
  config: Arc<Config>,
  #[cfg(feature = "cookie-jar")]
  cookie_store: Arc<CookieStore>,
}

impl<S, D> fmt::Debug for HttpClient<S, D> {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    f.debug_struct("HttpClient")
      .field("config", &self.config)
      .finish_non_exhaustive()
  }
}

impl<S, D> Clone for HttpClient<S, D> {
  fn clone(&self) -> Self {
    Self {
      pool: Arc::clone(&self.pool),
      dns: Arc::clone(&self.dns),
      config: Arc::clone(&self.config),
      #[cfg(feature = "cookie-jar")]
      cookie_store: Arc::clone(&self.cookie_store),
    }
  }
}

impl HttpClient<crate::socket::OsBlockingSocket, crate::dns::OsDnsResolver> {
  /// OS socket + DNS, default config.
  #[must_use]
  pub fn new() -> Self {
    Self::with_adapters(crate::dns::OsDnsResolver, Config::default())
  }

  /// OS socket + DNS, custom config.
  #[must_use]
  pub fn with_config(config: Config) -> Self {
    Self::with_adapters(crate::dns::OsDnsResolver, config)
  }
}

impl Default for HttpClient<crate::socket::OsBlockingSocket, crate::dns::OsDnsResolver> {
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> HttpClient<S, D>
where
  S: BlockingSocketFactory,
  D: DnsResolver,
{
  /// Custom DNS + config. Socket type `S` comes from `S::new()` at connect time
  /// (`HttpClient::<OsBlockingSocket, _>::with_adapters(dns, config)`).
  #[must_use]
  pub fn with_adapters(
    dns: D,
    config: Config,
  ) -> Self {
    Self {
      pool: Arc::new(ConnectionPool::new(config.max_idle_per_host(), config.max_idle_age())),
      dns: Arc::new(dns),
      config: Arc::new(config),
      #[cfg(feature = "cookie-jar")]
      cookie_store: Arc::new(CookieStore::new()),
    }
  }

  /// Request with an arbitrary method.
  pub fn method(
    &self,
    method: Method,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    ClientRequestBuilder::new(self.clone(), method, url)
  }

  /// GET (no body).
  pub fn get(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Get, url)
  }

  /// POST (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  pub fn post(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Post, url)
  }

  /// PUT (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  pub fn put(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Put, url)
  }

  /// DELETE (no request body).
  pub fn delete(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Delete, url)
  }

  /// HEAD (no body).
  pub fn head(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Head, url)
  }

  /// PATCH (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  pub fn patch(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Patch, url)
  }

  /// Shared cookie store (`cookie-jar` feature).
  #[cfg(feature = "cookie-jar")]
  #[must_use]
  pub const fn cookie_store(&self) -> &Arc<CookieStore> {
    &self.cookie_store
  }

  /// Shared client config.
  #[must_use]
  pub fn config(&self) -> &Config {
    self.config.as_ref()
  }

  /// Redirect loop; each hop connect / write / read.
  ///
  /// # Errors
  /// [`Error::InvalidUrl`], [`Error::Dns`], [`Error::Socket`], [`Error::Parse`],
  /// redirect / TLS / size-limit variants, or [`Error::HttpStatus`] when configured.
  pub fn request(
    &self,
    method: Method,
    url: &str,
    custom_headers: &Headers,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    self.request_with_config(self.config.as_ref(), method, url, custom_headers, body)
  }

  /// [`Self::request`] with a caller-supplied config.
  ///
  /// # Errors
  /// Same as [`Self::request`].
  pub fn request_with_config(
    &self,
    config: &Config,
    method: Method,
    url: &str,
    custom_headers: &Headers,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    // Refuse assume_tls_socket with cleartext OS adapter.
    if config.assume_tls_socket() && S::is_os_cleartext() {
      return Err(Error::TlsNotConfigured);
    }

    let mut current_url = String::from(url);
    let mut current_method = method;
    let mut current_body = body;
    let mut current_headers = custom_headers.clone();
    let mut visited_urls: Vec<String> = Vec::new();
    let mut redirect_count = 0_u32;

    loop {
      let uri = Uri::parse(&current_url).map_err(Error::Parse)?;
      validate_protocol(config, &uri)?;

      #[cfg(feature = "cookie-jar")]
      let mut headers_with_cookies = current_headers.clone();
      #[cfg(feature = "cookie-jar")]
      {
        let is_secure = uri.scheme().eq_ignore_ascii_case("https");
        let cookie_header = self
          .cookie_store
          .request_cookie_header(&current_url, is_secure);
        headers_with_cookies.merge_cookie(&cookie_header);
      }

      #[cfg(feature = "cookie-jar")]
      let headers_to_use = &headers_with_cookies;
      #[cfg(not(feature = "cookie-jar"))]
      let headers_to_use = &current_headers;

      let raw = execute(
        &self.pool,
        self.dns.as_ref(),
        config,
        &uri,
        current_method,
        headers_to_use,
        current_body.as_deref(),
      )?;

      #[cfg(feature = "cookie-jar")]
      {
        let set_cookie_headers: Vec<String> = raw
          .headers
          .get_all(Headers::SET_COOKIE)
          .into_iter()
          .map(alloc::string::ToString::to_string)
          .collect();

        if !set_cookie_headers.is_empty() {
          self
            .cookie_store
            .store_response_cookies(&current_url, &set_cookie_headers);
        }
      }

      let response = raw_to_response(raw, current_method, config.max_response_body_size())?;

      if config.http_status_as_error() && (400..600).contains(&response.status_code()) {
        return Err(Error::HttpStatus(response.status_code(), response));
      }

      let Some((next_url, next_method, next_body)) = follow_redirect(
        config,
        &mut visited_urls,
        &mut redirect_count,
        &response,
        &uri,
        &current_url,
        current_method,
        current_body,
      )?
      else {
        return Ok(response);
      };

      sanitize_redirect_headers(&mut current_headers, next_body.is_none());
      current_url = next_url;
      current_method = next_method;
      current_body = next_body;
    }
  }
}

const fn pooling_enabled(config: &Config) -> bool {
  config.max_idle_per_host() > 0
}

fn raw_to_response(
  raw: RawResponse,
  method: Method,
  max_body: usize,
) -> Result<Response, Error> {
  let RawResponse {
    status_code,
    reason,
    mut headers,
    version,
    body_bytes,
  } = raw;

  let (response_body, trailers) = if method == Method::Head {
    (Vec::new(), Vec::new())
  } else {
    Response::parse_body_from_owned(body_bytes, &mut headers, status_code, version, max_body).map_err(|e| match e {
      crate::error::ParseError::BodyExceedsLimit(n) => Error::BodyExceedsLimit(n),
      other => Error::Parse(other),
    })?
  };

  Ok(Response::from_parts(
    status_code,
    reason,
    headers,
    response_body,
    trailers,
  ))
}

/// Check scheme against `assume_tls_socket` and `https_only`.
pub const fn validate_protocol(
  config: &Config,
  uri: &Uri,
) -> Result<(), Error> {
  if uri.scheme().eq_ignore_ascii_case("https") && !config.assume_tls_socket() {
    return Err(Error::TlsNotConfigured);
  }
  if config.https_only() && !uri.scheme().eq_ignore_ascii_case("https") {
    return Err(Error::HttpsOnly);
  }
  Ok(())
}

/// Strip hop-by-hop headers plus Authorization and Cookie on every redirect hop
/// (`RedirectAuthHeaders::Never`). Always strip Content-Length and Host (rebuilt
/// for the next hop). When `drop_body`, also strip Content-Type (body is empty).
pub fn sanitize_redirect_headers(
  headers: &mut Headers,
  drop_body: bool,
) {
  for name in [
    "Connection",
    "Keep-Alive",
    "Proxy-Authenticate",
    "Proxy-Authorization",
    "TE",
    "Trailer",
    "Transfer-Encoding",
    "Upgrade",
  ] {
    headers.remove(name);
  }
  headers.remove("Authorization");
  headers.remove("Cookie");
  headers.remove("Content-Length");
  headers.remove(Headers::HOST);
  if drop_body {
    headers.remove("Content-Type");
  }
}

/// True for followable redirects: 301/302/303/307/308.
const fn is_followable_redirect(status: u16) -> bool {
  matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// `Ok(None)` = return this response. `Ok(Some(...))` = follow redirect.
///
/// Method / body rules match ureq (`ureq_proto` redirect):
/// - 301/302/303: GET/HEAD keep method; all others become GET with no body.
/// - 307/308: GET/HEAD keep method; POST/PUT/PATCH/DELETE -> [`Error::RedirectFailed`].
pub fn follow_redirect(
  config: &Config,
  visited_urls: &mut Vec<String>,
  redirect_count: &mut u32,
  response: &Response,
  current_uri: &Uri,
  current_url: &str,
  current_method: Method,
  current_body: Option<Vec<u8>>,
) -> Result<Option<(String, Method, Option<Vec<u8>>)>, Error> {
  // `max_redirects == 0` means do not follow (return the redirect response).
  if config.max_redirects() == 0 || !is_followable_redirect(response.status_code()) {
    return Ok(None);
  }

  if *redirect_count >= config.max_redirects() {
    return Err(Error::TooManyRedirects);
  }

  let location = response
    .header("location")
    .ok_or(Error::MissingRedirectLocation)?;

  let next_url = current_uri
    .resolve_relative(location)
    .map_err(Error::Parse)?;

  if visited_urls.iter().any(|u| u.as_str() == next_url.as_str()) {
    return Err(Error::RedirectLoop);
  }

  visited_urls.push(String::from(current_url));

  let (next_method, next_body) = redirect_method_and_body(response.status_code(), current_method, current_body)?;

  *redirect_count = redirect_count.saturating_add(1);

  Ok(Some((next_url, next_method, next_body)))
}

fn redirect_method_and_body(
  status: u16,
  method: Method,
  body: Option<Vec<u8>>,
) -> Result<(Method, Option<Vec<u8>>), Error> {
  match status {
    307 | 308 => {
      // Retain method only when there is no request body to replay (ureq).
      if method.needs_request_body() || method == Method::Delete {
        return Err(Error::RedirectFailed);
      }
      Ok((method, body))
    },
    // 301, 302, 303 (and only those are followable besides 307/308)
    _ => {
      if matches!(method, Method::Get | Method::Head) {
        Ok((method, body))
      } else {
        Ok((Method::Get, None))
      }
    },
  }
}

/// HTTP hop: pool/connect, send, read, maybe return socket to pool.
///
/// On I/O failure with a reused pooled socket, drops it and retries once with a fresh connect.
fn execute<S, D>(
  pool: &Arc<ConnectionPool<S>>,
  dns: &D,
  config: &Config,
  uri: &Uri,
  method: Method,
  custom_headers: &Headers,
  body: Option<&[u8]>,
) -> Result<RawResponse, Error>
where
  S: BlockingSocket + BlockingSocketFactory,
  D: DnsResolver,
{
  let host_str = host_from_uri(uri);
  let port = uri.port_or_default();
  let pool_key = PoolKey::new(uri.scheme().to_ascii_lowercase(), &host_str, port);

  let (mut socket, reused) = get_or_create_socket(pool, config, &pool_key)?;
  match try_one_hop(
    dns,
    config,
    uri,
    method,
    custom_headers,
    body,
    &host_str,
    port,
    &mut socket,
    reused,
  ) {
    Ok((raw, reusable)) => {
      if pooling_enabled(config) && reusable {
        pool.return_connection(pool_key, socket);
      }
      Ok(raw)
    },
    Err(e) if reused && matches!(e, Error::Socket(_)) => {
      drop(socket);
      let mut fresh = S::new().map_err(Error::Socket)?;
      let (raw, reusable) = try_one_hop(
        dns,
        config,
        uri,
        method,
        custom_headers,
        body,
        &host_str,
        port,
        &mut fresh,
        false,
      )?;
      if pooling_enabled(config) && reusable {
        pool.return_connection(pool_key, fresh);
      }
      Ok(raw)
    },
    Err(e) => Err(e),
  }
}

fn try_one_hop<S, D>(
  dns: &D,
  config: &Config,
  uri: &Uri,
  method: Method,
  custom_headers: &Headers,
  body: Option<&[u8]>,
  host_str: &str,
  port: u16,
  socket: &mut S,
  reused: bool,
) -> Result<(RawResponse, bool), Error>
where
  S: BlockingSocket + BlockingSocketFactory,
  D: DnsResolver,
{
  let mut conn = crate::transport::connection::connect(socket, dns, uri, config, reused)?;
  let request_bytes = build_request(uri, method, host_str, port, custom_headers, body, config)?;
  conn.send_request(&request_bytes)?;
  let raw = conn.read_raw_response(method != Method::Head)?;
  let reusable = conn.is_reusable();
  Ok((raw, reusable))
}

fn host_from_uri(uri: &Uri) -> String {
  let Some(auth) = uri.authority() else {
    return String::new();
  };
  match auth.host() {
    Host::RegName(name) => String::from(*name),
    Host::IpAddr(addr) => crate::util::format_ip_for_host(*addr),
  }
}

fn get_or_create_socket<S>(
  pool: &Arc<ConnectionPool<S>>,
  config: &Config,
  pool_key: &PoolKey,
) -> Result<(S, bool), Error>
where
  S: BlockingSocket + BlockingSocketFactory,
{
  if pooling_enabled(config)
    && let Some(s) = pool.get(pool_key)
  {
    return Ok((s, true));
  }
  S::new().map(|s| (s, false)).map_err(Error::Socket)
}

/// Build wire request bytes (HTTP/1.1 + Host + origin-form target).
///
/// Exposed for unit tests that assert wire serialization.
pub fn build_request(
  uri: &Uri,
  method: Method,
  host_str: &str,
  port: u16,
  custom_headers: &Headers,
  body: Option<&[u8]>,
  config: &Config,
) -> Result<Vec<u8>, Error> {
  // Userinfo rejected at Uri::parse for HTTP client use.

  let host_header = if (uri.scheme().eq_ignore_ascii_case("http") && port == 80)
    || (uri.scheme().eq_ignore_ascii_case("https") && port == 443)
  {
    String::from(host_str)
  } else {
    format!("{host_str}:{port}")
  };

  let mut headers = custom_headers.clone();

  // Always rebuild Host for the current hop (user Host may be stale after redirects).
  headers.set(Headers::HOST, host_header.as_str());

  // RFC 9112 §9.3: client that will not reuse MUST send Connection: close
  if !pooling_enabled(config) {
    headers.set(Headers::CONNECTION, "close");
  }

  if !config.user_agent().is_empty() && !headers.contains(Headers::USER_AGENT) {
    headers.insert(Headers::USER_AGENT, config.user_agent());
  }

  if !config.accept().is_empty() && !headers.contains(Headers::ACCEPT) {
    headers.insert(Headers::ACCEPT, config.accept());
  }

  #[cfg(any(feature = "gzip", feature = "zstd"))]
  if !headers.contains(Headers::ACCEPT_ENCODING) {
    #[allow(unused_mut)]
    let mut encodings: Vec<&str> = Vec::new();

    #[cfg(feature = "gzip")]
    {
      encodings.push("gzip");
      encodings.push("deflate");
    }

    #[cfg(feature = "zstd")]
    encodings.push("zstd");

    if !encodings.is_empty() {
      let accept_encoding = encodings.join(", ");
      headers.insert(Headers::ACCEPT_ENCODING, accept_encoding.as_str());
    }
  }

  serialize_request(method.as_str(), &uri.to_path_and_query(), &headers, body).map_err(Error::Parse)
}
