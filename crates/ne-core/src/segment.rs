//! Segment table entries.

use crate::reloc::Reloc;

pub mod flags {
    pub const DATA: u16 = 0x0001;
    pub const ALLOCATED: u16 = 0x0002;
    pub const LOADED: u16 = 0x0004;
    pub const ITERATED: u16 = 0x0008;
    pub const MOVEABLE: u16 = 0x0010;
    pub const SHAREABLE: u16 = 0x0020;
    pub const PRELOAD: u16 = 0x0040;
    pub const EXECUTEONLY: u16 = 0x0080;
    pub const RELOCINFO: u16 = 0x0100;
    pub const CONFORMING: u16 = 0x0200;
    pub const DISCARDABLE: u16 = 0x1000;
    /// For a data segment this bit means READONLY instead of EXECUTEONLY.
    pub const READONLY: u16 = 0x0080;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SegKind {
    Code,
    Data,
}

impl SegKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SegKind::Code => "CODE",
            SegKind::Data => "DATA",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    /// 1-based, matching the numbering relocations and entry points use.
    pub index: u16,
    /// File offset, already shifted by the header's alignment unit. Zero for
    /// a segment with no file image (BSS-like allocation only).
    pub file_offset: u64,
    /// Bytes present in the file.
    pub length: u32,
    /// Bytes the loader allocates; larger than `length` for a segment with
    /// uninitialised tail space.
    pub min_alloc: u32,
    pub flags: u16,
    pub data: Vec<u8>,
    pub relocs: Vec<Reloc>,
}

impl Segment {
    pub fn kind(&self) -> SegKind {
        if self.flags & flags::DATA != 0 {
            SegKind::Data
        } else {
            SegKind::Code
        }
    }

    pub fn is_code(&self) -> bool {
        self.kind() == SegKind::Code
    }

    pub fn is_moveable(&self) -> bool {
        self.flags & flags::MOVEABLE != 0
    }

    pub fn has_relocs(&self) -> bool {
        self.flags & flags::RELOCINFO != 0
    }

    pub fn flag_names(&self) -> Vec<&'static str> {
        use flags::*;
        let mut v = vec![self.kind().as_str()];
        if self.flags & MOVEABLE != 0 {
            v.push("MOVEABLE");
        }
        if self.flags & SHAREABLE != 0 {
            v.push("SHAREABLE");
        }
        if self.flags & PRELOAD != 0 {
            v.push("PRELOAD");
        }
        if self.flags & ITERATED != 0 {
            v.push("ITERATED");
        }
        if self.flags & EXECUTEONLY != 0 {
            v.push(if self.kind() == SegKind::Data {
                "READONLY"
            } else {
                "EXECUTEONLY"
            });
        }
        if self.flags & RELOCINFO != 0 {
            v.push("RELOC");
        }
        if self.flags & CONFORMING != 0 {
            v.push("CONFORMING");
        }
        if self.flags & DISCARDABLE != 0 {
            v.push("DISCARDABLE");
        }
        v
    }
}
