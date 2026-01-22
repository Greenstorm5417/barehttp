use crate::parser::headers::HeaderField;
extern crate alloc;
use alloc::vec::Vec;

#[test]
fn test_basic_header_field() {
  let input = b"Content-Type: text/html\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  assert!(field.is_some());
  let field = field.unwrap();
  assert_eq!(field.name, b"Content-Type");
  assert_eq!(field.value, b"text/html");
}

#[test]
fn test_header_with_leading_whitespace() {
  let input = b"Content-Length:   123\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"Content-Length");
  assert_eq!(field.value, b"123");
}

#[test]
fn test_header_with_trailing_whitespace() {
  let input = b"Host: example.com   \r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"Host");
  assert_eq!(field.value, b"example.com");
}

#[test]
fn test_header_with_tab_whitespace() {
  let input = b"Accept:\t\ttext/plain\t\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"Accept");
  assert_eq!(field.value, b"text/plain");
}

#[test]
fn test_header_no_whitespace() {
  let input = b"Connection:close\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"Connection");
  assert_eq!(field.value, b"close");
}

#[test]
fn test_header_empty_value() {
  let input = b"X-Custom-Header:\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"X-Custom-Header");
  assert_eq!(field.value, b"");
}

#[test]
fn test_header_empty_value_with_spaces() {
  let input = b"X-Empty:   \r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.value, b"");
}

#[test]
fn test_header_whitespace_before_colon_rejected() {
  let input = b"Content-Type : text/html\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_field_name_case_preserved() {
  let input = b"CoNtEnT-TyPe: application/json\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.name, b"CoNtEnT-TyPe");
}

#[test]
fn test_header_missing_colon_rejected() {
  let input = b"InvalidHeader\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_empty_name_rejected() {
  let input = b": value\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_with_special_chars_in_value() {
  let input = b"Set-Cookie: session=abc123; Path=/; HttpOnly\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.value, b"session=abc123; Path=/; HttpOnly");
}

#[test]
fn test_end_of_headers_crlf() {
  let input = b"\r\nBody data";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, rest) = result.unwrap();
  assert!(field.is_none());
  assert_eq!(rest, b"Body data");
}

#[test]
fn test_end_of_headers_lf() {
  let input = b"\nBody data";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, rest) = result.unwrap();
  assert!(field.is_none());
  assert_eq!(rest, b"Body data");
}

#[test]
fn test_header_value_with_quoted_string() {
  let input = b"Content-Disposition: attachment; filename=\"test.txt\"\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.value, b"attachment; filename=\"test.txt\"");
}

#[test]
fn test_header_invalid_char_in_name() {
  let input = b"Invalid@Header: value\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_multiple_colons_in_value() {
  let input = b"Date: Mon, 01 Jan 2024 12:00:00 GMT\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.value, b"Mon, 01 Jan 2024 12:00:00 GMT");
}

#[test]
fn test_header_with_comma_separated_values() {
  let input = b"Accept-Encoding: gzip, deflate, br\r\n";
  let result = HeaderField::parse(input);
  assert!(result.is_ok());
  let (field, _) = result.unwrap();
  let field = field.unwrap();
  assert_eq!(field.value, b"gzip, deflate, br");
}

#[test]
fn test_very_long_header_value() {
  let mut input = Vec::from(&b"X-Long-Header: "[..]);
  input.extend_from_slice(&[b'a'; 8000]);
  input.extend_from_slice(b"\r\n");
  let result = HeaderField::parse(&input);
  assert!(result.is_ok());
}
