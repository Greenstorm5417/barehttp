use core::net::{Ipv4Addr, Ipv6Addr};

use crate::error::ParseError;
use crate::parser::uri::{Host, Uri};
use crate::util::IpAddr;

#[test]
fn test_scheme_basic_http() {
  let uri = Uri::parse("http://example.com").unwrap();
  assert_eq!(uri.scheme(), "http");
}

#[test]
fn test_scheme_https() {
  let uri = Uri::parse("https://example.com").unwrap();
  assert_eq!(uri.scheme(), "https");
}

#[test]
fn test_scheme_case_insensitive() {
  let uri = Uri::parse("HTTP://example.com").unwrap();
  assert_eq!(uri.scheme(), "HTTP");
}

#[test]
fn test_scheme_rejects_non_http() {
  assert!(Uri::parse("ftp://ftp.example.com").is_err());
  assert!(Uri::parse("git+ssh://example.com").is_err());
  assert!(Uri::parse("urn:example:animal:ferret:nose").is_err());
  assert!(Uri::parse("mailto:John.Doe@example.com").is_err());
  assert!(Uri::parse("123://example.com").is_err());
}

#[test]
fn test_authority_with_host_only() {
  let uri = Uri::parse("http://example.com").unwrap();
  let auth = uri.authority().unwrap();
  assert!(matches!(auth.host(), Host::RegName(_)));
  assert!(auth.port().is_none());
}

#[test]
fn test_authority_with_port() {
  let uri = Uri::parse("http://example.com:8080").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(8080));
}

#[test]
fn test_authority_empty_port() {
  let uri = Uri::parse("http://example.com:").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(0));
}

#[test]
fn test_authority_localhost() {
  let uri = Uri::parse("http://localhost").unwrap();
  let auth = uri.authority().unwrap();
  assert!(matches!(auth.host(), Host::RegName(name) if name == &"localhost"));
}

#[test]
fn test_rejects_userinfo() {
  assert!(matches!(
    Uri::parse("http://user:pass@example.com"),
    Err(ParseError::InvalidUri)
  ));
  assert!(matches!(
    Uri::parse("https://user@example.com/path"),
    Err(ParseError::InvalidUri)
  ));
}

#[test]
fn test_host_ipv4_basic() {
  let uri = Uri::parse("http://192.168.1.1").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(*addr, Ipv4Addr::new(192, 168, 1, 1));
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv4_with_port() {
  let uri = Uri::parse("http://10.0.0.1:80").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(*addr, Ipv4Addr::new(10, 0, 0, 1));
  } else {
    panic!("Expected IPv4 address");
  }
  assert_eq!(auth.port(), Some(80));
}

#[test]
fn test_host_ipv4_zeros() {
  let uri = Uri::parse("http://0.0.0.0").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(*addr, Ipv4Addr::UNSPECIFIED);
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv4_max() {
  let uri = Uri::parse("http://255.255.255.255").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(*addr, Ipv4Addr::BROADCAST);
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv6_loopback() {
  let uri = Uri::parse("http://[::1]").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V6(addr)) = auth.host() {
    assert_eq!(*addr, Ipv6Addr::LOCALHOST);
  } else {
    panic!("Expected IPv6 address");
  }
}

#[test]
fn test_host_ipv6_full() {
  let uri = Uri::parse("http://[2001:db8::7]").unwrap();
  let auth = uri.authority().unwrap();
  assert!(matches!(auth.host(), Host::IpAddr(IpAddr::V6(_))));
}

#[test]
fn test_host_ipv6_with_port() {
  let uri = Uri::parse("http://[::1]:8080").unwrap();
  let auth = uri.authority().unwrap();
  assert!(matches!(auth.host(), Host::IpAddr(IpAddr::V6(_))));
  assert_eq!(auth.port(), Some(8080));
}

#[test]
fn test_host_reg_name_simple() {
  let uri = Uri::parse("http://example.com").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"example.com");
  } else {
    panic!("Expected reg-name");
  }
}

#[test]
fn test_host_reg_name_with_dash() {
  let uri = Uri::parse("http://my-example.com").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"my-example.com");
  } else {
    panic!("Expected reg-name");
  }
}

#[test]
fn test_host_reg_name_percent_encoded() {
  let uri = Uri::parse("http://example%2Ecom").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"example%2Ecom");
  } else {
    panic!("Expected reg-name");
  }
}

#[test]
fn test_port_standard_http() {
  let uri = Uri::parse("http://example.com:80").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(80));
}

#[test]
fn test_port_standard_https() {
  let uri = Uri::parse("https://example.com:443").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(443));
}

#[test]
fn test_port_custom() {
  let uri = Uri::parse("http://example.com:8080").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(8080));
}

#[test]
fn test_port_high_number() {
  let uri = Uri::parse("http://example.com:65535").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(65535));
}

#[test]
fn test_port_zero() {
  let uri = Uri::parse("http://example.com:0").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(0));
}

#[test]
fn test_port_single_digit() {
  let uri = Uri::parse("http://example.com:8").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(8));
}

#[test]
fn test_port_multiple_digits() {
  let uri = Uri::parse("http://example.com:12345").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(12345));
}

