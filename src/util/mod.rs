/// Percent-encode a string; leaves RFC 3986 unreserved (`A-Z` `a-z` `0-9` `-` `_` `.` `~`) alone.
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
  use core::fmt::Write;

  let mut result = String::new();
  for &byte in input {
    match byte {
      b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        result.push(byte as char);
      },
      b' ' if space_as_plus => result.push('+'),
      _ => {
        result.push('%');
        let _ = write!(result, "{byte:02X}");
      },
    }
  }
  result
}

pub use core::net::IpAddr;

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
  #[cfg(unix)]
  {
    // SAFETY: time(NULL) is well-defined; negative means error → 0.
    let t = unsafe { libc::time(core::ptr::null_mut()) };
    if t < 0 {
      0
    } else {
      u64::try_from(t).unwrap_or(0)
    }
  }
  #[cfg(windows)]
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
  #[cfg(not(any(unix, windows)))]
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
