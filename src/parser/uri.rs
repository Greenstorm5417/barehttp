use crate::error::ParseError;
use crate::util::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri<'a> {
  scheme: &'a str,
  authority: Option<Authority<'a>>,
  path: &'a str,
  query: Option<&'a str>,
  fragment: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authority<'a> {
  userinfo: Option<&'a str>,
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

  #[allow(dead_code)]
  pub const fn path(&self) -> &'a str {
    self.path
  }

  pub fn path_and_query(&self) -> alloc::string::String {
    self.query.map_or_else(
      || alloc::string::String::from(self.path),
      |query| alloc::format!("{}?{}", self.path, query),
    )
  }

  /// Resolves a relative URL against this URI as a base
  ///
  /// # Errors
  /// Returns `ParseError::InvalidUri` if the location is not a valid relative or absolute URL
  pub fn resolve_relative(
    &self,
    location: &str,
  ) -> Result<alloc::string::String, ParseError> {
    if location.starts_with("http://") || location.starts_with("https://") {
      Ok(alloc::string::String::from(location))
    } else if location.starts_with('/') {
      let authority = self.authority.as_ref().ok_or(ParseError::InvalidUri)?;
      let port = authority.port.unwrap_or_else(|| {
        if self.scheme == "https" {
          443
        } else {
          80
        }
      });

      let host_str = match &authority.host {
        Host::RegName(name) => *name,
        Host::IpAddr(_) => return Err(ParseError::InvalidUri),
      };

      if (self.scheme == "http" && port == 80) || (self.scheme == "https" && port == 443) {
        Ok(alloc::format!(
          "{scheme}://{host}{location}",
          scheme = self.scheme,
          host = host_str
        ))
      } else {
        Ok(alloc::format!(
          "{scheme}://{host}:{port}{location}",
          scheme = self.scheme,
          host = host_str
        ))
      }
    } else {
      Err(ParseError::InvalidUri)
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

    if self.peek() != Some(b':') {
      return Err(ParseError::InvalidUri);
    }
    self.advance();

    let (authority, path) = self.parse_hier_part()?;

    let query = if self.peek() == Some(b'?') {
      self.advance();
      Some(self.parse_query()?)
    } else {
      None
    };

    let fragment = if self.peek() == Some(b'#') {
      self.advance();
      Some(self.parse_fragment()?)
    } else {
      None
    };

    if self.pos != self.input.len() {
      return Err(ParseError::InvalidUri);
    }

    Ok(Uri {
      scheme,
      authority,
      path,
      query,
      fragment,
    })
  }

  fn parse_scheme(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;

    if !matches!(self.peek(), Some(b'A'..=b'Z' | b'a'..=b'z')) {
      return Err(ParseError::InvalidUri);
    }
    self.advance();

    while let Some(ch) = self.peek() {
      match ch {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => {
          self.advance();
        },
        _ => break,
      }
    }

    if start == self.pos {
      return Err(ParseError::InvalidUri);
    }

    Ok(self.slice_from(start))
  }

  fn parse_hier_part(&mut self) -> Result<(Option<Authority<'a>>, &'a str), ParseError> {
    if self.peek() == Some(b'/') && self.peek_at(1) == Some(b'/') {
      self.advance_by(2);
      let authority = self.parse_authority()?;
      let path = self.parse_path_abempty();
      Ok((Some(authority), path))
    } else if self.peek() == Some(b'/') {
      let path = self.parse_path_absolute()?;
      Ok((None, path))
    } else if self.peek().is_some() && !matches!(self.peek(), Some(b'?' | b'#')) {
      let path = self.parse_path_rootless()?;
      Ok((None, path))
    } else {
      Ok((None, ""))
    }
  }

  fn parse_authority(&mut self) -> Result<Authority<'a>, ParseError> {
    let has_at = self.find_char_in_authority(b'@');

    let userinfo = if has_at {
      let userinfo_start = self.pos;
      while let Some(ch) = self.peek() {
        if ch == b'@' {
          self.advance();
          break;
        } else if is_userinfo_char(ch) {
          self.advance();
        } else {
          return Err(ParseError::InvalidUri);
        }
      }
      Some(&self.input[userinfo_start..self.pos.saturating_sub(1)])
    } else {
      None
    };

    let host = self.parse_host()?;

    let port = if self.peek() == Some(b':') {
      self.advance();
      Some(self.parse_port()?)
    } else {
      None
    };

    Ok(Authority { userinfo, host, port })
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
      self.parse_ip_literal()
    } else {
      let start = self.pos;
      let mut dots = 0_usize;
      let mut all_digits = true;

      while let Some(ch) = self.peek() {
        match ch {
          b':' | b'/' | b'?' | b'#' => break,
          b'.' => {
            dots = dots.saturating_add(1);
            self.advance();
          },
          b'0'..=b'9' => {
            self.advance();
          },
          _ if is_reg_name_char(ch) => {
            all_digits = false;
            self.advance();
          },
          _ => break,
        }
      }

      let host_str = self.slice_from(start);

      if all_digits
        && dots == 3
        && let Ok(ipv4) = parse_ipv4(host_str)
      {
        return Ok(Host::IpAddr(IpAddr::V4(ipv4)));
      }

      Ok(Host::RegName(host_str))
    }
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

    parse_ipv6(addr_str).map_or(Err(ParseError::InvalidUri), |ipv6| Ok(Host::IpAddr(IpAddr::V6(ipv6))))
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
      self.parse_segment();
    }

    self.slice_from(start)
  }

  fn parse_path_absolute(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;

    if self.peek() != Some(b'/') {
      return Err(ParseError::InvalidUri);
    }
    self.advance();

    if self.peek().is_some() && !matches!(self.peek(), Some(b'/' | b'?' | b'#')) {
      self.parse_segment_nz()?;

      while self.peek() == Some(b'/') {
        self.advance();
        self.parse_segment();
      }
    }

    Ok(self.slice_from(start))
  }

  fn parse_path_rootless(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;

    self.parse_segment_nz()?;

    while self.peek() == Some(b'/') {
      self.advance();
      self.parse_segment();
    }

    Ok(self.slice_from(start))
  }

  fn parse_segment(&mut self) {
    while let Some(ch) = self.peek() {
      if is_pchar(ch) {
        self.advance();
      } else {
        break;
      }
    }
  }

  fn parse_segment_nz(&mut self) -> Result<(), ParseError> {
    let start = self.pos;
    self.parse_segment();
    if start == self.pos {
      return Err(ParseError::InvalidUri);
    }
    Ok(())
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

  fn parse_fragment(&mut self) -> Result<&'a str, ParseError> {
    let start = self.pos;

    while let Some(ch) = self.peek() {
      if is_pchar(ch) || ch == b'/' || ch == b'?' {
        self.advance();
      } else {
        return Err(ParseError::InvalidUri);
      }
    }

    Ok(self.slice_from(start))
  }
}

