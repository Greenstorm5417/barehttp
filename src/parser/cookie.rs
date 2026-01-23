extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCookie {
  pub name: String,
  pub value: String,
  pub expires: Option<CookieDate>,
  pub max_age: Option<i64>,
  pub domain: Option<String>,
  pub path: Option<String>,
  pub secure: bool,
  pub http_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookieDate {
  pub year: u16,
  pub month: u8,
  pub day: u8,
  pub hour: u8,
  pub minute: u8,
  pub second: u8,
}

impl SetCookie {
  pub fn parse(input: &str) -> Option<Self> {
    let input_bytes = input.as_bytes();

    let semicolon_pos = input_bytes.iter().position(|&b| b == b';');

    let name_value_pair = semicolon_pos.map_or(input_bytes, |pos| input_bytes.get(..pos).unwrap_or(input_bytes));

    let unparsed_attributes = semicolon_pos.map_or(&[][..], |pos| input_bytes.get(pos..).unwrap_or(&[]));

    let equals_pos = name_value_pair.iter().position(|&b| b == b'=')?;

    let name_bytes = name_value_pair.get(..equals_pos)?;
    let value_bytes = name_value_pair.get(equals_pos.checked_add(1)?..)?;

    let name_trimmed = trim_wsp(name_bytes);
    let value_trimmed = trim_wsp(value_bytes);

    if name_trimmed.is_empty() {
      return None;
    }

    let name = String::from_utf8_lossy(name_trimmed).into_owned();
    let value = String::from_utf8_lossy(value_trimmed).into_owned();

    let attributes = parse_cookie_attributes(unparsed_attributes);

    Some(Self {
      name,
      value,
      expires: attributes.expires,
      max_age: attributes.max_age,
      domain: attributes.domain,
      path: attributes.path,
      secure: attributes.secure,
      http_only: attributes.http_only,
    })
  }
}

#[derive(Default)]
struct CookieAttributes {
  expires: Option<CookieDate>,
  max_age: Option<i64>,
  domain: Option<String>,
  path: Option<String>,
  secure: bool,
  http_only: bool,
}

struct AttrIter<'a> {
  input: &'a [u8],
}

impl<'a> AttrIter<'a> {
  const fn new(input: &'a [u8]) -> Self {
    Self { input }
  }
}

impl<'a> Iterator for AttrIter<'a> {
  type Item = (&'a [u8], &'a [u8]);

  fn next(&mut self) -> Option<Self::Item> {
    while self.input.first() == Some(&b';') {
      self.input = self.input.get(1..).unwrap_or(&[]);
    }

    if self.input.is_empty() {
      return None;
    }

    let end = self
      .input
      .iter()
      .position(|&b| b == b';')
      .unwrap_or(self.input.len());

    let av = self.input.get(..end)?;
    self.input = self.input.get(end..).unwrap_or(&[]);

    let eq = av.iter().position(|&b| b == b'=');
    Some(eq.map_or((av, &[]), |i| {
      let name = av.get(..i).unwrap_or(&[]);
      let value = av.get(i.checked_add(1).unwrap_or(i)..).unwrap_or(&[]);
      (name, value)
    }))
  }
}

fn eq_ignore_ascii(
  a: &[u8],
  b: &[u8],
) -> bool {
  a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_ascii_lowercase() == *y)
}

fn parse_domain(value: &[u8]) -> Option<String> {
  if value.is_empty() {
    return None;
  }

  let domain_value = if value.first() == Some(&b'.') {
    value.get(1..).unwrap_or(&[])
  } else {
    value
  };

  Some(String::from_utf8_lossy(domain_value).to_lowercase())
}

fn parse_path(value: &[u8]) -> Option<String> {
  if value.is_empty() || value.first() != Some(&b'/') {
    return None;
  }

  Some(String::from_utf8_lossy(value).into_owned())
}

