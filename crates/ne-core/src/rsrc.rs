//! Decoders for the structured resource types.
//!
//! These are the Windows 3.x on-disk layouts, which are ANSI and 16-bit
//! throughout -- not the Win32 forms with UTF-16 strings and `DWORD`
//! alignment that most modern tooling assumes.

use crate::read::latin1;

// ---------------------------------------------------------------- strings

/// Decode one `RT_STRING` block.
///
/// String resources are stored sixteen to a block; the resource whose id is
/// `n` holds string ids `(n - 1) * 16` through `(n - 1) * 16 + 15`. Empty
/// slots have a zero length byte and are skipped.
pub fn decode_string_block(data: &[u8], res_id: u16) -> Vec<(u16, String)> {
    let base = res_id.wrapping_sub(1).wrapping_mul(16);
    let mut out = Vec::new();
    let mut p = 0usize;
    for i in 0..16u16 {
        let Some(&len) = data.get(p) else { break };
        p += 1;
        let len = len as usize;
        if len == 0 {
            continue;
        }
        let Some(bytes) = data.get(p..p + len) else {
            break;
        };
        p += len;
        out.push((base.wrapping_add(i), latin1(bytes)));
    }
    out
}

// ------------------------------------------------------------------ menu

pub mod mf {
    pub const GRAYED: u16 = 0x0001;
    pub const DISABLED: u16 = 0x0002;
    pub const CHECKED: u16 = 0x0008;
    pub const POPUP: u16 = 0x0010;
    pub const MENUBARBREAK: u16 = 0x0020;
    pub const MENUBREAK: u16 = 0x0040;
    pub const END: u16 = 0x0080;
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub flags: u16,
    /// `None` for a popup, which has no command id of its own.
    pub id: Option<u16>,
    pub text: String,
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    pub fn is_separator(&self) -> bool {
        self.id == Some(0) && self.text.is_empty() && self.flags & mf::POPUP == 0
    }

    pub fn flag_names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.flags & mf::GRAYED != 0 {
            v.push("GRAYED");
        }
        if self.flags & mf::DISABLED != 0 {
            v.push("DISABLED");
        }
        if self.flags & mf::CHECKED != 0 {
            v.push("CHECKED");
        }
        if self.flags & mf::MENUBARBREAK != 0 {
            v.push("MENUBARBREAK");
        }
        if self.flags & mf::MENUBREAK != 0 {
            v.push("MENUBREAK");
        }
        v
    }
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub version: u16,
    pub items: Vec<MenuItem>,
}

pub fn decode_menu(data: &[u8]) -> Option<Menu> {
    if data.len() < 4 {
        return None;
    }
    let version = u16::from_le_bytes([data[0], data[1]]);
    let header_size = u16::from_le_bytes([data[2], data[3]]) as usize;
    let mut p = 4 + header_size;
    let items = decode_menu_items(data, &mut p, 0)?;
    Some(Menu { version, items })
}

