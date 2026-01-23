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
fn test_scheme_ftp() {
  let uri = Uri::parse("ftp://ftp.example.com").unwrap();
  assert_eq!(uri.scheme(), "ftp");
}

#[test]
fn test_scheme_with_plus() {
  let uri = Uri::parse("git+ssh://example.com").unwrap();
  assert_eq!(uri.scheme(), "git+ssh");
}

#[test]
fn test_scheme_with_dash() {
  let uri = Uri::parse("my-scheme://example.com").unwrap();
  assert_eq!(uri.scheme(), "my-scheme");
}

#[test]
fn test_scheme_with_dot() {
  let uri = Uri::parse("my.scheme://example.com").unwrap();
  assert_eq!(uri.scheme(), "my.scheme");
}

#[test]
fn test_scheme_with_numbers() {
  let uri = Uri::parse("h2c://example.com").unwrap();
  assert_eq!(uri.scheme(), "h2c");
}

#[test]
fn test_scheme_case_insensitive() {
  let uri = Uri::parse("HTTP://example.com").unwrap();
  assert_eq!(uri.scheme(), "HTTP");
}

#[test]
fn test_scheme_urn() {
  let uri = Uri::parse("urn:example:animal:ferret:nose").unwrap();
  assert_eq!(uri.scheme(), "urn");
}

#[test]
fn test_scheme_must_start_with_letter() {
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
fn test_authority_no_authority() {
  let uri = Uri::parse("urn:example:animal").unwrap();
  assert!(uri.authority().is_none());
}

#[test]
fn test_authority_localhost() {
  let uri = Uri::parse("http://localhost").unwrap();
  let auth = uri.authority().unwrap();
  assert!(matches!(auth.host(), Host::RegName(name) if name == &"localhost"));
}

#[test]
fn test_host_ipv4_basic() {
  let uri = Uri::parse("http://192.168.1.1").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(addr, &[192, 168, 1, 1]);
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv4_with_port() {
  let uri = Uri::parse("http://10.0.0.1:80").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(addr, &[10, 0, 0, 1]);
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
    assert_eq!(addr, &[0, 0, 0, 0]);
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv4_max() {
  let uri = Uri::parse("http://255.255.255.255").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V4(addr)) = auth.host() {
    assert_eq!(addr, &[255, 255, 255, 255]);
  } else {
    panic!("Expected IPv4 address");
  }
}

#[test]
fn test_host_ipv6_loopback() {
  let uri = Uri::parse("http://[::1]").unwrap();
  let auth = uri.authority().unwrap();
  if let Host::IpAddr(IpAddr::V6(addr)) = auth.host() {
    assert_eq!(addr, &[0, 0, 0, 0, 0, 0, 0, 1]);
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
fn test_path_absolute_no_authority() {
  let uri = Uri::parse("file:///path/to/file").unwrap();
  assert_eq!(uri.path(), "/path/to/file");
}

#[test]
fn test_path_rootless() {
  let uri = Uri::parse("urn:example:animal:ferret:nose").unwrap();
  assert_eq!(uri.path(), "example:animal:ferret:nose");
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
fn test_rfc3986_example_ftp() {
  let uri = Uri::parse("ftp://ftp.is.co.za/rfc/rfc1808.txt").unwrap();
  assert_eq!(uri.scheme(), "ftp");
  assert_eq!(uri.path(), "/rfc/rfc1808.txt");
}

#[test]
fn test_rfc3986_example_http() {
  let uri = Uri::parse("http://www.ietf.org/rfc/rfc2396.txt").unwrap();
  assert_eq!(uri.scheme(), "http");
  assert_eq!(uri.path(), "/rfc/rfc2396.txt");
}

#[test]
fn test_rfc3986_example_ldap() {
  let uri = Uri::parse("ldap://[2001:db8::7]/c=GB?objectClass?one").unwrap();
  assert_eq!(uri.scheme(), "ldap");
  assert_eq!(uri.path(), "/c=GB");
}

#[test]
fn test_rfc3986_example_mailto() {
  let uri = Uri::parse("mailto:John.Doe@example.com").unwrap();
  assert_eq!(uri.scheme(), "mailto");
  assert_eq!(uri.path(), "John.Doe@example.com");
}

#[test]
fn test_rfc3986_example_news() {
  let uri = Uri::parse("news:comp.infosystems.www.servers.unix").unwrap();
  assert_eq!(uri.scheme(), "news");
  assert_eq!(uri.path(), "comp.infosystems.www.servers.unix");
}

#[test]
fn test_rfc3986_example_tel() {
  let uri = Uri::parse("tel:+1-816-555-1212").unwrap();
  assert_eq!(uri.scheme(), "tel");
  assert_eq!(uri.path(), "+1-816-555-1212");
}

#[test]
fn test_rfc3986_example_telnet() {
  let uri = Uri::parse("telnet://192.0.2.16:80/").unwrap();
  assert_eq!(uri.scheme(), "telnet");
  let auth = uri.authority().unwrap();
  assert_eq!(auth.port(), Some(80));
  assert_eq!(uri.path(), "/");
}

#[test]
fn test_rfc3986_example_urn() {
  let uri = Uri::parse("urn:oasis:names:specification:docbook:dtd:xml:4.1.2").unwrap();
  assert_eq!(uri.scheme(), "urn");
  assert_eq!(uri.path(), "oasis:names:specification:docbook:dtd:xml:4.1.2");
}

#[test]
fn test_rfc3986_example_with_fragment() {
  let uri = Uri::parse("http://www.ics.uci.edu/pub/ietf/uri/#Related").unwrap();
  assert_eq!(uri.scheme(), "http");
  assert_eq!(uri.path(), "/pub/ietf/uri/");
}

#[test]
fn test_rfc3986_complex_example() {
  let uri = Uri::parse("foo://example.com:8042/over/there?name=ferret#nose").unwrap();
  assert_eq!(uri.scheme(), "foo");
  let auth = uri.authority().unwrap();
  if let Host::RegName(name) = auth.host() {
    assert_eq!(name, &"example.com");
  }
  assert_eq!(auth.port(), Some(8042));
  assert_eq!(uri.path(), "/over/there");
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
