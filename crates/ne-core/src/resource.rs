//! The resource table, and reassembly of resources into standalone files.

use crate::dib;
use std::fmt;

pub mod rt {
    pub const CURSOR: u16 = 1;
    pub const BITMAP: u16 = 2;
    pub const ICON: u16 = 3;
    pub const MENU: u16 = 4;
    pub const DIALOG: u16 = 5;
    pub const STRING: u16 = 6;
    pub const FONTDIR: u16 = 7;
    pub const FONT: u16 = 8;
    pub const ACCELERATOR: u16 = 9;
    pub const RCDATA: u16 = 10;
    pub const MESSAGETABLE: u16 = 11;
    pub const GROUP_CURSOR: u16 = 12;
    pub const GROUP_ICON: u16 = 14;
    pub const NAMETABLE: u16 = 15;
    pub const VERSION: u16 = 16;
}

pub mod resflags {
    pub const MOVEABLE: u16 = 0x0010;
    pub const PURE: u16 = 0x0020;
    pub const PRELOAD: u16 = 0x0040;
    pub const DISCARDABLE: u16 = 0x1000;
}

/// A resource type or name: either a 16-bit integer or a string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ResId {
    Id(u16),
    Name(String),
}

impl ResId {
    pub fn as_id(&self) -> Option<u16> {
        match self {
            ResId::Id(n) => Some(*n),
            ResId::Name(_) => None,
        }
    }
}

impl fmt::Display for ResId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResId::Id(n) => write!(f, "#{n}"),
            ResId::Name(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub type_id: ResId,
    pub res_id: ResId,
    /// File offset, already shifted by the resource alignment unit.
    pub offset: u64,
    pub length: u32,
    pub flags: u16,
}

impl Resource {
    pub fn type_name(&self) -> String {
        match &self.type_id {
            ResId::Name(s) => s.clone(),
            ResId::Id(n) => standard_type_name(*n)
                .map(str::to_string)
                .unwrap_or_else(|| format!("TYPE{n}")),
        }
    }

    /// Filename stem used when extracting, safe for any filesystem.
    pub fn label(&self) -> String {
        let id = match &self.res_id {
            ResId::Id(n) => n.to_string(),
            ResId::Name(s) => s.clone(),
        };
        sanitize(&format!("{}_{}", self.type_name(), id))
    }

    /// Extension matching what `file_bytes` produces.
    pub fn extension(&self) -> &'static str {
        match self.type_id.as_id() {
            Some(rt::BITMAP) => "bmp",
            Some(rt::ICON) | Some(rt::GROUP_ICON) => "ico",
            Some(rt::CURSOR) | Some(rt::GROUP_CURSOR) => "cur",
            Some(rt::FONT) => "fnt",
            Some(rt::VERSION) => "ver",
            _ => "bin",
        }
    }

    pub fn flag_names(&self) -> Vec<&'static str> {
        use resflags::*;
        let mut v = Vec::new();
        if self.flags & MOVEABLE != 0 {
            v.push("MOVEABLE");
        }
        if self.flags & PURE != 0 {
            v.push("PURE");
        }
        if self.flags & PRELOAD != 0 {
            v.push("PRELOAD");
        }
        if self.flags & DISCARDABLE != 0 {
            v.push("DISCARDABLE");
        }
        v
    }
}

pub fn standard_type_name(n: u16) -> Option<&'static str> {
    Some(match n {
        rt::CURSOR => "CURSOR",
        rt::BITMAP => "BITMAP",
        rt::ICON => "ICON",
        rt::MENU => "MENU",
        rt::DIALOG => "DIALOG",
        rt::STRING => "STRING",
        rt::FONTDIR => "FONTDIR",
        rt::FONT => "FONT",
        rt::ACCELERATOR => "ACCELERATOR",
        rt::RCDATA => "RCDATA",
        rt::MESSAGETABLE => "MESSAGETABLE",
        rt::GROUP_CURSOR => "GROUP_CURSOR",
        rt::GROUP_ICON => "GROUP_ICON",
        rt::NAMETABLE => "NAMETABLE",
        rt::VERSION => "VERSION",
        _ => return None,
    })
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// One entry of an `RT_GROUP_ICON` / `RT_GROUP_CURSOR` directory.
#[derive(Debug, Clone, Copy)]
pub struct GroupEntry {
    pub width: u16,
    pub height: u16,
    pub color_count: u8,
    pub planes: u16,
    pub bit_count: u16,
    pub bytes_in_res: u32,
    /// Ordinal of the `RT_ICON` / `RT_CURSOR` resource holding the image.
    pub res_ordinal: u16,
}

#[derive(Debug, Clone)]
pub struct GroupDir {
    /// 1 for icons, 2 for cursors.
    pub res_type: u16,
    pub entries: Vec<GroupEntry>,
}