fn decode_menu_items(data: &[u8], p: &mut usize, depth: usize) -> Option<Vec<MenuItem>> {
    // Malformed menus can nest without ever setting MF_END; cap the depth so
    // a bad resource cannot recurse the parser to death.
    if depth > 32 {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    loop {
        let flags = u16::from_le_bytes([*data.get(*p)?, *data.get(*p + 1)?]);
        *p += 2;
        let id = if flags & mf::POPUP == 0 {
            let v = u16::from_le_bytes([*data.get(*p)?, *data.get(*p + 1)?]);
            *p += 2;
            Some(v)
        } else {
            None
        };
        let text = read_cstr(data, p)?;
        let children = if flags & mf::POPUP != 0 {
            decode_menu_items(data, p, depth + 1)?
        } else {
            Vec::new()
        };
        out.push(MenuItem {
            flags,
            id,
            text,
            children,
        });
        if flags & mf::END != 0 {
            break;
        }
        if *p >= data.len() {
            break;
        }
    }
    Some(out)
}

fn read_cstr(data: &[u8], p: &mut usize) -> Option<String> {
    let start = *p;
    let end = data[start..].iter().position(|&b| b == 0)? + start;
    *p = end + 1;
    Some(latin1(&data[start..end]))
}

// ----------------------------------------------------------- accelerators

pub mod facc {
    pub const VIRTKEY: u8 = 0x01;
    pub const NOINVERT: u8 = 0x02;
    pub const SHIFT: u8 = 0x04;
    pub const CONTROL: u8 = 0x08;
    pub const ALT: u8 = 0x10;
    /// Marks the final entry of the table.
    pub const LAST: u8 = 0x80;
}

#[derive(Debug, Clone, Copy)]
pub struct Accel {
    pub flags: u8,
    pub event: u16,
    pub id: u16,
}

impl Accel {
    pub fn is_virtkey(&self) -> bool {
        self.flags & facc::VIRTKEY != 0
    }

    /// Human-readable key combination, e.g. `Ctrl+Shift+VK_F1`.
    pub fn key_name(&self) -> String {
        let mut s = String::new();
        if self.flags & facc::CONTROL != 0 {
            s.push_str("Ctrl+");
        }
        if self.flags & facc::SHIFT != 0 {
            s.push_str("Shift+");
        }
        if self.flags & facc::ALT != 0 {
            s.push_str("Alt+");
        }
        if self.is_virtkey() {
            s.push_str(&vk_name(self.event));
        } else {
            let c = self.event as u8;
            if c.is_ascii_graphic() {
                s.push('\'');
                s.push(c as char);
                s.push('\'');
            } else {
                s.push_str(&format!("{:#04X}", self.event));
            }
        }
        s
    }
}

pub fn decode_accelerators(data: &[u8]) -> Vec<Accel> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 5 <= data.len() {
        let a = Accel {
            flags: data[p],
            event: u16::from_le_bytes([data[p + 1], data[p + 2]]),
            id: u16::from_le_bytes([data[p + 3], data[p + 4]]),
        };
        p += 5;
        let last = a.flags & facc::LAST != 0;
        out.push(a);
        if last {
            break;
        }
    }
    out
}

pub fn vk_name(vk: u16) -> String {
    let name = match vk {
        0x01 => "VK_LBUTTON",
        0x02 => "VK_RBUTTON",
        0x03 => "VK_CANCEL",
        0x08 => "VK_BACK",
        0x09 => "VK_TAB",
        0x0D => "VK_RETURN",
        0x10 => "VK_SHIFT",
        0x11 => "VK_CONTROL",
        0x12 => "VK_MENU",
        0x13 => "VK_PAUSE",
        0x14 => "VK_CAPITAL",
        0x1B => "VK_ESCAPE",
        0x20 => "VK_SPACE",
        0x21 => "VK_PRIOR",
        0x22 => "VK_NEXT",
        0x23 => "VK_END",
        0x24 => "VK_HOME",
        0x25 => "VK_LEFT",
        0x26 => "VK_UP",
        0x27 => "VK_RIGHT",
        0x28 => "VK_DOWN",
        0x2C => "VK_SNAPSHOT",
        0x2D => "VK_INSERT",
        0x2E => "VK_DELETE",
        0x70..=0x87 => return format!("VK_F{}", vk - 0x6F),
        0x30..=0x39 | 0x41..=0x5A => return format!("'{}'", vk as u8 as char),
        _ => return format!("{vk:#04X}"),
    };
    name.to_string()
}

// ---------------------------------------------------------------- dialog

pub mod ds {
    pub const SETFONT: u32 = 0x0000_0040;
}

/// A dialog field that can be a string, an integer ordinal, or absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameOrOrd {
    None,
    Name(String),
    Ord(u16),
}

impl std::fmt::Display for NameOrOrd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameOrOrd::None => Ok(()),
            NameOrOrd::Name(s) => f.write_str(s),
            NameOrOrd::Ord(n) => write!(f, "#{n}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DialogItem {
    pub x: i16,
    pub y: i16,
    pub cx: i16,
    pub cy: i16,
    pub id: u16,
    pub style: u32,
    pub class: NameOrOrd,
    pub text: NameOrOrd,
    pub extra: Vec<u8>,
}

