/// Check if buffer contains complete HTTP headers (`\r\n\r\n` or LF-only `\n\n`).
#[inline]
pub fn has_complete_headers(data: &[u8]) -> bool {
  data.windows(4).any(|w| w == b"\r\n\r\n") || data.windows(2).any(|w| w == b"\n\n")
}

/// Heuristic: does `data` look like a complete chunked body?
///
/// RFC 9112 Section 7.1: last chunk is size 0, then optional trailers, then blank line.
/// Minimal forms: `0\r\n\r\n` / `0\n\n` (`ChunkedDecoder` accepts LF-only).
pub fn has_chunked_terminator(data: &[u8]) -> bool {
  // ponytail: Connection read-stop only — ChunkedDecoder is authoritative.
  // End-anchored so mid-body `0\r\n\r\n` / `0\n\n` in chunk data does not stop early.
  // Ceiling: chunk data ending with `\n0\r\n...\r\n\r\n` can still false-positive;
  // drive ChunkedDecoder in the read loop if that bites.
  if data.ends_with(b"0\r\n\r\n") || data.ends_with(b"0\n\n") {
    return true;
  }
  // Trailers after last chunk: ...0\r\n<field>\r\n...\r\n\r\n or LF-only equivalent
  if data.len() >= 5 && data.ends_with(b"\r\n\r\n") {
    if data.starts_with(b"0\r\n") {
      return true;
    }
    return data.windows(4).any(|w| w == b"\n0\r\n");
  }
  if data.len() >= 3 && data.ends_with(b"\n\n") {
    if data.starts_with(b"0\n") {
      return true;
    }
    return data.windows(3).any(|w| w == b"\n0\n");
  }
  false
}
