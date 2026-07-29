//! RFC / regression corpus driver: loads manifests under `tests/corpus/` and runs
//! `Response::parse`, `Uri::parse`, and (with `gzip`) `decompress_gzip`.
//!
//! Add new fixtures + a `cases.json` entry when fuzz finds a keepable regression.

use barehttp::{ParseError, Response, Uri};
use std::fs;
#[cfg(feature = "gzip")]
use std::path::Path;
use std::path::PathBuf;

fn corpus_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Minimal extractor for `"key": "value"` string fields inside a JSON object blob.
fn json_str_field<'a>(
  obj: &'a str,
  key: &str,
) -> Option<&'a str> {
  let needle = format!("\"{key}\"");
  let idx = obj.find(&needle)?;
  let after = &obj[idx + needle.len()..];
  let colon = after.find(':')?;
  let rest = after[colon + 1..].trim_start();
  if !rest.starts_with('"') {
    return None;
  }
  let body = &rest[1..];
  let end = body.find('"')?;
  Some(&body[..end])
}

fn json_u64_field(
  obj: &str,
  key: &str,
) -> Option<u64> {
  let needle = format!("\"{key}\"");
  let idx = obj.find(&needle)?;
  let after = &obj[idx + needle.len()..];
  let colon = after.find(':')?;
  let rest = after[colon + 1..].trim_start();
  let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
  num.parse().ok()
}

fn iter_json_objects(text: &str) -> Vec<&str> {
  let mut out = Vec::new();
  let mut depth = 0i32;
  let mut start = None;
  for (i, ch) in text.char_indices() {
    match ch {
      '{' => {
        if depth == 0 {
          start = Some(i);
        }
        depth += 1;
      },
      '}' => {
        depth -= 1;
        if depth == 0
          && let Some(s) = start.take()
        {
          out.push(&text[s..=i]);
        }
      },
      _ => {},
    }
  }
  out
}

fn error_name(err: ParseError) -> &'static str {
  match err {
    ParseError::InvalidHttpVersion => "InvalidHttpVersion",
    ParseError::InvalidStatusCode => "InvalidStatusCode",
    ParseError::InvalidReasonPhrase => "InvalidReasonPhrase",
    ParseError::InvalidHeaderName => "InvalidHeaderName",
    ParseError::InvalidHeaderValue => "InvalidHeaderValue",
    ParseError::InvalidUri => "InvalidUri",
    ParseError::MissingCrlf => "MissingCrlf",
    ParseError::BareCarriageReturn => "BareCarriageReturn",
    ParseError::UnexpectedEndOfInput => "UnexpectedEndOfInput",
    ParseError::InvalidWhitespace => "InvalidWhitespace",
    ParseError::InvalidChunkSize => "InvalidChunkSize",
    ParseError::InvalidContentLength => "InvalidContentLength",
    ParseError::ConflictingFraming => "ConflictingFraming",
    ParseError::ChunkedNotFinal => "ChunkedNotFinal",
    ParseError::WhitespaceBeforeHeaders => "WhitespaceBeforeHeaders",
    ParseError::ExtraDataAfterResponse => "ExtraDataAfterResponse",
    ParseError::MissingHostHeader => "MissingHostHeader",
    ParseError::ObsoleteFoldInHeader => "ObsoleteFoldInHeader",
    ParseError::InvalidTransferEncodingForStatus => "InvalidTransferEncodingForStatus",
    ParseError::ChunkedInTeHeader => "ChunkedInTeHeader",
    ParseError::TeHeaderMissingConnection => "TeHeaderMissingConnection",
    ParseError::MultipleHostHeaders => "MultipleHostHeaders",
    ParseError::InvalidHostHeaderValue => "InvalidHostHeaderValue",
    ParseError::TransferEncodingRequiresHttp11 => "TransferEncodingRequiresHttp11",
    ParseError::ChunkedAppliedMultipleTimes => "ChunkedAppliedMultipleTimes",
    ParseError::RequestTransferEncodingUnsupported => "RequestTransferEncodingUnsupported",
    ParseError::Decompression(_) => "Decompression",
    ParseError::BodyExceedsLimit(_) => "BodyExceedsLimit",
    _ => "UnknownParseError",
  }
}

