//! Gzip member parse + inflate (RFC 1952).

use super::DecompressError;
use super::crc32::{crc32, update_crc};
use super::inflate;
use alloc::vec::Vec;

const ID1: u8 = 0x1f;
const ID2: u8 = 0x8b;
const CM_DEFLATE: u8 = 8;

const FHCRC: u8 = 1 << 1;
const FEXTRA: u8 = 1 << 2;
const FNAME: u8 = 1 << 3;
const FCOMMENT: u8 = 1 << 4;
const RESERVED: u8 = 0xe0;

/// Decompress one gzip member.
pub(super) fn decompress_member(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  let mut i = 0usize;

  let id1 = *data.get(i).ok_or(DecompressError::InvalidInput)?;
  i = i.saturating_add(1);
  let id2 = *data.get(i).ok_or(DecompressError::InvalidInput)?;
  i = i.saturating_add(1);
  if id1 != ID1 || id2 != ID2 {
    return Err(DecompressError::InvalidInput);
  }

  let cm = *data.get(i).ok_or(DecompressError::InvalidInput)?;
  i = i.saturating_add(1);
  if cm != CM_DEFLATE {
    return Err(DecompressError::InvalidInput);
  }

  let flg = *data.get(i).ok_or(DecompressError::InvalidInput)?;
  i = i.saturating_add(1);
  // RFC 1952 §2.3.1.2: reserved FLG bits MUST be zero.
  if flg & RESERVED != 0 {
    return Err(DecompressError::InvalidInput);
  }

  // MTIME (4) + XFL (1) + OS (1)
  i = i.checked_add(6).ok_or(DecompressError::InvalidInput)?;
  if i > data.len() {
    return Err(DecompressError::InvalidInput);
  }

  if flg & FEXTRA != 0 {
    let b0 = *data.get(i).ok_or(DecompressError::InvalidInput)?;
    let b1 = *data.get(i.saturating_add(1)).ok_or(DecompressError::InvalidInput)?;
    let xlen = usize::from(u16::from_le_bytes([b0, b1]));
    i = i
      .checked_add(2)
      .and_then(|v| v.checked_add(xlen))
      .ok_or(DecompressError::InvalidInput)?;
    if i > data.len() {
      return Err(DecompressError::InvalidInput);
    }
  }

  if flg & FNAME != 0 {
    i = skip_cstr(data, i)?;
  }
  if flg & FCOMMENT != 0 {
    i = skip_cstr(data, i)?;
  }

  if flg & FHCRC != 0 {
    let header = data.get(..i).ok_or(DecompressError::InvalidInput)?;
    let expect = u16::try_from(crc32(header) & 0xffff).map_err(|_| DecompressError::InvalidInput)?;
    let b0 = *data.get(i).ok_or(DecompressError::InvalidInput)?;
    let b1 = *data.get(i.saturating_add(1)).ok_or(DecompressError::InvalidInput)?;
    let got = u16::from_le_bytes([b0, b1]);
    if got != expect {
      return Err(DecompressError::InvalidInput);
    }
    i = i.checked_add(2).ok_or(DecompressError::InvalidInput)?;
  }

  let deflate = data.get(i..).ok_or(DecompressError::InvalidInput)?;
  let (out, consumed) = inflate::inflate(deflate, max_out)?;
  let trailer_off = i.checked_add(consumed).ok_or(DecompressError::InvalidInput)?;
  let b0 = *data.get(trailer_off).ok_or(DecompressError::InvalidInput)?;
  let b1 = *data.get(trailer_off.saturating_add(1)).ok_or(DecompressError::InvalidInput)?;
  let b2 = *data.get(trailer_off.saturating_add(2)).ok_or(DecompressError::InvalidInput)?;
  let b3 = *data.get(trailer_off.saturating_add(3)).ok_or(DecompressError::InvalidInput)?;
  let crc_got = u32::from_le_bytes([b0, b1, b2, b3]);
  let i0 = *data.get(trailer_off.saturating_add(4)).ok_or(DecompressError::InvalidInput)?;
  let i1 = *data.get(trailer_off.saturating_add(5)).ok_or(DecompressError::InvalidInput)?;
  let i2 = *data.get(trailer_off.saturating_add(6)).ok_or(DecompressError::InvalidInput)?;
  let i3 = *data.get(trailer_off.saturating_add(7)).ok_or(DecompressError::InvalidInput)?;
  let isize = u32::from_le_bytes([i0, i1, i2, i3]);

  let crc_expect = update_crc(0, &out);
  if crc_got != crc_expect {
    return Err(DecompressError::InvalidInput);
  }
  let isize_expect = u32::try_from(out.len() & 0xffff_ffff).map_err(|_| DecompressError::InvalidInput)?;
  if isize != isize_expect {
    return Err(DecompressError::InvalidInput);
  }
  Ok(out)
}

fn skip_cstr(
  data: &[u8],
  mut i: usize,
) -> Result<usize, DecompressError> {
  loop {
    let b = *data.get(i).ok_or(DecompressError::InvalidInput)?;
    i = i.checked_add(1).ok_or(DecompressError::InvalidInput)?;
    if b == 0 {
      return Ok(i);
    }
  }
}
