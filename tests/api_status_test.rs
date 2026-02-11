//! Integration tests for StatusCode and StatusClass

use barehttp::{StatusClass, StatusCode};

#[test]
fn test_status_code_new_valid() {
  assert!(StatusCode::new(200).is_some());
  assert!(StatusCode::new(404).is_some());
  assert!(StatusCode::new(500).is_some());
  assert!(StatusCode::new(100).is_some());
  assert!(StatusCode::new(599).is_some());
}

#[test]
fn test_status_code_new_invalid() {
  assert!(StatusCode::new(99).is_none());
  assert!(StatusCode::new(600).is_none());
  assert!(StatusCode::new(0).is_none());
  assert!(StatusCode::new(1000).is_none());
}

#[test]
fn test_status_code_as_u16() {
  let code = StatusCode::new(200).unwrap();
  assert_eq!(code.as_u16(), 200);

  let code = StatusCode::new(404).unwrap();
  assert_eq!(code.as_u16(), 404);
}

#[test]
fn test_status_code_class() {
  let code = StatusCode::new(100).unwrap();
  assert_eq!(code.class(), StatusClass::Informational);

  let code = StatusCode::new(200).unwrap();
  assert_eq!(code.class(), StatusClass::Successful);

  let code = StatusCode::new(300).unwrap();
  assert_eq!(code.class(), StatusClass::Redirection);

  let code = StatusCode::new(400).unwrap();
  assert_eq!(code.class(), StatusClass::ClientError);

  let code = StatusCode::new(500).unwrap();
  assert_eq!(code.class(), StatusClass::ServerError);
}

#[test]
fn test_status_code_reason_phrase() {
  assert_eq!(StatusCode::new(200).unwrap().reason_phrase(), "OK");
  assert_eq!(StatusCode::new(404).unwrap().reason_phrase(), "Not Found");
  assert_eq!(StatusCode::new(500).unwrap().reason_phrase(), "Internal Server Error");
  assert_eq!(StatusCode::new(301).unwrap().reason_phrase(), "Moved Permanently");
  assert_eq!(StatusCode::new(418).unwrap().reason_phrase(), "I'm a teapot");
}

#[test]
fn test_status_code_is_cacheable_by_default() {
  assert!(StatusCode::new(200).unwrap().is_cacheable_by_default());
  assert!(StatusCode::new(404).unwrap().is_cacheable_by_default());
  assert!(!StatusCode::new(201).unwrap().is_cacheable_by_default());
  assert!(!StatusCode::new(400).unwrap().is_cacheable_by_default());
}

#[test]
fn test_status_code_is_informational() {
  assert!(StatusCode::new(100).unwrap().is_informational());
  assert!(StatusCode::new(101).unwrap().is_informational());
  assert!(!StatusCode::new(200).unwrap().is_informational());
  assert!(!StatusCode::new(404).unwrap().is_informational());
}

#[test]
fn test_status_code_is_successful() {
  assert!(StatusCode::new(200).unwrap().is_successful());
  assert!(StatusCode::new(201).unwrap().is_successful());
  assert!(StatusCode::new(204).unwrap().is_successful());
  assert!(!StatusCode::new(300).unwrap().is_successful());
  assert!(!StatusCode::new(404).unwrap().is_successful());
}

#[test]
fn test_status_code_is_redirection() {
  assert!(StatusCode::new(300).unwrap().is_redirection());
  assert!(StatusCode::new(301).unwrap().is_redirection());
  assert!(StatusCode::new(302).unwrap().is_redirection());
  assert!(!StatusCode::new(200).unwrap().is_redirection());
  assert!(!StatusCode::new(404).unwrap().is_redirection());
}

#[test]
fn test_status_code_is_client_error() {
  assert!(StatusCode::new(400).unwrap().is_client_error());
  assert!(StatusCode::new(404).unwrap().is_client_error());
  assert!(StatusCode::new(403).unwrap().is_client_error());
  assert!(!StatusCode::new(200).unwrap().is_client_error());
  assert!(!StatusCode::new(500).unwrap().is_client_error());
}

#[test]
fn test_status_code_is_server_error() {
  assert!(StatusCode::new(500).unwrap().is_server_error());
  assert!(StatusCode::new(502).unwrap().is_server_error());
  assert!(StatusCode::new(503).unwrap().is_server_error());
  assert!(!StatusCode::new(200).unwrap().is_server_error());
  assert!(!StatusCode::new(404).unwrap().is_server_error());
}

#[test]
fn test_status_code_is_interim() {
  assert!(StatusCode::new(100).unwrap().is_interim());
  assert!(StatusCode::new(101).unwrap().is_interim());
  assert!(!StatusCode::new(200).unwrap().is_interim());
}

#[test]
fn test_status_code_is_final() {
  assert!(StatusCode::new(200).unwrap().is_final());
  assert!(StatusCode::new(404).unwrap().is_final());
  assert!(!StatusCode::new(100).unwrap().is_final());
}

