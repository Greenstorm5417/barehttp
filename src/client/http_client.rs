use crate::client::policy::{PolicyDecision, RequestPolicy, sanitize_redirect_headers};
use crate::client::request_executor;
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::Response;
use crate::parser::uri::Uri;
use crate::request_builder::{ClientRequestBuilder, WithBody, WithoutBody};
use crate::socket::BlockingSocket;
use crate::transport::ConnectionPool;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[cfg(feature = "cookie-jar")]
use crate::cookie_jar::CookieStore;

/// HTTP client. `S` = socket (`BlockingSocket`), `D` = DNS (`DnsResolver`).
/// Clone shares the connection pool (and cookie store when enabled).
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

impl HttpClient<crate::socket::blocking::OsBlockingSocket, crate::dns::resolver::OsDnsResolver> {
  /// OS socket + DNS, default config.
  #[must_use]
  pub fn new() -> Self {
    Self::with_config(Config::default())
  }

  /// OS socket + DNS, custom config.
  #[must_use]
  pub fn with_config(config: Config) -> Self {
    Self {
      pool: Arc::new(ConnectionPool::new(config.max_idle_per_host, config.idle_timeout)),
      dns: Arc::new(crate::dns::resolver::OsDnsResolver::new()),
      config: Arc::new(config),
      #[cfg(feature = "cookie-jar")]
      cookie_store: Arc::new(CookieStore::new()),
    }
  }
}

impl Default for HttpClient<crate::socket::blocking::OsBlockingSocket, crate::dns::resolver::OsDnsResolver> {
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> HttpClient<S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  /// Custom DNS. Socket type `S` is not passed — name it at the type
  /// (`HttpClient::<OsBlockingSocket, _>::new_with_adapters(dns)`); sockets come from `S::new()`.
  #[must_use]
  pub fn new_with_adapters(dns: D) -> Self {
    Self::with_adapters_and_config(dns, Config::default())
  }

  /// Custom DNS + config. Same `S` note as [`Self::new_with_adapters`].
  #[must_use]
  pub fn with_adapters_and_config(
    dns: D,
    config: Config,
  ) -> Self {
    Self {
      pool: Arc::new(ConnectionPool::new(config.max_idle_per_host, config.idle_timeout)),
      dns: Arc::new(dns),
      config: Arc::new(config),
      #[cfg(feature = "cookie-jar")]
      cookie_store: Arc::new(CookieStore::new()),
    }
  }

  /// GET (no body).
  pub fn get(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithoutBody> {
    ClientRequestBuilder::<S, D, WithoutBody>::new(self.clone(), crate::method::Method::Get, url)
  }

  /// POST (body required).
  pub fn post(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithBody> {
    ClientRequestBuilder::<S, D, WithBody>::new(self.clone(), crate::method::Method::Post, url)
  }

  /// PUT (body required).
  pub fn put(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithBody> {
    ClientRequestBuilder::<S, D, WithBody>::new(self.clone(), crate::method::Method::Put, url)
  }

  /// DELETE (no request body).
  pub fn delete(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithoutBody> {
    ClientRequestBuilder::<S, D, WithoutBody>::new(self.clone(), crate::method::Method::Delete, url)
  }

  /// HEAD (no body).
  pub fn head(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithoutBody> {
    ClientRequestBuilder::<S, D, WithoutBody>::new(self.clone(), crate::method::Method::Head, url)
  }

  /// OPTIONS (no body).
  pub fn options(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithoutBody> {
    ClientRequestBuilder::<S, D, WithoutBody>::new(self.clone(), crate::method::Method::Options, url)
  }

  /// PATCH (body required).
  pub fn patch(
    &self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<S, D, WithBody> {
    ClientRequestBuilder::<S, D, WithBody>::new(self.clone(), crate::method::Method::Patch, url)
  }

  /// Shared cookie store (`cookie-jar` feature).
  #[cfg(feature = "cookie-jar")]
  #[must_use]
  pub const fn cookie_store(&self) -> &Arc<CookieStore> {
    &self.cookie_store
  }

  /// Redirect loop + policy; each hop goes through [`request_executor::execute`].
  ///
  /// # Errors
  /// URL parse, DNS, connect, HTTP, or policy failure.
  pub(crate) fn request(
    &self,
    method: crate::method::Method,
    url: &str,
    custom_headers: &crate::headers::Headers,
    body: Option<Vec<u8>>,
    request_config: Option<&Config>,
  ) -> Result<Response, Error> {
    let config = request_config.unwrap_or_else(|| self.config.as_ref());
    let mut current_url = String::from(url);
    let mut current_method = method;
    let mut current_body = body;
    let mut current_headers = custom_headers.clone();
    let mut policy = RequestPolicy::new(config);

    loop {
      let uri = Uri::parse(&current_url).map_err(Error::Parse)?;
      policy.validate_protocol(&uri)?;

      #[cfg(feature = "cookie-jar")]
      let mut headers_with_cookies = current_headers.clone();
      #[cfg(feature = "cookie-jar")]
      {
        let is_secure = current_url.starts_with("https://");
        let cookie_header = self
          .cookie_store
          .get_request_cookies(&current_url, is_secure);
        if !cookie_header.is_empty() {
          headers_with_cookies.insert(crate::headers::Headers::COOKIE, &cookie_header);
        }
      }

      #[cfg(feature = "cookie-jar")]
      let headers_to_use = &headers_with_cookies;
      #[cfg(not(feature = "cookie-jar"))]
      let headers_to_use = &current_headers;

      let raw = request_executor::execute(
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
          .get_all(crate::headers::Headers::SET_COOKIE)
          .into_iter()
          .map(alloc::string::ToString::to_string)
          .collect();

        if !set_cookie_headers.is_empty() {
          self
            .cookie_store
            .store_response_cookies(&current_url, &set_cookie_headers);
        }
      }

      match policy.process_raw_response(raw, &uri, &current_url, current_method, current_body)? {
        PolicyDecision::Return(response) => return Ok(response),
        PolicyDecision::Redirect {
          next_uri,
          next_method,
          next_body,
          cross_origin,
        } => {
          sanitize_redirect_headers(&mut current_headers, cross_origin, next_body.is_none());
          current_url = next_uri;
          current_method = next_method;
          current_body = next_body;
        },
      }
    }
  }
}