impl DialogItem {
    /// Predefined control classes are encoded as a single byte in the range
    /// 0x80..=0x85 rather than as a class-name string.
    pub fn class_name(&self) -> String {
        match &self.class {
            NameOrOrd::Ord(n) => match n {
                0x80 => "BUTTON".into(),
                0x81 => "EDIT".into(),
                0x82 => "STATIC".into(),
                0x83 => "LISTBOX".into(),
                0x84 => "SCROLLBAR".into(),
                0x85 => "COMBOBOX".into(),
                other => format!("class#{other}"),
            },
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub style: u32,
    pub x: i16,
    pub y: i16,
    pub cx: i16,
    pub cy: i16,
    pub menu: NameOrOrd,
    pub class: NameOrOrd,
    pub caption: String,
    pub font: Option<(u16, String)>,
    pub items: Vec<DialogItem>,
}

pub fn decode_dialog(data: &[u8]) -> Option<Dialog> {
    let mut p = 0usize;
    let style = read_u32(data, &mut p)?;
    let count = *data.get(p)?;
    p += 1;
    let x = read_i16(data, &mut p)?;
    let y = read_i16(data, &mut p)?;
    let cx = read_i16(data, &mut p)?;
    let cy = read_i16(data, &mut p)?;
    let menu = read_name_or_ord(data, &mut p)?;
    let class = read_name_or_ord(data, &mut p)?;
    let caption = read_cstr(data, &mut p)?;
    let font = if style & ds::SETFONT != 0 {
        let size = read_u16(data, &mut p)?;
        Some((size, read_cstr(data, &mut p)?))
    } else {
        None
    };

    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let ix = read_i16(data, &mut p)?;
        let iy = read_i16(data, &mut p)?;
        let icx = read_i16(data, &mut p)?;
        let icy = read_i16(data, &mut p)?;
        let id = read_u16(data, &mut p)?;
        let istyle = read_u32(data, &mut p)?;
        let class = read_class(data, &mut p)?;
        let text = read_name_or_ord(data, &mut p)?;
        let extra_len = *data.get(p)? as usize;
        p += 1;
        let extra = data.get(p..p + extra_len)?.to_vec();
        p += extra_len;
        items.push(DialogItem {
            x: ix,
            y: iy,
            cx: icx,
            cy: icy,
            id,
            style: istyle,
            class,
            text,
            extra,
        });
    }
    Some(Dialog {
        style,
        x,
        y,
        cx,
        cy,
        menu,
        class,
        caption,
        font,
        items,
    })
}

/// `0x00` means absent, `0xFF` introduces a 16-bit ordinal, anything else
/// starts a NUL-terminated string.
fn read_name_or_ord(data: &[u8], p: &mut usize) -> Option<NameOrOrd> {
    match *data.get(*p)? {
        0x00 => {
            *p += 1;
            Some(NameOrOrd::None)
        }
        0xFF => {
            *p += 1;
            Some(NameOrOrd::Ord(read_u16(data, p)?))
        }
        _ => Some(NameOrOrd::Name(read_cstr(data, p)?)),
    }
}

/// A control class is either a predefined single byte or a class-name string.
fn read_class(data: &[u8], p: &mut usize) -> Option<NameOrOrd> {
    let b = *data.get(*p)?;
    if (0x80..=0x8F).contains(&b) {
        *p += 1;
        return Some(NameOrOrd::Ord(b as u16));
    }
    read_name_or_ord(data, p)
}

fn read_u16(data: &[u8], p: &mut usize) -> Option<u16> {
    let v = u16::from_le_bytes([*data.get(*p)?, *data.get(*p + 1)?]);
    *p += 2;
    Some(v)
}

fn read_i16(data: &[u8], p: &mut usize) -> Option<i16> {
    read_u16(data, p).map(|v| v as i16)
}

fn read_u32(data: &[u8], p: &mut usize) -> Option<u32> {
    let v = u32::from_le_bytes([
        *data.get(*p)?,
        *data.get(*p + 1)?,
        *data.get(*p + 2)?,
        *data.get(*p + 3)?,
    ]);
    *p += 4;
    Some(v)
}

// --------------------------------------------------------------- version

pub const VS_FFI_SIGNATURE: u32 = 0xFEEF_04BD;

#[derive(Debug, Clone)]
pub struct FixedFileInfo {
    pub file_version: (u16, u16, u16, u16),
    pub product_version: (u16, u16, u16, u16),
    pub file_flags_mask: u32,
    pub file_flags: u32,
    pub file_os: u32,
    pub file_type: u32,
    pub file_subtype: u32,
}

impl FixedFileInfo {
    pub fn file_version_string(&self) -> String {
        let v = self.file_version;
        format!("{}.{}.{}.{}", v.0, v.1, v.2, v.3)
    }

