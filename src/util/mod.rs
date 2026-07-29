/// Percent-encode a string; leaves RFC 3986 unreserved (`A-Z` `a-z` `0-9` `-` `_` `.` `~`) alone.
#[must_use]
pub fn percent_encode(input: &str) -> alloc::string::String {
  use alloc::string::String;
  use core::fmt::Write;

  let mut result = String::new();
  for byte in input.bytes() {
    match byte {
      b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        result.push(byte as char);
      },
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
