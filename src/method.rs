use compact_str::CompactString;
use core::fmt;
use core::str::FromStr;

/// Owned RFC 9110 extension-method token (opaque; not a public `CompactString`).
///
/// Construct via [`Method::new`] / [`str::parse`]. Useful when matching
/// [`Method::Extension`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtensionMethod {
  token: CompactString,
}

impl ExtensionMethod {
  /// Wire token bytes as `&str`.
  #[must_use]
  pub fn as_str(&self) -> &str {
    self.token.as_str()
  }
}

impl AsRef<str> for ExtensionMethod {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl fmt::Display for ExtensionMethod {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

/// HTTP request method (RFC 9110 method token).
///
/// Standard methods are unit variants. Any other valid `token` is
/// [`Method::Extension`]. The enum is `#[non_exhaustive]` so further registered
/// methods can be added without a major bump.
///
/// # Examples
///
/// ```
/// use barehttp::Method;
/// use core::str::FromStr;
///
/// assert_eq!(Method::Get.as_str(), "GET");
/// assert_eq!(Method::from_str("POST")?, Method::Post);
/// assert_eq!(Method::from_str("OPTIONS")?, Method::Options);
/// let custom = Method::new("PURGE")?;
/// assert!(matches!(custom, Method::Extension(_)));
/// assert_eq!(custom.as_str(), "PURGE");
/// # Ok::<(), barehttp::ParseMethodError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Method {
  /// `GET`
  #[default]
  Get,
  /// `POST`
  Post,
  /// `PUT`
  Put,
  /// `DELETE`
  Delete,
  /// `HEAD`
  Head,
  /// `PATCH`
  Patch,
  /// `OPTIONS`
  Options,
  /// `CONNECT` (parsed / matchable; execution rejects until tunnel support exists).
  ///
  /// RFC 9112 §3.2.3 requires authority-form for CONNECT; §9.3.6 requires ignoring
  /// `Content-Length` / `Transfer-Encoding` on a successful response. This crate has no
  /// tunneled-socket API yet, so builders/clients return
  /// [`crate::InvalidRequest::ConnectUnsupported`] instead of sending a mis-framed request.
  Connect,
  /// `TRACE`
  Trace,
  /// Extension / custom method token (RFC 9110 `token`).
  Extension(ExtensionMethod),
}

impl Method {
  /// Parse a method token (`impl AsRef<str>` at the boundary).
  ///
  /// Recognizes the standard methods by exact case-sensitive match; any other
  /// valid RFC 9110 `token` becomes [`Method::Extension`].
  ///
  /// # Errors
  /// [`ParseMethodError::InvalidToken`] when empty or not a `tchar` token.
  pub fn new(token: impl AsRef<str>) -> Result<Self, ParseMethodError> {
    parse_method_token(token.as_ref())
  }

  /// Wire token (`"GET"`, `"POST"`, …, or the extension token).
  #[must_use]
  pub fn as_str(&self) -> &str {
    match self {
      Self::Get => "GET",
      Self::Post => "POST",
      Self::Put => "PUT",
      Self::Delete => "DELETE",
      Self::Head => "HEAD",
      Self::Patch => "PATCH",
      Self::Options => "OPTIONS",
      Self::Connect => "CONNECT",
      Self::Trace => "TRACE",
      Self::Extension(ext) => ext.as_str(),
    }
  }

  /// Whether the method is expected to carry a request body (POST, PUT, PATCH).
  #[must_use]
  pub const fn needs_request_body(&self) -> bool {
    matches!(self, Self::Post | Self::Put | Self::Patch)
  }
}

impl fmt::Display for Method {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl AsRef<str> for Method {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

/// Failure from [`Method::new`] / [`Method::from_str`] / [`str::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseMethodError {
  /// Empty or contains octets outside RFC 9110 `tchar`.
  InvalidToken,
}

impl fmt::Display for ParseMethodError {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    match self {
      Self::InvalidToken => f.write_str("invalid HTTP method token"),
    }
  }
}

impl core::error::Error for ParseMethodError {}

impl FromStr for Method {
  type Err = ParseMethodError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    parse_method_token(s)
  }
}

fn parse_method_token(s: &str) -> Result<Method, ParseMethodError> {
  if s.is_empty() || !s.bytes().all(is_tchar) {
    return Err(ParseMethodError::InvalidToken);
  }
  Ok(match s {
    "GET" => Method::Get,
    "POST" => Method::Post,
    "PUT" => Method::Put,
    "DELETE" => Method::Delete,
    "HEAD" => Method::Head,
    "PATCH" => Method::Patch,
    "OPTIONS" => Method::Options,
    "CONNECT" => Method::Connect,
    "TRACE" => Method::Trace,
    other => Method::Extension(ExtensionMethod {
      token: CompactString::from(other),
    }),
  })
}

/// RFC 9110 `tchar`.
const fn is_tchar(b: u8) -> bool {
  matches!(
    b,
    b'!'
      | b'#'
      | b'$'
      | b'%'
      | b'&'
      | b'\''
      | b'*'
      | b'+'
      | b'-'
      | b'.'
      | b'^'
      | b'_'
      | b'`'
      | b'|'
      | b'~'
      | b'0'..=b'9'
      | b'A'..=b'Z'
      | b'a'..=b'z'
  )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
  use super::{ExtensionMethod, Method, ParseMethodError};
  use alloc::format;
  use core::str::FromStr;

  #[test]
  fn from_str_standard_and_extension() {
    assert_eq!(Method::from_str("GET"), Ok(Method::Get));
    assert_eq!(Method::from_str("OPTIONS"), Ok(Method::Options));
    assert_eq!(Method::from_str("CONNECT"), Ok(Method::Connect));
    assert_eq!(Method::from_str("TRACE"), Ok(Method::Trace));
    let purge = Method::from_str("PURGE").expect("token");
    assert!(matches!(purge, Method::Extension(_)));
    assert_eq!(purge.as_str(), "PURGE");
    assert_eq!(Method::new("PURGE").unwrap().as_str(), "PURGE");
  }

  #[test]
  fn from_str_rejects_invalid_token() {
    assert_eq!(Method::from_str(""), Err(ParseMethodError::InvalidToken));
    assert_eq!(Method::from_str("GET "), Err(ParseMethodError::InvalidToken));
    assert_eq!(Method::from_str("A/B"), Err(ParseMethodError::InvalidToken));
  }

  #[test]
  fn extension_display_and_as_ref() {
    let m = Method::new("FOO").unwrap();
    assert_eq!(format!("{m}"), "FOO");
    assert_eq!(m.as_ref(), "FOO");
    match m {
      Method::Extension(ext) => {
        let _: &ExtensionMethod = &ext;
        assert_eq!(ext.as_str(), "FOO");
      },
      other => panic!("expected extension, got {other:?}"),
    }
  }

  #[test]
  fn needs_request_body_only_entity_methods() {
    assert!(Method::Post.needs_request_body());
    assert!(!Method::Options.needs_request_body());
    assert!(!Method::Connect.needs_request_body());
    assert!(!Method::new("PURGE").unwrap().needs_request_body());
  }
}
