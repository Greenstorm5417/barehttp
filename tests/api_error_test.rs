//! Integration tests for Error enum

use barehttp::Error;

#[test]
fn test_error_debug() {
  let error = Error::InvalidUrl;
  let debug_str = format!("{:?}", error);
  assert!(debug_str.contains("InvalidUrl"));
}

#[test]
fn test_error_variants_exist() {
  let _error1 = Error::InvalidUrl;
  let _error2 = Error::NoAddresses;
  let _error3 = Error::IpAddressNotSupported;
  let _error4 = Error::TooManyRedirects;
  let _error5 = Error::MissingRedirectLocation;
  let _error6 = Error::InvalidRedirectLocation;
  let _error7 = Error::RedirectLoop;
  let _error8 = Error::HttpStatus(404);
  let _error9 = Error::HttpsRequired;
  let _error10 = Error::ResponseHeaderTooLarge;
  let _error11 = Error::Utf8Error;
}

#[test]
fn test_error_http_status() {
  let error = Error::HttpStatus(404);
  match error {
    Error::HttpStatus(code) => assert_eq!(code, 404),
    _ => panic!("Expected HttpStatus variant"),
  }
}

#[test]
fn test_error_from_utf8_error() {
  let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
  let utf8_error = String::from_utf8(invalid_utf8).unwrap_err();
  let error: Error = utf8_error.into();

  match error {
    Error::Utf8Error => {},
    _ => panic!("Expected Utf8Error variant"),
  }
}
