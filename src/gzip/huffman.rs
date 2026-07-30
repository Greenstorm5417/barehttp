//! Canonical Huffman decode from code lengths (RFC 1951 §3.2.2).
//!
//! Dynamic tables use a capped primary lookup (`ROOT_BITS`) plus a slow path for
//! longer codes, avoiding `1 << max_bits` allocations (up to 128 KiB at 15 bits).

use super::DecompressError;
use super::bit::BitReader;
use alloc::vec::Vec;

const MAX_BITS: usize = 15;

/// Primary table peek width for dynamic alphabets (zlib-style). Codes longer than
/// this use [`LongCode`] slow-path matching.
pub(super) const ROOT_BITS: u8 = 9;

/// Sentinel in the primary table: this `ROOT_BITS` prefix belongs to a longer code.
const LONG_SENTINEL: u32 = 0x00FF_0000;

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

/// One code longer than [`ROOT_BITS`] (LSB-first bit order, matching peeks).
#[derive(Clone, Copy)]
struct LongCode {
  /// Reversed (LSB-first) code value.
  code: u16,
  len: u8,
  sym: u16,
}

/// Reusable buffers for building dynamic Huffman tables (peak alloc once per inflate).
pub(super) struct HuffmanPool {
  table0: Vec<u32>,
  table1: Vec<u32>,
  long0: Vec<LongCode>,
  long1: Vec<LongCode>,
}

impl HuffmanPool {
  pub(super) const fn new() -> Self {
    Self {
      table0: Vec::new(),
      table1: Vec::new(),
      long0: Vec::new(),
      long1: Vec::new(),
    }
  }

  /// Build a decoder using pool slot 0 (reuses prior capacity).
  pub(super) fn take_decoder0(
    &mut self,
    lengths: &[u8],
  ) -> Result<HuffmanDecoder, DecompressError> {
    let table = core::mem::take(&mut self.table0);
    let long = core::mem::take(&mut self.long0);
    HuffmanDecoder::from_lengths_in(lengths, table, long)
  }

  /// Build a decoder using pool slot 1.
  pub(super) fn take_decoder1(
    &mut self,
    lengths: &[u8],
  ) -> Result<HuffmanDecoder, DecompressError> {
    let table = core::mem::take(&mut self.table1);
    let long = core::mem::take(&mut self.long1);
    HuffmanDecoder::from_lengths_in(lengths, table, long)
  }

  /// Return lit (slot 0) + dist (slot 1) buffers after a dynamic block.
  pub(super) fn recycle_pair(
    &mut self,
    lit: HuffmanDecoder,
    dist: HuffmanDecoder,
  ) {
    let (t0, l0) = lit.into_buffers();
    let (t1, l1) = dist.into_buffers();
    self.table0 = t0;
    self.long0 = l0;
    self.table1 = t1;
    self.long1 = l1;
  }

  /// Reclaim one decoder into slot 0 (e.g. after code-length alphabet is done).
  pub(super) fn recycle0(
    &mut self,
    dec: HuffmanDecoder,
  ) {
    let (t, l) = dec.into_buffers();
    self.table0 = t;
    self.long0 = l;
  }
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
  /// Primary table: `1 << root_bits` packed entries (or empty alphabet).
  table: Table,
  /// Peek width for the primary table (`min(max_bits, ROOT_BITS)` when owned).
  root_bits: u8,
  /// True max code length in this alphabet (for long-path peeks / EOF).
  max_bits: u8,
  long_codes: Vec<LongCode>,
}

impl HuffmanDecoder {
  /// Build from per-symbol code lengths (`0` = unused). RFC 1951 §3.2.2.
  ///
  /// Fresh heap buffers — production inflate uses [`HuffmanPool`] instead.
  #[cfg(test)]
  pub(super) fn from_lengths(lengths: &[u8]) -> Result<Self, DecompressError> {
    Self::from_lengths_in(lengths, Vec::new(), Vec::new())
  }

  /// Build reusing `table` / `long` capacity (cleared first).
  fn from_lengths_in(
    lengths: &[u8],
    mut table: Vec<u32>,
    mut long_codes: Vec<LongCode>,
  ) -> Result<Self, DecompressError> {
    table.clear();
    long_codes.clear();

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
      return Ok(Self {
        table: Table::Owned(table),
        root_bits: 0,
        max_bits: 0,
        long_codes,
      });
    }

    let root_bits = if max_bits > ROOT_BITS {
      ROOT_BITS
    } else {
      max_bits
    };

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
      .checked_shl(u32::from(root_bits))
      .ok_or(DecompressError::InvalidInput)?;
    table.resize(table_size, 0);

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

