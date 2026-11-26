/// Percent-encode octets; RFC 3986 unreserved (`A-Z` `a-z` `0-9` `-` `_` `.` `~`) pass through.
///
/// Space becomes `%20` (query-string style).
#[must_use]
pub fn percent_encode(input: &str) -> alloc::string::String {
  encode(input.as_bytes(), false)
}

/// Encode for `application/x-www-form-urlencoded` (space as `+`).
#[must_use]
pub fn form_url_encode(input: &str) -> alloc::string::String {
  encode(input.as_bytes(), true)
}

fn encode(
  input: &[u8],
  space_as_plus: bool,
) -> alloc::string::String {
  use alloc::string::String;
  use alloc::vec::Vec;

  const HEX: &[u8; 16] = b"0123456789ABCDEF";
  // Worst case: every byte → `%XX` (3 chars).
  let mut buf = Vec::with_capacity(input.len().saturating_mul(3));
  for &byte in input {
    match byte {
      b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        buf.push(byte);
      },
      b' ' if space_as_plus => buf.push(b'+'),
      _ => {
        buf.push(b'%');
        buf.push(HEX.get((byte >> 4) as usize).copied().unwrap_or(b'?'));
        buf.push(HEX.get((byte & 0xf) as usize).copied().unwrap_or(b'?'));
      },
    }
  }
  // HEX digits and unreserved/input path bytes are always ASCII.
  debug_assert!(buf.iter().all(u8::is_ascii));
  // SAFETY: encoder only emits ASCII unreserved / `+` / `%XX`.
  unsafe { String::from_utf8_unchecked(buf) }
}

pub use core::net::IpAddr;

/// `uri-host` text for an IP literal, formatted on the stack (IPv6 bracketed).
///
/// Longest form is `[` + 39-char IPv6 + `]` = 41 bytes.
#[derive(Clone, Copy)]
pub struct IpHostBuf {
  buf: [u8; 48],
  len: u8,
}

impl IpHostBuf {
  /// Format `addr` like [`format_ip_for_host`] without allocating.
  #[must_use]
  pub fn new(addr: IpAddr) -> Self {
    use core::fmt::Write as _;
    let mut out = Self { buf: [0; 48], len: 0 };
    // Cannot fail: 48 bytes fits every IPv4 / bracketed IPv6 rendering.
    let _ = match addr {
      IpAddr::V4(v4) => write!(out, "{v4}"),
      IpAddr::V6(v6) => write!(out, "[{v6}]"),
    };
    out
  }

  /// Formatted host text.
  #[must_use]
  pub fn as_str(&self) -> &str {
    let bytes = self.buf.get(..usize::from(self.len)).unwrap_or(&[]);
    // SAFETY: only `fmt::Write::write_str` appends here, and it copies whole `&str`s.
    unsafe { core::str::from_utf8_unchecked(bytes) }
  }
}

impl core::fmt::Write for IpHostBuf {
  fn write_str(
    &mut self,
    s: &str,
  ) -> core::fmt::Result {
    let start = usize::from(self.len);
    let end = start.checked_add(s.len()).ok_or(core::fmt::Error)?;
    let dst = self.buf.get_mut(start..end).ok_or(core::fmt::Error)?;
    dst.copy_from_slice(s.as_bytes());
    self.len = u8::try_from(end).map_err(|_| core::fmt::Error)?;
    Ok(())
  }
}

impl core::fmt::Debug for IpHostBuf {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    core::fmt::Debug::fmt(self.as_str(), f)
  }
}

/// Hop hostname: borrowed reg-name, or a stack-formatted IP literal.
#[derive(Debug, Clone, Copy)]
pub enum HostText<'a> {
  /// Registered name, borrowed from the URI.
  Name(&'a str),
  /// IP literal text (IPv6 bracketed).
  Ip(IpHostBuf),
}

impl HostText<'_> {
  /// Host text for this hop.
  #[must_use]
  pub fn as_str(&self) -> &str {
    match self {
      Self::Name(n) => n,
      Self::Ip(b) => b.as_str(),
    }
  }
}

