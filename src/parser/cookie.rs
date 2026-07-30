extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// `SameSite` attribute (RFC 10025). `Default` = attribute absent / unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SameSite {
  /// `SameSite=Strict`
  Strict,
  /// `SameSite=Lax`
  Lax,
  /// `SameSite=None` (requires `Secure` at store time)
  None,
  /// Attribute missing or unrecognized; browser UAs enforce like Lax.
  #[default]
  Default,
}

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
  pub same_site: SameSite,
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

impl CookieDate {
  /// Convert to Unix seconds (UTC). Pre-1970 dates clamp to `0`.
  #[must_use]
  pub fn to_unix_secs(self) -> Option<u64> {
    if !(1..=12).contains(&self.month) || !(1..=31).contains(&self.day) {
      return None;
    }
    if self.hour > 23 || self.minute > 59 || self.second > 59 {
      return None;
    }
    let days = days_from_civil(i64::from(self.year), self.month, self.day)?;
    let secs = days
      .checked_mul(86_400)?
      .checked_add(i64::from(self.hour) * 3_600)?
      .checked_add(i64::from(self.minute) * 60)?
      .checked_add(i64::from(self.second))?;
    if secs < 0 {
      Some(0)
    } else {
      u64::try_from(secs).ok()
    }
  }
}

/// Civil date → days since Unix epoch (Howard Hinnant). Returns `None` if out of range.
#[allow(clippy::integer_division)] // calendar arithmetic is exact integer division
fn days_from_civil(
  mut y: i64,
  month: u8,
  day: u8,
) -> Option<i64> {
  let month_u = u32::from(month);
  let day_u = u32::from(day);
  if month_u <= 2 {
    y = y.checked_sub(1)?;
  }
  let era = if y >= 0 {
    y
  } else {
    y.checked_sub(399)?
  }
  .div_euclid(400);
  let yoe = u32::try_from(y.checked_sub(era.checked_mul(400)?)?).ok()?;
  let mp = if month_u > 2 {
    month_u - 3
  } else {
    month_u + 9
  };
  let doy = (153 * mp + 2) / 5 + day_u - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era
    .checked_mul(146_097)?
    .checked_add(i64::from(doe))?
    .checked_sub(719_468)
}

