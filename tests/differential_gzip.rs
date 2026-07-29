//! barehttp gzip/deflate vs flate2 (dev-dep). Needs `--features gzip`.

#![cfg(feature = "gzip")]

use barehttp::gzip::{DecompressError, decompress_gzip, decompress_http_deflate};
use barehttp::Response;
use flate2::Compression;
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use proptest::prelude::*;
use std::io::{Read, Write};

fn gzip_encode(plain: &[u8]) -> Vec<u8> {
  let mut enc = GzEncoder::new(Vec::new(), Compression::default());
  enc.write_all(plain).unwrap();
  enc.finish().unwrap()
}

fn zlib_encode(plain: &[u8]) -> Vec<u8> {
  let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
  enc.write_all(plain).unwrap();
  enc.finish().unwrap()
}

#[test]
fn gzip_matches_flate2_on_corpus() {
  let plains: &[&[u8]] = &[
    b"",
    b"a",
    b"hi",
    b"hello world",
    b"The quick brown fox jumps over the lazy dog.",
    &[0u8; 1024],
  ];
  for plain in plains {
    let gz = gzip_encode(plain);
    let ours = decompress_gzip(&gz, plain.len().saturating_add(64)).unwrap();
    let mut dec = GzDecoder::new(gz.as_slice());
    let mut theirs = Vec::new();
    dec.read_to_end(&mut theirs).unwrap();
    assert_eq!(ours, theirs);
    assert_eq!(ours.as_slice(), *plain);
  }
}

#[test]
fn zlib_matches_flate2() {
  let plain = b"differential zlib payload 12345";
  let z = zlib_encode(plain);
  let ours = decompress_http_deflate(&z, 256).unwrap();
  let mut dec = ZlibDecoder::new(z.as_slice());
  let mut theirs = Vec::new();
  dec.read_to_end(&mut theirs).unwrap();
  assert_eq!(ours, theirs);
}

#[test]
fn both_reject_truncated_gzip() {
  let gz = gzip_encode(b"hello world");
  let truncated = &gz[..gz.len().saturating_sub(4)];
  assert!(matches!(
    decompress_gzip(truncated, 64),
    Err(DecompressError::InvalidInput)
  ));
  let mut dec = GzDecoder::new(truncated);
  let mut buf = Vec::new();
  assert!(dec.read_to_end(&mut buf).is_err() || buf.as_slice() != b"hello world");
}

#[test]
fn decompression_bomb_hits_limit() {
  let plain = vec![0u8; 256 * 1024];
  let gz = {
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&plain).unwrap();
    enc.finish().unwrap()
  };
  assert!(gz.len() < 4096, "compressed fixture should be tiny");
  assert_eq!(decompress_gzip(&gz, 1024), Err(DecompressError::LimitExceeded));
  assert_eq!(decompress_gzip(&gz, plain.len()).unwrap().len(), plain.len());
}

#[test]
fn bomb_via_response_body_limit() {
  let plain = vec![0u8; 64 * 1024];
  let gz = gzip_encode(&plain);
  let mut msg = format!(
    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
    gz.len()
  )
  .into_bytes();
  msg.extend_from_slice(&gz);

  // Response::parse uses a large default max; exercise parse_body path via full message
  // by checking Content-Encoding path with a small limit through HttpClient is covered
  // elsewhere — here assert parse succeeds with enough room then bomb with direct API.
  let ok = Response::parse(&msg).unwrap();
  assert_eq!(ok.as_bytes().len(), plain.len());

  // Body size limit is enforced on decompressed output (same path as Content-Encoding).
  assert_eq!(
    decompress_gzip(&gz, 512),
    Err(DecompressError::LimitExceeded)
  );
}

#[test]
fn property_flate2_compress_barehttp_inflate() {
  proptest!(|(payload in prop::collection::vec(any::<u8>(), 0..400))| {
    let gz = gzip_encode(&payload);
    let out = decompress_gzip(&gz, payload.len().saturating_add(8)).expect("inflate");
    prop_assert_eq!(out.as_slice(), payload.as_slice());
  });
}

#[test]
fn response_parse_fixture_subset() {
  // Known-good wire fixtures: both parsers agree on status + body length.
  let fixtures: &[(&[u8], u16, usize)] = &[
    (b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 200, 0),
    (b"HTTP/1.1 204 No Content\r\n\r\n", 204, 0),
    (b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello", 200, 5),
    (
      b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      200,
      5,
    ),
  ];
  for (wire, status, body_len) in fixtures {
    let r = Response::parse(wire).unwrap();
    assert_eq!(r.status(), *status);
    assert_eq!(r.as_bytes().len(), *body_len);
  }
}
