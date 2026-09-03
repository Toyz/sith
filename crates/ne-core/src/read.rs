//! Bounds-checked little-endian readers. Every field in an NE file comes from
//! a byte range that may be past the end of a truncated or hand-edited image,
//! so nothing indexes the buffer directly.

use crate::{Error, Result};

pub fn u8(buf: &[u8], off: usize) -> Result<u8> {
    buf.get(off).copied().ok_or(Error::Truncated {
        what: "u8",
        offset: off as u64,
    })
}

pub fn u16(buf: &[u8], off: usize) -> Result<u16> {
    let b = buf.get(off..off + 2).ok_or(Error::Truncated {
        what: "u16",
        offset: off as u64,
    })?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

pub fn u32(buf: &[u8], off: usize) -> Result<u32> {
    let b = buf.get(off..off + 4).ok_or(Error::Truncated {
        what: "u32",
        offset: off as u64,
    })?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A length-prefixed, non-terminated name as used by every NE name table.
pub fn pascal_string(buf: &[u8], off: usize) -> Result<(String, usize)> {
    let len = u8(buf, off)? as usize;
    let bytes = buf.get(off + 1..off + 1 + len).ok_or(Error::Truncated {
        what: "pascal string",
        offset: off as u64,
    })?;
    Ok((latin1(bytes), 1 + len))
}

/// NE files predate Unicode; every string is a codepage-dependent byte run.
/// Latin-1 is the lossless choice: it round-trips all 256 values so nothing is
/// silently destroyed, and it renders correctly for the common case.
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}
