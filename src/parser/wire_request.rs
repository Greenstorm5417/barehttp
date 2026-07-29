extern crate alloc;
use crate::body::Body;
use crate::error::ParseError;
use crate::headers::Headers;
use alloc::string::String;
use alloc::vec::Vec;

/// Wire-format HTTP/1.1 request serializer (crate-internal).
#[derive(Debug, Clone)]
pub struct WireRequest {
  method: String,
  path: String,
  headers: Headers,
  body: Option<Body>,
}

impl WireRequest {
  pub fn new(
    method: &str,
    path: &str,
  ) -> Self {
    Self {
      method: String::from(method),
      path: String::from(path),
      headers: Headers::new(),
      body: None,
    }
  }

  pub fn header(
    mut self,
    name: &str,
    value: &str,
  ) -> Self {
    self.headers.insert(name, value);
    self
  }

  pub fn body(
    mut self,
    body: Vec<u8>,
  ) -> Self {
    self.body = Some(Body::from_bytes(body));
    self
  }

  pub fn build(self) -> Result<Vec<u8>, ParseError> {
    // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
    if !self.headers.contains(Headers::HOST) {
      return Err(ParseError::MissingHostHeader);
    }

    // RFC 9112 Section 3.2: Server responds 400 if multiple Host headers present
    let host_headers = self.headers.get_all(Headers::HOST);
    if host_headers.len() > 1 {
      return Err(ParseError::MultipleHostHeaders);
    }

    // RFC 9112 Section 3.2: Validate Host header value format
    if let Some(host_value) = self.headers.get(Headers::HOST)
      && !Self::is_valid_host_value(host_value)
    {
      return Err(ParseError::InvalidHostHeaderValue);
    }

    // Validate all header names/values for RFC 9112 compliance (no injection)
    for (name, value) in &self.headers {
      if !is_header_name_token(name) {
        return Err(ParseError::InvalidHeaderName);
      }
      // No CTLs except HTAB; blocks CRLF / LF injection into the wire message
      if value.bytes().any(|b| matches!(b, 0..=8 | 0x0A..=0x1F | 0x7F)) {
        return Err(ParseError::InvalidHeaderValue);
      }

      // RFC 9112 Section 7.4: Client MUST NOT send "chunked" in TE
      if name.eq_ignore_ascii_case(Headers::TE) && value.to_lowercase().contains("chunked") {
        return Err(ParseError::ChunkedInTeHeader);
      }

      // RFC 9112 Section 7.4: Sender of TE MUST also send "TE" in Connection
      if name.eq_ignore_ascii_case(Headers::TE) {
        if let Some(conn_value) = self.headers.get(Headers::CONNECTION) {
          if !conn_value.to_lowercase().contains("te") {
            return Err(ParseError::TeHeaderMissingConnection);
          }
        } else {
          return Err(ParseError::TeHeaderMissingConnection);
        }
      }

      // RFC 9112 Section 6.1: MUST NOT apply chunked more than once
      if name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING) {
        let te_lower = value.to_lowercase();
        let chunked_count = te_lower.matches("chunked").count();
        if chunked_count > 1 {
          return Err(ParseError::ChunkedAppliedMultipleTimes);
        }
      }
    }

    // RFC 9112 Section 6.2: Sender MUST NOT send CL when TE present
    let has_te = self.headers.contains(Headers::TRANSFER_ENCODING);
    let has_cl = self.headers.contains(Headers::CONTENT_LENGTH);
    if has_te && has_cl {
      return Err(ParseError::ConflictingFraming);
    }

    let mut request = Vec::new();

    request.extend_from_slice(self.method.as_bytes());
    request.push(b' ');

    // RFC 9112 Section 3.2.1: If origin-form path is empty, send "/"
    let path = if self.path.is_empty() {
      "/"
    } else {
      &self.path
    };
    request.extend_from_slice(path.as_bytes());
    request.extend_from_slice(b" HTTP/1.1\r\n");

