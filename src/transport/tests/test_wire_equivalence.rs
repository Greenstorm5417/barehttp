//! The live receive path (scratch head buffer, in-place chunked decode, arena
//! tail bodies) must agree with the buffered [`Response::parse`] on every
//! well-formed message, under arbitrary TCP fragmentation.

use crate::method::Method;
use crate::parser::Response;
use crate::transport::connection::{Connection, SCRATCH_LEN};
use crate::transport::tests::mock_socket::MockSocket;
use alloc::format;
use alloc::vec::Vec;

struct Rng(u64);
impl Rng {
  fn next(&mut self) -> u64 {
    self.0 ^= self.0 << 13;
    self.0 ^= self.0 >> 7;
    self.0 ^= self.0 << 17;
    self.0
  }
  fn below(
    &mut self,
    n: usize,
  ) -> usize {
    usize::try_from(self.next() % (n.max(1) as u64)).unwrap()
  }
}

/// Well-formed message; `until_close` bodies only end at EOF.
fn message(rng: &mut Rng) -> Vec<u8> {
  let mut m = format!("HTTP/1.1 {} OK\r\n", [200, 201, 404, 500][rng.below(4)]).into_bytes();
  // Header count spans inline (<= 8), stack-buffered, and heap-overflow lists;
  // occasionally a head larger than the connection scratch (spill path).
  let fields = [0, 1, 7, 8, 9, 31, 33, 60][rng.below(8)];
  for i in 0..fields {
    m.extend_from_slice(format!("X-F{i}: v{}{}\r\n", i, "p".repeat(rng.below(12))).as_bytes());
  }
  if rng.below(10) == 0 {
    m.extend_from_slice(format!("X-Big: {}\r\n", "b".repeat(SCRATCH_LEN + rng.below(3000))).as_bytes());
  }
  let body: Vec<u8> = (0..[0, 1, 5, 100, 5000, 70_000][rng.below(6)])
    .map(|i| u8::try_from((i * 7 + 3) % 251).unwrap())
    .collect();
  match rng.below(3) {
    0 => {
      m.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
      m.extend_from_slice(&body);
    },
    1 => {
      m.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
      let mut rest = body.as_slice();
      while !rest.is_empty() {
        let n = (1 + rng.below(9000)).min(rest.len());
        m.extend_from_slice(format!("{n:x}\r\n").as_bytes());
        m.extend_from_slice(&rest[..n]);
        m.extend_from_slice(b"\r\n");
        rest = &rest[n..];
      }
      m.extend_from_slice(b"0\r\n");
      if rng.below(2) == 0 {
        m.extend_from_slice(b"X-Trailer: t\r\n");
      }
      m.extend_from_slice(b"\r\n");
    },
    _ => {
      m.extend_from_slice(b"Connection: close\r\n\r\n");
      m.extend_from_slice(&body);
    },
  }
  m
}

#[test]
fn connection_matches_buffered_parse_under_fragmentation() {
  let mut rng = Rng(0x5DEE_CE66_D1CE_4E5B);
  for _ in 0..400 {
    let wire = message(&mut rng);
    let want = Response::parse(&wire).unwrap();
    let sizes: Vec<usize> = (0..64)
      .map(|_| [1, 2, 3, 7, 64, 1000, 4096, 9000][rng.below(8)])
      .collect();
    let mut socket = MockSocket::with_read_sizes(&wire, &sizes);
    let mut conn = Connection::new(&mut socket, 64 * 1024, usize::MAX);
    let raw = conn.read_raw_response(true).unwrap();
    let got = crate::client::raw_to_response(raw, &Method::Get, usize::MAX).unwrap();
    assert_eq!(got.status_code(), want.status_code());
    assert_eq!(got.headers(), want.headers());
    assert_eq!(got.body(), want.body(), "body mismatch ({} bytes)", want.body().len());
    assert_eq!(got.trailers(), want.trailers());
    assert_eq!(got, want);
  }
}
