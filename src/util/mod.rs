/// Network utilities
pub mod network;

pub use network::IpAddr;

/// Percent-encode a string for use in URLs
///
/// Encodes all characters except unreserved characters (A-Z, a-z, 0-9, -, _, ., ~).
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