#[test]
fn test_status_code_is_redirection_method_preserving() {
  assert!(
    StatusCode::new(307)
      .unwrap()
      .is_redirection_method_preserving()
  );
  assert!(
    StatusCode::new(308)
      .unwrap()
      .is_redirection_method_preserving()
  );
  assert!(
    !StatusCode::new(301)
      .unwrap()
      .is_redirection_method_preserving()
  );
  assert!(
    !StatusCode::new(302)
      .unwrap()
      .is_redirection_method_preserving()
  );
}

#[test]
fn test_status_code_is_redirection_suggests_get() {
  assert!(StatusCode::new(303).unwrap().is_redirection_suggests_get());
  assert!(!StatusCode::new(301).unwrap().is_redirection_suggests_get());
  assert!(!StatusCode::new(307).unwrap().is_redirection_suggests_get());
}

#[test]
fn test_status_code_constants() {
  assert_eq!(StatusCode::OK.as_u16(), 200);
  assert_eq!(StatusCode::CREATED.as_u16(), 201);
  assert_eq!(StatusCode::NO_CONTENT.as_u16(), 204);
  assert_eq!(StatusCode::MOVED_PERMANENTLY.as_u16(), 301);
  assert_eq!(StatusCode::FOUND.as_u16(), 302);
  assert_eq!(StatusCode::NOT_MODIFIED.as_u16(), 304);
  assert_eq!(StatusCode::BAD_REQUEST.as_u16(), 400);
  assert_eq!(StatusCode::UNAUTHORIZED.as_u16(), 401);
  assert_eq!(StatusCode::FORBIDDEN.as_u16(), 403);
  assert_eq!(StatusCode::NOT_FOUND.as_u16(), 404);
  assert_eq!(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), 500);
  assert_eq!(StatusCode::BAD_GATEWAY.as_u16(), 502);
  assert_eq!(StatusCode::SERVICE_UNAVAILABLE.as_u16(), 503);
}

#[test]
fn test_status_code_more_constants() {
  assert_eq!(StatusCode::CONTINUE.as_u16(), 100);
  assert_eq!(StatusCode::SWITCHING_PROTOCOLS.as_u16(), 101);
  assert_eq!(StatusCode::ACCEPTED.as_u16(), 202);
  assert_eq!(StatusCode::PARTIAL_CONTENT.as_u16(), 206);
  assert_eq!(StatusCode::SEE_OTHER.as_u16(), 303);
  assert_eq!(StatusCode::TEMPORARY_REDIRECT.as_u16(), 307);
  assert_eq!(StatusCode::PERMANENT_REDIRECT.as_u16(), 308);
  assert_eq!(StatusCode::METHOD_NOT_ALLOWED.as_u16(), 405);
  assert_eq!(StatusCode::CONFLICT.as_u16(), 409);
  assert_eq!(StatusCode::GONE.as_u16(), 410);
  assert_eq!(StatusCode::TOO_MANY_REQUESTS.as_u16(), 429);
  assert_eq!(StatusCode::NOT_IMPLEMENTED.as_u16(), 501);
  assert_eq!(StatusCode::GATEWAY_TIMEOUT.as_u16(), 504);
}

#[test]
fn test_status_code_clone() {
  let code1 = StatusCode::new(200).unwrap();
  let code2 = code1;
  assert_eq!(code1.as_u16(), code2.as_u16());
}

#[test]
fn test_status_code_copy() {
  let code1 = StatusCode::new(200).unwrap();
  let code2 = code1;
  assert_eq!(code1.as_u16(), code2.as_u16());
}

#[test]
fn test_status_code_equality() {
  let code1 = StatusCode::new(200).unwrap();
  let code2 = StatusCode::new(200).unwrap();
  let code3 = StatusCode::new(404).unwrap();

  assert_eq!(code1, code2);
  assert_ne!(code1, code3);
}

#[test]
fn test_status_code_debug() {
  let code = StatusCode::new(200).unwrap();
  let debug_str = format!("{:?}", code);
  assert!(!debug_str.is_empty());
}

#[test]
fn test_status_code_hash() {
  use std::collections::HashSet;

  let mut set = HashSet::new();
  set.insert(StatusCode::new(200).unwrap());
  set.insert(StatusCode::new(404).unwrap());
  set.insert(StatusCode::new(200).unwrap());

  assert_eq!(set.len(), 2);
}

#[test]
fn test_status_class_equality() {
  assert_eq!(StatusClass::Informational, StatusClass::Informational);
  assert_ne!(StatusClass::Successful, StatusClass::ClientError);
}

#[test]
fn test_status_class_debug() {
  let class = StatusClass::Successful;
  let debug_str = format!("{:?}", class);
  assert!(debug_str.contains("Successful"));
}

#[test]
fn test_status_class_clone() {
  let class1 = StatusClass::Successful;
  let class2 = class1;
  assert_eq!(class1, class2);
}

#[test]
fn test_status_class_copy() {
  let class1 = StatusClass::Redirection;
  let class2 = class1;
  assert_eq!(class1, class2);
}
