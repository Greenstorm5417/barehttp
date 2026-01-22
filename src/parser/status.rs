#![allow(missing_docs)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusClass {
  Informational,
  Successful,
  Redirection,
  ClientError,
  ServerError,
}

impl StatusCode {
  #[must_use]
  pub const fn new(code: u16) -> Option<Self> {
    if code >= 100 && code <= 599 {
      Some(Self(code))
    } else {
      None
    }
  }

  #[must_use]
  pub const fn as_u16(self) -> u16 {
    self.0
  }

  #[must_use]
  pub const fn class(self) -> StatusClass {
    match self.0 {
      100..=199 => StatusClass::Informational,
      200..=299 => StatusClass::Successful,
      300..=399 => StatusClass::Redirection,
      400..=499 => StatusClass::ClientError,
      _ => StatusClass::ServerError,
    }
  }

  #[must_use]
  pub const fn reason_phrase(self) -> &'static str {
    match self.0 {
      100 => "Continue",
      101 => "Switching Protocols",
      102 => "Processing",
      103 => "Early Hints",
      200 => "OK",
      201 => "Created",
      202 => "Accepted",
      203 => "Non-Authoritative Information",
      204 => "No Content",
      205 => "Reset Content",
      206 => "Partial Content",
      207 => "Multi-Status",
      208 => "Already Reported",
      226 => "IM Used",
      300 => "Multiple Choices",
      301 => "Moved Permanently",
      302 => "Found",
      303 => "See Other",
      304 => "Not Modified",
      305 => "Use Proxy",
      307 => "Temporary Redirect",
      308 => "Permanent Redirect",
      400 => "Bad Request",
      401 => "Unauthorized",
      402 => "Payment Required",
      403 => "Forbidden",
      404 => "Not Found",
      405 => "Method Not Allowed",
      406 => "Not Acceptable",
      407 => "Proxy Authentication Required",
      408 => "Request Timeout",
      409 => "Conflict",
      410 => "Gone",
      411 => "Length Required",
      412 => "Precondition Failed",
      413 => "Content Too Large",
      414 => "URI Too Long",
      415 => "Unsupported Media Type",
      416 => "Range Not Satisfiable",
      417 => "Expectation Failed",
      418 => "I'm a teapot",
      421 => "Misdirected Request",
      422 => "Unprocessable Content",
      423 => "Locked",
      424 => "Failed Dependency",
      425 => "Too Early",
      426 => "Upgrade Required",
      428 => "Precondition Required",
      429 => "Too Many Requests",
      431 => "Request Header Fields Too Large",
      451 => "Unavailable For Legal Reasons",
      500 => "Internal Server Error",
      501 => "Not Implemented",
      502 => "Bad Gateway",
      503 => "Service Unavailable",
      504 => "Gateway Timeout",
      505 => "HTTP Version Not Supported",
      506 => "Variant Also Negotiates",
      507 => "Insufficient Storage",
      508 => "Loop Detected",
      510 => "Not Extended",
      511 => "Network Authentication Required",
      _ => "Unknown Status Code",
    }
  }

  #[must_use]
  pub const fn is_cacheable_by_default(self) -> bool {
    matches!(
      self.0,
      200 | 203 | 204 | 206 | 300 | 301 | 304 | 308 | 404 | 405 | 410 | 414 | 501
    )
  }

  #[must_use]
  pub const fn is_informational(self) -> bool {
    matches!(self.class(), StatusClass::Informational)
  }

  #[must_use]
  pub const fn is_successful(self) -> bool {
    matches!(self.class(), StatusClass::Successful)
  }

  #[must_use]
  pub const fn is_redirection(self) -> bool {
    matches!(self.class(), StatusClass::Redirection)
  }

  #[must_use]
  pub const fn is_client_error(self) -> bool {
    matches!(self.class(), StatusClass::ClientError)
  }

  #[must_use]
  pub const fn is_server_error(self) -> bool {
    matches!(self.class(), StatusClass::ServerError)
  }

  #[must_use]
  pub const fn is_interim(self) -> bool {
    self.is_informational()
  }

  #[must_use]
  pub const fn is_final(self) -> bool {
    !self.is_interim()
  }

  #[must_use]
  pub const fn is_redirection_method_preserving(self) -> bool {
    matches!(self.0, 307 | 308)
  }

  #[must_use]
  pub const fn is_redirection_suggests_get(self) -> bool {
    matches!(self.0, 303)
  }

  pub const CONTINUE: Self = Self(100);
  pub const SWITCHING_PROTOCOLS: Self = Self(101);
  pub const PROCESSING: Self = Self(102);
  pub const EARLY_HINTS: Self = Self(103);
  pub const UPLOAD_RESUMPTION_SUPPORTED: Self = Self(104);

  pub const OK: Self = Self(200);
  pub const CREATED: Self = Self(201);
  pub const ACCEPTED: Self = Self(202);
  pub const NON_AUTHORITATIVE_INFORMATION: Self = Self(203);
  pub const NO_CONTENT: Self = Self(204);
  pub const RESET_CONTENT: Self = Self(205);
  pub const PARTIAL_CONTENT: Self = Self(206);
  pub const MULTI_STATUS: Self = Self(207);
  pub const ALREADY_REPORTED: Self = Self(208);
  pub const IM_USED: Self = Self(226);

  pub const MULTIPLE_CHOICES: Self = Self(300);
  pub const MOVED_PERMANENTLY: Self = Self(301);
  pub const FOUND: Self = Self(302);
  pub const SEE_OTHER: Self = Self(303);
  pub const NOT_MODIFIED: Self = Self(304);
  pub const USE_PROXY: Self = Self(305);
  pub const UNUSED_306: Self = Self(306);
  pub const TEMPORARY_REDIRECT: Self = Self(307);
  pub const PERMANENT_REDIRECT: Self = Self(308);

  pub const BAD_REQUEST: Self = Self(400);
  pub const UNAUTHORIZED: Self = Self(401);
  pub const PAYMENT_REQUIRED: Self = Self(402);
  pub const FORBIDDEN: Self = Self(403);
  pub const NOT_FOUND: Self = Self(404);
  pub const METHOD_NOT_ALLOWED: Self = Self(405);
  pub const NOT_ACCEPTABLE: Self = Self(406);
  pub const PROXY_AUTHENTICATION_REQUIRED: Self = Self(407);
  pub const REQUEST_TIMEOUT: Self = Self(408);
  pub const CONFLICT: Self = Self(409);
  pub const GONE: Self = Self(410);
  pub const LENGTH_REQUIRED: Self = Self(411);
  pub const PRECONDITION_FAILED: Self = Self(412);
  pub const CONTENT_TOO_LARGE: Self = Self(413);
  pub const URI_TOO_LONG: Self = Self(414);
  pub const UNSUPPORTED_MEDIA_TYPE: Self = Self(415);
  pub const RANGE_NOT_SATISFIABLE: Self = Self(416);
  pub const EXPECTATION_FAILED: Self = Self(417);
  pub const IM_A_TEAPOT: Self = Self(418);
  pub const MISDIRECTED_REQUEST: Self = Self(421);
  pub const UNPROCESSABLE_CONTENT: Self = Self(422);
  pub const LOCKED: Self = Self(423);
  pub const FAILED_DEPENDENCY: Self = Self(424);
  pub const TOO_EARLY: Self = Self(425);
  pub const UPGRADE_REQUIRED: Self = Self(426);
  pub const PRECONDITION_REQUIRED: Self = Self(428);
  pub const TOO_MANY_REQUESTS: Self = Self(429);
  pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Self = Self(431);
  pub const UNAVAILABLE_FOR_LEGAL_REASONS: Self = Self(451);

  pub const INTERNAL_SERVER_ERROR: Self = Self(500);
  pub const NOT_IMPLEMENTED: Self = Self(501);
  pub const BAD_GATEWAY: Self = Self(502);
  pub const SERVICE_UNAVAILABLE: Self = Self(503);
  pub const GATEWAY_TIMEOUT: Self = Self(504);
  pub const HTTP_VERSION_NOT_SUPPORTED: Self = Self(505);
  pub const VARIANT_ALSO_NEGOTIATES: Self = Self(506);
  pub const INSUFFICIENT_STORAGE: Self = Self(507);
  pub const LOOP_DETECTED: Self = Self(508);
  pub const NOT_EXTENDED: Self = Self(510);
  pub const NETWORK_AUTHENTICATION_REQUIRED: Self = Self(511);
}