/// Host of `uri` without allocating (empty name when there is no authority).
#[must_use]
pub fn host_text<'a>(uri: &crate::parser::uri::Uri<'a>) -> HostText<'a> {
  use crate::parser::uri::Host;
  match uri.authority().map(crate::parser::uri::Authority::host) {
    Some(Host::RegName(name)) => HostText::Name(name),
    Some(Host::IpAddr(addr)) => HostText::Ip(IpHostBuf::new(*addr)),
    None => HostText::Name(""),
  }
}

/// Host / authority form: IPv6 literals need brackets.
#[must_use]
pub fn format_ip_for_host(addr: IpAddr) -> alloc::string::String {
  match addr {
    IpAddr::V4(v4) => alloc::format!("{v4}"),
    IpAddr::V6(v6) => alloc::format!("[{v6}]"),
  }
}

/// Wall-clock Unix seconds (cookie expiry, pool idle age).
#[must_use]
pub fn now_unix_secs() -> u64 {
  // Miri cannot call libc::time / Win32 FILETIME; keep a fixed clock for tests.
  #[cfg(miri)]
  {
    1_700_000_000
  }
  #[cfg(all(unix, not(miri)))]
  {
    // SAFETY: time(NULL) is well-defined; negative means error → 0.
    let t = unsafe { libc::time(core::ptr::null_mut()) };
    if t < 0 {
      0
    } else {
      u64::try_from(t).unwrap_or(0)
    }
  }
  #[cfg(all(windows, not(miri)))]
  {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    let mut ft = FILETIME {
      dwLowDateTime: 0,
      dwHighDateTime: 0,
    };
    // SAFETY: writes into stack FILETIME.
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    // FILETIME: 100ns since 1601-01-01; Unix epoch offset = 11_644_473_600 s
    ticks
      .checked_div(10_000_000)
      .and_then(|s| s.checked_sub(11_644_473_600))
      .unwrap_or(0)
  }
  #[cfg(not(any(unix, windows, miri)))]
  {
    0
  }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used)]
  use super::*;

  #[test]
  fn percent_encode_space_and_reserved() {
    assert_eq!(percent_encode("a b"), "a%20b");
    assert_eq!(percent_encode("a+b"), "a%2Bb");
    assert_eq!(percent_encode("un-reserved._~"), "un-reserved._~");
  }

  #[test]
  fn form_url_encode_space_as_plus() {
    assert_eq!(form_url_encode("a b"), "a+b");
    assert_eq!(form_url_encode("a+b"), "a%2Bb");
    assert_eq!(form_url_encode("ok"), "ok");
  }

  #[test]
  fn encode_modes_match_public_helpers() {
    assert_eq!(percent_encode("x y!"), encode(b"x y!", false));
    assert_eq!(form_url_encode("x y!"), encode(b"x y!", true));
  }

  #[test]
  fn property_unreserved_unchanged() {
    use proptest::prelude::*;
    const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    proptest::proptest!(|(s in prop::collection::vec(
      prop::sample::select(UNRESERVED),
      0..64
    ))| {
      // Selected bytes are ASCII unreserved → always valid UTF-8.
      let input = core::str::from_utf8(&s).unwrap_or("");
      prop_assert_eq!(percent_encode(input), input);
      prop_assert_eq!(form_url_encode(input), input);
    });
  }

  #[test]
  fn property_encode_never_panics_and_is_ascii() {
    use proptest::prelude::*;
    proptest::proptest!(|(s in ".*")| {
      let out = percent_encode(&s);
      prop_assert!(out.is_ascii());
      let form = form_url_encode(&s);
      prop_assert!(form.is_ascii());
    });
  }

  #[test]
  fn property_space_encoding_differs() {
    use proptest::prelude::*;
    proptest::proptest!(|(prefix in "[A-Za-z0-9]{0,8}", suffix in "[A-Za-z0-9]{0,8}")| {
      let s = alloc::format!("{prefix} {suffix}");
      let pct = percent_encode(&s);
      let form = form_url_encode(&s);
      prop_assert!(pct.contains("%20"));
      prop_assert!(form.contains('+'));
      prop_assert_ne!(pct, form);
    });
  }
}
