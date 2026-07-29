//! Integration-level metamorphic / proptest properties (std available).

use barehttp::ParseError;
use barehttp::Response;
use barehttp::Uri;
use proptest::prelude::*;

#[test]
fn property_response_parse_rejects_junk_after_cl() {
  proptest!(|(
    body in prop::collection::vec(any::<u8>().prop_filter("printable", |b| (32..127).contains(b)), 0..32),
    junk in prop::collection::vec(any::<u8>(), 1..12)
  )| {
    let mut msg = format!(
      "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
      body.len()
    )
    .into_bytes();
    msg.extend_from_slice(&body);
    prop_assert!(Response::parse(&msg).is_ok());
    msg.extend_from_slice(&junk);
    prop_assert_eq!(
      Response::parse(&msg),
      Err(ParseError::ExtraDataAfterResponse)
    );
  });
}

#[test]
fn property_uri_reject_spaces() {
  proptest!(|(left in "[A-Za-z0-9]{1,6}", right in "[A-Za-z0-9]{1,6}")| {
    let uri = format!("http://example.com/{left} {right}");
    prop_assert!(matches!(Uri::parse(&uri), Err(ParseError::InvalidUri)));
  });
}

#[cfg(feature = "gzip")]
#[test]
fn property_gzip_compress_decompress_roundtrip() {
  use barehttp::gzip::decompress_gzip;
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;

  proptest!(|(payload in prop::collection::vec(any::<u8>(), 0..200))| {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&payload).unwrap();
    let gz = enc.finish().unwrap();
    let out = decompress_gzip(&gz, payload.len().saturating_add(128)).expect("decompress");
    prop_assert_eq!(out.as_slice(), payload.as_slice());
  });
}
