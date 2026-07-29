//! Differential parse: barehttp [`Response::parse`] vs httparse (headers/status).
//!
//! httparse parses headers only; it does not enforce RFC 9112 framing
//! (TE+CL, chunked-final, HTTP/1.0 TE, body length). When we reject and httparse
//! accepts the header section, classify as `IntentionalStrictness`.
//!
//! Outcomes:
//! - `BothAccept` / `BothReject`
//! - `IntentionalStrictness`: we reject, they accept (allowlisted)
//! - `IntentionalLeniency`: we accept, they reject (allowlisted; e.g. some CTLs)
//! - `PotentialBug`: unexpected divergence (must be empty on this corpus)

use barehttp::Response;
use httparse::{EMPTY_HEADER, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
  BothAccept,
  BothReject,
  IntentionalStrictness,
  IntentionalLeniency,
  PotentialBug,
}

struct Case {
  name: &'static str,
  data: &'static [u8],
  /// Expected classification when outcomes diverge in a known way.
  expect: Option<Outcome>,
}

fn httparse_accepts(data: &[u8]) -> bool {
  let mut headers = [EMPTY_HEADER; 64];
  let mut resp = httparse::Response::new(&mut headers);
  matches!(resp.parse(data), Ok(Status::Complete(_)))
}

fn classify(
  ours_ok: bool,
  theirs_ok: bool,
) -> Outcome {
  match (ours_ok, theirs_ok) {
    (true, true) => Outcome::BothAccept,
    (false, false) => Outcome::BothReject,
    (false, true) => Outcome::IntentionalStrictness,
    (true, false) => Outcome::IntentionalLeniency,
  }
}

/// Corpus of messages covering accept / reject / intentional diffs.
fn corpus() -> Vec<Case> {
  vec![
    Case {
      name: "normal_200_cl",
      data: b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello",
      expect: Some(Outcome::BothAccept),
    },
    Case {
      name: "chunked",
      data: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      expect: Some(Outcome::BothAccept),
    },
    Case {
      name: "http10_cl",
      data: b"HTTP/1.0 200 OK\r\nContent-Length: 3\r\n\r\nfoo",
      expect: Some(Outcome::BothAccept),
    },
    Case {
      name: "empty_body_cl0",
      data: b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n",
      expect: Some(Outcome::BothAccept),
    },
    Case {
      name: "multi_host_response",
      data: b"HTTP/1.1 200 OK\r\nHost: a\r\nHost: b\r\nContent-Length: 0\r\n\r\n",
      expect: Some(Outcome::BothAccept),
    },
    Case {
      name: "lf_only_framing",
      data: b"HTTP/1.1 200 OK\nContent-Length: 0\n\n",
      expect: Some(Outcome::BothAccept),
    },
    // Both reject obs-fold / bare CR / bad names (httparse also rejects).
    Case {
      name: "obs_fold",
      data: b"HTTP/1.1 200 OK\r\nX-Fold: line1\r\n continued\r\n\r\n",
      expect: Some(Outcome::BothReject),
    },
    Case {
      name: "bare_cr_in_value",
      data: b"HTTP/1.1 200 OK\r\nX-Header: val\rue\r\n\r\n",
      expect: Some(Outcome::BothReject),
    },
    Case {
      name: "ws_before_header_name",
      data: b"HTTP/1.1 200 OK\r\n Content-Type: text/html\r\n\r\n",
      expect: Some(Outcome::BothReject),
    },
    Case {
      name: "bad_header_name_space",
      data: b"HTTP/1.1 200 OK\r\nBad Name: value\r\n\r\n",
      expect: Some(Outcome::BothReject),
    },
    // IntentionalStrictness: we enforce RFC framing; httparse only parses headers.
    Case {
      name: "te_plus_cl",
      data: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      expect: Some(Outcome::IntentionalStrictness),
    },
    Case {
      name: "conflicting_content_lengths",
      data: b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 10\r\n\r\nHelloWorld",
      expect: Some(Outcome::IntentionalStrictness),
    },
    Case {
      name: "chunked_not_final",
      data: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked, gzip\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      expect: Some(Outcome::IntentionalStrictness),
    },
    Case {
      name: "http10_with_te",
      data: b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      expect: Some(Outcome::IntentionalStrictness),
    },
    Case {
      name: "extra_data_after_chunked",
      data: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
      expect: Some(Outcome::IntentionalStrictness),
    },
    Case {
      name: "huge_advertised_cl_short_body",
      data: b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999\r\n\r\n",
      expect: Some(Outcome::IntentionalStrictness),
    },
    // IntentionalLeniency allowlist: we accept some header CTLs httparse rejects.
    // RFC 9110 forbids most CTLs; tightening later need not reclassify these as bugs.
    Case {
      name: "null_byte_in_header_value",
      data: b"HTTP/1.1 200 OK\r\nX-Header: value\x00injected\r\nContent-Length: 0\r\n\r\n",
      expect: Some(Outcome::IntentionalLeniency),
    },
    Case {
      name: "vertical_tab_in_header_value",
      data: b"HTTP/1.1 200 OK\r\nX-Header:\x0Bvalue\r\nContent-Length: 0\r\n\r\n",
      expect: Some(Outcome::IntentionalLeniency),
    },
  ]
}

