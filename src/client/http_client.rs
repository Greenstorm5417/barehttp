use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::parser::serialize_request;
use crate::parser::uri::{Host, Uri};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::BlockingSocket;
use crate::transport::{ConnectionPool, PoolKey, RawResponse};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[cfg(feature = "cookie-jar")]
use crate::cookie_jar::CookieStore;

/// HTTP client. `S` = socket (`BlockingSocket`), `D` = DNS (`DnsResolver`).
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

impl HttpClient<crate::socket::OsBlockingSocket, crate::dns::resolver::OsDnsResolver> {
  /// OS socket + DNS, default config.
  #[must_use]
  pub fn new() -> Self {
    Self::with_adapters(crate::dns::resolver::OsDnsResolver, Config::default())
  }

  /// OS socket + DNS, custom config.
  #[must_use]
  pub fn with_config(config: Config) -> Self {
    Self::with_adapters(crate::dns::resolver::OsDnsResolver, config)
  }
}

impl Default for HttpClient<crate::socket::OsBlockingSocket, crate::dns::resolver::OsDnsResolver> {
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> HttpClient<S, D>
where
  S: BlockingSocket,
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
      pool: Arc::new(ConnectionPool::new(config.max_idle_per_host)),
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
  ) -> ClientRequestBuilder<'_, S, D> {
    ClientRequestBuilder::new(self, method, url)
  }

  /// GET (no body).
  pub fn get(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Get, url)
  }

  /// POST (body required).
  pub fn post(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Post, url)
  }

  /// PUT (body required).
  pub fn put(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Put, url)
  }

  /// DELETE (no request body).
  pub fn delete(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Delete, url)
  }

  /// HEAD (no body).
  pub fn head(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Head, url)
  }

  /// PATCH (body required).
  pub fn patch(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D> {
    self.method(Method::Patch, url)
  }

  /// Shared cookie store (`cookie-jar` feature).
  #[cfg(feature = "cookie-jar")]
  #[must_use]
  pub const fn cookie_store(&self) -> &Arc<CookieStore> {
    &self.cookie_store
  }

  /// Redirect loop; each hop performs connect / write / read.
  ///
  /// # Errors
  /// URL parse, DNS, connect, HTTP, or policy failure.
  pub fn request(
    &self,
    method: Method,
    url: &str,
    custom_headers: &Headers,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    let config = self.config.as_ref();
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
          .get_request_cookies(&current_url, is_secure);
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

      let response = raw_to_response(raw, current_method)?;

      if config.http_status_as_error && (400..600).contains(&response.status_code) {
        return Err(Error::HttpStatus(response.status_code));
      }

      let Some((next_url, next_method, next_body, cross_origin)) = follow_redirect(
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

      sanitize_redirect_headers(&mut current_headers, cross_origin, next_body.is_none());
      current_url = next_url;
      current_method = next_method;
      current_body = next_body;
    }
  }
}

const fn pooling_enabled(config: &Config) -> bool {
  config.max_idle_per_host > 0
}

fn raw_to_response(
  raw: RawResponse,
  method: Method,
) -> Result<Response, Error> {
  let (response_body, trailers) = if method == Method::Head {
    (Vec::new(), Vec::new())
  } else {
    Response::parse_body_from_bytes(&raw.body_bytes, &raw.headers, raw.status_code, raw.version)
      .map_err(Error::Parse)?
  };

  Ok(Response {
    status_code: raw.status_code,
    reason: raw.reason,
    headers: raw.headers,
    body: response_body,
    trailers,
  })
}

/// Enforce TLS honesty and `https_only`.
pub const fn validate_protocol(
  config: &Config,
  uri: &Uri,
) -> Result<(), Error> {
  if uri.scheme().eq_ignore_ascii_case("https") && !config.assume_tls_socket {
    return Err(Error::HttpsRequired);
  }
  if config.https_only && !uri.scheme().eq_ignore_ascii_case("https") {
    return Err(Error::HttpsRequired);
  }
  Ok(())
}

/// Strip hop-by-hop headers; on cross-origin also strip Authorization and Cookie.
/// When `drop_body` is true (redirect became GET / body cleared), also strip
/// Content-Length and Content-Type so they cannot disagree with an empty body.
pub fn sanitize_redirect_headers(
  headers: &mut Headers,
  cross_origin: bool,
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
  if drop_body {
    headers.remove("Content-Length");
    headers.remove("Content-Type");
  }
  if cross_origin {
    headers.remove("Authorization");
    headers.remove("Cookie");
  }
}