impl GroupDir {
    /// The directory layouts differ: an icon entry stores width and height as
    /// bytes with a color count, a cursor entry stores them as words and its
    /// "planes/bitcount" slots are really the hotspot.
    pub fn parse(data: &[u8], is_cursor: bool) -> Option<GroupDir> {
        if data.len() < 6 {
            return None;
        }
        let res_type = u16::from_le_bytes([data[2], data[3]]);
        let count = u16::from_le_bytes([data[4], data[5]]) as usize;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let o = 6 + i * 14;
            let e = data.get(o..o + 14)?;
            let entry = if is_cursor {
                GroupEntry {
                    width: u16::from_le_bytes([e[0], e[1]]),
                    // A cursor directory records the mask height too.
                    height: u16::from_le_bytes([e[2], e[3]]) / 2,
                    color_count: 0,
                    planes: u16::from_le_bytes([e[4], e[5]]),
                    bit_count: u16::from_le_bytes([e[6], e[7]]),
                    bytes_in_res: u32::from_le_bytes([e[8], e[9], e[10], e[11]]),
                    res_ordinal: u16::from_le_bytes([e[12], e[13]]),
                }
            } else {
                GroupEntry {
                    width: if e[0] == 0 { 256 } else { e[0] as u16 },
                    height: if e[1] == 0 { 256 } else { e[1] as u16 },
                    color_count: e[2],
                    planes: u16::from_le_bytes([e[4], e[5]]),
                    bit_count: u16::from_le_bytes([e[6], e[7]]),
                    bytes_in_res: u32::from_le_bytes([e[8], e[9], e[10], e[11]]),
                    res_ordinal: u16::from_le_bytes([e[12], e[13]]),
                }
            };
            entries.push(entry);
        }
        Some(GroupDir { res_type, entries })
    }
}

/// Build a `.ico` or `.cur` file from a group directory and its images.
///
/// `images` are the raw `RT_ICON` / `RT_CURSOR` resource bodies in directory
/// order. A cursor body is prefixed with a 4-byte hotspot that belongs in the
/// directory entry, not in the image, so it is moved rather than copied.
pub fn build_icon_file(dir: &GroupDir, images: &[Vec<u8>]) -> Vec<u8> {
    let is_cursor = dir.res_type == 2;
    let n = dir.entries.len().min(images.len());
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&dir.res_type.to_le_bytes());
    out.extend_from_slice(&(n as u16).to_le_bytes());

    let mut offset = 6 + n * 16;
    let mut bodies = Vec::with_capacity(n);
    for i in 0..n {
        let e = &dir.entries[i];
        let raw = &images[i];
        let (hotspot, body) = if is_cursor && raw.len() >= 4 {
            (
                (
                    u16::from_le_bytes([raw[0], raw[1]]),
                    u16::from_le_bytes([raw[2], raw[3]]),
                ),
                raw[4..].to_vec(),
            )
        } else {
            ((0, 0), raw.clone())
        };
        let (w, h) = (e.width.min(255) as u8, e.height.min(255) as u8);
        out.push(w);
        out.push(h);
        out.push(e.color_count);
        out.push(0);
        if is_cursor {
            out.extend_from_slice(&hotspot.0.to_le_bytes());
            out.extend_from_slice(&hotspot.1.to_le_bytes());
        } else {
            out.extend_from_slice(&e.planes.to_le_bytes());
            out.extend_from_slice(&e.bit_count.to_le_bytes());
        }
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += body.len();
        bodies.push(body);
    }
    for b in bodies {
        out.extend_from_slice(&b);
    }
    out
}

/// A standalone `.ico` / `.cur` wrapping one loose `RT_ICON` / `RT_CURSOR`,
/// for images not referenced by any group directory.
pub fn build_single_icon_file(raw: &[u8], is_cursor: bool) -> Option<Vec<u8>> {
    let (hotspot, body) = if is_cursor && raw.len() >= 4 {
        (
            (
                u16::from_le_bytes([raw[0], raw[1]]),
                u16::from_le_bytes([raw[2], raw[3]]),
            ),
            &raw[4..],
        )
    } else {
        ((0, 0), raw)
    };
    let info = dib::DibInfo::parse(body)?;
    let dir = GroupDir {
        res_type: if is_cursor { 2 } else { 1 },
        entries: vec![GroupEntry {
            width: info.abs_width() as u16,
            height: (info.abs_height() / 2) as u16,
            color_count: if info.bit_count <= 8 {
                (1u16 << info.bit_count).min(256) as u8
            } else {
                0
            },
            planes: if is_cursor { hotspot.0 } else { info.planes },
            bit_count: if is_cursor { hotspot.1 } else { info.bit_count },
            bytes_in_res: body.len() as u32,
            res_ordinal: 0,
        }],
    };
    Some(build_icon_file(&dir, std::slice::from_ref(&raw.to_vec())))
}