    pub fn product_version_string(&self) -> String {
        let v = self.product_version;
        format!("{}.{}.{}.{}", v.0, v.1, v.2, v.3)
    }
}

/// One node of the `VS_VERSIONINFO` tree.
#[derive(Debug, Clone)]
pub struct VersionNode {
    pub key: String,
    /// Text value where the node's `wType` says the value is a string.
    pub value_text: Option<String>,
    pub value_bytes: Vec<u8>,
    pub children: Vec<VersionNode>,
}

#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub fixed: Option<FixedFileInfo>,
    pub root: VersionNode,
}

impl VersionInfo {
    /// Flatten the `StringFileInfo` subtree into `(key, value)` pairs.
    pub fn strings(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for sfi in self.root.children.iter().filter(|c| c.key == "StringFileInfo") {
            for table in &sfi.children {
                for s in &table.children {
                    out.push((s.key.clone(), s.value_text.clone().unwrap_or_default()));
                }
            }
        }
        out
    }
}

pub fn decode_version(data: &[u8]) -> Option<VersionInfo> {
    let (root, _) = decode_version_node(data, 0, 0)?;
    let fixed = if root.value_bytes.len() >= 52 {
        parse_fixed(&root.value_bytes)
    } else {
        None
    };
    Some(VersionInfo { fixed, root })
}

fn parse_fixed(v: &[u8]) -> Option<FixedFileInfo> {
    let rd = |o: usize| -> Option<u32> {
        Some(u32::from_le_bytes(v.get(o..o + 4)?.try_into().ok()?))
    };
    if rd(0)? != VS_FFI_SIGNATURE {
        return None;
    }
    let quad = |o: usize| -> Option<(u16, u16, u16, u16)> {
        let lo = rd(o)?;
        let hi = rd(o + 4)?;
        // Stored most-significant DWORD first, each holding two words.
        Some((
            (lo >> 16) as u16,
            (lo & 0xFFFF) as u16,
            (hi >> 16) as u16,
            (hi & 0xFFFF) as u16,
        ))
    };
    Some(FixedFileInfo {
        file_version: quad(8)?,
        product_version: quad(16)?,
        file_flags_mask: rd(24)?,
        file_flags: rd(28)?,
        file_os: rd(32)?,
        file_type: rd(36)?,
        file_subtype: rd(40)?,
    })
}

/// Win16 version nodes use ANSI keys and align each field to a DWORD relative
/// to the start of the resource.
fn decode_version_node(data: &[u8], start: usize, depth: usize) -> Option<(VersionNode, usize)> {
    if depth > 16 {
        return None;
    }
    let len = u16::from_le_bytes([*data.get(start)?, *data.get(start + 1)?]) as usize;
    let value_len = u16::from_le_bytes([*data.get(start + 2)?, *data.get(start + 3)?]) as usize;
    if len < 6 {
        return None;
    }
    let end = (start + len).min(data.len());

    let mut p = start + 4;
    let key_start = p;
    let key_end = data[p..end].iter().position(|&b| b == 0)? + p;
    let key = latin1(&data[key_start..key_end]);
    p = align4(key_end + 1, start);

    let value_bytes = data.get(p..(p + value_len).min(end))?.to_vec();
    // wValueLength counts characters for a text value and bytes for binary;
    // for the ANSI Win16 form those are the same, so a value that is all
    // printable and NUL-terminated is treated as text.
    let value_text = if value_len > 0 && value_bytes.iter().all(|&b| b == 0 || b >= 0x20) {
        Some(latin1(
            &value_bytes[..value_bytes.iter().position(|&b| b == 0).unwrap_or(value_bytes.len())],
        ))
    } else {
        None
    };
    p = align4(p + value_len, start);

    let mut children = Vec::new();
    while p + 6 <= end {
        let Some((child, next)) = decode_version_node(data, p, depth + 1) else {
            break;
        };
        if next <= p {
            break;
        }
        children.push(child);
        p = align4(next, start);
    }

    Some((
        VersionNode {
            key,
            value_text,
            value_bytes,
            children,
        },
        end,
    ))
}

fn align4(p: usize, base: usize) -> usize {
    base + ((p - base) + 3) / 4 * 4
}
