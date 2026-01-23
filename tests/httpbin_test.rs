// Integration tests against httpbin.org endpoints
// These test the HTTP client against a real HTTP service
#![cfg(test)]

use barehttp::config::{Config, HttpStatusHandling, RedirectPolicy};
use barehttp::response::ResponseExt;
use barehttp::{HttpClient, delete, get, post, put};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

// ============================================================================
// HTTP Methods
// ============================================================================

#[test]
fn test_httpbin_get() {
  let result = get(&format!("{}/get", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"url\""));
}

#[test]
fn test_httpbin_post() {
  let result = post(&format!("{}/post", httpbin_url()), b"test data".to_vec());
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"data\""));
}

#[test]
fn test_httpbin_put() {
  let result = put(&format!("{}/put", httpbin_url()), b"test data".to_vec());
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"data\""));
}

#[test]
fn test_httpbin_delete() {
  let result = delete(&format!("{}/delete", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"url\""));
}

#[test]
fn test_httpbin_patch() {
  use barehttp::Request;
  let result = Request::patch(format!("{}/patch", httpbin_url()))
    .body(b"test data".to_vec())
    .send();
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

// ============================================================================
// Status Codes
// ============================================================================

#[test]
fn test_httpbin_status_200() {
  let result = get(&format!("{}/status/200", httpbin_url()));
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 200);
}

#[test]
fn test_httpbin_status_404() {
  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/404", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 404);
}

#[test]
fn test_httpbin_status_500() {
  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/500", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 500);
}

#[test]
fn test_httpbin_status_201() {
  let result = get(&format!("{}/status/201", httpbin_url()));
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 201);
}

