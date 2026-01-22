/// HTTP request method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
  /// GET method - retrieve resource
  Get,
  /// POST method - submit data
  Post,
  /// PUT method - replace resource
  Put,
  /// DELETE method - remove resource
  Delete,
  /// HEAD method - retrieve headers only
  Head,
  /// OPTIONS method - query supported methods
  Options,
  /// PATCH method - partial modification
  Patch,
  /// TRACE method - diagnostic loopback
  Trace,
  /// CONNECT method - establish tunnel
  Connect,
}

impl Method {
  /// Returns the method as a string slice
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Get => "GET",
      Self::Post => "POST",
      Self::Put => "PUT",
      Self::Delete => "DELETE",
      Self::Head => "HEAD",
      Self::Options => "OPTIONS",
      Self::Patch => "PATCH",
      Self::Trace => "TRACE",
      Self::Connect => "CONNECT",
    }
  }

  /// Returns true if this method typically has a request body
  #[must_use]
  pub const fn has_body(self) -> bool {
    matches!(self, Self::Post | Self::Put | Self::Patch)
  }

  /// Returns true if this method should never have a request body
  #[must_use]
  pub const fn without_body(self) -> bool {
    matches!(
      self,
      Self::Get | Self::Head | Self::Options | Self::Trace | Self::Connect
    )
  }
}
