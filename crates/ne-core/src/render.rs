//! Text rendering of decoded resources, shared by the CLI and the GUI so both
//! show the same thing.

use crate::resource::rt;
use crate::rsrc::{self, Dialog, Menu, MenuItem, VersionNode};
use crate::{NeFile, Resource};

/// Render a resource as text, or `None` if the type has no text form.
pub fn resource_text(ne: &NeFile, r: &Resource) -> Option<String> {
    let data = ne.resource_data(r);
    match r.type_id.as_id()? {
        rt::STRING => {
            let id = r.res_id.as_id()?;
            let mut s = String::new();
            for (n, text) in rsrc::decode_string_block(data, id) {
                s.push_str(&format!("{n:>6}  {text}\n"));
            }
            Some(s)
        }
        rt::MENU => rsrc::decode_menu(data).map(|m| menu_text(&m)),
        rt::DIALOG => rsrc::decode_dialog(data).map(|d| dialog_text(&d)),
        rt::ACCELERATOR => {
            let accels = rsrc::decode_accelerators(data);
            if accels.is_empty() {
                return None;
            }
            let mut s = String::new();
            for a in accels {
                s.push_str(&format!(
                    "  {:<24} id {:<6} flags {:02X}\n",
                    a.key_name(),
                    a.id,
                    a.flags
                ));
            }
            Some(s)
        }
        rt::VERSION => rsrc::decode_version(data).map(|v| version_text(&v)),
        rt::FONT => crate::fnt::parse(data).map(|f| font_text(&f)).or_else(|| {
            // A vector font has no bitmaps, but its header still says what it
            // is, and saying so beats a hex dump.
            crate::fnt::parse_header(data).map(|h| {
                format!(
                    "vector font, version {}.{}\n{} point, weight {}\n",
                    h.version >> 8,
                    h.version & 0xFF,
                    h.points,
                    h.weight
                )
            })
        }),
        rt::FONTDIR => Some(fontdir_text(ne, data)),
        rt::BITMAP | rt::ICON | rt::CURSOR => {
            let body = if r.type_id.as_id() == Some(rt::CURSOR) && data.len() > 4 {
                &data[4..]
            } else {
                data
            };
            let info = crate::dib::DibInfo::parse(body)?;
            Some(format!(
                "{}x{} {}bpp  {}  planes {}  palette {}  header {} bytes\n",
                info.abs_width(),
                info.abs_height(),
                info.bit_count,
                info.compression_name(),
                info.planes,
                info.palette_len,
                info.header_size
            ))
        }
        _ => None,
    }
}

pub fn font_text(f: &crate::fnt::Font) -> String {
    let h = &f.header;
    let mut s = String::new();
    s.push_str(&format!("FACE      {}\n", f.face));
    if !f.device.is_empty() {
        s.push_str(&format!("DEVICE    {}\n", f.device));
    }
    s.push_str(&format!(
        "SIZE      {} point, {}x{} px cell, ascent {}\n",
        h.points, h.pix_width, h.pix_height, h.ascent
    ));
    s.push_str(&format!(
        "PITCH     {}{}\n",
        if h.is_proportional() {
            "proportional"
        } else {
            "fixed"
        },
        if h.is_proportional() {
            format!(", average {} px, widest {} px", h.avg_width, h.max_width)
        } else {
            String::new()
        }
    ));
    s.push_str(&format!(
        "STYLE     weight {} ({}), family {}{}{}{}\n",
        h.weight,
        h.weight_name(),
        h.family_name(),
        if h.italic { ", italic" } else { "" },
        if h.underline { ", underline" } else { "" },
        if h.strikeout { ", strikeout" } else { "" }
    ));
    s.push_str(&format!(
        "CHARSET   {} ({}), {:#04X}..{:#04X}, default {:#04X}, break {:#04X}\n",
        h.charset_name(),
        h.charset,
        h.first_char,
        h.last_char,
        h.default_char,
        h.break_char
    ));
    s.push_str(&format!(
        "RESOLUTION {} x {} dpi, leading {} internal / {} external\n",
        h.horiz_res, h.vert_res, h.internal_leading, h.external_leading
    ));
    s.push_str(&format!(
        "GLYPHS    {} present, version {}.{}\n",
        f.glyphs.len(),
        h.version >> 8,
        h.version & 0xFF
    ));
    if !h.copyright.is_empty() {
        s.push_str(&format!("COPYRIGHT {}\n", h.copyright));
    }
    s
}

