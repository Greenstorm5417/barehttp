use crate::parser::cookie::{SetCookie, parse_cookie_date, serialize_cookie_header};
use alloc::string::ToString;

#[test]
fn parse_simple_cookie() {
  let result = SetCookie::parse("SID=31d4d96e407aad42");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.value, "31d4d96e407aad42");
  assert!(!cookie.secure);
  assert!(!cookie.http_only);
  assert!(cookie.domain.is_none());
  assert!(cookie.path.is_none());
}

#[test]
fn parse_cookie_with_path_and_domain() {
  let result = SetCookie::parse("SID=31d4d96e407aad42; Path=/; Domain=example.com");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.value, "31d4d96e407aad42");
  assert_eq!(cookie.path, Some("/".to_string()));
  assert_eq!(cookie.domain, Some("example.com".to_string()));
}

#[test]
fn parse_cookie_with_secure_httponly() {
  let result = SetCookie::parse("SID=31d4d96e407aad42; Path=/; Secure; HttpOnly");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.value, "31d4d96e407aad42");
  assert!(cookie.secure);
  assert!(cookie.http_only);
}

#[test]
fn parse_cookie_with_expires() {
  let result = SetCookie::parse("lang=en-US; Expires=Wed, 09 Jun 2021 10:18:14 GMT");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "lang");
  assert_eq!(cookie.value, "en-US");
  assert!(cookie.expires.is_some());

  let expires = cookie.expires.unwrap();
  assert_eq!(expires.year, 2021);
  assert_eq!(expires.month, 6);
  assert_eq!(expires.day, 9);
  assert_eq!(expires.hour, 10);
  assert_eq!(expires.minute, 18);
  assert_eq!(expires.second, 14);
}

#[test]
fn parse_cookie_with_max_age() {
  let result = SetCookie::parse("id=123; Max-Age=3600");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "id");
  assert_eq!(cookie.value, "123");
  assert_eq!(cookie.max_age, Some(3600));
}

#[test]
fn parse_cookie_with_negative_max_age() {
  let result = SetCookie::parse("id=123; Max-Age=-1");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.max_age, Some(-1));
}

#[test]
fn parse_cookie_strips_whitespace() {
  let result = SetCookie::parse("  name  =  value  ");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "name");
  assert_eq!(cookie.value, "value");
}

#[test]
fn parse_cookie_with_quoted_value() {
  let result = SetCookie::parse("name=\"quoted value\"");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "name");
  assert_eq!(cookie.value, "\"quoted value\"");
}

#[test]
fn parse_cookie_rejects_empty_name() {
  let result = SetCookie::parse("=value");
  assert!(result.is_none());
}

#[test]
fn parse_cookie_rejects_missing_equals() {
  let result = SetCookie::parse("namevalue");
  assert!(result.is_none());
}

#[test]
fn parse_cookie_domain_strips_leading_dot() {
  let result = SetCookie::parse("id=123; Domain=.example.com");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.domain, Some("example.com".to_string()));
}

#[test]
fn parse_cookie_domain_converts_to_lowercase() {
  let result = SetCookie::parse("id=123; Domain=EXAMPLE.COM");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.domain, Some("example.com".to_string()));
}

#[test]
fn parse_cookie_ignores_empty_domain() {
  let result = SetCookie::parse("id=123; Domain=");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert!(cookie.domain.is_none());
}

#[test]
fn parse_cookie_path_requires_leading_slash() {
  let result = SetCookie::parse("id=123; Path=no-slash");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert!(cookie.path.is_none());
}

#[test]
fn parse_cookie_path_accepts_leading_slash() {
  let result = SetCookie::parse("id=123; Path=/valid");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.path, Some("/valid".to_string()));
}

#[test]
fn parse_cookie_ignores_unrecognized_attributes() {
  let result = SetCookie::parse("id=123; Unknown=value; AnotherOne=test");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "id");
  assert_eq!(cookie.value, "123");
}

#[test]
fn parse_cookie_last_attribute_wins() {
  let result = SetCookie::parse("id=123; Path=/first; Path=/second");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.path, Some("/second".to_string()));
}

