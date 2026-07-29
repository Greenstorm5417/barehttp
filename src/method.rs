use core::fmt;
use core::str::FromStr;

/// HTTP request method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Method {
  /// `GET`
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
}

impl Method {
  /// Wire token (`"GET"`, `"POST"`, ...).
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Get => "GET",
      Self::Post => "POST",
      Self::Put => "PUT",
      Self::Delete => "DELETE",
      Self::Head => "HEAD",
      Self::Patch => "PATCH",
    }
  }

  /// Whether the method is expected to carry a request body (POST, PUT, PATCH).
  #[must_use]
  pub const fn needs_request_body(self) -> bool {
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

impl FromStr for Method {
  type Err = ();

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "GET" => Ok(Self::Get),
      "POST" => Ok(Self::Post),
      "PUT" => Ok(Self::Put),
      "DELETE" => Ok(Self::Delete),
      "HEAD" => Ok(Self::Head),
      "PATCH" => Ok(Self::Patch),
      _ => Err(()),
    }
  }
}
