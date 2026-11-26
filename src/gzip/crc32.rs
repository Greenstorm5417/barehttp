//! CRC-32 (RFC 1952 appendix; poly `0xEDB88320`).

/// Precomputed CRC table (RFC 1952 sample `make_crc_table`).
#[allow(clippy::indexing_slicing)] // const table init
const fn make_crc_table() -> [u32; 256] {
  let mut table = [0u32; 256];
  let mut n = 0u32;
  while n < 256 {
    let mut c = n;
    let mut k = 0u8;
    while k < 8 {
      if c & 1 != 0 {
        c = 0xedb8_8320 ^ (c >> 1);
      } else {
        c >>= 1;
      }
      k += 1;
    }
    table[n as usize] = c;
    n += 1;
  }
  table
}

const CRC_TABLE: [u32; 256] = make_crc_table();

/// Slice-by-8 tables: `CRC_TABLE_N[i] = CRC of byte i followed by N zero bytes`.
#[allow(clippy::indexing_slicing, clippy::cast_possible_truncation)] // const table init
const fn make_crc_table_n(n_zeros: u8) -> [u32; 256] {
  let mut table = [0u32; 256];
  let mut i = 0usize;
  while i < 256 {
    let mut c = CRC_TABLE[i];
    let mut z = 0u8;
    while z < n_zeros {
      c = CRC_TABLE[(c & 0xff) as usize] ^ (c >> 8);
      z += 1;
    }
    table[i] = c;
    i += 1;
  }
  table
}

const CRC_TABLE1: [u32; 256] = make_crc_table_n(1);
const CRC_TABLE2: [u32; 256] = make_crc_table_n(2);
const CRC_TABLE3: [u32; 256] = make_crc_table_n(3);
const CRC_TABLE4: [u32; 256] = make_crc_table_n(4);
const CRC_TABLE5: [u32; 256] = make_crc_table_n(5);
const CRC_TABLE6: [u32; 256] = make_crc_table_n(6);
const CRC_TABLE7: [u32; 256] = make_crc_table_n(7);

/// CRC of `buf` with running value `crc` (init `0`; pre/post conditioning inside).
#[allow(clippy::indexing_slicing, clippy::cast_possible_truncation, clippy::cast_lossless)]
pub(super) fn update_crc(
  crc: u32,
  buf: &[u8],
) -> u32 {
  let mut c = crc ^ 0xffff_ffff;
  let (chunks, rem) = buf.as_chunks::<8>();
  for chunk in chunks {
    // Two LE words per step (first XOR'd with the running CRC), eight
    // independent table lookups — half the serial dependency of slice-by-4.
    // The high word indexes straight from memory bytes (one `movzx` each);
    // only the low word needs the running CRC folded in.
    let lo = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ c;
    c = CRC_TABLE7[(lo & 0xff) as usize]
      ^ CRC_TABLE6[((lo >> 8) & 0xff) as usize]
      ^ CRC_TABLE5[((lo >> 16) & 0xff) as usize]
      ^ CRC_TABLE4[(lo >> 24) as usize]
      ^ CRC_TABLE3[chunk[4] as usize]
      ^ CRC_TABLE2[chunk[5] as usize]
      ^ CRC_TABLE1[chunk[6] as usize]
      ^ CRC_TABLE[chunk[7] as usize];
  }
  for &byte in rem {
    c = CRC_TABLE[((c ^ u32::from(byte)) & 0xff) as usize] ^ (c >> 8);
  }
  c ^ 0xffff_ffff
}

/// CRC-32 of `buf`.
pub(super) fn crc32(buf: &[u8]) -> u32 {
  update_crc(0, buf)
}

#[cfg(test)]
mod tests {
  #![allow(clippy::cast_possible_truncation)]
  use super::{CRC_TABLE, update_crc};

  fn bytewise(buf: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in buf {
      c = CRC_TABLE
        .get(((c ^ u32::from(b)) & 0xff) as usize)
        .copied()
        .unwrap_or(0)
        ^ (c >> 8);
    }
    c ^ 0xffff_ffff
  }

  #[test]
  fn slice_by_8_matches_bytewise_at_every_length_and_split() {
    let data: alloc::vec::Vec<u8> = (0u32..300)
      .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
      .collect();
    for len in 0..data.len() {
      let buf = data.get(..len).unwrap_or(&[]);
      assert_eq!(update_crc(0, buf), bytewise(buf), "len {len}");
      let (a, b) = buf.split_at(len >> 1);
      assert_eq!(update_crc(update_crc(0, a), b), bytewise(buf), "split {len}");
    }
  }
}
