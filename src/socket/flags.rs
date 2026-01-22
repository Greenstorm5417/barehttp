#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketFlags {
  bits: u32,
}

impl SocketFlags {
  pub const TCP_NODELAY: Self = Self { bits: 0b0001 };
  pub const KEEPALIVE: Self = Self { bits: 0b0010 };
  pub const REUSEADDR: Self = Self { bits: 0b0100 };
  pub const CLOSE_ON_DROP: Self = Self { bits: 0b1000 };

  pub const fn empty() -> Self {
    Self { bits: 0 }
  }

  pub const fn all() -> Self {
    Self { bits: 0b1111 }
  }

  pub const fn bits(self) -> u32 {
    self.bits
  }

  pub const fn from_bits(bits: u32) -> Option<Self> {
    if bits & !0b1111 == 0 {
      Some(Self { bits })
    } else {
      None
    }
  }

  pub const fn from_bits_truncate(bits: u32) -> Self {
    Self {
      bits: bits & 0b1111,
    }
  }

  pub const fn contains(self, other: Self) -> bool {
    self.bits & other.bits == other.bits
  }

  pub const fn insert(&mut self, other: Self) {
    self.bits |= other.bits;
  }

  pub const fn remove(&mut self, other: Self) {
    self.bits &= !other.bits;
  }

  pub const fn toggle(&mut self, other: Self) {
    self.bits ^= other.bits;
  }

  pub const fn set(&mut self, other: Self, value: bool) {
    if value {
      self.bits |= other.bits;
    } else {
      self.bits &= !other.bits;
    }
  }

  pub const fn is_empty(self) -> bool {
    self.bits == 0
  }

  pub const fn is_all(self) -> bool {
    self.bits == 0b1111
  }

  pub const fn union(self, other: Self) -> Self {
    Self {
      bits: self.bits | other.bits,
    }
  }

  pub const fn intersection(self, other: Self) -> Self {
    Self {
      bits: self.bits & other.bits,
    }
  }

  pub const fn difference(self, other: Self) -> Self {
    Self {
      bits: self.bits & !other.bits,
    }
  }

  pub const fn symmetric_difference(self, other: Self) -> Self {
    Self {
      bits: self.bits ^ other.bits,
    }
  }
}

impl core::ops::BitOr for SocketFlags {
  type Output = Self;

  fn bitor(self, other: Self) -> Self {
    self.union(other)
  }
}

impl core::ops::BitOrAssign for SocketFlags {
  fn bitor_assign(&mut self, other: Self) {
    self.insert(other);
  }
}

impl core::ops::BitAnd for SocketFlags {
  type Output = Self;

  fn bitand(self, other: Self) -> Self {
    self.intersection(other)
  }
}

impl core::ops::BitAndAssign for SocketFlags {
  fn bitand_assign(&mut self, other: Self) {
    *self = self.intersection(other);
  }
}

impl core::ops::BitXor for SocketFlags {
  type Output = Self;

  fn bitxor(self, other: Self) -> Self {
    self.symmetric_difference(other)
  }
}

impl core::ops::BitXorAssign for SocketFlags {
  fn bitxor_assign(&mut self, other: Self) {
    self.toggle(other);
  }
}

impl core::ops::Not for SocketFlags {
  type Output = Self;

  fn not(self) -> Self {
    Self::from_bits_truncate(!self.bits)
  }
}