impl SetCookie {
  /// Parse one `Set-Cookie` header value (RFC 10025 §5.6).
  ///
  /// Returns `None` when the string is ignored (CTLs, empty name+value, etc.).
  pub fn parse(input: &str) -> Option<Self> {
    let input_bytes = input.as_bytes();

    // RFC 10025 §5.6: reject CTL octets excluding HTAB in the set-cookie-string.
    if input_bytes
      .iter()
      .any(|&b| matches!(b, 0x00..=0x08 | 0x0A..=0x1F | 0x7F))
    {
      return None;
    }

    let semicolon_pos = input_bytes.iter().position(|&b| b == b';');

    let name_value_pair = semicolon_pos.map_or(input_bytes, |pos| input_bytes.get(..pos).unwrap_or(input_bytes));

    let unparsed_attributes = semicolon_pos.map_or(&[][..], |pos| input_bytes.get(pos..).unwrap_or(&[]));

    let (name_bytes, value_bytes) = match name_value_pair.iter().position(|&b| b == b'=') {
      Some(equals_pos) => (
        name_value_pair.get(..equals_pos)?,
        name_value_pair.get(equals_pos.checked_add(1)?..)?,
      ),
      // No `=`: empty name, whole token is the value (RFC 10025 §5.6).
      None => (&[][..], name_value_pair),
    };

    let name_trimmed = trim_wsp(name_bytes);
    let value_trimmed = trim_wsp(value_bytes);

    // Reject cookies with neither name nor value.
    if name_trimmed.is_empty() && value_trimmed.is_empty() {
      return None;
    }

    // RFC 10025 §5.7: name+value > 4096 octets → ignore.
    if name_trimmed.len().saturating_add(value_trimmed.len()) > 4096 {
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
      same_site: attributes.same_site,
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
  same_site: SameSite,
}

fn parse_cookie_attributes(mut input: &[u8]) -> CookieAttributes {
  let mut attrs = CookieAttributes::default();

  while !input.is_empty() {
    while input.first() == Some(&b';') {
      input = input.get(1..).unwrap_or(&[]);
    }
    if input.is_empty() {
      break;
    }

    let end = input.iter().position(|&b| b == b';').unwrap_or(input.len());
    let av = input.get(..end).unwrap_or(&[]);
    input = input.get(end..).unwrap_or(&[]);

    let (name, value) = av.iter().position(|&b| b == b'=').map_or_else(
      || (av, &[][..]),
      |i| {
        (
          av.get(..i).unwrap_or(&[]),
          av.get(i.checked_add(1).unwrap_or(i)..).unwrap_or(&[]),
        )
      },
    );

    let name_trimmed = trim_wsp(name);
    let value_trimmed = trim_wsp(value);

    if name_trimmed.eq_ignore_ascii_case(b"secure") {
      attrs.secure = true;
    } else if name_trimmed.eq_ignore_ascii_case(b"httponly") {
      attrs.http_only = true;
    } else if name_trimmed.eq_ignore_ascii_case(b"expires") {
      if let Ok(s) = core::str::from_utf8(value_trimmed) {
        attrs.expires = parse_cookie_date(s);
      }
    } else if name_trimmed.eq_ignore_ascii_case(b"max-age") {
      if let Ok(s) = core::str::from_utf8(value_trimmed) {
        attrs.max_age = parse_max_age(s);
      }
    } else if name_trimmed.eq_ignore_ascii_case(b"domain") {
      attrs.domain = parse_domain(value_trimmed);
    } else if name_trimmed.eq_ignore_ascii_case(b"path") {
      attrs.path = parse_path(value_trimmed);
    } else if name_trimmed.eq_ignore_ascii_case(b"samesite") {
      attrs.same_site = parse_same_site(value_trimmed);
    }
  }

  attrs
}

fn parse_same_site(value: &[u8]) -> SameSite {
  if value.eq_ignore_ascii_case(b"strict") {
    SameSite::Strict
  } else if value.eq_ignore_ascii_case(b"lax") {
    SameSite::Lax
  } else if value.eq_ignore_ascii_case(b"none") {
    SameSite::None
  } else {
    SameSite::Default
  }
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

  // RFC 10025: Domain attribute must be ASCII.
  if !domain_value.is_ascii() {
    return None;
  }

  Some(String::from_utf8_lossy(domain_value).to_lowercase())
}

fn parse_path(value: &[u8]) -> Option<String> {
  if value.is_empty() || value.first() != Some(&b'/') {
    return None;
  }

  Some(String::from_utf8_lossy(value).into_owned())
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

  if digits.is_empty() || digits.iter().any(|b| !b.is_ascii_digit()) {
    return None;
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

  if !(1..=31).contains(&day) || year < 1601 || hour > 23 || minute > 59 || second > 59 {
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

  if !(1..=2).contains(&hour_str.len()) || !(1..=2).contains(&minute_str.len()) || !(1..=2).contains(&second_str.len())
  {
    return None;
  }

  Some((
    hour_str.parse().ok()?,
    minute_str.parse().ok()?,
    second_str.parse().ok()?,
  ))
}

fn parse_day_of_month(token: &str) -> Option<u8> {
  let bytes = token.as_bytes();
  if bytes.is_empty() || bytes.len() > 2 || bytes.iter().any(|b| !b.is_ascii_digit()) {
    return None;
  }
  token.parse().ok()
}

fn parse_month(token: &str) -> Option<u8> {
  let lower = token.to_ascii_lowercase();
  if lower.len() < 3 {
    return None;
  }
  match &lower[..3] {
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
