use core::net::{Ipv4Addr, Ipv6Addr};
use core::str::FromStr;

use crate::error::ParseError;
use crate::util::IpAddr;

/// Parsed HTTP URI (absolute-form or origin-form used by the client).
///
/// # Examples
///
/// ```
/// use barehttp::Uri;
///
/// let uri = Uri::parse("http://example.com/path?q=1")?;
/// assert_eq!(uri.scheme(), "http");
/// assert_eq!(uri.path(), "/path");
/// assert_eq!(uri.query(), Some("q=1"));
/// assert_eq!(uri.port_or_default(), 80);
/// # Ok::<(), barehttp::ParseError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uri<'a> {
  scheme: &'a str,
  authority: Option<Authority<'a>>,
  path: &'a str,
  query: Option<&'a str>,
}

/// Authority component (`host[:port]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Authority<'a> {
  host: Host<'a>,
  port: Option<u16>,
}

/// Host as an IP literal or registered name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Host<'a> {
  /// Literal IPv4 / IPv6 address.
  IpAddr(IpAddr),
  /// DNS / reg-name.
  RegName(&'a str),
}

impl<'a> Uri<'a> {
  /// Parse an absolute URI (`http://…` / `https://…`).
  ///
  /// # Errors
  /// [`ParseError::InvalidUri`] on a malformed absolute URI.
  pub fn parse(input: &'a str) -> Result<Self, ParseError> {
    Parser::new(input).parse_uri()
  }

  /// URI scheme (`http` / `https`).
  #[must_use]
  pub const fn scheme(&self) -> &'a str {
    self.scheme
  }

  /// Authority (`host[:port]`), if present.
  #[must_use]
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

  /// Path component (may be empty before normalization).
  #[must_use]
  pub const fn path(&self) -> &'a str {
    self.path
  }

  /// Query string without the leading `?`.
  #[must_use]
  pub const fn query(&self) -> Option<&'a str> {
    self.query
  }

  /// `path` + optional `?query` for the request-target.
  #[must_use]
  pub fn to_path_and_query(&self) -> alloc::string::String {
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
  /// [`ParseError::InvalidUri`] if `location` is not a valid relative reference or absolute URL.
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
  /// Host (IP or reg-name).
  #[must_use]
  pub const fn host(&self) -> &Host<'a> {
    &self.host
  }

  /// Explicit port, if any.
  #[must_use]
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

    let scheme = if rest
      .get(..5)
      .is_some_and(|s| s.eq_ignore_ascii_case("https"))
    {
      self.advance_by(5);
      self.slice_from(start)
    } else if rest
      .get(..4)
      .is_some_and(|s| s.eq_ignore_ascii_case("http"))
    {
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

    // Skip `Ipv4Addr::from_str` for hostnames (no digit/dot-only shape).
    if looks_like_ipv4(host_str)
      && let Ok(v4) = Ipv4Addr::from_str(host_str)
    {
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

    let v6 = parse_ipv6(addr_str)?;
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

/// True when `host` is only digits/dots and contains a `.` (cheap IPv4 gate).
#[inline]
fn looks_like_ipv4(host: &str) -> bool {
  let bytes = host.as_bytes();
  if bytes.is_empty() {
    return false;
  }
  let mut has_dot = false;
  for &b in bytes {
    match b {
      b'0'..=b'9' => {},
      b'.' => has_dot = true,
      _ => return false,
    }
  }
  has_dot
}

/// Parse bracketed-host IPv6 text matching `Ipv6Addr::from_str` acceptance
/// (compressed `::`, IPv4-mapped/embedded dotted-quad, no zone id).
fn parse_ipv6(s: &str) -> Result<Ipv6Addr, ParseError> {
  let bytes = s.as_bytes();
  let mut pos = 0usize;

  let mut head = [0u16; 8];
  let (head_size, head_ipv4) = read_ipv6_groups(bytes, &mut pos, &mut head)?;

  if head_size == 8 {
    if pos != bytes.len() {
      return Err(ParseError::InvalidUri);
    }
    return Ok(Ipv6Addr::from(head));
  }

  // Embedded IPv4 is only valid as the final component (never before `::`).
  if head_ipv4 {
    return Err(ParseError::InvalidUri);
  }

  // `::` marks one or more zero hextets.
  if bytes.get(pos).copied() != Some(b':') || bytes.get(pos.saturating_add(1)).copied() != Some(b':')
  {
    return Err(ParseError::InvalidUri);
  }
  pos = pos.saturating_add(2);

  // At least one hextet must be compressed, so the tail fits in 7 slots.
  let mut tail = [0u16; 7];
  let limit = 8usize.saturating_sub(head_size.saturating_add(1));
  let (tail_size, _) = read_ipv6_groups(bytes, &mut pos, &mut tail[..limit])?;

  if pos != bytes.len() {
    return Err(ParseError::InvalidUri);
  }

  let fill_at = 8usize.saturating_sub(tail_size);
  head[fill_at..8].copy_from_slice(&tail[..tail_size]);
  Ok(Ipv6Addr::from(head))
}

/// Read colon-separated hextets (and optional trailing embedded IPv4) into `groups`.
///
/// Returns `(groups_filled, saw_embedded_ipv4)`.
fn read_ipv6_groups(
  bytes: &[u8],
  pos: &mut usize,
  groups: &mut [u16],
) -> Result<(usize, bool), ParseError> {
  let limit = groups.len();
  let mut i = 0usize;

  while i < limit {
    // Trailing dotted-quad needs two hextet slots.
    if i < limit.saturating_sub(1) {
      let save = *pos;
      if i > 0 {
        if bytes.get(*pos).copied() == Some(b':') {
          *pos = pos.saturating_add(1);
        } else {
          // No separator: end of this side (e.g. before `::`).
          *pos = save;
          return Ok((i, false));
        }
      }
      match read_embedded_ipv4(bytes, pos) {
        Some(v4) => {
          let oct = v4.octets();
          groups[i] = u16::from_be_bytes([oct[0], oct[1]]);
          groups[i.saturating_add(1)] = u16::from_be_bytes([oct[2], oct[3]]);
          return Ok((i.saturating_add(2), true));
        },
        None => {
          *pos = save;
        },
      }
    }

    let save = *pos;
    if i > 0 {
      if bytes.get(*pos).copied() == Some(b':') {
        *pos = pos.saturating_add(1);
      } else {
        *pos = save;
        return Ok((i, false));
      }
    }

    match read_hextet(bytes, pos) {
      Some(g) => {
        groups[i] = g;
        i = i.saturating_add(1);
      },
      None => {
        *pos = save;
        return Ok((i, false));
      },
    }
  }

  Ok((limit, false))
}

/// One hextet: 1..=4 hex digits.
fn read_hextet(
  bytes: &[u8],
  pos: &mut usize,
) -> Option<u16> {
  let start = *pos;
  let mut value: u16 = 0;
  let mut digits = 0u32;

  while let Some(&b) = bytes.get(*pos) {
    let digit = match b {
      b'0'..=b'9' => u16::from(b - b'0'),
      b'a'..=b'f' => u16::from(b - b'a') + 10,
      b'A'..=b'F' => u16::from(b - b'A') + 10,
      _ => break,
    };
    if digits >= 4 {
      *pos = start;
      return None;
    }
    value = (value << 4) | digit;
    digits = digits.saturating_add(1);
    *pos = pos.saturating_add(1);
  }

  if digits == 0 {
    *pos = start;
    None
  } else {
    Some(value)
  }
}

/// Embedded IPv4 (RFC 4291): four decimal octets, no leading zeros / octal.
fn read_embedded_ipv4(
  bytes: &[u8],
  pos: &mut usize,
) -> Option<Ipv4Addr> {
  let start = *pos;
  let mut octets = [0u8; 4];

  for (i, slot) in octets.iter_mut().enumerate() {
    if i > 0 {
      if bytes.get(*pos).copied() != Some(b'.') {
        *pos = start;
        return None;
      }
      *pos = pos.saturating_add(1);
    }
    match read_decimal_octet(bytes, pos) {
      Some(o) => *slot = o,
      None => {
        *pos = start;
        return None;
      },
    }
  }

  Some(Ipv4Addr::from(octets))
}

fn read_decimal_octet(
  bytes: &[u8],
  pos: &mut usize,
) -> Option<u8> {
  let first = bytes.get(*pos).copied()?;
  if !first.is_ascii_digit() {
    return None;
  }
  *pos = pos.saturating_add(1);
  let mut value = u32::from(first - b'0');
  let mut digits = 1u32;

  while let Some(&b) = bytes.get(*pos) {
    if !b.is_ascii_digit() {
      break;
    }
    if digits >= 3 {
      return None;
    }
    value = value.saturating_mul(10).saturating_add(u32::from(b - b'0'));
    digits = digits.saturating_add(1);
    *pos = pos.saturating_add(1);
  }

  // Disallow octal-style leading zeros (`01`, `00`, …).
  if first == b'0' && digits > 1 {
    return None;
  }
  u8::try_from(value).ok()
}

impl core::fmt::Display for Uri<'_> {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.write_str(self.scheme)?;
    f.write_str("://")?;
    if let Some(auth) = &self.authority {
      match &auth.host {
        Host::RegName(name) => f.write_str(name)?,
        Host::IpAddr(IpAddr::V4(v4)) => write!(f, "{v4}")?,
        Host::IpAddr(IpAddr::V6(v6)) => write!(f, "[{v6}]")?,
      }
      if let Some(port) = auth.port {
        write!(f, ":{port}")?;
      }
    }
    f.write_str(self.path)?;
    if let Some(query) = self.query {
      write!(f, "?{query}")?;
    }
    Ok(())
  }
}