fn parse_cookie_attributes(input: &[u8]) -> CookieAttributes {
  let mut attrs = CookieAttributes::default();

  for (name, value) in AttrIter::new(input) {
    let name_trimmed = trim_wsp(name);
    let value_trimmed = trim_wsp(value);

    match name_trimmed {
      _ if eq_ignore_ascii(name_trimmed, b"secure") => attrs.secure = true,
      _ if eq_ignore_ascii(name_trimmed, b"httponly") => attrs.http_only = true,
      _ if eq_ignore_ascii(name_trimmed, b"expires") => {
        if let Ok(s) = core::str::from_utf8(value_trimmed) {
          attrs.expires = parse_cookie_date(s);
        }
      },
      _ if eq_ignore_ascii(name_trimmed, b"max-age") => {
        if let Ok(s) = core::str::from_utf8(value_trimmed) {
          attrs.max_age = parse_max_age(s);
        }
      },
      _ if eq_ignore_ascii(name_trimmed, b"domain") => {
        attrs.domain = parse_domain(value_trimmed);
      },
      _ if eq_ignore_ascii(name_trimmed, b"path") => {
        attrs.path = parse_path(value_trimmed);
      },
      _ => {},
    }
  }

  attrs
}

fn parse_max_age(value: &str) -> Option<i64> {
  let bytes = value.as_bytes();

  if bytes.is_empty() {
    return None;
  }

  let (is_negative, digits) = if bytes.first() == Some(&b'-') {
    (true, bytes.get(1..).unwrap_or(&[]))
  } else {
    (false, bytes)
  };

  if digits.is_empty() {
    return None;
  }

  for &b in digits {
    if !b.is_ascii_digit() {
      return None;
    }
  }

  let digits_str = core::str::from_utf8(digits).ok()?;
  let abs_value: i64 = digits_str.parse().ok()?;

  if is_negative {
    Some(-abs_value)
  } else {
    Some(abs_value)
  }
}

struct DateParts {
  time: Option<(u8, u8, u8)>,
  day: Option<u8>,
  month: Option<u8>,
  year: Option<u16>,
}

impl DateParts {
  const fn new() -> Self {
    Self {
      time: None,
      day: None,
      month: None,
      year: None,
    }
  }
}

pub fn parse_cookie_date(input: &str) -> Option<CookieDate> {
  let tokens = tokenize_date(input);
  let mut parts = DateParts::new();

  for token in tokens {
    if parts.time.is_none()
      && let Some(time) = parse_time_token(token)
    {
      parts.time = Some(time);
      continue;
    }

    if parts.day.is_none()
      && let Some(d) = parse_day_of_month(token)
    {
      parts.day = Some(d);
      continue;
    }

    if parts.month.is_none()
      && let Some(m) = parse_month(token)
    {
      parts.month = Some(m);
      continue;
    }

    if parts.year.is_none()
      && let Some(y) = parse_year(token)
    {
      parts.year = Some(y);
    }
  }

  let time = parts.time?;
  let day = parts.day?;
  let month = parts.month?;
  let mut year = parts.year?;

  let (hour, minute, second) = time;

  if (70..=99).contains(&year) {
    year += 1900;
  } else if year <= 69 {
    year += 2000;
  }

  if !(1..=31).contains(&day) {
    return None;
  }

  if year < 1601 {
    return None;
  }

  if hour > 23 {
    return None;
  }

  if minute > 59 {
    return None;
  }

  if second > 59 {
    return None;
  }

  Some(CookieDate {
    year,
    month,
    day,
    hour,
    minute,
    second,
  })
}

fn tokenize_date(input: &str) -> Vec<&str> {
  let mut tokens = Vec::new();
  let mut start = None;

  for (i, c) in input.char_indices() {
    let is_delim = matches!(c, '\t' | ' '..='/' | ';'..='@' | '['..='`' | '{'..='~');

    if is_delim {
      if let Some(s) = start {
        tokens.push(&input[s..i]);
        start = None;
      }
    } else if start.is_none() {
      start = Some(i);
    }
  }

  if let Some(s) = start {
    tokens.push(&input[s..]);
  }

  tokens
}