/// The font directory, described from the fonts it points at rather than from
/// its own copies of their headers.
pub fn fontdir_text(ne: &NeFile, data: &[u8]) -> String {
    let ordinals = crate::fnt::parse_fontdir(data);
    let mut s = format!("{} font(s)\n\n", ordinals.len());
    for ord in ordinals {
        match ne
            .resource_by_type_ordinal(rt::FONT, ord)
            .map(|r| ne.resource_data(r))
            .and_then(crate::fnt::parse)
        {
            Some(f) => s.push_str(&format!(
                "  @{:<5} {:<20} {} point  {}x{}  {}\n",
                ord,
                f.face,
                f.header.points,
                f.header.pix_width,
                f.header.pix_height,
                f.header.charset_name()
            )),
            None => s.push_str(&format!("  @{ord:<5} (font resource not found)\n")),
        }
    }
    s
}

pub fn menu_text(m: &Menu) -> String {
    let mut s = format!("MENU version {}\n", m.version);
    for item in &m.items {
        menu_item_text(item, 1, &mut s);
    }
    s
}

fn menu_item_text(item: &MenuItem, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    if item.is_separator() {
        out.push_str(&format!("{pad}SEPARATOR\n"));
    } else if item.children.is_empty() && item.id.is_some() {
        let flags = item.flag_names();
        out.push_str(&format!(
            "{pad}MENUITEM {:<30} id {}{}\n",
            format!("{:?}", item.text),
            item.id.unwrap_or(0),
            if flags.is_empty() {
                String::new()
            } else {
                format!("  [{}]", flags.join(" "))
            }
        ));
    } else {
        out.push_str(&format!("{pad}POPUP {:?}\n", item.text));
        for c in &item.children {
            menu_item_text(c, depth + 1, out);
        }
    }
}

pub fn dialog_text(d: &Dialog) -> String {
    let mut s = format!(
        "DIALOG {}, {}, {}, {}   style {:08X}\n",
        d.x, d.y, d.cx, d.cy, d.style
    );
    if !d.caption.is_empty() {
        s.push_str(&format!("CAPTION {:?}\n", d.caption));
    }
    if d.menu != rsrc::NameOrOrd::None {
        s.push_str(&format!("MENU {}\n", d.menu));
    }
    if d.class != rsrc::NameOrOrd::None {
        s.push_str(&format!("CLASS {}\n", d.class));
    }
    if let Some((size, name)) = &d.font {
        s.push_str(&format!("FONT {size}, {name:?}\n"));
    }
    s.push_str(&format!("{{  ; {} controls\n", d.items.len()));
    for it in &d.items {
        s.push_str(&format!(
            "  {:<10} {:<24} id {:<6} at {},{} size {}x{}  style {:08X}\n",
            it.class_name(),
            it.text.to_string(),
            it.id,
            it.x,
            it.y,
            it.cx,
            it.cy,
            it.style
        ));
    }
    s.push_str("}\n");
    s
}

pub fn version_text(v: &rsrc::VersionInfo) -> String {
    let mut s = String::new();
    if let Some(f) = &v.fixed {
        s.push_str(&format!("FILEVERSION    {}\n", f.file_version_string()));
        s.push_str(&format!("PRODUCTVERSION {}\n", f.product_version_string()));
        s.push_str(&format!(
            "FILEFLAGS      {:08X} (mask {:08X})\n",
            f.file_flags, f.file_flags_mask
        ));
        s.push_str(&format!(
            "FILEOS         {:08X}   FILETYPE {:08X}.{:X}\n",
            f.file_os, f.file_type, f.file_subtype
        ));
        s.push('\n');
    }
    node_text(&v.root, 0, &mut s);
    s
}

fn node_text(n: &VersionNode, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    match &n.value_text {
        Some(v) if !v.is_empty() => out.push_str(&format!("{pad}{}: {v}\n", n.key)),
        _ => out.push_str(&format!("{pad}{}\n", n.key)),
    }
    for c in &n.children {
        node_text(c, depth + 1, out);
    }
}