const fn is_alpha(ch: u8) -> bool {
  ch.is_ascii_alphabetic()
}

const fn is_digit(ch: u8) -> bool {
  ch.is_ascii_digit()
}

const fn is_hexdig(ch: u8) -> bool {
  ch.is_ascii_hexdigit()
}

const fn is_unreserved(ch: u8) -> bool {
  is_alpha(ch) || is_digit(ch) || matches!(ch, b'-' | b'.' | b'_' | b'~')
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

const fn is_userinfo_char(ch: u8) -> bool {
  is_unreserved(ch) || is_sub_delim(ch) || ch == b':' || ch == b'%'
}

const fn is_reg_name_char(ch: u8) -> bool {
  is_unreserved(ch) || is_sub_delim(ch) || ch == b'%'
}

fn parse_ipv4(s: &str) -> Result<[u8; 4], ParseError> {
  let mut octets = [0u8; 4];
  let mut idx = 0;
  let mut current = 0u16;
  let mut has_digits = false;

  for byte in s.as_bytes() {
    match byte {
      b'0'..=b'9' => {
        current = current
          .saturating_mul(10)
          .saturating_add(u16::from(byte - b'0'));
        has_digits = true;
        if current > 255 {
          return Err(ParseError::InvalidUri);
        }
      },
      b'.' => {
        if !has_digits || idx >= 3 {
          return Err(ParseError::InvalidUri);
        }
        #[allow(clippy::cast_possible_truncation)]
        if let Some(octet) = octets.get_mut(idx) {
          *octet = current as u8;
        } else {
          return Err(ParseError::InvalidUri);
        }
        idx = idx.saturating_add(1);
        current = 0;
        has_digits = false;
      },
      _ => return Err(ParseError::InvalidUri),
    }
  }

  if !has_digits || idx != 3 {
    return Err(ParseError::InvalidUri);
  }

  #[allow(clippy::cast_possible_truncation)]
  if let Some(octet) = octets.get_mut(3) {
    *octet = current as u8;
  } else {
    return Err(ParseError::InvalidUri);
  }

  Ok(octets)
}

fn parse_ipv6(s: &str) -> Result<[u16; 8], ParseError> {
  if s.is_empty() {
    return Err(ParseError::InvalidUri);
  }

  let bytes = s.as_bytes();
  let mut result = [0u16; 8];
  let mut i = 0;
  let mut j = 0;
  let mut double_colon_pos = None;

  if bytes.len() >= 2
    && let (Some(&b0), Some(&b1)) = (bytes.first(), bytes.get(1))
    && b0 == b':'
    && b1 == b':'
  {
    double_colon_pos = Some(0);
    i = 2;
  }

  while i < bytes.len() {
    if i + 1 < bytes.len()
      && let (Some(&bi), Some(&bi1)) = (bytes.get(i), bytes.get(i + 1))
      && bi == b':'
      && bi1 == b':'
    {
      if double_colon_pos.is_some() {
        return Err(ParseError::InvalidUri);
      }
      double_colon_pos = Some(j);
      i = i.saturating_add(2);
      if i >= bytes.len() {
        break;
      }
      continue;
    }

    if let Some(&bi) = bytes.get(i) {
      if bi == b':' {
        i = i.saturating_add(1);
        continue;
      }
    } else {
      break;
    }

    let start = i;
    while i < bytes.len() {
      if let Some(&bi) = bytes.get(i) {
        if is_hexdig(bi) {
          i = i.saturating_add(1);
        } else {
          break;
        }
      } else {
        break;
      }
    }

    if start == i {
      break;
    }

    if i.saturating_sub(start) > 4 {
      return Err(ParseError::InvalidUri);
    }

    let hex_str = bytes
      .get(start..i)
      .ok_or(ParseError::InvalidUri)
      .map(|slice| unsafe { core::str::from_utf8_unchecked(slice) })?;
    let value = u16::from_str_radix(hex_str, 16).map_err(|_| ParseError::InvalidUri)?;

    if j >= 8 {
      return Err(ParseError::InvalidUri);
    }
    if let Some(slot) = result.get_mut(j) {
      *slot = value;
    } else {
      return Err(ParseError::InvalidUri);
    }
    j = j.saturating_add(1);
  }

  if let Some(pos) = double_colon_pos {
    let num_after = j.saturating_sub(pos);
    let zeros_needed = 8_usize.saturating_sub(j);

    if num_after > 0 {
      for k in (0..num_after).rev() {
        let src_idx = pos.saturating_add(k);
        let dst_idx = 7_usize.saturating_sub(k);
        if let Some(&val) = result.get(src_idx)
          && let Some(dst) = result.get_mut(dst_idx)
        {
          *dst = val;
        }
        if let Some(src) = result.get_mut(src_idx) {
          *src = 0;
        }
      }
    }

    for k in 0..zeros_needed {
      let idx = pos.saturating_add(k);
      if let Some(slot) = result.get_mut(idx) {
        *slot = 0;
      }
    }
  } else if j != 8 {
    return Err(ParseError::InvalidUri);
  }

  Ok(result)
}
