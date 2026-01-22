use crate::body::Body;
use crate::config::{Config, HttpStatusHandling, ProtocolRestriction, RedirectPolicy};
use crate::error::Error;
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
    next_method: &'static str,
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
  pub fn validate_protocol(&self, uri: &Uri) -> Result<(), Error> {
    if self.config.protocol_restriction == ProtocolRestriction::HttpsOnly
      && uri.scheme() != "https"
    {
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
    current_method: &str,
    current_body: Option<Vec<u8>>,
  ) -> Result<PolicyDecision, Error> {
    let is_head_request = current_method == "HEAD";

    let response_body = if is_head_request {
      Body::from_bytes(Vec::new())
    } else {
      Response::parse_body_from_bytes(&raw.body_bytes, &raw.headers, raw.status_code)
        .map_err(Error::Parse)?
    };

    let response = Response {
      status_code: raw.status_code,
      reason: raw.reason,
      headers: raw.headers,
      body: response_body,
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
        || (response.status_code == 301 || response.status_code == 302)
          && current_method == "POST"
      {
        ("GET", None)
      } else {
        let method_static: &'static str = match current_method {
          "POST" => "POST",
          "PUT" => "PUT",
          "DELETE" => "DELETE",
          "HEAD" => "HEAD",
          "OPTIONS" => "OPTIONS",
          "PATCH" => "PATCH",
          "TRACE" => "TRACE",
          "CONNECT" => "CONNECT",
          _ => "GET",
        };
        (method_static, current_body)
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

#[cfg(test)]
#[allow(
  clippy::unwrap_used,
  clippy::panic,
  clippy::match_wildcard_for_single_variants
)]
mod tests {
  use super::*;
  use crate::headers::Headers;
  use alloc::vec;

  fn make_redirect_response(status: u16, location: &str) -> RawResponse {
    let mut headers = Headers::new();
    headers.insert("Location", location);
    RawResponse {
      status_code: status,
      reason: String::from("Redirect"),
      headers,
      body_bytes: Vec::new(),
    }
  }

  #[test]
  fn https_only_policy_rejects_http() {
    let policy = RequestPolicy::new(&Config {
      protocol_restriction: ProtocolRestriction::HttpsOnly,
      ..Default::default()
    });

    let uri = Uri::parse("http://example.com").unwrap();
    let result = policy.validate_protocol(&uri);

    assert!(matches!(result, Err(Error::HttpsRequired)));
  }

  #[test]
  fn https_only_policy_allows_https() {
    let policy = RequestPolicy::new(&Config {
      protocol_restriction: ProtocolRestriction::HttpsOnly,
      ..Default::default()
    });

    let uri = Uri::parse("https://example.com").unwrap();
    assert!(policy.validate_protocol(&uri).is_ok());
  }

  #[test]
  fn policy_drops_body_for_head_requests() {
    let mut policy = RequestPolicy::new(&Config::default());

    let mut headers = Headers::new();
    headers.insert("Content-Length", "10");

    let raw = RawResponse {
      status_code: 200,
      reason: String::from("OK"),
      headers,
      body_bytes: b"1234567890".to_vec(),
    };

    let decision = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://example.com").unwrap(),
        "http://example.com",
        "HEAD",
        None,
      )
      .unwrap();

    match decision {
      PolicyDecision::Return(resp) => {
        assert_eq!(resp.status_code, 200);
        assert!(
          resp.body.as_bytes().is_empty(),
          "HEAD response body should be empty"
        );
      }
      _ => panic!("Expected PolicyDecision::Return"),
    }
  }

  #[test]
  fn post_302_redirect_becomes_get() {
    let mut policy = RequestPolicy::new(&Config::default());

    let raw = make_redirect_response(302, "/next");

    let decision = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://a.com").unwrap(),
        "http://a.com",
        "POST",
        Some(vec![1, 2, 3]),
      )
      .unwrap();

    match decision {
      PolicyDecision::Redirect {
        next_method,
        next_body,
        ..
      } => {
        assert_eq!(next_method, "GET", "POST 302 should become GET");
        assert!(next_body.is_none(), "GET should not have body");
      }
      _ => panic!("Expected PolicyDecision::Redirect"),
    }
  }

  #[test]
  fn post_301_redirect_becomes_get() {
    let mut policy = RequestPolicy::new(&Config::default());

    let raw = make_redirect_response(301, "/next");

    let decision = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://a.com").unwrap(),
        "http://a.com",
        "POST",
        Some(vec![1, 2, 3]),
      )
      .unwrap();

    match decision {
      PolicyDecision::Redirect {
        next_method,
        next_body,
        ..
      } => {
        assert_eq!(next_method, "GET");
        assert!(next_body.is_none());
      }
      _ => panic!("Expected PolicyDecision::Redirect"),
    }
  }

  #[test]
  fn post_303_redirect_becomes_get() {
    let mut policy = RequestPolicy::new(&Config::default());

    let raw = make_redirect_response(303, "/next");

    let decision = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://a.com").unwrap(),
        "http://a.com",
        "POST",
        Some(vec![1, 2, 3]),
      )
      .unwrap();

    match decision {
      PolicyDecision::Redirect {
        next_method,
        next_body,
        ..
      } => {
        assert_eq!(next_method, "GET");
        assert!(next_body.is_none());
      }
      _ => panic!("Expected PolicyDecision::Redirect"),
    }
  }

  #[test]
  fn get_redirect_stays_get() {
    let mut policy = RequestPolicy::new(&Config::default());

    let raw = make_redirect_response(302, "/next");

    let decision = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://a.com").unwrap(),
        "http://a.com",
        "GET",
        None,
      )
      .unwrap();

    match decision {
      PolicyDecision::Redirect { next_method, .. } => {
        assert_eq!(next_method, "GET");
      }
      _ => panic!("Expected PolicyDecision::Redirect"),
    }
  }

  #[test]
  fn redirect_loop_is_detected() {
    let mut policy = RequestPolicy::new(&Config::default());

    let raw = make_redirect_response(301, "http://a.com");
    let uri = Uri::parse("http://a.com").unwrap();

    policy
      .process_raw_response(raw.clone(), &uri, "http://a.com", "GET", None)
      .unwrap();

    let err = policy
      .process_raw_response(raw, &uri, "http://a.com", "GET", None)
      .unwrap_err();

    assert!(matches!(err, Error::RedirectLoop));
  }

  #[test]
  fn status_4xx_is_error_when_configured() {
    let mut policy = RequestPolicy::new(&Config {
      http_status_handling: HttpStatusHandling::AsError,
      ..Default::default()
    });

    let raw = RawResponse {
      status_code: 404,
      reason: String::from("Not Found"),
      headers: Headers::new(),
      body_bytes: Vec::new(),
    };

    let err = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://example.com").unwrap(),
        "http://example.com",
        "GET",
        None,
      )
      .unwrap_err();

    assert!(matches!(err, Error::HttpStatus(404)));
  }

  #[test]
  fn status_5xx_is_error_when_configured() {
    let mut policy = RequestPolicy::new(&Config {
      http_status_handling: HttpStatusHandling::AsError,
      ..Default::default()
    });

    let raw = RawResponse {
      status_code: 500,
      reason: String::from("Internal Server Error"),
      headers: Headers::new(),
      body_bytes: Vec::new(),
    };

    let err = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://example.com").unwrap(),
        "http://example.com",
        "GET",
        None,
      )
      .unwrap_err();

    assert!(matches!(err, Error::HttpStatus(500)));
  }

  #[test]
  fn status_4xx_is_ok_when_configured_as_response() {
    let mut policy = RequestPolicy::new(&Config {
      http_status_handling: HttpStatusHandling::AsResponse,
      ..Default::default()
    });

    let raw = RawResponse {
      status_code: 404,
      reason: String::from("Not Found"),
      headers: Headers::new(),
      body_bytes: Vec::new(),
    };

    let result = policy.process_raw_response(
      raw,
      &Uri::parse("http://example.com").unwrap(),
      "http://example.com",
      "GET",
      None,
    );

    assert!(result.is_ok());
    match result.unwrap() {
      PolicyDecision::Return(resp) => assert_eq!(resp.status_code, 404),
      _ => panic!("Expected PolicyDecision::Return"),
    }
  }

  #[test]
  fn too_many_redirects_is_error() {
    let mut policy = RequestPolicy::new(&Config {
      max_redirects: 2,
      ..Default::default()
    });

    let raw = make_redirect_response(301, "/next");

    policy
      .process_raw_response(
        raw.clone(),
        &Uri::parse("http://a.com").unwrap(),
        "http://a.com",
        "GET",
        None,
      )
      .unwrap();

    policy
      .process_raw_response(
        raw.clone(),
        &Uri::parse("http://b.com").unwrap(),
        "http://b.com",
        "GET",
        None,
      )
      .unwrap();

    let err = policy
      .process_raw_response(
        raw,
        &Uri::parse("http://c.com").unwrap(),
        "http://c.com",
        "GET",
        None,
      )
      .unwrap_err();

    assert!(matches!(err, Error::TooManyRedirects));
  }

  #[test]
  fn no_follow_policy_returns_redirect_response() {
    let mut policy = RequestPolicy::new(&Config {
      redirect_policy: RedirectPolicy::NoFollow,
      ..Default::default()
    });

    let raw = make_redirect_response(302, "/next");

    let result = policy.process_raw_response(
      raw,
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      "GET",
      None,
    );

    match result.unwrap() {
      PolicyDecision::Return(resp) => assert_eq!(resp.status_code, 302),
      PolicyDecision::Redirect { .. } => {
        panic!("Should not follow redirect with NoFollow policy")
      }
    }
  }
}
