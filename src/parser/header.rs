#![allow(missing_docs)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeaderName;

impl HeaderName {
  pub const ACCEPT: &'static str = "accept";
  pub const ACCEPT_CHARSET: &'static str = "accept-charset";
  pub const ACCEPT_ENCODING: &'static str = "accept-encoding";
  pub const ACCEPT_LANGUAGE: &'static str = "accept-language";
  pub const ACCEPT_RANGES: &'static str = "accept-ranges";
  pub const ALLOW: &'static str = "allow";
  pub const AUTHENTICATION_INFO: &'static str = "authentication-info";
  pub const AUTHORIZATION: &'static str = "authorization";
  pub const CONNECTION: &'static str = "connection";
  pub const CONTENT_ENCODING: &'static str = "content-encoding";
  pub const CONTENT_LANGUAGE: &'static str = "content-language";
  pub const CONTENT_LENGTH: &'static str = "content-length";
  pub const CONTENT_LOCATION: &'static str = "content-location";
  pub const CONTENT_RANGE: &'static str = "content-range";
  pub const CONTENT_TYPE: &'static str = "content-type";
  pub const DATE: &'static str = "date";
  pub const ETAG: &'static str = "etag";
  pub const EXPECT: &'static str = "expect";
  pub const FROM: &'static str = "from";
  pub const HOST: &'static str = "host";
  pub const IF_MATCH: &'static str = "if-match";
  pub const IF_MODIFIED_SINCE: &'static str = "if-modified-since";
  pub const IF_NONE_MATCH: &'static str = "if-none-match";
  pub const IF_RANGE: &'static str = "if-range";
  pub const IF_UNMODIFIED_SINCE: &'static str = "if-unmodified-since";
  pub const LAST_MODIFIED: &'static str = "last-modified";
  pub const LOCATION: &'static str = "location";
  pub const MAX_FORWARDS: &'static str = "max-forwards";
  pub const PROXY_AUTHENTICATE: &'static str = "proxy-authenticate";
  pub const PROXY_AUTHENTICATION_INFO: &'static str = "proxy-authentication-info";
  pub const PROXY_AUTHORIZATION: &'static str = "proxy-authorization";
  pub const RANGE: &'static str = "range";
  pub const REFERER: &'static str = "referer";
  pub const RETRY_AFTER: &'static str = "retry-after";
  pub const SERVER: &'static str = "server";
  pub const TE: &'static str = "te";
  pub const TRAILER: &'static str = "trailer";
  pub const UPGRADE: &'static str = "upgrade";
  pub const USER_AGENT: &'static str = "user-agent";
  pub const VARY: &'static str = "vary";
  pub const VIA: &'static str = "via";
  pub const WWW_AUTHENTICATE: &'static str = "www-authenticate";
}