#[test]
fn corpus_http_response_parse() {
  let root = corpus_root().join("http");
  let manifest = fs::read_to_string(root.join("cases.json")).expect("http cases.json");
  for obj in iter_json_objects(&manifest) {
    let file = json_str_field(obj, "file").expect("file");
    let expect = json_str_field(obj, "expect").expect("expect");
    let path = root.join(file);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let result = Response::parse(&bytes);
    match expect {
      "accept" => {
        assert!(result.is_ok(), "expected accept for {file}: {:?}", result.err());
      },
      "reject" => {
        let err = result.expect_err(&format!("expected reject for {file}"));
        if let Some(want) = json_str_field(obj, "error") {
          assert_eq!(error_name(err), want, "wrong error for {file}: got {err:?}");
        }
      },
      other => panic!("unknown expect={other} in {file}"),
    }
  }
}

#[test]
fn corpus_uri_parse() {
  let root = corpus_root().join("uri");
  let manifest = fs::read_to_string(root.join("cases.json")).expect("uri cases.json");
  for obj in iter_json_objects(&manifest) {
    let file = json_str_field(obj, "file").expect("file");
    let line_no = json_u64_field(obj, "line").expect("line") as usize;
    let expect = json_str_field(obj, "expect").expect("expect");
    let text = fs::read_to_string(root.join(file)).unwrap();
    let uri_line = text
      .lines()
      .nth(line_no.saturating_sub(1))
      .unwrap_or_else(|| panic!("missing line {line_no} in {file}"));
    let result = Uri::parse(uri_line);
    match expect {
      "accept" => {
        assert!(result.is_ok(), "expected accept for {uri_line:?}: {:?}", result.err());
      },
      "reject" => {
        let err = result.expect_err(&format!("expected reject for {uri_line:?}"));
        if let Some(want) = json_str_field(obj, "error") {
          assert_eq!(error_name(err), want, "wrong error for {uri_line:?}: got {err:?}");
        }
      },
      other => panic!("unknown expect={other}"),
    }
  }
}

#[cfg(feature = "gzip")]
#[test]
fn corpus_gzip_decompress() {
  use barehttp::DecompressError;
  use barehttp::gzip::decompress_gzip;

  let root = corpus_root().join("gzip");
  let manifest = fs::read_to_string(root.join("cases.json")).expect("gzip cases.json");
  for obj in iter_json_objects(&manifest) {
    let file = json_str_field(obj, "file").expect("file");
    let expect = json_str_field(obj, "expect").expect("expect");
    let path = resolve_rel(&root, file);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let result = decompress_gzip(&bytes, 1 << 20);
    match expect {
      "accept" => {
        let out = result.unwrap_or_else(|e| panic!("gzip accept {}: {e:?}", path.display()));
        if let Some(plain) = json_str_field(obj, "plain") {
          let want = fs::read(resolve_rel(&root, plain)).expect("plain");
          assert_eq!(out, want, "mismatch for {}", path.display());
        }
      },
      "reject" => {
        let err = result.expect_err(&format!("expected reject for {}", path.display()));
        if let Some(want) = json_str_field(obj, "error") {
          let name = match err {
            DecompressError::InvalidInput => "InvalidInput",
            DecompressError::LimitExceeded => "LimitExceeded",
            _ => "UnknownDecompressError",
          };
          assert_eq!(name, want);
        }
      },
      other => panic!("unknown expect={other}"),
    }
  }
}

#[cfg(feature = "gzip")]
fn resolve_rel(
  base: &Path,
  rel: &str,
) -> PathBuf {
  let mut p = base.to_path_buf();
  for part in rel.split('/') {
    if part == ".." {
      p.pop();
    } else if part != "." && !part.is_empty() {
      p.push(part);
    }
  }
  p
}
