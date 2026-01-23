/// Connection state tracking for RFC 9112 Section 9.6 compliance
/// Tracks whether "Connection: close" has been sent or received

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionState {
  pub(crate) sent_close: bool,
  pub(crate) received_close: bool,
  pub(crate) can_reuse: bool,
}

impl ConnectionState {
  pub const fn new() -> Self {
    Self {
      sent_close: false,
      received_close: false,
      can_reuse: true,
    }
  }

  /// Mark that we sent "Connection: close" header
  /// RFC 9112 Section 9.6: Client MUST NOT send further requests on this connection
  pub const fn mark_sent_close(&mut self) {
    self.sent_close = true;
    self.can_reuse = false;
  }

  /// Mark that we received "Connection: close" header
  /// RFC 9112 Section 9.6: Client MUST stop sending and close after reading response
  pub const fn mark_received_close(&mut self) {
    self.received_close = true;
    self.can_reuse = false;
  }

  /// Check if connection can be reused for another request
  /// RFC 9112 Section 9.6: Only reusable if neither side sent "Connection: close"
  pub const fn can_be_reused(self) -> bool {
    self.can_reuse
  }
}

impl Default for ConnectionState {
  fn default() -> Self {
    Self::new()
  }
}