#[test]
fn differential_corpus_no_potential_bugs() {
  let mut bugs = Vec::new();
  let mut intentional_strict = Vec::new();
  let mut intentional_lenient = Vec::new();

  for case in corpus() {
    let ours_ok = Response::parse(case.data).is_ok();
    let theirs_ok = httparse_accepts(case.data);
    let got = classify(ours_ok, theirs_ok);

    // Unexpected divergence classes become PotentialBug.
    let effective = match (got, case.expect) {
      (Outcome::BothAccept, Some(Outcome::BothAccept) | None)
      | (Outcome::BothReject, Some(Outcome::BothReject) | None) => got,
      (Outcome::IntentionalStrictness, Some(Outcome::IntentionalStrictness)) => {
        intentional_strict.push(case.name);
        got
      },
      (Outcome::IntentionalLeniency, Some(Outcome::IntentionalLeniency)) => {
        intentional_lenient.push(case.name);
        got
      },
      (actual, expected) if expected.is_some() && expected != Some(actual) => {
        bugs.push(format!(
          "{}: expected {:?}, got {:?} (ours_ok={ours_ok}, theirs_ok={theirs_ok})",
          case.name,
          expected.unwrap(),
          actual
        ));
        Outcome::PotentialBug
      },
      (Outcome::IntentionalStrictness | Outcome::IntentionalLeniency, None) => {
        bugs.push(format!(
          "{}: unlisted divergence {:?} (ours_ok={ours_ok}, theirs_ok={theirs_ok})",
          case.name, got
        ));
        Outcome::PotentialBug
      },
      (other, _) => other,
    };

    if effective == Outcome::PotentialBug && bugs.last().is_none_or(|b| !b.starts_with(case.name)) {
      bugs.push(format!(
        "{}: PotentialBug ours_ok={ours_ok} theirs_ok={theirs_ok}",
        case.name
      ));
    }

    // On BothAccept, status codes must agree when httparse exposes one.
    if got == Outcome::BothAccept {
      let ours = Response::parse(case.data).expect("ours");
      let mut headers = [EMPTY_HEADER; 64];
      let mut resp = httparse::Response::new(&mut headers);
      let _ = resp.parse(case.data).expect("theirs");
      assert_eq!(
        ours.status_code(),
        resp.code.expect("httparse status"),
        "status mismatch on {}",
        case.name
      );
    }
  }

  eprintln!("IntentionalStrictness: {intentional_strict:?}");
  eprintln!("IntentionalLeniency: {intentional_lenient:?}");
  assert!(
    bugs.is_empty(),
    "PotentialBug / mismatched expectations:\n{}",
    bugs.join("\n")
  );
  assert!(
    !intentional_strict.is_empty(),
    "corpus should list IntentionalStrictness cases"
  );
}

#[test]
fn intentional_strictness_cases_are_explicit() {
  let listed: Vec<_> = corpus()
    .into_iter()
    .filter(|c| c.expect == Some(Outcome::IntentionalStrictness))
    .map(|c| c.name)
    .collect();
  assert!(listed.contains(&"te_plus_cl"));
  assert!(listed.contains(&"conflicting_content_lengths"));
  assert!(listed.contains(&"chunked_not_final"));
  assert!(listed.contains(&"http10_with_te"));
  assert!(!listed.contains(&"obs_fold"));
}
