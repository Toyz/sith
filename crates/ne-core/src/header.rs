//! The NE header proper, at the offset the MZ stub's `e_lfanew` points to.

use crate::read::*;
use crate::{Error, Result};

/// Program flags (offset 0x0C).
pub mod flags {
    pub const DGROUP_NONE: u16 = 0x0000;
    pub const DGROUP_SINGLEDATA: u16 = 0x0001;
    pub const DGROUP_MULTIPLEDATA: u16 = 0x0002;
    pub const GLOBAL_INIT: u16 = 0x0004;
    pub const PROTECTED_MODE_ONLY: u16 = 0x0008;
    pub const INS8086: u16 = 0x0010;
    pub const INS80286: u16 = 0x0020;
    pub const INS80386: u16 = 0x0040;
    pub const INS8087: u16 = 0x0080;
    pub const LINK_ERRORS: u16 = 0x2000;
    pub const LIBRARY: u16 = 0x8000;
}

/// Application flags (offset 0x0D, the high byte of the 0x0C word).
pub mod appflags {
    pub const FULLSCREEN: u16 = 0x0100;
    pub const WINPMCOMPAT: u16 = 0x0200;
    pub const WINPMUSES: u16 = 0x0300;
    pub const OS2FAMILY: u16 = 0x0800;
    pub const SELFLOAD: u16 = 0x0800;
}

/// Target operating system (offset 0x36).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum TargetOs {
    Unknown,
    Os2,
    Windows,
    DosV4,
    Windows386,
    Boss,
    Other(u8),
}

impl TargetOs {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => TargetOs::Unknown,
            1 => TargetOs::Os2,
            2 => TargetOs::Windows,
            3 => TargetOs::DosV4,
            4 => TargetOs::Windows386,
            5 => TargetOs::Boss,
            n => TargetOs::Other(n),
        }
    }

    pub fn name(&self) -> String {
        match self {
            TargetOs::Unknown => "unknown".into(),
            TargetOs::Os2 => "OS/2".into(),
            TargetOs::Windows => "Windows".into(),
            TargetOs::DosV4 => "European MS-DOS 4.x".into(),
            TargetOs::Windows386 => "Windows 386".into(),
            TargetOs::Boss => "BOSS".into(),
            TargetOs::Other(n) => format!("os{n}"),
        }
    }
}

/// The fixed part of the NE header. Field names follow the `IMAGE_OS2_HEADER`
/// layout from the Windows 3.1 SDK so they can be cross-checked against the
/// original documentation.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NeHeader {
    /// File offset of the `NE` signature.
    pub ne_offset: u32,
    pub linker_version: (u8, u8),
    /// Entry table offset, relative to the NE header.
    pub entry_table: u16,
    pub entry_table_len: u16,
    pub crc: u32,
    pub flags: u16,
    /// Segment number of the automatic data segment, or 0 if there is none.
    pub auto_data_segment: u16,
    pub heap_size: u16,
    pub stack_size: u16,
    /// Initial CS:IP, packed as `(segment << 16) | offset`.
    pub cs_ip: u32,
    /// Initial SS:SP, packed as `(segment << 16) | offset`.
    pub ss_sp: u32,
    pub segment_count: u16,
    pub module_ref_count: u16,
    pub nonresident_names_len: u16,
    pub segment_table: u16,
    pub resource_table: u16,
    pub resident_names_table: u16,
    pub module_ref_table: u16,
    pub imported_names_table: u16,
    /// Absolute file offset (not NE-relative) of the non-resident name table.
    pub nonresident_names_table: u32,
    pub moveable_entry_count: u16,
    /// Log2 of the segment/resource alignment unit.
    pub align_shift: u16,
    pub resource_count: u16,
    pub target_os: TargetOs,
    pub other_flags: u8,
    pub gangload_offset: u16,
    pub gangload_len: u16,
    pub swap_area: u16,
    /// Expected Windows version, packed as `(major << 8) | minor`.
    pub expected_version: u16,
}