    for (name, value) in &self.headers {
      request.extend_from_slice(name.as_bytes());
      request.extend_from_slice(b": ");
      request.extend_from_slice(value.as_bytes());
      request.extend_from_slice(b"\r\n");
    }

    if let Some(body) = &self.body
      && !self.headers.contains(Headers::CONTENT_LENGTH)
    {
      use alloc::string::ToString;
      request.extend_from_slice(b"Content-Length: ");
      request.extend_from_slice(body.len().to_string().as_bytes());
      request.extend_from_slice(b"\r\n");
    }

    request.extend_from_slice(b"\r\n");

    if let Some(body) = &self.body {
      request.extend_from_slice(body.as_bytes());
    }

    Ok(request)
  }

  /// Validate Host header value format per RFC 9112 Section 3.2
  /// Host = uri-host [ ":" port ]
  /// uri-host = <host from URI syntax>
  fn is_valid_host_value(host: &str) -> bool {
    if host.is_empty() {
      // Empty Host is valid per RFC 9112 Section 3.2
      return true;
    }

    // Check for invalid characters
    if host.contains(char::is_whitespace) {
      return false;
    }

    // Handle IPv6 literals specially (they contain colons)
    if host.starts_with('[') {
      // IPv6 literal format: [ipv6]:port or [ipv6]
      if let Some(bracket_end) = host.find(']') {
        let Some(ipv6_part) = host.get(..=bracket_end) else {
          return false;
        };
        let Some(after_bracket) = host.get(bracket_end + 1..) else {
          return false;
        };

        if after_bracket.is_empty() {
          // Just [ipv6]
          return Self::is_valid_hostname(ipv6_part);
        } else if let Some(port_str) = after_bracket.strip_prefix(':') {
          // [ipv6]:port
          if port_str.is_empty() || !port_str.chars().all(|c| c.is_ascii_digit()) {
            return false;
          }
          if let Ok(port) = port_str.parse::<u16>() {
            if port == 0 {
              return false;
            }
          } else {
            return false;
          }
          return Self::is_valid_hostname(ipv6_part);
        }
        return false;
      }
      return false;
    }

    // Split host and port if present (for non-IPv6)
    let parts: Vec<&str> = host.rsplitn(2, ':').collect();

    if parts.len() == 2 {
      // Has port - validate it
      let Some(port_str) = parts.first() else {
        return false;
      };
      if port_str.is_empty() || !port_str.chars().all(|c| c.is_ascii_digit()) {
        return false;
      }
      // Check port range
      if let Ok(port) = port_str.parse::<u16>() {
        if port == 0 {
          return false;
        }
      } else {
        return false;
      }

      // Validate hostname part
      let Some(hostname) = parts.get(1) else {
        return false;
      };
      Self::is_valid_hostname(hostname)
    } else {
      // No port, just validate hostname
      Self::is_valid_hostname(host)
    }
  }

  /// Validate hostname format (simplified check for common cases)
  fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() {
      return false;
    }

    // Check for IPv6 literal
    if hostname.starts_with('[') && hostname.ends_with(']') {
      // Basic IPv6 validation - just check it has hex digits and colons
      let Some(inner) = hostname.get(1..hostname.len().saturating_sub(1)) else {
        return false;
      };
      return !inner.is_empty() && inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':');
    }

    // Regular hostname or IPv4
    // Allow alphanumeric, dots, hyphens
    hostname
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
  }
}

/// RFC 9110 token: header names used on the wire.
fn is_header_name_token(name: &str) -> bool {
  !name.is_empty()
    && name.bytes().all(|b| {
      matches!(
        b,
        b'!'
          | b'#'
          | b'$'
          | b'%'
          | b'&'
          | b'\''
          | b'*'
          | b'+'
          | b'-'
          | b'.'
          | b'^'
          | b'_'
          | b'`'
          | b'|'
          | b'~'
          | b'0'..=b'9'
          | b'A'..=b'Z'
          | b'a'..=b'z'
      )
    })
}
