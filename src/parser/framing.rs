/// Buffer already contains a complete header section (`\r\n\r\n` or LF-only `\n\n`).
#[inline]
pub fn has_complete_headers(data: &[u8]) -> bool {
  data.windows(4).any(|w| w == b"\r\n\r\n") || data.windows(2).any(|w| w == b"\n\n")
}