#[test]
fn parse_cookie_date_rfc1123_format() {
  let date = parse_cookie_date("Wed, 09 Jun 2021 10:18:14 GMT");
  assert!(date.is_some());

  let d = date.unwrap();
  assert_eq!(d.year, 2021);
  assert_eq!(d.month, 6);
  assert_eq!(d.day, 9);
  assert_eq!(d.hour, 10);
  assert_eq!(d.minute, 18);
  assert_eq!(d.second, 14);
}

#[test]
fn parse_cookie_date_flexible_format() {
  let date = parse_cookie_date("09-Jun-2021 10:18:14");
  assert!(date.is_some());

  let d = date.unwrap();
  assert_eq!(d.year, 2021);
  assert_eq!(d.month, 6);
  assert_eq!(d.day, 9);
}

#[test]
fn parse_cookie_date_two_digit_year_70_99() {
  let date = parse_cookie_date("09 Jun 95 10:18:14");
  assert!(date.is_some());

  let d = date.unwrap();
  assert_eq!(d.year, 1995);
}

#[test]
fn parse_cookie_date_two_digit_year_00_69() {
  let date = parse_cookie_date("09 Jun 25 10:18:14");
  assert!(date.is_some());

  let d = date.unwrap();
  assert_eq!(d.year, 2025);
}

#[test]
fn parse_cookie_date_rejects_invalid_day() {
  let date1 = parse_cookie_date("32 Jun 2021 10:18:14");
  assert!(date1.is_none());

  let date2 = parse_cookie_date("0 Jun 2021 10:18:14");
  assert!(date2.is_none());
}

#[test]
fn parse_cookie_date_rejects_invalid_year() {
  let date = parse_cookie_date("09 Jun 1600 10:18:14");
  assert!(date.is_none());
}

#[test]
fn parse_cookie_date_rejects_invalid_time() {
  let date1 = parse_cookie_date("09 Jun 2021 24:18:14");
  assert!(date1.is_none());

  let date2 = parse_cookie_date("09 Jun 2021 10:60:14");
  assert!(date2.is_none());

  let date3 = parse_cookie_date("09 Jun 2021 10:18:60");
  assert!(date3.is_none());
}

#[test]
fn parse_cookie_date_rejects_missing_components() {
  assert!(parse_cookie_date("09 Jun 2021").is_none());
  assert!(parse_cookie_date("10:18:14").is_none());
  assert!(parse_cookie_date("09 2021 10:18:14").is_none());
  assert!(parse_cookie_date("Jun 2021 10:18:14").is_none());
}

#[test]
fn parse_cookie_date_all_months() {
  let months = [
    ("Jan", 1),
    ("Feb", 2),
    ("Mar", 3),
    ("Apr", 4),
    ("May", 5),
    ("Jun", 6),
    ("Jul", 7),
    ("Aug", 8),
    ("Sep", 9),
    ("Oct", 10),
    ("Nov", 11),
    ("Dec", 12),
  ];

  for (month_str, month_num) in months {
    let input = alloc::format!("15 {month_str} 2021 12:00:00");
    let date = parse_cookie_date(&input);
    assert!(date.is_some(), "Failed to parse {month_str}");
    assert_eq!(date.unwrap().month, month_num);
  }
}

#[test]
fn parse_cookie_date_case_insensitive_month() {
  let date1 = parse_cookie_date("09 jun 2021 10:18:14");
  let date2 = parse_cookie_date("09 JUN 2021 10:18:14");
  let date3 = parse_cookie_date("09 Jun 2021 10:18:14");

  assert!(date1.is_some());
  assert!(date2.is_some());
  assert!(date3.is_some());

  assert_eq!(date1.unwrap().month, 6);
  assert_eq!(date2.unwrap().month, 6);
  assert_eq!(date3.unwrap().month, 6);
}

#[test]
fn serialize_cookie_header_single() {
  let cookies = alloc::vec![("sessionid".to_string(), "abc123".to_string())];
  let result = serialize_cookie_header(&cookies);
  assert_eq!(result, "sessionid=abc123");
}

