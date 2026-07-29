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

/// Slice-by-4 tables: `CRC_TABLE_N[i] = CRC of byte i followed by N zero bytes`.
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

/// CRC of `buf` with running value `crc` (init `0`; pre/post conditioning inside).
#[allow(clippy::indexing_slicing, clippy::cast_possible_truncation, clippy::cast_lossless)]
pub(super) fn update_crc(
  crc: u32,
  buf: &[u8],
) -> u32 {
  let mut c = crc ^ 0xffff_ffff;
  let (chunks, rem) = buf.as_chunks::<4>();
  for chunk in chunks {
    // One LE word XOR'd into the running CRC, then four table lookups.
    let word = u32::from_le_bytes(*chunk) ^ c;
    c = CRC_TABLE3[(word & 0xff) as usize]
      ^ CRC_TABLE2[((word >> 8) & 0xff) as usize]
      ^ CRC_TABLE1[((word >> 16) & 0xff) as usize]
      ^ CRC_TABLE[(word >> 24) as usize];
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
