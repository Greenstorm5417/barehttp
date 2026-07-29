/// HTTP request method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
  /// Wire token (`"GET"`, `"POST"`, …).
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
  pub const fn need_request_body(self) -> bool {
    matches!(self, Self::Post | Self::Put | Self::Patch)
  }
}
