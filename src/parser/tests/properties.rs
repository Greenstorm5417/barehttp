//! Metamorphic / property-based parser tests (bounded sizes).

use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::Response;
use crate::parser::chunked::{ChunkedDecoder, FeedResult};
use crate::parser::uri::Uri;
use alloc::format;
use alloc::vec::Vec;

#[test]
fn response_parse_idempotent_on_valid_cl() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-A: b\r\n\r\nHello";
  let a = Response::parse(input).unwrap();
  let b = Response::parse(input).unwrap();
  assert_eq!(a.status_code(), b.status_code());
  assert_eq!(a.body(), b.body());
  assert_eq!(a.header("x-a"), b.header("X-A"));
}

#[test]
fn metamorphic_extra_junk_after_cl_is_extra_data() {
  let base = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nping";
  assert!(Response::parse(base).is_ok());
  let mut junked = base.to_vec();
  junked.extend_from_slice(b"TRAILER-JUNK");
  assert_eq!(
    Response::parse(&junked).unwrap_err(),
    ParseError::ExtraDataAfterResponse
  );
}

#[test]
fn uri_parse_rejects_empty_and_spaces() {
  assert!(matches!(Uri::parse(""), Err(ParseError::InvalidUri)));
  assert!(matches!(
    Uri::parse("http://example.com/has space"),
    Err(ParseError::InvalidUri)
  ));
  assert!(matches!(Uri::parse("   "), Err(ParseError::InvalidUri)));
}

#[test]
fn chunked_feed_byte_at_a_time_matches_full() {
  let wire = b"5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
  let mut full_out = Vec::new();
  let mut full = ChunkedDecoder::new();
  match full.feed(wire, Some(&mut full_out)).unwrap() {
    FeedResult::Done { rest } => assert!(rest.is_empty()),
    FeedResult::NeedMore { .. } => panic!("full feed incomplete"),
  }

  let mut inc = ChunkedDecoder::new();
  let mut inc_out = Vec::new();
  let mut held = Vec::new();
  let mut done = false;
  for &b in wire {
    held.push(b);
    match inc.feed(&held, Some(&mut inc_out)).unwrap() {
      FeedResult::NeedMore { consumed } => {
        let _ = held.drain(..consumed);
      },
      FeedResult::Done { rest } => {
        assert!(rest.is_empty());
        done = true;
        break;
      },
    }
  }
  assert!(done, "incremental feed never completed");
  assert_eq!(inc_out, full_out);
  assert_eq!(inc_out, b"Hello World");
}

#[test]
fn chunked_two_fragment_feed_matches_full() {
  let wire = b"4\r\nping\r\n0\r\n\r\n";
  let mut want = Vec::new();
  let mut d = ChunkedDecoder::new();
  assert!(matches!(
    d.feed(wire, Some(&mut want)).unwrap(),
    FeedResult::Done { rest } if rest.is_empty()
  ));

  for i in 1..wire.len() {
    let mut dec = ChunkedDecoder::new();
    let mut out = Vec::new();
    let mut held = wire[..i].to_vec();
    match dec.feed(&held, Some(&mut out)).unwrap() {
      FeedResult::NeedMore { consumed } => {
        let _ = held.drain(..consumed);
        held.extend_from_slice(&wire[i..]);
        match dec.feed(&held, Some(&mut out)).unwrap() {
          FeedResult::Done { rest } => assert!(rest.is_empty()),
          FeedResult::NeedMore { .. } => panic!("split {i} still needs more"),
        }
      },
      FeedResult::Done { .. } => panic!("first fragment alone should be incomplete at {i}"),
    }
    assert_eq!(out, want, "split at {i}");
  }
}

#[test]
fn property_header_lookup_case_insensitive() {
  use proptest::prelude::*;
  proptest::proptest!(|(
    name in "[A-Za-z][A-Za-z0-9-]{0,16}",
    value in "[ -~]{0,24}",
    upper in any::<bool>()
  )| {
    let mut h = Headers::new();
    h.insert(name.clone(), value.clone());
    let probe = if upper {
      name.to_ascii_uppercase()
    } else {
      name.to_ascii_lowercase()
    };
    prop_assert_eq!(h.get(&probe), Some(value.as_str()));
  });
}

#[test]
fn property_uri_reject_empty_or_spaces() {
  use proptest::prelude::*;
  proptest::proptest!(|(s in " {0,8}")| {
    prop_assert!(Uri::parse(&s).is_err());
  });
  proptest::proptest!(|(path in "[A-Za-z]{1,8} [A-Za-z]{1,8}")| {
    let uri = format!("http://example.com/{path}");
    prop_assert!(Uri::parse(&uri).is_err());
  });
}

#[test]
fn property_valid_cl_response_roundtrip_status_body() {
  use proptest::prelude::*;
  proptest::proptest!(|(
    code in (200u16..600).prop_filter("entity body ok", |c| !matches!(*c, 204 | 304)),
    body in "[ -~]{0,40}"
  )| {
    let msg = format!(
      "HTTP/1.1 {code} OK\r\nContent-Length: {}\r\n\r\n{body}",
      body.len()
    );
    let parsed = Response::parse(msg.as_bytes()).expect("parse");
    prop_assert_eq!(parsed.status_code(), code);
    prop_assert_eq!(parsed.body(), body.as_bytes());
    let again = Response::parse(msg.as_bytes()).unwrap();
    prop_assert_eq!(again.status_code(), parsed.status_code());
    prop_assert_eq!(again.body(), parsed.body());
  });
}

#[test]
fn property_metamorphic_junk_after_complete_cl() {
  use proptest::prelude::*;
  proptest::proptest!(|(
    body in prop::collection::vec(
      any::<u8>().prop_filter("printable", |b| (32..127).contains(b)),
      0..24
    ),
    junk in prop::collection::vec(any::<u8>(), 1..16)
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
      Response::parse(&msg).unwrap_err(),
      ParseError::ExtraDataAfterResponse
    );
  });
}

/// Fixed regression: empty CL body + junk.
#[test]
fn regression_empty_body_extra_data() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\nX";
  assert_eq!(Response::parse(input).unwrap_err(), ParseError::ExtraDataAfterResponse);
}
