use crate::parser::cookie::{SetCookie, parse_cookie_date};
use alloc::string::ToString;

#[test]
fn parse_simple_cookie() {
  let cookie = SetCookie::parse("SID=31d4d96e407aad42").unwrap();
  assert_eq!(cookie.name, "SID");
  assert_eq!(cookie.value, "31d4d96e407aad42");
  assert!(!cookie.secure);
  assert!(!cookie.http_only);
  assert_eq!(cookie.same_site, crate::parser::cookie::SameSite::Default);
}

#[test]
fn parse_cookie_with_attributes() {
  let cookie = SetCookie::parse(
    "id=a3fWa; Expires=Wed, 21 Oct 2015 07:28:00 GMT; Secure; HttpOnly; Max-Age=2592000; Domain=.EXAMPLE.COM; Path=/docs",
  )
  .unwrap();
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
fn parse_cookie_edge_cases() {
  // Empty name with a value is allowed (RFC 10025 nameless cookie).
  let empty_name = SetCookie::parse("=value").unwrap();
  assert_eq!(empty_name.name, "");
  assert_eq!(empty_name.value, "value");

  // No `=`: empty name, token is the value.
  let bare = SetCookie::parse("namevalue").unwrap();
  assert_eq!(bare.name, "");
  assert_eq!(bare.value, "namevalue");

  assert!(SetCookie::parse("=").is_none());
  assert!(SetCookie::parse("").is_none());
  assert!(SetCookie::parse("id=1\nX").is_none());

  let empty_val = SetCookie::parse("name=").unwrap();
  assert_eq!(empty_val.value, "");

  let eq_in_value = SetCookie::parse("data=key=value").unwrap();
  assert_eq!(eq_in_value.value, "key=value");

  let no_slash = SetCookie::parse("id=123; Path=no-slash").unwrap();
  assert!(no_slash.path.is_none());

  let empty_domain = SetCookie::parse("id=123; Domain=").unwrap();
  assert!(empty_domain.domain.is_none());

  let last_wins = SetCookie::parse("id=123; Path=/first; Path=/second").unwrap();
  assert_eq!(last_wins.path, Some("/second".to_string()));

  let case_ins = SetCookie::parse("id=123; PATH=/; DOMAIN=example.com; SECURE; HTTPONLY").unwrap();
  assert!(case_ins.secure && case_ins.http_only);

  let neg = SetCookie::parse("id=123; Max-Age=-1").unwrap();
  assert_eq!(neg.max_age, Some(-1));

  let bad_age = SetCookie::parse("id=123; Max-Age=abc").unwrap();
  assert!(bad_age.max_age.is_none());

  let ss = SetCookie::parse("id=1; SameSite=Lax").unwrap();
  assert_eq!(ss.same_site, crate::parser::cookie::SameSite::Lax);

  let ss_none = SetCookie::parse("id=1; SameSite=None; Secure").unwrap();
  assert_eq!(ss_none.same_site, crate::parser::cookie::SameSite::None);
}

#[test]
fn parse_cookie_date_cases() {
  let d = parse_cookie_date("Wed, 09 Jun 2021 10:18:14 GMT").unwrap();
  assert_eq!(
    (d.year, d.month, d.day, d.hour, d.minute, d.second),
    (2021, 6, 9, 10, 18, 14)
  );

  assert_eq!(parse_cookie_date("09 Jun 95 10:18:14").unwrap().year, 1995);
  assert_eq!(parse_cookie_date("09 Jun 25 10:18:14").unwrap().year, 2025);
  assert!(parse_cookie_date("32 Jun 2021 10:18:14").is_none());
  assert!(parse_cookie_date("09 Jun 1600 10:18:14").is_none());
  assert!(parse_cookie_date("09 Jun 2021 24:18:14").is_none());
  assert!(parse_cookie_date("09 Jun 2021").is_none());

  for (month_str, month_num) in [
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
  ] {
    let input = alloc::format!("15 {month_str} 2021 12:00:00");
    assert_eq!(parse_cookie_date(&input).unwrap().month, month_num);
  }
}