#[test]
fn test_port_empty_defaults_to_zero() {
  let uri = Uri::parse("http://example.com:").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(0));
}

#[test]
fn test_port_overflow() {
  assert!(Uri::parse("http://example.com:99999").is_err());
}

#[test]
fn test_port_with_ipv6() {
  let uri = Uri::parse("http://[::1]:3000").unwrap();
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(3000));
}

#[test]
fn test_path_empty() {
  let uri = Uri::parse("http://example.com").unwrap();
  assert_eq!(uri.path(), "");
}

#[test]
fn test_path_root() {
  let uri = Uri::parse("http://example.com/").unwrap();
  assert_eq!(uri.path(), "/");
}

#[test]
fn test_path_simple() {
  let uri = Uri::parse("http://example.com/path").unwrap();
  assert_eq!(uri.path(), "/path");
}

#[test]
fn test_path_multiple_segments() {
  let uri = Uri::parse("http://example.com/path/to/resource").unwrap();
  assert_eq!(uri.path(), "/path/to/resource");
}

#[test]
fn test_path_with_percent_encoding() {
  let uri = Uri::parse("http://example.com/path%20with%20spaces").unwrap();
  assert_eq!(uri.path(), "/path%20with%20spaces");
}

#[test]
fn test_path_with_dot_segments() {
  let uri = Uri::parse("http://example.com/a/b/c/./../../g").unwrap();
  assert_eq!(uri.path(), "/a/b/c/./../../g");
}

#[test]
fn test_path_trailing_slash() {
  let uri = Uri::parse("http://example.com/path/").unwrap();
  assert_eq!(uri.path(), "/path/");
}

#[test]
fn test_path_special_chars() {
  let uri = Uri::parse("http://example.com/path:with@special!chars").unwrap();
  assert_eq!(uri.path(), "/path:with@special!chars");
}

#[test]
fn test_query() {
  let uri = Uri::parse("http://example.com/path?name=ferret&x=1").unwrap();
  assert_eq!(uri.path(), "/path");
  assert_eq!(uri.query(), Some("name=ferret&x=1"));
}

#[test]
fn test_http_example_with_path() {
  let uri = Uri::parse("http://www.ietf.org/rfc/rfc2396.txt").unwrap();
  assert_eq!(uri.scheme(), "http");
  assert_eq!(uri.path(), "/rfc/rfc2396.txt");
}

#[test]
fn test_rejects_fragment() {
  assert!(matches!(
    Uri::parse("http://www.ics.uci.edu/pub/ietf/uri/#Related"),
    Err(ParseError::InvalidUri)
  ));
}

#[test]
fn test_error_no_scheme() {
  assert!(matches!(Uri::parse("//example.com"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_error_invalid_scheme_char() {
  assert!(matches!(Uri::parse("ht_tp://example.com"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_invalid_scheme_with_multibyte_boundary() {
  assert!(matches!(Uri::parse(")5;Π5"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_error_missing_colon_after_scheme() {
  assert!(matches!(Uri::parse("http//example.com"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_error_ipv4_out_of_range() {
  let uri = Uri::parse("http://256.1.1.1").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"256.1.1.1");
  } else {
    panic!("Expected reg-name, not IPv4");
  }
}

#[test]
fn test_error_ipv4_invalid_format() {
  let uri = Uri::parse("http://192.168.1").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"192.168.1");
  } else {
    panic!("Expected reg-name, not IPv4");
  }
}

#[test]
fn test_error_ipv6_missing_bracket() {
  assert!(matches!(Uri::parse("http://::1"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_error_ipv6_unclosed_bracket() {
  assert!(matches!(Uri::parse("http://[::1"), Err(ParseError::InvalidUri)));
}

#[test]
fn test_error_port_too_large() {
  assert!(matches!(
    Uri::parse("http://example.com:99999"),
    Err(ParseError::InvalidUri)
  ));
}

#[test]
fn test_error_invalid_query_char() {
  assert!(matches!(
    Uri::parse("http://example.com?query<invalid>"),
    Err(ParseError::InvalidUri)
  ));
}

#[test]
fn test_error_leftover_input() {
  assert!(matches!(
    Uri::parse("http://example.com invalid"),
    Err(ParseError::InvalidUri)
  ));
}

#[test]
fn test_resolve_path_relative_location() {
  let base = Uri::parse("http://example.com/dir/page.html").unwrap();
  assert_eq!(
    base.resolve_relative("other.html").unwrap(),
    "http://example.com/dir/other.html"
  );
}

#[test]
fn test_to_path_and_query_empty_path_with_query() {
  let uri = Uri::parse("http://example.com?a=1").unwrap();
  assert_eq!(uri.to_path_and_query(), "/?a=1");
  assert_eq!(uri.query(), Some("a=1"));
}

#[test]
fn test_resolve_network_to_path_and_query_only() {
  let base = Uri::parse("http://example.com/dir/page.html").unwrap();
  assert_eq!(base.resolve_relative("//other.com/x").unwrap(), "http://other.com/x");
  assert_eq!(
    base.resolve_relative("?next=1").unwrap(),
    "http://example.com/dir/page.html?next=1"
  );
  assert_eq!(
    base.resolve_relative("HTTPS://other.com/y").unwrap(),
    "https://other.com/y"
  );
}