impl NeHeader {
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < 0x40 || &buf[..2] != b"MZ" {
            return Err(Error::NotMz);
        }
        let ne_offset = u32(buf, 0x3C)?;
        let n = ne_offset as usize;
        if buf.len() < n + 0x40 {
            return Err(Error::Truncated {
                what: "NE header",
                offset: ne_offset as u64,
            });
        }
        if &buf[n..n + 2] != b"NE" {
            return Err(Error::NotNe { offset: ne_offset });
        }
        Ok(NeHeader {
            ne_offset,
            linker_version: (buf[n + 2], buf[n + 3]),
            entry_table: u16(buf, n + 0x04)?,
            entry_table_len: u16(buf, n + 0x06)?,
            crc: u32(buf, n + 0x08)?,
            flags: u16(buf, n + 0x0C)?,
            auto_data_segment: u16(buf, n + 0x0E)?,
            heap_size: u16(buf, n + 0x10)?,
            stack_size: u16(buf, n + 0x12)?,
            cs_ip: u32(buf, n + 0x14)?,
            ss_sp: u32(buf, n + 0x18)?,
            segment_count: u16(buf, n + 0x1C)?,
            module_ref_count: u16(buf, n + 0x1E)?,
            nonresident_names_len: u16(buf, n + 0x20)?,
            segment_table: u16(buf, n + 0x22)?,
            resource_table: u16(buf, n + 0x24)?,
            resident_names_table: u16(buf, n + 0x26)?,
            module_ref_table: u16(buf, n + 0x28)?,
            imported_names_table: u16(buf, n + 0x2A)?,
            nonresident_names_table: u32(buf, n + 0x2C)?,
            moveable_entry_count: u16(buf, n + 0x30)?,
            align_shift: u16(buf, n + 0x32)?,
            resource_count: u16(buf, n + 0x34)?,
            target_os: TargetOs::from_byte(buf[n + 0x36]),
            other_flags: buf[n + 0x37],
            gangload_offset: u16(buf, n + 0x38)?,
            gangload_len: u16(buf, n + 0x3A)?,
            swap_area: u16(buf, n + 0x3C)?,
            expected_version: u16(buf, n + 0x3E)?,
        })
    }

    /// A DLL rather than an application.
    pub fn is_library(&self) -> bool {
        self.flags & flags::LIBRARY != 0
    }

    /// Self-loading modules supply their own segment loader; the first 16
    /// bytes of segment 1 are a jump table the loader calls instead of
    /// reading segments itself.
    pub fn is_self_loading(&self) -> bool {
        self.flags & appflags::SELFLOAD != 0 && !self.is_library()
    }

    /// Alignment unit for segment and resource file offsets.
    pub fn align_shift_or_default(&self) -> u16 {
        if self.align_shift == 0 {
            9
        } else {
            self.align_shift
        }
    }

    pub fn flag_names(&self) -> Vec<&'static str> {
        use flags::*;
        let mut v = Vec::new();
        if self.flags & DGROUP_SINGLEDATA != 0 {
            v.push("SINGLEDATA");
        }
        if self.flags & DGROUP_MULTIPLEDATA != 0 {
            v.push("MULTIPLEDATA");
        }
        if self.flags & GLOBAL_INIT != 0 {
            v.push("GLOBAL_INIT");
        }
        if self.flags & PROTECTED_MODE_ONLY != 0 {
            v.push("PMODE_ONLY");
        }
        if self.flags & INS8086 != 0 {
            v.push("8086");
        }
        if self.flags & INS80286 != 0 {
            v.push("286");
        }
        if self.flags & INS80386 != 0 {
            v.push("386");
        }
        if self.flags & INS8087 != 0 {
            v.push("8087");
        }
        if self.flags & LINK_ERRORS != 0 {
            v.push("LINK_ERRORS");
        }
        if self.flags & LIBRARY != 0 {
            v.push("LIBRARY");
        }
        v
    }
}
