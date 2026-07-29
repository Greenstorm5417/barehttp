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
      k = k.saturating_add(1);
    }
    table[n as usize] = c;
    n = n.saturating_add(1);
  }
  table
}

const CRC_TABLE: [u32; 256] = make_crc_table();

/// CRC of `buf` with running value `crc` (init `0`; pre/post conditioning inside).
pub(super) fn update_crc(
  crc: u32,
  buf: &[u8],
) -> u32 {
  let mut c = crc ^ 0xffff_ffff;
  for &byte in buf {
    let idx = usize::try_from((c ^ u32::from(byte)) & 0xff).unwrap_or(0);
    let entry = match CRC_TABLE.get(idx) {
      Some(&e) => e,
      None => 0,
    };
    c = entry ^ (c >> 8);
  }
  c ^ 0xffff_ffff
}

/// CRC-32 of `buf`.
pub(super) fn crc32(buf: &[u8]) -> u32 {
  update_crc(0, buf)
}
