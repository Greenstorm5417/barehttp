use crate::body::Body;
use crate::config::{Config, HttpStatusHandling, ProtocolRestriction, RedirectPolicy};
use crate::error::Error;
use crate::method::Method;
use crate::parser::Response;
use crate::parser::uri::Uri;
use crate::transport::RawResponse;
use alloc::string::String;
use alloc::vec::Vec;

/// Policy decision after processing a response
#[derive(Debug)]
pub enum PolicyDecision {
  Return(Response),
  Redirect {
    next_uri: String,
    next_method: Method,
    next_body: Option<Vec<u8>>,
  },
}

/// Request policy handler for status codes and redirects
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

  /// Validate protocol restrictions (HTTPS-only enforcement)
  pub fn validate_protocol(
    &self,
    uri: &Uri,
  ) -> Result<(), Error> {
    if self.config.protocol_restriction == ProtocolRestriction::HttpsOnly && uri.scheme() != "https" {
      return Err(Error::HttpsRequired);
    }
    Ok(())
  }

  /// Process raw response and decide what to do next
  ///
  /// This method encapsulates all policy decisions:
  /// - HEAD method body dropping
  /// - Status code error handling
  /// - Redirect detection and loop prevention
  /// - Method transformation on redirects
  pub fn process_raw_response(
    &mut self,
    raw: RawResponse,
    current_uri: &Uri,
    current_url: &str,
    current_method: Method,
    current_body: Option<Vec<u8>>,
  ) -> Result<PolicyDecision, Error> {
    let is_head_request = current_method == Method::Head;

    let response_body = if is_head_request {
      Body::from_bytes(Vec::new())
    } else {
      Response::parse_body_from_bytes(&raw.body_bytes, &raw.headers, raw.status_code).map_err(Error::Parse)?
    };

    let response = Response {
      status_code: raw.status_code,
      reason: raw.reason,
      headers: raw.headers,
      body: response_body,
      trailers: Vec::new(), // No trailers in two-phase reading
    };

    if self.config.http_status_handling == HttpStatusHandling::AsError
      && (response.status_code >= 400 && response.status_code < 600)
    {
      return Err(Error::HttpStatus(response.status_code));
    }

    if self.config.redirect_policy == RedirectPolicy::NoFollow {
      return Ok(PolicyDecision::Return(response));
    }

    if response.status_code >= 300 && response.status_code < 400 {
      if self.redirect_count >= self.config.max_redirects {
        if self.config.redirect_policy == RedirectPolicy::Follow {
          return Err(Error::TooManyRedirects);
        }
        return Ok(PolicyDecision::Return(response));
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
        .any(|u: &String| u.as_str() == next_url.as_str())
      {
        return Err(Error::RedirectLoop);
      }

      self.visited_urls.push(String::from(current_url));

      let (next_method, next_body) = if response.status_code == 303
        || (response.status_code == 301 || response.status_code == 302) && current_method == Method::Post
      {
        (Method::Get, None)
      } else {
        (current_method, current_body)
      };

      self.redirect_count += 1;

      return Ok(PolicyDecision::Redirect {
        next_uri: next_url,
        next_method,
        next_body,
      });
    }

    Ok(PolicyDecision::Return(response))
  }
}
