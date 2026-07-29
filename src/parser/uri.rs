use core::net::{Ipv4Addr, Ipv6Addr};
use core::str::FromStr;

use crate::error::ParseError;
use crate::util::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri<'a> {
  scheme: &'a str,
  authority: Option<Authority<'a>>,
  path: &'a str,
  query: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authority<'a> {
  host: Host<'a>,
  port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host<'a> {
  IpAddr(IpAddr),
  RegName(&'a str),
}

impl<'a> Uri<'a> {
  pub fn parse(input: &'a str) -> Result<Self, ParseError> {
    Parser::new(input).parse_uri()
  }

  pub const fn scheme(&self) -> &'a str {
    self.scheme
  }

  pub const fn authority(&self) -> Option<&Authority<'a>> {
    self.authority.as_ref()
  }

  /// Explicit port, or 443 for https / 80 otherwise.
  #[must_use]
  pub fn port_or_default(&self) -> u16 {
    self
      .authority()
      .and_then(Authority::port)
      .unwrap_or_else(|| {
        if self.scheme.eq_ignore_ascii_case("https") {
          443
        } else {
          80
        }
      })
  }

  pub const fn path(&self) -> &'a str {
    self.path
  }

  #[allow(dead_code)] // exercised by parser tests
  pub const fn query(&self) -> Option<&'a str> {
    self.query
  }

  pub fn path_and_query(&self) -> alloc::string::String {
    let path = if self.path().is_empty() {
      "/"
    } else {
      self.path()
    };
    self.query.map_or_else(
      || alloc::string::String::from(path),
      |query| alloc::format!("{path}?{query}"),
    )
  }

  /// Resolve `location` against this URI as base (RFC 3986 §5.2).
  ///
  /// # Errors
  /// [`ParseError::InvalidUri`] if `location` is neither a usable relative nor absolute URL.
  pub fn resolve_relative(
    &self,
    location: &str,
  ) -> Result<alloc::string::String, ParseError> {
    // Absolute http(s) URI — normalize scheme to lowercase so == "https" checks work
    if let Some((scheme, rest)) = location.split_once(':')
      && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
    {
      let scheme_lower = scheme.to_ascii_lowercase();
      return Ok(alloc::format!("{scheme_lower}:{rest}"));
    }

    // Network-path reference: //authority/...
    if let Some(rest) = location.strip_prefix("//") {
      return Ok(alloc::format!("{}://{rest}", self.scheme));
    }

    // Query-only reference: keep base path, replace query
    if location.starts_with('?') {
      let path = if self.path.is_empty() {
        "/"
      } else {
        self.path
      };
      return self.recompose_with_path(&alloc::format!("{path}{location}"));
    }

    let path = if location.starts_with('/') {
      alloc::string::String::from(location)
    } else {
      // RFC 3986 §5.2.3 merge: replace final base segment with relative reference
      if self.path.is_empty() {
        alloc::format!("/{location}")
      } else {
        let dir_end = self.path.rfind('/').map_or(0, |i| i.saturating_add(1));
        let prefix = self.path.get(..dir_end).unwrap_or("");
        alloc::format!("{prefix}{location}")
      }
    };

    self.recompose_with_path(&path)
  }

  fn recompose_with_path(
    &self,
    path: &str,
  ) -> Result<alloc::string::String, ParseError> {
    let authority = self.authority.as_ref().ok_or(ParseError::InvalidUri)?;
    let port = self.port_or_default();

    let host_str = match &authority.host {
      Host::RegName(name) => alloc::string::String::from(*name),
      Host::IpAddr(addr) => crate::util::format_ip_for_host(*addr),
    };

    if (self.scheme.eq_ignore_ascii_case("http") && port == 80)
      || (self.scheme.eq_ignore_ascii_case("https") && port == 443)
    {
      Ok(alloc::format!(
        "{scheme}://{host}{path}",
        scheme = self.scheme,
        host = host_str
      ))
    } else {
      Ok(alloc::format!(
        "{scheme}://{host}:{port}{path}",
        scheme = self.scheme,
        host = host_str
      ))
    }
  }
}

impl<'a> Authority<'a> {
  pub const fn host(&self) -> &Host<'a> {
    &self.host
  }

  pub const fn port(&self) -> Option<u16> {
    self.port
  }
}

struct Parser<'a> {
  input: &'a str,
  pos: usize,
}

impl<'a> Parser<'a> {
  const fn new(input: &'a str) -> Self {
    Self { input, pos: 0 }
  }

  fn peek(&self) -> Option<u8> {
    self.input.as_bytes().get(self.pos).copied()
  }

  fn peek_at(
    &self,
    offset: usize,
  ) -> Option<u8> {
    let idx = self.pos.saturating_add(offset);
    self.input.as_bytes().get(idx).copied()
  }

  const fn advance(&mut self) {
    if self.pos < self.input.len() {
      self.pos = self.pos.saturating_add(1);
    }
  }

  fn advance_by(
    &mut self,
    n: usize,
  ) {
    self.pos = self.pos.saturating_add(n).min(self.input.len());
  }