#[test]
fn test_httpbin_status_204() {
  let result = get(&format!("{}/status/204", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 204);
  assert!(response.body().is_empty());
}

#[test]
fn test_httpbin_status_301() {
  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/301", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 301);
}

#[test]
fn test_httpbin_status_302() {
  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/302", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 302);
}

#[test]
fn test_httpbin_status_400() {
  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/400", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 400);
}

#[test]
fn test_httpbin_status_401() {
  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/401", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 401);
}

#[test]
fn test_httpbin_status_403() {
  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/status/403", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().status_code, 403);
}

// ============================================================================
// Request Inspection
// ============================================================================

#[test]
fn test_httpbin_headers() {
  let result = get(&format!("{}/headers", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"headers\""));
}

#[test]
fn test_httpbin_ip() {
  let result = get(&format!("{}/ip", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"origin\""));
}

#[test]
fn test_httpbin_user_agent() {
  let result = get(&format!("{}/user-agent", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"user-agent\""));
}

// ============================================================================
// Response Formats
// ============================================================================

#[test]
fn test_httpbin_json() {
  let result = get(&format!("{}/json", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let content_type = response.get_header("content-type").unwrap_or("");
  assert!(content_type.contains("application/json"));
}

#[test]
fn test_httpbin_html() {
  let result = get(&format!("{}/html", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("<html>"));
}

#[test]
fn test_httpbin_xml() {
  let result = get(&format!("{}/xml", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("<?xml"));
}

#[test]
fn test_httpbin_robots_txt() {
  let result = get(&format!("{}/robots.txt", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("User-agent:"));
}

#[test]
fn test_httpbin_deny() {
  let result = get(&format!("{}/deny", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_encoding_utf8() {
  let result = get(&format!("{}/encoding/utf8", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

// ============================================================================
// Dynamic Data
// ============================================================================

#[test]
fn test_httpbin_uuid() {
  let result = get(&format!("{}/uuid", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"uuid\""));
}

#[test]
fn test_httpbin_base64_decode() {
  let encoded = "SFRUUEJJTiBpcyBhd2Vzb21l"; // "HTTPBIN is awesome"
  let result = get(&format!("{}/base64/{}", httpbin_url(), encoded));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_bytes() {
  let result = get(&format!("{}/bytes/100", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.body().len(), 100);
}

#[test]
fn test_httpbin_delay_1() {
  let result = get(&format!("{}/delay/1", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_delay_2() {
  let result = get(&format!("{}/delay/2", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_stream_5() {
  let result = get(&format!("{}/stream/5", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_links() {
  let result = get(&format!("{}/links/5/0", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("<a href"));
}

// ============================================================================
// Redirects
// ============================================================================

#[test]
fn test_httpbin_redirect_1() {
  let result = get(&format!("{}/redirect/1", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_redirect_3() {
  let result = get(&format!("{}/redirect/3", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_redirect_5() {
  let result = get(&format!("{}/redirect/5", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_relative_redirect() {
  let result = get(&format!("{}/relative-redirect/2", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_absolute_redirect() {
  let result = get(&format!("{}/absolute-redirect/2", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_redirect_to() {
  let result = get(&format!("{}/redirect-to?url=http://httpbin.org/get", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

// ============================================================================
// Cookies
// ============================================================================

#[test]
fn test_httpbin_cookies() {
  let result = get(&format!("{}/cookies", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"cookies\""));
}

#[test]
fn test_httpbin_cookies_set() {
  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/cookies/set?test=value", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 302);
}

#[test]
fn test_httpbin_cookies_set_name_value() {
  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config).unwrap();

  use barehttp::Request;
  let result = Request::get(format!("{}/cookies/set/session/abc123", httpbin_url())).send_with(&mut client);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 302);
}

// ============================================================================
// Images
// ============================================================================

#[test]
fn test_httpbin_image_png() {
  let result = get(&format!("{}/image/png", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let content_type = response.get_header("content-type").unwrap_or("");
  assert!(content_type.contains("image/png"));
}

#[test]
fn test_httpbin_image_jpeg() {
  let result = get(&format!("{}/image/jpeg", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let content_type = response.get_header("content-type").unwrap_or("");
  assert!(content_type.contains("image/jpeg"));
}

#[test]
fn test_httpbin_image_svg() {
  let result = get(&format!("{}/image/svg", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let content_type = response.get_header("content-type").unwrap_or("");
  assert!(content_type.contains("image/svg"));
}

#[test]
fn test_httpbin_image_webp() {
  let result = get(&format!("{}/image/webp", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let content_type = response.get_header("content-type").unwrap_or("");
  assert!(content_type.contains("image/webp"));
}

// ============================================================================
// Anything Endpoints
// ============================================================================

#[test]
fn test_httpbin_anything_get() {
  let result = get(&format!("{}/anything", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"method\": \"GET\""));
}

#[test]
fn test_httpbin_anything_post() {
  let result = post(&format!("{}/anything", httpbin_url()), b"test".to_vec());
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"method\": \"POST\""));
}

#[test]
fn test_httpbin_anything_put() {
  let result = put(&format!("{}/anything", httpbin_url()), b"test".to_vec());
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"method\": \"PUT\""));
}

#[test]
fn test_httpbin_anything_delete() {
  let result = delete(&format!("{}/anything", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("\"method\": \"DELETE\""));
}

#[test]
fn test_httpbin_anything_with_path() {
  let result = get(&format!("{}/anything/foo/bar", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert!(response.text().unwrap().contains("/anything/foo/bar"));
}

// ============================================================================
// Decompression
// ============================================================================

#[test]
#[cfg(feature = "gzip-decompression")]
fn test_httpbin_gzip_decompression() {
  let result = get(&format!("{}/gzip", httpbin_url()));
  if let Err(ref e) = result {
    eprintln!("Error: {:?}", e);
  }
  assert!(result.is_ok(), "Request failed: {:?}", result.err());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);

  let body_text = response.text().unwrap();
  assert!(body_text.contains("\"gzipped\": true"));
  assert!(body_text.contains("\"method\": \"GET\""));
}

#[test]
#[cfg(feature = "gzip-decompression")]
fn test_httpbin_deflate_decompression() {
  let result = get(&format!("{}/deflate", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);

  let body_text = response.text().unwrap();
  assert!(body_text.contains("\"deflated\": true"));
  assert!(body_text.contains("\"method\": \"GET\""));
}

#[test]
#[cfg(feature = "gzip-decompression")]
fn test_httpbin_gzip_with_client() {
  let client = HttpClient::new().unwrap();
  let response = client
    .get(format!("{}/gzip", httpbin_url()))
    .call()
    .unwrap();

  assert_eq!(response.status_code, 200);
  let body_text = response.text().unwrap();
  assert!(body_text.contains("\"gzipped\": true"));
}

#[test]
#[cfg(feature = "gzip-decompression")]
fn test_accept_encoding_header_automatically_sent() {
  let client = HttpClient::new().unwrap();
  let response = client
    .get(format!("{}/headers", httpbin_url()))
    .call()
    .unwrap();

  assert_eq!(response.status_code, 200);
  let body_text = response.text().unwrap();

  #[cfg(all(feature = "gzip-decompression", feature = "zstd-decompression"))]
  assert!(body_text.contains("\"Accept-Encoding\": \"gzip, deflate, zstd\""));

  #[cfg(all(feature = "gzip-decompression", not(feature = "zstd-decompression")))]
  assert!(body_text.contains("\"Accept-Encoding\": \"gzip, deflate\""));

  #[cfg(all(not(feature = "gzip-decompression"), feature = "zstd-decompression"))]
  assert!(body_text.contains("\"Accept-Encoding\": \"zstd\""));
}

#[test]
#[cfg(feature = "gzip-decompression")]
fn test_manual_accept_encoding_not_overridden() {
  use barehttp::Request;

  let mut client = HttpClient::new().unwrap();
  let response = Request::get(format!("{}/headers", httpbin_url()))
    .header("Accept-Encoding", "custom")
    .send_with(&mut client)
    .unwrap();

  assert_eq!(response.status_code, 200);
  let body_text = response.text().unwrap();
  assert!(body_text.contains("\"Accept-Encoding\": \"custom\""));
}

// ============================================================================
// Response Inspection
// ============================================================================

#[test]
fn test_httpbin_cache() {
  let result = get(&format!("{}/cache", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_httpbin_cache_value() {
  let result = get(&format!("{}/cache/60", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let cache_control = response.get_header("cache-control");
  assert!(cache_control.is_some());
}

#[test]
fn test_httpbin_etag() {
  let result = get(&format!("{}/etag/test-etag", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  let etag = response.get_header("etag");
  assert!(etag.is_some());
}

#[test]
fn test_httpbin_response_headers() {
  let result = get(&format!("{}/response-headers?X-Custom=test", httpbin_url()));
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}
