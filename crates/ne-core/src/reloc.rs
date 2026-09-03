//! Segment relocations, and the fixup chains they are threaded through.
//!
//! An NE relocation record names one *chain*, not one site. Where the record
//! is non-additive, the word at the fixup site holds the offset of the next
//! site to patch, terminated by 0xFFFF -- so the operand bytes of an unfixed
//! far call contain a link pointer, not an address. Disassembling the raw
//! bytes without walking these chains produces plausible but wrong targets,
//! which is the single biggest trap in reading NE code.

use std::fmt;

/// What the fixup writes at the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum AddrType {
    /// Low byte of the target offset.
    LoByte,
    /// 16-bit segment selector.
    Segment,
    /// 32-bit far pointer, `offset:segment`.
    Far,
    /// 16-bit offset within the target segment.
    Offset,
    /// 48-bit far pointer, `offset32:segment`.
    Far48,
    /// 32-bit offset within the target segment.
    Offset32,
    Other(u8),
}

impl AddrType {
    pub fn from_byte(b: u8) -> Self {
        match b & 0x0F {
            0 => AddrType::LoByte,
            2 => AddrType::Segment,
            3 => AddrType::Far,
            5 => AddrType::Offset,
            11 => AddrType::Far48,
            13 => AddrType::Offset32,
            n => AddrType::Other(n),
        }
    }

    /// Bytes the fixup writes at the site.
    pub fn width(self) -> usize {
        match self {
            AddrType::LoByte => 1,
            AddrType::Segment | AddrType::Offset => 2,
            AddrType::Far | AddrType::Offset32 => 4,
            AddrType::Far48 => 6,
            AddrType::Other(_) => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AddrType::LoByte => "lobyte",
            AddrType::Segment => "segment",
            AddrType::Far => "far",
            AddrType::Offset => "offset",
            AddrType::Far48 => "far48",
            AddrType::Offset32 => "off32",
            AddrType::Other(_) => "?",
        }
    }
}

impl fmt::Display for AddrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddrType::Other(n) => write!(f, "a{n}"),
            other => f.write_str(other.as_str()),
        }
    }
}

/// Where the fixup points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum RelKind {
    /// Another segment of this same module.
    Internal,
    /// An import named by ordinal.
    ImportOrdinal,
    /// An import named by string.
    ImportName,
    /// A kernel-supplied fixup (floating point emulator entry points).
    OsFixup,
}

impl RelKind {
    pub fn from_byte(b: u8) -> Self {
        match b & 3 {
            0 => RelKind::Internal,
            1 => RelKind::ImportOrdinal,
            2 => RelKind::ImportName,
            _ => RelKind::OsFixup,
        }
    }
}

/// One 8-byte record from a segment's relocation table.
#[derive(Debug, Clone, Copy)]
pub struct Reloc {
    pub addr_type: AddrType,
    pub kind: RelKind,
    /// The stored value is an addend applied to what is already at the site,
    /// and the record covers exactly one site rather than a chain.
    pub additive: bool,
    /// First site, or the only site for an additive fixup.
    pub offset: u16,
    pub target1: u16,
    pub target2: u16,
}

impl Reloc {
    pub fn parse(b: &[u8]) -> Self {
        Reloc {
            addr_type: AddrType::from_byte(b[0]),
            kind: RelKind::from_byte(b[1]),
            additive: b[1] & 4 != 0,
            offset: u16::from_le_bytes([b[2], b[3]]),
            target1: u16::from_le_bytes([b[4], b[5]]),
            target2: u16::from_le_bytes([b[6], b[7]]),
        }
    }

    /// Every offset in `data` this record patches.
    ///
    /// The chain is stored in the file image itself: the link word sits at
    /// the site, and 0xFFFF ends the list. Corrupt images can produce cycles,
    /// so visited offsets are tracked and revisits stop the walk.
    pub fn sites(&self, data: &[u8]) -> Vec<u16> {
        if self.additive {
            return vec![self.offset];
        }
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut off = self.offset;
        while off != 0xFFFF && seen.insert(off) {
            out.push(off);
            let i = off as usize;
            let Some(link) = data.get(i..i + 2) else { break };
            let next = u16::from_le_bytes([link[0], link[1]]);
            if next == off {
                break;
            }
            off = next;
        }
        out
    }
}

/// A fixup target with module and export names already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Target {
    /// A segment of this module.
    ///
    /// `offset` is `None` when the record does not carry it and it cannot be
    /// recovered from the code -- an `ADDR_SEGMENT` fixup patching a bare
    /// `mov ax, seg X` has no offset word to read (see
    /// `NeFile::fixup_target`).
    Internal { segment: u16, offset: Option<u16> },
    /// An entry-table slot of this module, reached through the movable-entry
    /// thunk rather than a direct segment reference.
    Entry {
        ordinal: u16,
        name: Option<String>,
        segment: u16,
        offset: u16,
    },
    ImportOrdinal {
        module: String,
        ordinal: u16,
        name: Option<String>,
    },
    ImportName {
        module: String,
        name: String,
    },
    OsFixup {
        target1: u16,
        target2: u16,
    },
}

impl Target {
    /// The symbol name, where the target has one.
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Target::Entry { name, .. } => name.as_deref(),
            Target::ImportOrdinal { name, .. } => name.as_deref(),
            Target::ImportName { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The module the target lives in, or `None` for an internal target.
    pub fn module(&self) -> Option<&str> {
        match self {
            Target::ImportOrdinal { module, .. } | Target::ImportName { module, .. } => {
                Some(module)
            }
            _ => None,
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Internal { segment, offset } => match offset {
                Some(o) => write!(f, "seg{segment:02X}:{o:04X}"),
                None => write!(f, "seg{segment:02X}:????"),
            },
            Target::Entry {
                ordinal,
                name,
                segment,
                offset,
            } => write!(
                f,
                "{}@{ordinal}[seg{segment:02X}:{offset:04X}]",
                name.as_deref().unwrap_or("ENTRY")
            ),
            Target::ImportOrdinal {
                module,
                ordinal,
                name,
            } => match name {
                Some(n) => write!(f, "{module}.{n}"),
                None => write!(f, "{module}.@{ordinal}"),
            },
            Target::ImportName { module, name } => write!(f, "{module}.{name}"),
            Target::OsFixup { target1, target2 } => {
                write!(f, "OSFIXUP({target1:04X},{target2:04X})")
            }
        }
    }
}

/// A single patch site: which record covers it and where the record points.
#[derive(Debug, Clone)]
pub struct Fixup {
    pub site: u16,
    pub addr_type: AddrType,
    pub additive: bool,
    pub target: Target,
}
