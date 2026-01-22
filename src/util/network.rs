/// IP address (IPv4 or IPv6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpAddr {
  /// IPv4 address (4 octets)
  V4([u8; 4]),
  /// IPv6 address (8 16-bit segments)
  V6([u16; 8]),
}

impl IpAddr {
  #[must_use]
  /// Returns the address as IPv4 if it is IPv4
  pub const fn as_v4(&self) -> Option<&[u8; 4]> {
    match self {
      Self::V4(addr) => Some(addr),
      Self::V6(_) => None,
    }
  }

  #[must_use]
  /// Returns the address as IPv6 if it is IPv6
  pub const fn as_v6(&self) -> Option<&[u16; 8]> {
    match self {
      Self::V4(_) => None,
      Self::V6(addr) => Some(addr),
    }
  }
}
