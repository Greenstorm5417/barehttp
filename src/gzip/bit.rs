//! LSB-first bit reader (RFC 1951 §3.1.1).

#![allow(clippy::cast_possible_truncation, clippy::cast_lossless)] // bitbuf low bits → u32/u8 by construction after masks

use super::DecompressError;
use alloc::vec::Vec;

/// Pulls bits from a byte slice; data elements packed LSB-first within each byte.
///
/// Uses a `u64` hold buffer so the inflate loop refills far less often than a `u32` buffer.
pub(super) struct BitReader<'a> {
  data: &'a [u8],
  /// Next unread byte index into `data`.
  pos: usize,
  bitbuf: u64,
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

  /// Bits currently buffered (for Huffman fast-path decisions).
  #[inline(always)]
  pub(super) const fn bitcnt(&self) -> u8 {
    self.bitcnt
  }

  /// Byte offset of the next unread input bit (after accounting for buffered bits).
  pub(super) fn byte_pos(&self) -> usize {
    let buffered = usize::from(self.bitcnt >> 3);
    self.pos.saturating_sub(buffered)
  }

  /// Pull bytes until the hold has as many bits as possible (up to 56+).
  #[inline(always)]
  #[allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]
  fn refill(&mut self) {
    // Bulk LE word loads when plenty of input remains.
    while self.bitcnt <= 32 {
      let Some(chunk) = self.data.get(self.pos..self.pos + 4) else {
        break;
      };
      let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
      self.bitbuf |= u64::from(word) << self.bitcnt;
      self.bitcnt += 32;
      self.pos += 4;
    }
    while self.bitcnt <= 56 {
      let Some(&byte) = self.data.get(self.pos) else {
        break;
      };
      self.pos += 1;
      self.bitbuf |= u64::from(byte) << self.bitcnt;
      self.bitcnt += 8;
    }
  }

  #[inline(always)]
  fn ensure(
    &mut self,
    need: u8,
  ) -> Result<(), DecompressError> {
    if self.bitcnt < need {
      self.refill();
      if self.bitcnt < need {
        return Err(DecompressError::InvalidInput);
      }
    }
    Ok(())
  }

  /// Read `n` bits as an integer (LSB of the value = first bit in the stream).
  #[inline(always)]
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
    self.ensure(n)?;
    let mask = (1u32 << n) - 1;
    let v = (self.bitbuf as u32) & mask;
    self.bitbuf >>= n;
    self.bitcnt -= n;
    Ok(v)
  }

  /// Peek the low `n` bits without consuming (caller must have ensured availability).
  #[inline(always)]
  pub(super) const fn peek_bits(
    &self,
    n: u8,
  ) -> u32 {
    let mask = (1u32 << n) - 1;
    (self.bitbuf as u32) & mask
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
    if self.bitcnt < n {
      self.refill();
    }
    if self.bitcnt == 0 {
      return Err(DecompressError::InvalidInput);
    }
    let have = if self.bitcnt < n {
      self.bitcnt
    } else {
      n
    };
    let mask = (1u32 << have) - 1;
    Ok(((self.bitbuf as u32) & mask, have))
  }

  /// Drop `n` bits previously peeked (must have been filled).
  #[inline(always)]
  pub(super) const fn drop_bits(
    &mut self,
    n: u8,
  ) {
    if n == 0 {
      return;
    }
    self.bitbuf >>= n;
    self.bitcnt -= n;
  }

  /// Discard bits up to the next byte boundary (RFC 1951 §3.2.4).
  pub(super) const fn align_to_byte(&mut self) {
    let rem = self.bitcnt & 7;
    if rem != 0 {
      self.bitbuf >>= rem;
      self.bitcnt -= rem;
    }
  }

  /// Read one byte after byte-alignment (may drain buffered full bytes).
  pub(super) fn get_aligned_byte(&mut self) -> Result<u8, DecompressError> {
    self.align_to_byte();
    if self.bitcnt >= 8 {
      let b = self.bitbuf as u8;
      self.bitbuf >>= 8;
      self.bitcnt -= 8;
      return Ok(b);
    }
    let b = self
      .data
      .get(self.pos)
      .copied()
      .ok_or(DecompressError::InvalidInput)?;
    self.pos += 1;
    Ok(b)
  }

  /// Append `len` aligned bytes into `out` (stored blocks). One limit check up front.
  pub(super) fn copy_aligned_bytes(
    &mut self,
    out: &mut Vec<u8>,
    len: usize,
    max_out: usize,
  ) -> Result<(), DecompressError> {
    self.align_to_byte();
    if out.len().saturating_add(len) > max_out {
      return Err(DecompressError::LimitExceeded);
    }
    out.reserve(len);
    let mut left = len;
    // Drain buffered full bytes in one extend (bitbuf holds ≤ 7 full bytes after align).
    if left > 0 && self.bitcnt >= 8 {
      let mut tmp = [0u8; 8];
      let mut n = 0usize;
      while left > 0 && self.bitcnt >= 8 && n < tmp.len() {
        if let Some(slot) = tmp.get_mut(n) {
          *slot = self.bitbuf as u8;
        }
        self.bitbuf >>= 8;
        self.bitcnt -= 8;
        n += 1;
        left -= 1;
      }
      out.extend_from_slice(tmp.get(..n).unwrap_or(&[]));
    }
    if left == 0 {
      return Ok(());
    }
    let end = self
      .pos
      .checked_add(left)
      .ok_or(DecompressError::InvalidInput)?;
    let slice = self
      .data
      .get(self.pos..end)
      .ok_or(DecompressError::InvalidInput)?;
    out.extend_from_slice(slice);
    self.pos = end;
    Ok(())
  }
}