fn parse_time_token(token: &str) -> Option<(u8, u8, u8)> {
  let bytes = token.as_bytes();

  let mut parts = [0usize; 2];
  let mut part_idx = 0;

  for (i, &b) in bytes.iter().enumerate() {
    if b == b':' {
      if part_idx >= 2 {
        return None;
      }
      if let Some(slot) = parts.get_mut(part_idx) {
        *slot = i;
      }
      part_idx += 1;
    }
  }

  if part_idx != 2 {
    return None;
  }

  let hour_str = token.get(..parts.first().copied().unwrap_or(0))?;
  let minute_str =
    token.get(parts.first().copied().unwrap_or(0).checked_add(1)?..parts.get(1).copied().unwrap_or(0))?;
  let second_str = token.get(parts.get(1).copied().unwrap_or(0).checked_add(1)?..)?;

  if hour_str.is_empty() || hour_str.len() > 2 {
    return None;
  }
  if minute_str.is_empty() || minute_str.len() > 2 {
    return None;
  }
  if second_str.is_empty() || second_str.len() > 2 {
    return None;
  }

  let hour: u8 = hour_str.parse().ok()?;
  let minute: u8 = minute_str.parse().ok()?;
  let second: u8 = second_str.parse().ok()?;

  Some((hour, minute, second))
}

fn parse_day_of_month(token: &str) -> Option<u8> {
  let bytes = token.as_bytes();

  if bytes.is_empty() || bytes.len() > 2 {
    return None;
  }

  for &b in bytes {
    if !b.is_ascii_digit() {
      return None;
    }
  }

  token.parse().ok()
}

fn parse_month(token: &str) -> Option<u8> {
  let lower = token.to_ascii_lowercase();

  if lower.len() < 3 {
    return None;
  }

  let prefix = &lower[..3];

  match prefix {
    "jan" => Some(1),
    "feb" => Some(2),
    "mar" => Some(3),
    "apr" => Some(4),
    "may" => Some(5),
    "jun" => Some(6),
    "jul" => Some(7),
    "aug" => Some(8),
    "sep" => Some(9),
    "oct" => Some(10),
    "nov" => Some(11),
    "dec" => Some(12),
    _ => None,
  }
}

fn parse_year(token: &str) -> Option<u16> {
  let bytes = token.as_bytes();

  if bytes.len() < 2 || bytes.len() > 4 {
    return None;
  }

  let mut digit_count = 0;
  for &b in bytes {
    if b.is_ascii_digit() {
      digit_count += 1;
    } else {
      break;
    }
  }

  if !(2..=4).contains(&digit_count) {
    return None;
  }

  let year_str = core::str::from_utf8(bytes.get(..digit_count)?).ok()?;
  year_str.parse().ok()
}

fn trim_wsp(input: &[u8]) -> &[u8] {
  let mut start = 0;
  let mut end = input.len();

  while start < end {
    if let Some(&byte) = input.get(start) {
      if byte == b' ' || byte == b'\t' {
        start += 1;
      } else {
        break;
      }
    } else {
      break;
    }
  }

  while end > start {
    if let Some(&byte) = input.get(end.saturating_sub(1)) {
      if byte == b' ' || byte == b'\t' {
        end -= 1;
      } else {
        break;
      }
    } else {
      break;
    }
  }

  input.get(start..end).unwrap_or(&[])
}

#[allow(dead_code)]
pub fn serialize_cookie_header(cookies: &[(String, String)]) -> String {
  let mut result = String::new();

  for (i, (name, value)) in cookies.iter().enumerate() {
    if i > 0 {
      result.push_str("; ");
    }
    result.push_str(name);
    result.push('=');
    result.push_str(value);
  }

  result
}
