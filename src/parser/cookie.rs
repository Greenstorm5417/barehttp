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

    let name_value_pair = semicolon_pos.map_or(input_bytes, |pos| {
      input_bytes.get(..pos).unwrap_or(input_bytes)
    });

    let unparsed_attributes =
      semicolon_pos.map_or(&[][..], |pos| input_bytes.get(pos..).unwrap_or(&[]));

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

fn parse_cookie_attributes(mut input: &[u8]) -> CookieAttributes {
  let mut attrs = CookieAttributes::default();

  if input.is_empty() {
    return attrs;
  }

  loop {
    if input.is_empty() {
      break;
    }

    if input.first() == Some(&b';') {
      input = input.get(1..).unwrap_or(&[]);
    } else {
      break;
    }

    let next_semicolon = input.iter().position(|&b| b == b';');

    let cookie_av = if let Some(pos) = next_semicolon {
      let av = input.get(..pos).unwrap_or(input);
      input = input.get(pos..).unwrap_or(&[]);
      av
    } else {
      let av = input;
      input = &[];
      av
    };

    let equals_pos = cookie_av.iter().position(|&b| b == b'=');

    let (attr_name_raw, attr_value_raw) = equals_pos.map_or_else(
      || (cookie_av, &[][..]),
      |pos| {
        let name = cookie_av.get(..pos).unwrap_or(&[]);
        let value = cookie_av
          .get(pos.checked_add(1).unwrap_or(pos)..)
          .unwrap_or(&[]);
        (name, value)
      },
    );

    let attr_name = trim_wsp(attr_name_raw);
    let attr_value = trim_wsp(attr_value_raw);

    let attr_name_lower = attr_name.to_ascii_lowercase();

    match attr_name_lower.as_slice() {
      b"expires" => {
        let value_str = String::from_utf8_lossy(attr_value);
        if let Some(date) = parse_cookie_date(&value_str) {
          attrs.expires = Some(date);
        }
      }
      b"max-age" => {
        let value_str = String::from_utf8_lossy(attr_value);
        if let Some(delta) = parse_max_age(&value_str) {
          attrs.max_age = Some(delta);
        }
      }
      b"domain" => {
        if attr_value.is_empty() {
          continue;
        }

        let domain_value = if attr_value.first() == Some(&b'.') {
          attr_value.get(1..).unwrap_or(&[])
        } else {
          attr_value
        };

        let domain_str = String::from_utf8_lossy(domain_value).to_lowercase();
        attrs.domain = Some(domain_str);
      }
      b"path" => {
        let path_value = if attr_value.is_empty() || attr_value.first() != Some(&b'/') {
          continue;
        } else {
          attr_value
        };

        let path_str = String::from_utf8_lossy(path_value).into_owned();
        attrs.path = Some(path_str);
      }
      b"secure" => {
        attrs.secure = true;
      }
      b"httponly" => {
        attrs.http_only = true;
      }
      _ => {}
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

pub fn parse_cookie_date(input: &str) -> Option<CookieDate> {
  let tokens = tokenize_date(input);

  let mut found_time = false;
  let mut found_day = false;
  let mut found_month = false;
  let mut found_year = false;

  let mut hour = 0u8;
  let mut minute = 0u8;
  let mut second = 0u8;
  let mut day = 0u8;
  let mut month = 0u8;
  let mut year = 0u16;

  for token in tokens {
    if !found_time && let Some((h, m, s)) = parse_time_token(token) {
      hour = h;
      minute = m;
      second = s;
      found_time = true;
      continue;
    }

    if !found_day && let Some(d) = parse_day_of_month(token) {
      day = d;
      found_day = true;
      continue;
    }

    if !found_month && let Some(m) = parse_month(token) {
      month = m;
      found_month = true;
      continue;
    }

    if !found_year && let Some(y) = parse_year(token) {
      year = y;
      found_year = true;
    }
  }

  if !found_time || !found_day || !found_month || !found_year {
    return None;
  }

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
  let minute_str = token.get(
    parts.first().copied().unwrap_or(0).checked_add(1)?
      ..parts.get(1).copied().unwrap_or(0),
  )?;
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