  fn slice_from(
    &self,
    start: usize,
  ) -> &'a str {
    &self.input[start..self.pos]
  }

  fn parse_uri(mut self) -> Result<Uri<'a>, ParseError> {
    let scheme = self.parse_scheme()?;

    if self.peek() != Some(b':') || self.peek_at(1) != Some(b'/') || self.peek_at(2) != Some(b'/') {
      return Err(ParseError::InvalidUri);
    }
    self.advance_by(3);

    let authority = self.parse_authority()?;
    let path = self.parse_path_abempty();

    let query = if self.peek() == Some(b'?') {
      self.advance();
      Some(self.parse_query()?)
    } else {
      None
    };

    // Fragments not supported for HTTP client use
    if self.pos != self.input.len() {
      return Err(ParseError::InvalidUri);
    }

    Ok(Uri {
      scheme,
      authority: Some(authority),
      path,
      query,
    })
  }

  fn parse_scheme(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;
    let rest = &self.input[start..];

    let scheme = if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("https") {
      self.advance_by(5);
      self.slice_from(start)
    } else if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("http") {
      // Avoid matching "http" as prefix of "https" — already handled above
      self.advance_by(4);
      self.slice_from(start)
    } else {
      return Err(ParseError::InvalidUri);
    };

    Ok(scheme)
  }

  fn parse_authority(&mut self) -> Result<Authority<'a>, ParseError> {
    // Reject userinfo — credentials in URLs are not supported
    if self.find_char_in_authority(b'@') {
      return Err(ParseError::InvalidUri);
    }

    let host = self.parse_host()?;

    let port = if self.peek() == Some(b':') {
      self.advance();
      Some(self.parse_port()?)
    } else {
      None
    };

    Ok(Authority { host, port })
  }

  fn find_char_in_authority(
    &self,
    target: u8,
  ) -> bool {
    let mut pos = self.pos;
    let bytes = self.input.as_bytes();
    while let Some(&ch) = bytes.get(pos) {
      match ch {
        b'/' | b'?' | b'#' => return false,
        _ if ch == target => return true,
        _ => pos = pos.saturating_add(1),
      }
    }
    false
  }

  fn parse_host(&mut self) -> Result<Host<'a>, ParseError> {
    if self.peek() == Some(b'[') {
      return self.parse_ip_literal();
    }

    let start = self.pos;
    while let Some(ch) = self.peek() {
      match ch {
        b':' | b'/' | b'?' | b'#' => break,
        _ if is_reg_name_char(ch) => self.advance(),
        _ => break,
      }
    }

    let host_str = self.slice_from(start);
    if host_str.is_empty() {
      return Err(ParseError::InvalidUri);
    }

    if let Ok(v4) = Ipv4Addr::from_str(host_str) {
      return Ok(Host::IpAddr(IpAddr::V4(v4)));
    }

    Ok(Host::RegName(host_str))
  }

  fn parse_ip_literal(&mut self) -> Result<Host<'a>, ParseError> {
    if self.peek() != Some(b'[') {
      return Err(ParseError::InvalidUri);
    }
    self.advance();

    let start = self.pos;
    while let Some(ch) = self.peek() {
      if ch == b']' {
        break;
      }
      self.advance();
    }

    if self.peek() != Some(b']') {
      return Err(ParseError::InvalidUri);
    }

    let addr_str = self.slice_from(start);
    self.advance();

    let v6 = Ipv6Addr::from_str(addr_str).map_err(|_| ParseError::InvalidUri)?;
    Ok(Host::IpAddr(IpAddr::V6(v6)))
  }

  fn parse_port(&mut self) -> Result<u16, ParseError> {
    let start = self.pos;

    while let Some(b'0'..=b'9') = self.peek() {
      self.advance();
    }

    if start == self.pos {
      return Ok(0);
    }

    let port_str = self.slice_from(start);
    port_str.parse::<u16>().map_err(|_| ParseError::InvalidUri)
  }

  fn parse_path_abempty(&mut self) -> &'a str {
    let start = self.pos;

    while self.peek() == Some(b'/') {
      self.advance();
      while let Some(ch) = self.peek() {
        if is_pchar(ch) {
          self.advance();
        } else {
          break;
        }
      }
    }

    self.slice_from(start)
  }

  fn parse_query(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;

    while let Some(ch) = self.peek() {
      match ch {
        b'#' => break,
        _ if is_pchar(ch) || ch == b'/' || ch == b'?' => {
          self.advance();
        },
        _ => return Err(ParseError::InvalidUri),
      }
    }

    Ok(self.slice_from(start))
  }
}

const fn is_unreserved(ch: u8) -> bool {
  ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'.' | b'_' | b'~')
}

const fn is_sub_delim(ch: u8) -> bool {
  matches!(
    ch,
    b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
  )
}

const fn is_pchar(ch: u8) -> bool {
  is_unreserved(ch) || is_sub_delim(ch) || ch == b':' || ch == b'@' || ch == b'%'
}

const fn is_reg_name_char(ch: u8) -> bool {
  is_unreserved(ch) || is_sub_delim(ch) || ch == b'%'
}