impl<'a> core::convert::TryFrom<&'a str> for Uri<'a> {
  type Error = ParseError;

  fn try_from(s: &'a str) -> Result<Self, Self::Error> {
    Self::parse(s)
  }
}

#[cfg(kani)]
mod kani_uri_proofs {
  use super::{Host, Uri};

  /// Reg-name host bytes are preserved as-is (pooling lowercases separately).
  #[kani::proof]
  fn reg_name_preserves_case() {
    let uri = Uri::parse("http://Example.COM/path").unwrap();
    match uri.authority().unwrap().host() {
      Host::RegName(name) => assert_eq!(*name, "Example.COM"),
      Host::IpAddr(_) => panic!("expected RegName"),
    }
  }

  /// Scheme case does not change default ports (eq_ignore_ascii_case).
  #[kani::proof]
  fn https_default_port_case_insensitive() {
    assert_eq!(
      Uri::parse("HTTPS://example.com/")
        .unwrap()
        .port_or_default(),
      443
    );
    assert_eq!(Uri::parse("Http://example.com/").unwrap().port_or_default(), 80);
  }

  /// ASCII lowercase idempotence for a bounded host label (pool key invariant).
  #[kani::proof]
  #[kani::unwind(16)]
  fn ascii_lowercase_idempotent_label() {
    let mut raw = [0u8; 4];
    for b in &mut raw {
      *b = kani::any();
      kani::assume(b.is_ascii_alphanumeric());
    }
    let s = core::str::from_utf8(&raw).unwrap();
    let once = s.to_ascii_lowercase();
    let twice = once.to_ascii_lowercase();
    assert_eq!(once, twice);
  }
}
