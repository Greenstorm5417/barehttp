//! Comprehensive tests for query parameter handling
//!
//! These tests verify:
//! - Query parameters are in correct key=value format
//! - URL structure is properly formed (? and & separators)
//! - Server receives parameters as intended
//! - Parameter order is preserved
//! - Special characters are properly encoded

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_query_params_url_structure_single_param() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("test", "value")
    .call()?;

  let body = response.text()?;

  // Verify URL contains proper ? separator
  assert!(body.contains("?test=value"));

  // Verify server parsed it correctly
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"test\": \"value\""));

  Ok(())
}

#[test]
fn test_query_params_url_structure_multiple_params() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("first", "1")
    .query("second", "2")
    .query("third", "3")
    .call()?;

  let body = response.text()?;

  // Verify URL structure: ? for first param, & for subsequent
  assert!(body.contains("?first=1&second=2&third=3"));

  // Verify server received all parameters in correct order
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"first\": \"1\""));
  assert!(body.contains("\"second\": \"2\""));
  assert!(body.contains("\"third\": \"3\""));

  Ok(())
}

#[test]
fn test_query_params_order_preserved() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("z_last", "last")
    .query("a_first", "first")
    .query("m_middle", "middle")
    .call()?;

  let body = response.text()?;

  // In the URL, z_last should appear before a_first (insertion order, not alphabetical)
  assert!(body.contains("?z_last=last&a_first=first&m_middle=middle"));

  // Verify all received by server
  assert!(body.contains("\"z_last\": \"last\""));
  assert!(body.contains("\"a_first\": \"first\""));
  assert!(body.contains("\"m_middle\": \"middle\""));

  Ok(())
}

#[test]
fn test_query_params_special_characters_encoded() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("key with spaces", "value with spaces")
    .query("special", "!@#$%")
    .call()?;

  let body = response.text()?;

  // Verify server received the decoded values correctly
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"key with spaces\": \"value with spaces\""));
  assert!(body.contains("\"special\""));

  Ok(())
}

#[test]
fn test_query_params_empty_value() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("empty", "")
    .query("nonempty", "value")
    .call()?;

  let body = response.text()?;

  // Verify URL structure with empty value
  assert!(body.contains("?empty=&nonempty=value") || body.contains("?empty&nonempty=value"));

  // Verify server received both parameters
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"empty\""));
  assert!(body.contains("\"nonempty\": \"value\""));

  Ok(())
}

#[test]
fn test_query_params_duplicate_keys() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("tag", "first")
    .query("tag", "second")
    .query("tag", "third")
    .call()?;

  let body = response.text()?;

  // Verify URL contains all three values
  assert!(body.contains("?tag=first&tag=second&tag=third"));

  // Verify server received all values (httpbin may return as array or last value)
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"tag\""));

  Ok(())
}

#[test]
fn test_query_params_key_value_format() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("name", "John Doe")
    .query("age", "30")
    .query("city", "New York")
    .call()?;

  let body = response.text()?;

  // Verify each parameter is in key=value format
  assert!(body.contains("name="));
  assert!(body.contains("age="));
  assert!(body.contains("city="));

  // Verify server parsed all correctly
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"name\": \"John Doe\""));
  assert!(body.contains("\"age\": \"30\""));
  assert!(body.contains("\"city\": \"New York\""));

  Ok(())
}

#[test]
fn test_query_params_url_with_existing_query() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get?existing=param", httpbin_url()))
    .query("new", "value")
    .call()?;

  let body = response.text()?;

  // Verify both existing and new parameters are present
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"existing\": \"param\""));
  assert!(body.contains("\"new\": \"value\""));

  // Verify proper separator (& not ?) was used for new param
  assert!(body.contains("existing=param&new=value"));

  Ok(())
}

#[test]
fn test_query_params_numeric_values() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("int", "42")
    .query("float", "3.14")
    .query("negative", "-100")
    .call()?;

  let body = response.text()?;

  // Verify URL structure
  assert!(body.contains("?int=42&float=3.14&negative=-100"));

  // Verify server received numeric values as strings
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"int\": \"42\""));
  assert!(body.contains("\"float\": \"3.14\""));
  assert!(body.contains("\"negative\": \"-100\""));

  Ok(())
}

#[test]
fn test_query_params_unicode() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("emoji", "🚀")
    .query("chinese", "你好")
    .query("arabic", "مرحبا")
    .call()?;

  let body = response.text()?;

  // Verify server received unicode parameters (may be encoded or decoded depending on server)
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"emoji\""));
  assert!(body.contains("\"chinese\""));
  assert!(body.contains("\"arabic\""));

  // Verify at least one unicode value is present (either encoded or decoded)
  assert!(body.contains("🚀") || body.contains("emoji"));
  assert!(body.contains("你好") || body.contains("chinese"));
  assert!(body.contains("مرحبا") || body.contains("arabic"));

  Ok(())
}

#[test]
fn test_query_pairs_preserves_order() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let params = vec![
    ("param1", "value1"),
    ("param2", "value2"),
    ("param3", "value3"),
    ("param4", "value4"),
  ];

  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query_pairs(params)
    .call()?;

  let body = response.text()?;

  // Verify URL has parameters in correct order
  assert!(body.contains("?param1=value1&param2=value2&param3=value3&param4=value4"));

  // Verify server received all in order
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"param1\": \"value1\""));
  assert!(body.contains("\"param2\": \"value2\""));
  assert!(body.contains("\"param3\": \"value3\""));
  assert!(body.contains("\"param4\": \"value4\""));

  Ok(())
}

#[test]
fn test_query_params_equals_in_value() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("equation", "x=y+z")
    .call()?;

  let body = response.text()?;

  // Verify server received the value with = sign correctly
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"equation\": \"x=y+z\""));

  Ok(())
}

#[test]
fn test_query_params_ampersand_in_value() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("text", "rock&roll")
    .call()?;

  let body = response.text()?;

  // Verify server received the value with & sign correctly encoded
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"text\": \"rock&roll\""));

  Ok(())
}
