#[cfg(test)]
pub mod tests;

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, InvalidRequest};
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::parser::uri::Uri;
use crate::parser::{HeadOptions, RequestHead};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::{BlockingSocket, BlockingSocketFactory};
use crate::transport::{ConnectionPool, PoolKeyRef, RawResponse};
use crate::util::{HostText, host_text};
use alloc::borrow::Cow;
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
  #[must_use]
  pub fn method(
    &self,
    method: Method,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    ClientRequestBuilder::new(self.clone(), method, url)
  }

  /// GET (no body).
  #[must_use]
  pub fn get(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Get, url)
  }

  /// POST (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  #[must_use]
  pub fn post(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Post, url)
  }

  /// PUT (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  #[must_use]
  pub fn put(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Put, url)
  }

  /// DELETE (no request body).
  #[must_use]
  pub fn delete(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Delete, url)
  }

  /// HEAD (no body).
  #[must_use]
  pub fn head(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Head, url)
  }

  /// PATCH (body via [`ClientRequestBuilder::send`] / [`ClientRequestBuilder::call`]).
  #[must_use]
  pub fn patch(
    &self,
    url: impl AsRef<str>,
  ) -> ClientRequestBuilder<S, D> {
    self.method(Method::Patch, url)
  }

  /// Shared cookie store (`cookie-jar` feature).
  ///
  /// Borrow of the store (not [`Arc`]); clone the client (or call again) to share
  /// without depending on the internal shared-ownership shape.
  #[cfg(feature = "cookie-jar")]
  #[must_use]
  pub fn cookie_store(&self) -> &CookieStore {
    self.cookie_store.as_ref()
  }

  /// Shared client config.
  #[must_use]
  pub fn config(&self) -> &Config {
    self.config.as_ref()
  }

  /// Redirect loop; each hop connect / write / read.
  ///
  /// Body is copied once into an owned buffer (needed for redirect replay).
  /// Pass `None::<&[u8]>` when there is no body.
  ///
  /// # Errors
  /// [`Error::InvalidRequest`] for [`Method::Connect`] (no tunnel API),
  /// [`Error::InvalidUrl`], [`Error::Dns`], [`Error::Socket`], [`Error::Parse`],
  /// redirect / TLS / size-limit variants, or [`Error::HttpStatus`] when configured.
  pub fn request(
    &self,
    method: Method,
    url: impl AsRef<str>,
    custom_headers: &Headers,
    body: Option<impl AsRef<[u8]>>,
  ) -> Result<Response, Error> {
    self.request_with_config(self.config.as_ref(), method, url, custom_headers, body)
  }

  /// [`Self::request`] with a caller-supplied config.
  ///
  /// Body is copied once into an owned buffer (needed for redirect replay).
  /// Pass `None::<&[u8]>` when there is no body.
  ///
  /// # Errors
  /// Same as [`Self::request`].
  pub fn request_with_config(
    &self,
    config: &Config,
    method: Method,
    url: impl AsRef<str>,
    custom_headers: &Headers,
    body: Option<impl AsRef<[u8]>>,
  ) -> Result<Response, Error> {
    // Public `&[u8]` / `AsRef` cannot move a `Vec`: one `to_vec` is the API cost.
    // Builder path moves an owned body into `request_with_config_owned` (no second copy).
    self.request_with_config_owned(
      config,
      method,
      url.as_ref(),
      Cow::Borrowed(custom_headers),
      body.map(|b| b.as_ref().to_vec()),
    )
  }

  /// Like [`Self::request_with_config`], with headers borrowed or owned (the map
  /// is only copied if a redirect has to strip fields) and an owned body.
  ///
  /// Redirect hops reuse the same `Option<Vec<u8>>` in place (borrow for send, never re-copy).
  pub(crate) fn request_with_config_owned(
    &self,
    config: &Config,
    method: Method,
    url: &str,
    mut current_headers: Cow<'_, Headers>,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    // No tunnel / authority-form yet (RFC 9112 §3.2.3 / §9.3.6).
    if matches!(method, Method::Connect) {
      return Err(Error::InvalidRequest(InvalidRequest::ConnectUnsupported));
    }
    // Refuse assume_tls_socket with cleartext OS adapter.
    if config.assume_tls_socket() && S::is_os_cleartext() {
      return Err(Error::TlsNotConfigured);
    }

    let mut current_url = Cow::Borrowed(url);
    let mut current_method = method;
    // One owned body buffer for the whole redirect chain (send borrows; hops mutate in place).
    let mut current_body = body;
    let mut visited_urls: Vec<String> = Vec::new();
    let mut redirect_count = 0_u32;

    loop {
      let uri = Uri::parse(current_url.as_ref()).map_err(Error::Parse)?;
      validate_protocol(config, &uri)?;

      // Jar cookies ride along as a head option (the caller's map is untouched).
      #[cfg(feature = "cookie-jar")]
      let jar_cookie = self.cookie_store.cookie_header_for_uri(&uri);
      #[cfg(feature = "cookie-jar")]
      let jar_cookie_str = jar_cookie.as_str();
      #[cfg(not(feature = "cookie-jar"))]
      let jar_cookie_str = "";

      let host = host_text(&uri);
      let (raw, to_pool) = execute(
        &self.pool,
        self.dns.as_ref(),
        config,
        &uri,
        &host,
        &current_method,
        &current_headers,
        current_body.as_deref(),
        jar_cookie_str,
      )?;

      #[cfg(feature = "cookie-jar")]
      {
        // Iterate Set-Cookie in place — no intermediate `get_all` Vec.
        if raw.headers.contains(Headers::SET_COOKIE) {
          self
            .cookie_store
            .store_response_cookies(current_url.as_ref(), raw.headers.values(Headers::SET_COOKIE))?;
        }
      }

      // Only idle-pool after body decode succeeds (gzip/limits/etc.).
      let response = raw_to_response(raw, &current_method, config.max_response_body_size())?;
      if let Some(socket) = to_pool {
        let key = PoolKeyRef::new(uri.scheme(), host.as_str(), uri.port_or_default());
        self.pool.return_connection(&key, socket);
      }

      if config.http_status_as_error() && (400..600).contains(&response.status_code()) {
        return Err(Error::HttpStatus(
          response.status_code(),
          alloc::boxed::Box::new(response),
        ));
      }

      let Some((next_url, next_method)) = follow_redirect(
        config,
        &mut visited_urls,
        &mut redirect_count,
        &response,
        &uri,
        current_url.as_ref(),
        &current_method,
        &mut current_body,
      )?
      else {
        return Ok(response);
      };

      sanitize_redirect_headers(current_headers.to_mut(), current_body.is_none());
      current_url = Cow::Owned(next_url);
      current_method = next_method;
    }
  }
}

