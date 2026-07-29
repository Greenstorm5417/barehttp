use crate::body::Body;
use crate::config::{Config, HttpStatusHandling, ProtocolRestriction, RedirectPolicy};
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::Response;
use crate::parser::uri::{Authority, Host, Uri};
use crate::transport::RawResponse;
use alloc::string::String;
use alloc::vec::Vec;

/// Next step after a raw response: return it or follow a redirect.
#[derive(Debug)]
pub enum PolicyDecision {
  Return(Response),
  Redirect {
    next_uri: String,
    next_method: Method,
    next_body: Option<Vec<u8>>,
    /// True when next hop has a different host or scheme.
    cross_origin: bool,
  },
}

/// Strip hop-by-hop headers; on cross-origin also strip Authorization and Cookie.
/// When `drop_body` is true (redirect became GET / body cleared), also strip
/// Content-Length so it cannot disagree with an empty body.
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

fn effective_port(uri: &Uri<'_>) -> u16 {
  uri.authority().and_then(Authority::port).unwrap_or_else(|| {
    if uri.scheme().eq_ignore_ascii_case("https") {
      443
    } else {
      80
    }
  })
}

fn is_cross_origin(
  current: &Uri<'_>,
  next: &Uri<'_>,
) -> bool {
  if !current.scheme().eq_ignore_ascii_case(next.scheme()) {
    return true;
  }
  match (current.authority(), next.authority()) {
    (Some(a), Some(b)) => {
      !host_eq(a.host(), b.host()) || effective_port(current) != effective_port(next)
    },
    _ => true,
  }
}

/// Status-code / redirect handling for one request (including redirect hops).
pub struct RequestPolicy {
  config: Config,
  visited_urls: Vec<String>,
  redirect_count: u32,
}

impl RequestPolicy {
  pub fn new(config: &Config) -> Self {
    Self {
      config: config.clone(),
      visited_urls: Vec::new(),
      redirect_count: 0,
    }
  }

  /// Enforce TLS honesty and [`ProtocolRestriction`].
  pub fn validate_protocol(
    &self,
    uri: &Uri,
  ) -> Result<(), Error> {
    if uri.scheme().eq_ignore_ascii_case("https") && !self.config.assume_tls_socket {
      return Err(Error::HttpsRequired);
    }
    if self.config.protocol_restriction == ProtocolRestriction::HttpsOnly
      && !uri.scheme().eq_ignore_ascii_case("https")
    {
      return Err(Error::HttpsRequired);
    }
    Ok(())
  }

  /// Parse body, apply status/redirect policy, return next action.
  pub fn process_raw_response(
    &mut self,
    raw: RawResponse,
    current_uri: &Uri,
    current_url: &str,
    current_method: Method,
    current_body: Option<Vec<u8>>,
  ) -> Result<PolicyDecision, Error> {
    let (response_body, trailers) = if current_method == Method::Head {
      (Body::from_bytes(Vec::new()), Vec::new())
    } else {
      Response::parse_body_from_bytes(&raw.body_bytes, &raw.headers, raw.status_code, raw.version)
        .map_err(Error::Parse)?
    };

    let response = Response {
      status_code: raw.status_code,
      reason: raw.reason,
      headers: raw.headers,
      body: response_body,
      trailers,
    };

    if self.config.http_status_handling == HttpStatusHandling::AsError
      && (response.status_code >= 400 && response.status_code < 600)
    {
      return Err(Error::HttpStatus(response.status_code));
    }

    if self.config.redirect_policy == RedirectPolicy::NoFollow {
      return Ok(PolicyDecision::Return(response));
    }

    if !(300..400).contains(&response.status_code) {
      return Ok(PolicyDecision::Return(response));
    }

    if self.redirect_count >= self.config.max_redirects {
      return Err(Error::TooManyRedirects);
    }

    let location = response
      .get_header("location")
      .or_else(|| response.get_header("Location"))
      .ok_or(Error::MissingRedirectLocation)?;

    let next_url = current_uri
      .resolve_relative(location)
      .map_err(Error::Parse)?;

    if self
      .visited_urls
      .iter()
      .any(|u| u.as_str() == next_url.as_str())
    {
      return Err(Error::RedirectLoop);
    }

    self.visited_urls.push(String::from(current_url));

    let (next_method, next_body) = if response.status_code == 303
      || ((response.status_code == 301 || response.status_code == 302) && current_method == Method::Post)
    {
      (Method::Get, None)
    } else {
      (current_method, current_body)
    };

    let next_uri_parsed = Uri::parse(&next_url).map_err(Error::Parse)?;
    let cross_origin = is_cross_origin(current_uri, &next_uri_parsed);

    self.redirect_count += 1;

    Ok(PolicyDecision::Redirect {
      next_uri: next_url,
      next_method,
      next_body,
      cross_origin,
    })
  }
}
