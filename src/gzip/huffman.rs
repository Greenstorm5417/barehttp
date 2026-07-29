//! Canonical Huffman decode from code lengths (RFC 1951 §3.2.2).

use super::DecompressError;
use super::bit::BitReader;
use alloc::vec;
use alloc::vec::Vec;

const MAX_BITS: usize = 15;

/// Decoder for one DEFLATE Huffman alphabet.
pub(super) struct HuffmanDecoder {
  /// `table[peek]` = `(symbol, code_len)` for a `max_bits`-bit peek (first stream bit in LSB).
  table: Vec<(u16, u8)>,
  max_bits: u8,
}

impl HuffmanDecoder {
  /// Build from per-symbol code lengths (`0` = unused). RFC 1951 §3.2.2.
  pub(super) fn from_lengths(lengths: &[u8]) -> Result<Self, DecompressError> {
    let mut bl_count = [0u16; MAX_BITS.saturating_add(1)];
    let mut max_bits = 0u8;
    for &len in lengths {
      if len > 15 {
        return Err(DecompressError::InvalidInput);
      }
      if len > 0 {
        let Some(slot) = bl_count.get_mut(usize::from(len)) else {
          return Err(DecompressError::InvalidInput);
        };
        *slot = slot.saturating_add(1);
        if len > max_bits {
          max_bits = len;
        }
      }
    }
    if max_bits == 0 {
      // Empty alphabet (e.g. no distance codes): decode always fails if used.
      return Ok(Self {
        table: Vec::new(),
        max_bits: 0,
      });
    }

    // next_code[bits] = smallest code of that length (MSB-first integers).
    let mut next_code = [0u16; MAX_BITS.saturating_add(1)];
    let mut code = 0u16;
    if let Some(slot) = bl_count.get_mut(0) {
      *slot = 0;
    }
    let mut bits = 1u8;
    while bits <= max_bits {
      let prev = bl_count
        .get(usize::from(bits.saturating_sub(1)))
        .copied()
        .unwrap_or(0);
      code = code.saturating_add(prev) << 1;
      if let Some(slot) = next_code.get_mut(usize::from(bits)) {
        *slot = code;
      }
      bits = bits.saturating_add(1);
    }

    let mut table_size = 1usize;
    let mut b = 0u8;
    while b < max_bits {
      table_size = table_size.saturating_mul(2);
      b = b.saturating_add(1);
    }
    let mut table = vec![(0u16, 0u8); table_size];

    for (sym, &len) in lengths.iter().enumerate() {
      if len == 0 {
        continue;
      }
      let Some(nc) = next_code.get_mut(usize::from(len)) else {
        return Err(DecompressError::InvalidInput);
      };
      let c_msb = *nc;
      *nc = nc.saturating_add(1);
      let sym_u = u16::try_from(sym).map_err(|_| DecompressError::InvalidInput)?;
      // Reverse `len` bits so table keys match LSB-first bit peeks (§3.1.1 / §3.2.2).
      let c_lsb = reverse_bits(c_msb, len);
      let step = 1usize << len;
      let mut fill = usize::from(c_lsb);
      while fill < table_size {
        if let Some(slot) = table.get_mut(fill) {
          *slot = (sym_u, len);
        }
        fill = fill.saturating_add(step);
      }
    }

    Ok(Self { table, max_bits })
  }

  pub(super) fn decode(
    &self,
    bits: &mut BitReader<'_>,
  ) -> Result<u16, DecompressError> {
    if self.max_bits == 0 {
      return Err(DecompressError::InvalidInput);
    }
    // Near EOF we may have fewer than `max_bits` left (e.g. 7-bit EOB). Peek what
    // remains and require the matched code length to fit in the available bits.
    let (peek, have) = bits.peek_bits_available(self.max_bits)?;
    let idx = usize::try_from(peek).map_err(|_| DecompressError::InvalidInput)?;
    let (sym, len) = self.table.get(idx).copied().ok_or(DecompressError::InvalidInput)?;
    if len == 0 || len > self.max_bits || len > have {
      return Err(DecompressError::InvalidInput);
    }
    bits.drop_bits(len);
    Ok(sym)
  }
}

const fn reverse_bits(
  code: u16,
  len: u8,
) -> u16 {
  let mut c = code;
  let mut r = 0u16;
  let mut i = 0u8;
  while i < len {
    r = (r << 1) | (c & 1);
    c >>= 1;
    i = i.saturating_add(1);
  }
  r
}
