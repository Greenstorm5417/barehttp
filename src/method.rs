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
  /// PATCH method - partial modification
  Patch,
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
      Self::Patch => "PATCH",
    }
  }
}