fn host_eq(
  a: &Host<'_>,
  b: &Host<'_>,
) -> bool {
  match (a, b) {
    (Host::RegName(x), Host::RegName(y)) => x.eq_ignore_ascii_case(y),
    (Host::IpAddr(x), Host::IpAddr(y)) => x == y,
    _ => false,
  }
}

fn is_cross_origin(
  current: &Uri<'_>,
  next: &Uri<'_>,
) -> bool {
  if !current.scheme().eq_ignore_ascii_case(next.scheme()) {
    return true;
  }
  match (current.authority(), next.authority()) {
    (Some(a), Some(b)) => !host_eq(a.host(), b.host()) || current.port_or_default() != next.port_or_default(),
    _ => true,
  }
}

/// `Ok(None)` = return this response. `Ok(Some(...))` = follow redirect.
pub fn follow_redirect(
  config: &Config,
  visited_urls: &mut Vec<String>,
  redirect_count: &mut u32,
  response: &Response,
  current_uri: &Uri,
  current_url: &str,
  current_method: Method,
  current_body: Option<Vec<u8>>,
) -> Result<Option<(String, Method, Option<Vec<u8>>, bool)>, Error> {
  if !config.follow_redirects || !(300..400).contains(&response.status_code) {
    return Ok(None);
  }

  if *redirect_count >= config.max_redirects {
    return Err(Error::TooManyRedirects);
  }

  let location = response
    .get_header("location")
    .ok_or(Error::MissingRedirectLocation)?;

  let next_url = current_uri
    .resolve_relative(location)
    .map_err(Error::Parse)?;

  if visited_urls.iter().any(|u| u.as_str() == next_url.as_str()) {
    return Err(Error::RedirectLoop);
  }

  visited_urls.push(String::from(current_url));

  let (next_method, next_body) = if response.status_code == 303
    || ((response.status_code == 301 || response.status_code == 302) && current_method == Method::Post)
  {
    (Method::Get, None)
  } else {
    (current_method, current_body)
  };

  let next_uri_parsed = Uri::parse(&next_url).map_err(Error::Parse)?;
  let cross_origin = is_cross_origin(current_uri, &next_uri_parsed);

  *redirect_count = redirect_count.saturating_add(1);

  Ok(Some((next_url, next_method, next_body, cross_origin)))
}

/// One HTTP hop: pool/connect, send, read, maybe return socket to pool.
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
  S: BlockingSocket,
  D: DnsResolver,
{
  let host_str = host_from_uri(uri);
  let port = uri.port_or_default();
  let pool_key = PoolKey::new(uri.scheme().to_ascii_lowercase(), host_str.clone(), port);

  let (mut socket, reused) = get_or_create_socket(pool, config, &pool_key)?;
  let mut conn = crate::transport::connection::connect(&mut socket, dns, uri, config, reused)?;

  let request_bytes = build_request(uri, method, &host_str, port, custom_headers, body, config)?;
  conn.send_request(&request_bytes)?;

  let raw = conn.read_raw_response(method != Method::Head)?;

  if pooling_enabled(config) && conn.is_reusable() {
    pool.return_connection(pool_key, socket);
  }

  Ok(raw)
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
  S: BlockingSocket,
{
  if pooling_enabled(config)
    && let Some(s) = pool.get(pool_key)
  {
    return Ok((s, true));
  }
  S::new().map(|s| (s, false)).map_err(Error::Socket)
}

fn build_request(
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

  if !headers.contains(Headers::HOST) {
    headers.insert(Headers::HOST, host_header.as_str());
  }

  if !pooling_enabled(config) {
    headers.insert(Headers::CONNECTION, "close");
  }

  if !config.user_agent.is_empty() && !headers.contains(Headers::USER_AGENT) {
    headers.insert(Headers::USER_AGENT, config.user_agent.as_str());
  }

  if !config.accept.is_empty() && !headers.contains(Headers::ACCEPT) {
    headers.insert(Headers::ACCEPT, config.accept.as_str());
  }

  #[cfg(any(feature = "gzip-decompression", feature = "zstd-decompression"))]
  if !headers.contains(Headers::ACCEPT_ENCODING) {
    #[allow(unused_mut)]
    let mut encodings: Vec<&str> = Vec::new();

    #[cfg(feature = "gzip-decompression")]
    {
      encodings.push("gzip");
      encodings.push("deflate");
    }

    #[cfg(feature = "zstd-decompression")]
    encodings.push("zstd");

    if !encodings.is_empty() {
      let accept_encoding = encodings.join(", ");
      headers.insert(Headers::ACCEPT_ENCODING, accept_encoding.as_str());
    }
  }

  serialize_request(method.as_str(), &uri.path_and_query(), &headers, body).map_err(Error::Parse)
}
