//! LSB-first bit reader (RFC 1951 §3.1.1).

use super::DecompressError;

/// Pulls bits from a byte slice; data elements packed LSB-first within each byte.
pub(super) struct BitReader<'a> {
  data: &'a [u8],
  /// Next unread byte index into `data`.
  pos: usize,
  bitbuf: u32,
  bitcnt: u8,
}

impl<'a> BitReader<'a> {
  pub(super) const fn new(data: &'a [u8]) -> Self {
    Self {
      data,
      pos: 0,
      bitbuf: 0,
      bitcnt: 0,
    }
  }

  /// Byte offset of the next unread input bit (after accounting for buffered bits).
  pub(super) fn byte_pos(&self) -> usize {
    let buffered = usize::from(self.bitcnt >> 3);
    self.pos.saturating_sub(buffered)
  }

  /// Pull in bytes until `need` bits are buffered, or input ends.
  fn fill_available(
    &mut self,
    need: u8,
  ) {
    while self.bitcnt < need {
      let Some(byte) = self.data.get(self.pos).copied() else {
        break;
      };
      self.pos = self.pos.saturating_add(1);
      self.bitbuf |= u32::from(byte) << self.bitcnt;
      self.bitcnt = self.bitcnt.saturating_add(8);
    }
  }

  fn fill(
    &mut self,
    need: u8,
  ) -> Result<(), DecompressError> {
    self.fill_available(need);
    if self.bitcnt < need {
      return Err(DecompressError::InvalidInput);
    }
    Ok(())
  }

  /// Read `n` bits as an integer (LSB of the value = first bit in the stream).
  pub(super) fn get_bits(
    &mut self,
    n: u8,
  ) -> Result<u32, DecompressError> {
    if n == 0 {
      return Ok(0);
    }
    if n > 24 {
      return Err(DecompressError::InvalidInput);
    }
    self.fill(n)?;
    let mask = (1u32 << n) - 1;
    let v = self.bitbuf & mask;
    self.bitbuf >>= n;
    self.bitcnt = self.bitcnt.saturating_sub(n);
    Ok(v)
  }

  /// Peek up to `n` bits (or fewer at EOF). Returns `(value, bits_available)`.
  pub(super) fn peek_bits_available(
    &mut self,
    n: u8,
  ) -> Result<(u32, u8), DecompressError> {
    if n == 0 {
      return Ok((0, 0));
    }
    if n > 24 {
      return Err(DecompressError::InvalidInput);
    }
    self.fill_available(n);
    if self.bitcnt == 0 {
      return Err(DecompressError::InvalidInput);
    }
    let have = if self.bitcnt < n { self.bitcnt } else { n };
    let mask = (1u32 << have) - 1;
    Ok((self.bitbuf & mask, have))
  }

  /// Drop `n` bits previously peeked (must have been filled).
  pub(super) const fn drop_bits(
    &mut self,
    n: u8,
  ) {
    if n == 0 {
      return;
    }
    self.bitbuf >>= n;
    self.bitcnt = self.bitcnt.saturating_sub(n);
  }

  /// Discard bits up to the next byte boundary (RFC 1951 §3.2.4).
  pub(super) const fn align_to_byte(&mut self) {
    let rem = self.bitcnt & 7;
    if rem != 0 {
      self.bitbuf >>= rem;
      self.bitcnt = self.bitcnt.saturating_sub(rem);
    }
  }

  /// Read one byte after byte-alignment (may drain buffered full bytes).
  pub(super) fn get_aligned_byte(&mut self) -> Result<u8, DecompressError> {
    self.align_to_byte();
    if self.bitcnt >= 8 {
      let b = u8::try_from(self.bitbuf & 0xff).map_err(|_| DecompressError::InvalidInput)?;
      self.bitbuf >>= 8;
      self.bitcnt = self.bitcnt.saturating_sub(8);
      return Ok(b);
    }
    let b = self.data.get(self.pos).copied().ok_or(DecompressError::InvalidInput)?;
    self.pos = self.pos.saturating_add(1);
    Ok(b)
  }
}