#[test]
fn serialize_cookie_header_multiple() {
  let cookies = alloc::vec![
    ("SID".to_string(), "31d4d96e407aad42".to_string()),
    ("lang".to_string(), "en-US".to_string()),
  ];
  let result = serialize_cookie_header(&cookies);
  assert_eq!(result, "SID=31d4d96e407aad42; lang=en-US");
}

#[test]
fn serialize_cookie_header_empty() {
  let cookies = alloc::vec![];
  let result = serialize_cookie_header(&cookies);
  assert_eq!(result, "");
}

#[test]
fn rfc6265_example_1() {
  let result = SetCookie::parse("SID=31d4d96e407aad42");
  assert!(result.is_some());
  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.value, "31d4d96e407aad42");
}

#[test]
fn rfc6265_example_2() {
  let result = SetCookie::parse("SID=31d4d96e407aad42; Path=/; Domain=example.com");
  assert!(result.is_some());
  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.path, Some("/".to_string()));
  assert_eq!(cookie.domain, Some("example.com".to_string()));
}

#[test]
fn rfc6265_example_3() {
  let result = SetCookie::parse("SID=31d4d96e407aad42; Path=/; Secure; HttpOnly");
  assert!(result.is_some());
  let cookie = result.unwrap();
  assert_eq!(cookie.name, "SID");
  assert!(cookie.secure);
  assert!(cookie.http_only);
}

#[test]
fn rfc6265_example_4() {
  let result = SetCookie::parse("lang=en-US; Path=/; Domain=example.com");
  assert!(result.is_some());
  let cookie = result.unwrap();
  assert_eq!(cookie.name, "lang");
  assert_eq!(cookie.value, "en-US");
}

#[test]
fn rfc6265_example_5() {
  let result = SetCookie::parse("lang=en-US; Expires=Wed, 09 Jun 2021 10:18:14 GMT");
  assert!(result.is_some());
  let cookie = result.unwrap();
  assert!(cookie.expires.is_some());
}

#[test]
fn parse_cookie_with_all_attributes() {
  let result = SetCookie::parse(
    "id=a3fWa; Expires=Wed, 21 Oct 2015 07:28:00 GMT; Secure; HttpOnly; Max-Age=2592000; Domain=example.com; Path=/docs",
  );
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "id");
  assert_eq!(cookie.value, "a3fWa");
  assert!(cookie.expires.is_some());
  assert_eq!(cookie.max_age, Some(2_592_000));
  assert_eq!(cookie.domain, Some("example.com".to_string()));
  assert_eq!(cookie.path, Some("/docs".to_string()));
  assert!(cookie.secure);
  assert!(cookie.http_only);
}

#[test]
fn parse_cookie_date_with_extra_text() {
  let date = parse_cookie_date("Wed, 09-Jun-2021 10:18:14 GMT");
  assert!(date.is_some());

  let d = date.unwrap();
  assert_eq!(d.day, 9);
  assert_eq!(d.month, 6);
  assert_eq!(d.year, 2021);
}

#[test]
fn parse_max_age_zero() {
  let result = SetCookie::parse("id=123; Max-Age=0");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.max_age, Some(0));
}

#[test]
fn parse_max_age_invalid_non_digit() {
  let result = SetCookie::parse("id=123; Max-Age=abc");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert!(cookie.max_age.is_none());
}

#[test]
fn parse_max_age_invalid_mixed() {
  let result = SetCookie::parse("id=123; Max-Age=123abc");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert!(cookie.max_age.is_none());
}

#[test]
fn cookie_attributes_case_insensitive() {
  let result = SetCookie::parse("id=123; PATH=/; DOMAIN=example.com; SECURE; HTTPONLY");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.path, Some("/".to_string()));
  assert_eq!(cookie.domain, Some("example.com".to_string()));
  assert!(cookie.secure);
  assert!(cookie.http_only);
}

#[test]
fn parse_cookie_empty_value() {
  let result = SetCookie::parse("name=");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "name");
  assert_eq!(cookie.value, "");
}

#[test]
fn parse_cookie_value_with_equals() {
  let result = SetCookie::parse("data=key=value");
  assert!(result.is_some());

  let cookie = result.unwrap();
  assert_eq!(cookie.name, "data");
  assert_eq!(cookie.value, "key=value");
}
