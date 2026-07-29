//! Canonical Huffman decode from code lengths (RFC 1951 §3.2.2).

use super::DecompressError;
use super::bit::BitReader;
use alloc::vec;
use alloc::vec::Vec;

const MAX_BITS: usize = 15;

/// Pack symbol + code length into one `u32` load (`symbol | (len << 16)`).
#[inline(always)]
fn pack_entry(
  sym: u16,
  len: u8,
) -> u32 {
  u32::from(sym) | (u32::from(len) << 16)
}

#[inline(always)]
#[allow(clippy::cast_possible_truncation)] // low 16 bits are the symbol by construction
const fn unpack_sym(entry: u32) -> u16 {
  entry as u16
}

#[inline(always)]
#[allow(clippy::cast_possible_truncation)] // bits 16..23 hold a length ≤ 15
const fn unpack_len(entry: u32) -> u8 {
  (entry >> 16) as u8
}

enum Table {
  Owned(Vec<u32>),
  Static(&'static [u32]),
}

impl Table {
  #[inline(always)]
  fn get(
    &self,
    idx: usize,
  ) -> Option<u32> {
    match self {
      Self::Owned(v) => v.get(idx).copied(),
      Self::Static(s) => s.get(idx).copied(),
    }
  }
}

/// Decoder for one DEFLATE Huffman alphabet.
pub(super) struct HuffmanDecoder {
  /// `table[peek]` packed entry for a `max_bits`-bit peek (first stream bit in LSB).
  table: Table,
  max_bits: u8,
}

impl HuffmanDecoder {
  /// Build from per-symbol code lengths (`0` = unused). RFC 1951 §3.2.2.
  pub(super) fn from_lengths(lengths: &[u8]) -> Result<Self, DecompressError> {
    let mut bl_count = [0u16; MAX_BITS + 1];
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
        table: Table::Owned(Vec::new()),
        max_bits: 0,
      });
    }

    // next_code[bits] = smallest code of that length (MSB-first integers).
    let mut next_code = [0u16; MAX_BITS + 1];
    let mut code = 0u16;
    if let Some(slot) = bl_count.get_mut(0) {
      *slot = 0;
    }
    let mut bits = 1u8;
    while bits <= max_bits {
      let prev = bl_count.get(usize::from(bits - 1)).copied().unwrap_or(0);
      code = code
        .checked_add(prev)
        .ok_or(DecompressError::InvalidInput)?
        .checked_mul(2)
        .ok_or(DecompressError::InvalidInput)?;
      if let Some(slot) = next_code.get_mut(usize::from(bits)) {
        *slot = code;
      }
      bits += 1;
    }

    let table_size = 1usize
      .checked_shl(u32::from(max_bits))
      .ok_or(DecompressError::InvalidInput)?;
    let mut table = vec![0u32; table_size];

    for (sym, &len) in lengths.iter().enumerate() {
      if len == 0 {
        continue;
      }
      let Some(nc) = next_code.get_mut(usize::from(len)) else {
        return Err(DecompressError::InvalidInput);
      };
      let c_msb = *nc;
      *nc = nc.checked_add(1).ok_or(DecompressError::InvalidInput)?;
      let sym_u = u16::try_from(sym).map_err(|_| DecompressError::InvalidInput)?;
      // Reverse `len` bits so table keys match LSB-first bit peeks (§3.1.1 / §3.2.2).
      let c_lsb = reverse_bits(c_msb, len);
      let step = 1usize << len;
      let entry = pack_entry(sym_u, len);
      let mut fill = usize::from(c_lsb);
      while fill < table_size {
        if let Some(slot) = table.get_mut(fill) {
          *slot = entry;
        }
        fill = fill
          .checked_add(step)
          .ok_or(DecompressError::InvalidInput)?;
      }
    }

    Ok(Self {
      table: Table::Owned(table),
      max_bits,
    })
  }

  #[inline(always)]
  #[allow(clippy::cast_possible_truncation)] // peek masked to ≤15 bits
  pub(super) fn decode(
    &self,
    bits: &mut BitReader<'_>,
  ) -> Result<u16, DecompressError> {
    if self.max_bits == 0 {
      return Err(DecompressError::InvalidInput);
    }

    // Fast path: enough bits already buffered — no EOF probing.
    if bits.bitcnt() >= self.max_bits {
      let idx = bits.peek_bits(self.max_bits) as usize;
      let entry = self.table.get(idx).ok_or(DecompressError::InvalidInput)?;
      let len = unpack_len(entry);
      if len == 0 || len > self.max_bits {
        return Err(DecompressError::InvalidInput);
      }
      bits.drop_bits(len);
      return Ok(unpack_sym(entry));
    }

    // Near EOF we may have fewer than `max_bits` left (e.g. 7-bit EOB).
    let (peek, have) = bits.peek_bits_available(self.max_bits)?;
    let entry = self
      .table
      .get(peek as usize)
      .ok_or(DecompressError::InvalidInput)?;
    let len = unpack_len(entry);
    if len == 0 || len > self.max_bits || len > have {
      return Err(DecompressError::InvalidInput);
    }
    bits.drop_bits(len);
    Ok(unpack_sym(entry))
  }

  /// Decode using a known static table sized exactly `1 << max_bits`.
  ///
  /// Fixed DEFLATE tables are fully populated, so empty-slot checks are omitted.
  #[inline(always)]
  #[allow(clippy::indexing_slicing, clippy::cast_possible_truncation, clippy::cast_lossless)]
  pub(super) fn decode_static(
    table: &'static [u32],
    max_bits: u8,
    bits: &mut BitReader<'_>,
  ) -> Result<u16, DecompressError> {
    debug_assert_eq!(table.len(), 1usize << max_bits);
    if bits.bitcnt() < max_bits {
      let dec = Self {
        table: Table::Static(table),
        max_bits,
      };
      return dec.decode(bits);
    }
    let peek = bits.peek_bits(max_bits) as usize;
    // Index in range: peek is masked to `max_bits` and `table.len() == 1 << max_bits`.
    let entry = table[peek];
    bits.drop_bits(unpack_len(entry));
    Ok(unpack_sym(entry))
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
    i += 1;
  }
  r
}