const fn pooling_enabled(config: &Config) -> bool {
  config.max_idle_per_host() > 0
}

pub(crate) fn raw_to_response(
  raw: RawResponse,
  method: &Method,
  max_body: usize,
) -> Result<Response, Error> {
  let RawResponse {
    status_code,
    reason,
    mut headers,
    version,
    body_bytes,
    body_tail,
    decoded_chunked_trailers,
  } = raw;

  if method == &Method::Head {
    return Ok(Response::from_parts(
      status_code,
      reason,
      headers,
      bytes::Bytes::new(),
      Headers::new(),
    ));
  }
  // Identity-coded body already in the header arena (size checked on read).
  if let Some(tail) = body_tail {
    return Ok(Response::from_tail(status_code, reason, headers, tail, Headers::new()));
  }

  let (response_body, trailers) = if let Some(trailers) = decoded_chunked_trailers {
    // Transport already decoded chunked framing on the wire — skip second pass.
    Response::finish_decoded_body(body_bytes, &mut headers, trailers, max_body).map_err(Error::from)?
  } else {
    Response::parse_body_from_owned(body_bytes, &mut headers, status_code, version, max_body).map_err(Error::from)?
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

/// On every redirect hop, strip hop-by-hop headers plus Authorization and Cookie
/// (`RedirectAuthHeaders::Never`). Content-Length and Host always go too (rebuilt
/// for the next hop); with `drop_body`, strip Content-Type because the body is empty.
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
  headers.remove(Headers::COOKIE);
  headers.remove(Headers::CONTENT_LENGTH);
  headers.remove(Headers::HOST);
  if drop_body {
    headers.remove(Headers::CONTENT_TYPE);
  }
}

/// True for followable redirects: 301/302/303/307/308.
const fn is_followable_redirect(status: u16) -> bool {
  matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// `Ok(None)` = return this response. `Ok(Some(...))` = follow redirect.
///
/// When following, updates `current_body` in place (clear on method change to GET;
/// otherwise the same `Vec` allocation is kept for the next hop — no re-copy).
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
  current_method: &Method,
  current_body: &mut Option<Vec<u8>>,
) -> Result<Option<(String, Method)>, Error> {
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

  let next_method = redirect_method_and_body(response.status_code(), current_method, current_body)?;

  *redirect_count = redirect_count.saturating_add(1);

  Ok(Some((next_url, next_method)))
}

fn redirect_method_and_body(
  status: u16,
  method: &Method,
  body: &mut Option<Vec<u8>>,
) -> Result<Method, Error> {
  match status {
    307 | 308 => {
      // Retain method only when there is no request body to replay (ureq).
      if method.needs_request_body() || method == &Method::Delete {
        return Err(Error::RedirectFailed);
      }
      // Body buffer stays put for the next hop.
      Ok(method.clone())
    },
    // 301, 302, 303 (and only those are followable besides 307/308)
    _ => {
      if matches!(method, &Method::Get | &Method::Head) {
        Ok(method.clone())
      } else {
        *body = None;
        Ok(Method::Get)
      }
    },
  }
}

/// HTTP hop: pool/connect, send, read. Returns raw response plus an optional idle socket
/// to return after the caller successfully finishes body decode / policy checks.
///
/// On I/O failure with a reused pooled socket, drops it and retries once with a fresh connect.
fn execute<S, D>(
  pool: &Arc<ConnectionPool<S>>,
  dns: &D,
  config: &Config,
  uri: &Uri,
  host: &HostText<'_>,
  method: &Method,
  headers: &Headers,
  body: Option<&[u8]>,
  cookie: &str,
) -> Result<(RawResponse, Option<S>), Error>
where
  S: BlockingSocket + BlockingSocketFactory,
  D: DnsResolver,
{
  let port = uri.port_or_default();
  // Validate + size the head before any DNS / connect work (fail fast, no I/O).
  let head = build_request(uri, method, host, port, headers, body, config, cookie)?;

  let (mut socket, reused) = get_or_create_socket(pool, config, uri, host, port)?;
  match try_one_hop(dns, config, uri, host, &head, body, method, &mut socket, reused) {
    Ok((raw, reusable)) => Ok((raw, (pooling_enabled(config) && reusable).then_some(socket))),
    Err(e) if reused && matches!(e, Error::Socket(_)) => {
      drop(socket);
      let mut fresh = S::new().map_err(Error::Socket)?;
      let (raw, reusable) = try_one_hop(dns, config, uri, host, &head, body, method, &mut fresh, false)?;
      Ok((raw, (pooling_enabled(config) && reusable).then_some(fresh)))
    },
    Err(e) => Err(e),
  }
}

fn try_one_hop<S, D>(
  dns: &D,
  config: &Config,
  uri: &Uri,
  host: &HostText<'_>,
  head: &RequestHead<'_>,
  body: Option<&[u8]>,
  method: &Method,
  socket: &mut S,
  reused: bool,
) -> Result<(RawResponse, bool), Error>
where
  S: BlockingSocket + BlockingSocketFactory,
  D: DnsResolver,
{
  let mut conn = crate::transport::connection::connect_host(socket, dns, uri, host, config, reused)?;
  conn.send_head(head, body.unwrap_or(&[]))?;
  let raw = conn.read_raw_response(method != &Method::Head)?;
  Ok((raw, conn.is_reusable()))
}

#[inline]
const fn host_omits_port(
  uri: &Uri<'_>,
  port: u16,
) -> bool {
  (uri.scheme().eq_ignore_ascii_case("http") && port == 80)
    || (uri.scheme().eq_ignore_ascii_case("https") && port == 443)
}

fn get_or_create_socket<S>(
  pool: &Arc<ConnectionPool<S>>,
  config: &Config,
  uri: &Uri,
  host: &HostText<'_>,
  port: u16,
) -> Result<(S, bool), Error>
where
  S: BlockingSocket + BlockingSocketFactory,
{
  if pooling_enabled(config)
    && let Some(s) = pool.get(&PoolKeyRef::new(uri.scheme(), host.as_str(), port))
  {
    return Ok((s, true));
  }
  S::new().map(|s| (s, false)).map_err(Error::Socket)
}

/// Default `Accept-Encoding` when compression features are on (no per-request join).
#[cfg(all(feature = "gzip", feature = "zstd"))]
const DEFAULT_ACCEPT_ENCODING: &str = "gzip, deflate, zstd";
#[cfg(all(feature = "gzip", not(feature = "zstd")))]
const DEFAULT_ACCEPT_ENCODING: &str = "gzip, deflate";
#[cfg(all(feature = "zstd", not(feature = "gzip")))]
const DEFAULT_ACCEPT_ENCODING: &str = "zstd";
#[cfg(not(any(feature = "gzip", feature = "zstd")))]
const DEFAULT_ACCEPT_ENCODING: &str = "";

/// Build the wire request head (HTTP/1.1 + Host + origin-form target).
///
/// Never mutates `headers`: Host, `Connection: close` (pooling off), the
/// default `User-Agent` / `Accept` / `Accept-Encoding`, and jar `cookie` are
/// layered on while writing. The head is written later straight into the
/// connection's scratch buffer; the body stays a separate borrowed slice.
pub fn build_request<'a>(
  uri: &'a Uri,
  method: &'a Method,
  host: &'a HostText<'_>,
  port: u16,
  headers: &'a Headers,
  body: Option<&[u8]>,
  config: &'a Config,
  cookie: &'a str,
) -> Result<RequestHead<'a>, Error> {
  // Userinfo rejected at Uri::parse for HTTP client use.
  let opts = HeadOptions {
    // Always rebuilt for the current hop (a caller Host may be stale after redirects).
    host: host.as_str(),
    host_port: (!host_omits_port(uri, port)).then_some(port),
    // RFC 9112 §9.3: client that will not reuse MUST send Connection: close
    connection_close: !pooling_enabled(config),
    user_agent: config.user_agent(),
    accept: config.accept(),
    accept_encoding: DEFAULT_ACCEPT_ENCODING,
    cookie,
  };
  RequestHead::new(method.as_str(), uri.path(), uri.query(), headers, body, opts).map_err(Error::Parse)
}