      if len <= root_bits {
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
      } else {
        // Mark the ROOT_BITS prefix; full match happens on the slow path.
        let mask = (1u16 << root_bits) - 1;
        let prefix = usize::from(c_lsb & mask);
        match table.get_mut(prefix) {
          Some(slot) if *slot == 0 || *slot == LONG_SENTINEL => *slot = LONG_SENTINEL,
          // Occupied by a short code → prefix-free violation (corrupt lengths).
          Some(_) | None => return Err(DecompressError::InvalidInput),
        }
        long_codes.push(LongCode {
          code: c_lsb,
          len,
          sym: sym_u,
        });
      }
    }

    Ok(Self {
      table: Table::Owned(table),
      root_bits,
      max_bits,
      long_codes,
    })
  }

  /// Return owned buffers for pool recycling.
  fn into_buffers(self) -> (Vec<u32>, Vec<LongCode>) {
    let table = match self.table {
      Table::Owned(v) => v,
      Table::Static(_) => Vec::new(),
    };
    (table, self.long_codes)
  }

  #[inline(always)]
  #[allow(clippy::cast_possible_truncation)] // peek masked to ≤15 bits
  pub(super) fn decode(
    &self,
    bits: &mut BitReader<'_>,
  ) -> Result<u16, DecompressError> {
    if self.root_bits == 0 {
      return Err(DecompressError::InvalidInput);
    }

    // Fast path: enough bits for the primary peek.
    if bits.bitcnt() >= self.root_bits {
      let idx = bits.peek_bits(self.root_bits) as usize;
      let entry = self.table.get(idx).ok_or(DecompressError::InvalidInput)?;
      if entry == LONG_SENTINEL {
        return self.decode_long(bits);
      }
      let len = unpack_len(entry);
      if len == 0 || len > self.root_bits {
        return Err(DecompressError::InvalidInput);
      }
      bits.drop_bits(len);
      return Ok(unpack_sym(entry));
    }

    // Near EOF we may have fewer than `root_bits` left (e.g. 7-bit EOB).
    let (peek, have) = bits.peek_bits_available(self.root_bits)?;
    let entry = self
      .table
      .get(peek as usize)
      .ok_or(DecompressError::InvalidInput)?;
    if entry == LONG_SENTINEL {
      return self.decode_long(bits);
    }
    let len = unpack_len(entry);
    if len == 0 || len > self.root_bits || len > have {
      return Err(DecompressError::InvalidInput);
    }
    bits.drop_bits(len);
    Ok(unpack_sym(entry))
  }

  #[inline(never)]
  fn decode_long(
    &self,
    bits: &mut BitReader<'_>,
  ) -> Result<u16, DecompressError> {
    let (peek, have) = bits.peek_bits_available(self.max_bits)?;
    for lc in &self.long_codes {
      if lc.len > have {
        continue;
      }
      let mask = (1u32 << lc.len) - 1;
      if (peek & mask) == u32::from(lc.code) {
        bits.drop_bits(lc.len);
        return Ok(lc.sym);
      }
    }
    Err(DecompressError::InvalidInput)
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
        root_bits: max_bits,
        max_bits,
        long_codes: Vec::new(),
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

#[cfg(test)]
mod huffman_tests {
  #![allow(clippy::unwrap_used, clippy::expect_used)]
  use super::super::bit::BitReader;
  use super::*;

  /// Alphabet with a code longer than ROOT_BITS to force the slow path.
  #[test]
  fn capped_table_decodes_long_codes() {
    // sym0 len=1 (c_lsb=0), sym1 len=10 (c_lsb=1) — see canonical next_code build.
    let lengths = [1u8, 10];
    let dec = HuffmanDecoder::from_lengths(&lengths).expect("build");
    assert_eq!(dec.root_bits, ROOT_BITS);
    assert_eq!(dec.max_bits, 10);
    assert_eq!(dec.long_codes.len(), 1);

    // Stream: sym0 bit `0`, then 10-bit code `1` LSB-first → bytes [0x02, 0x00].
    let data = [0x02u8, 0x00];
    let mut br = BitReader::new(&data);
    assert_eq!(dec.decode(&mut br).unwrap(), 0);
    assert_eq!(dec.decode(&mut br).unwrap(), 1);
  }

  #[test]
  fn short_only_alphabet_matches_full_width() {
    let lengths = [3u8; 8];
    let dec = HuffmanDecoder::from_lengths(&lengths).expect("build");
    assert_eq!(dec.root_bits, 3);
    assert_eq!(dec.max_bits, 3);
    assert!(dec.long_codes.is_empty());
  }
}
