//! Windows bitmap fonts.
//!
//! A `.FON` file is an NE library whose only payload is fonts: one `RT_FONTDIR`
//! listing what is inside, and an `RT_FONT` per face holding an FNT structure.
//! The same resources turn up inside ordinary programs that ship a font of
//! their own, so this is a resource decoder rather than a separate file format.
//!
//! Glyph bitmaps are the part worth knowing about. They are stored *column
//! major*: a glyph `w` pixels wide is split into `ceil(w / 8)` column strips,
//! and each strip holds one byte per scanline, all of a strip's scanlines
//! together before the next strip begins. Reading them row-major -- the
//! obvious thing -- produces a recognisable but sheared mess.

use crate::dib::Image;

/// Version 2.0 (Windows 2.x) and 3.0 (Windows 3.x) headers differ in length
/// and in the width of a character-table offset.
pub const VERSION_2: u16 = 0x0200;
pub const VERSION_3: u16 = 0x0300;

#[derive(Debug, Clone)]
pub struct FntHeader {
    pub version: u16,
    pub size: u32,
    pub copyright: String,
    /// Bit 0 clear for a raster font, set for a vector font.
    pub kind: u16,
    pub points: u16,
    pub vert_res: u16,
    pub horiz_res: u16,
    pub ascent: u16,
    pub internal_leading: u16,
    pub external_leading: u16,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub weight: u16,
    pub charset: u8,
    /// Zero for a proportional font, in which case widths come from the
    /// character table.
    pub pix_width: u16,
    pub pix_height: u16,
    pub pitch_and_family: u8,
    pub avg_width: u16,
    pub max_width: u16,
    pub first_char: u8,
    pub last_char: u8,
    pub default_char: u8,
    pub break_char: u8,
    pub width_bytes: u16,
    /// Offsets into the resource of the device and face name strings.
    pub device_offset: u32,
    pub face_offset: u32,
    pub bits_offset: u32,
}

impl FntHeader {
    pub fn is_raster(&self) -> bool {
        self.kind & 1 == 0
    }

    pub fn is_proportional(&self) -> bool {
        self.pix_width == 0
    }

    /// Byte offset of the character table, which follows the header.
    fn char_table_offset(&self) -> usize {
        if self.version >= VERSION_3 {
            0x94
        } else {
            0x76
        }
    }

    /// Size of one character-table entry: version 3 widened the offset.
    fn char_entry_size(&self) -> usize {
        if self.version >= VERSION_3 {
            6
        } else {
            4
        }
    }

    pub fn weight_name(&self) -> &'static str {
        match self.weight {
            0 => "unspecified",
            1..=150 => "thin",
            151..=250 => "extra light",
            251..=350 => "light",
            351..=450 => "regular",
            451..=550 => "medium",
            551..=650 => "semi bold",
            651..=750 => "bold",
            751..=850 => "extra bold",
            _ => "heavy",
        }
    }

    pub fn charset_name(&self) -> &'static str {
        match self.charset {
            0 => "ANSI",
            1 => "DEFAULT",
            2 => "SYMBOL",
            77 => "MAC",
            128 => "SHIFTJIS",
            129 => "HANGUL",
            134 => "GB2312",
            136 => "CHINESEBIG5",
            161 => "GREEK",
            162 => "TURKISH",
            177 => "HEBREW",
            178 => "ARABIC",
            186 => "BALTIC",
            204 => "RUSSIAN",
            222 => "THAI",
            238 => "EASTEUROPE",
            255 => "OEM",
            _ => "?",
        }
    }

    /// The low two bits of `dfPitchAndFamily` are the pitch; the high nibble
    /// is the family.
    pub fn family_name(&self) -> &'static str {
        match self.pitch_and_family & 0xF0 {
            0x00 => "don't care",
            0x10 => "roman",
            0x20 => "swiss",
            0x30 => "modern",
            0x40 => "script",
            0x50 => "decorative",
            _ => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Glyph {
    /// Character code this glyph renders.
    pub code: u8,
    pub width: u16,
    /// One bit per pixel, row-major, `width` bits per row.
    pub rows: Vec<Vec<bool>>,
}

#[derive(Debug, Clone)]
pub struct Font {
    pub header: FntHeader,
    pub face: String,
    pub device: String,
    pub glyphs: Vec<Glyph>,
}

impl Font {
    pub fn height(&self) -> usize {
        self.header.pix_height as usize
    }

    /// Total advance width of a string, for laying out a sample.
    pub fn text_width(&self, text: &str) -> usize {
        text.chars()
            .map(|c| {
                self.glyph(c as u32 as u8)
                    .map(|g| g.width as usize)
                    .unwrap_or(0)
            })
            .sum()
    }

    pub fn glyph(&self, code: u8) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.code == code)
    }
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(o)?,
        *b.get(o + 1)?,
        *b.get(o + 2)?,
        *b.get(o + 3)?,
    ]))
}

/// A NUL-terminated ASCII string at `offset`, as the FNT tables store names.
fn cstr(data: &[u8], offset: usize) -> String {
    let Some(rest) = data.get(offset..) else {
        return String::new();
    };
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    crate::read::latin1(&rest[..end])
}

pub fn parse_header(data: &[u8]) -> Option<FntHeader> {
    if data.len() < 0x76 {
        return None;
    }
    let version = u16le(data, 0x00)?;
    if version != VERSION_2 && version != VERSION_3 {
        return None;
    }
    Some(FntHeader {
        version,
        size: u32le(data, 0x02)?,
        copyright: crate::read::latin1(
            data.get(0x06..0x42)?
                .split(|&b| b == 0)
                .next()
                .unwrap_or_default(),
        )
        .trim()
        .to_string(),
        kind: u16le(data, 0x42)?,
        points: u16le(data, 0x44)?,
        vert_res: u16le(data, 0x46)?,
        horiz_res: u16le(data, 0x48)?,
        ascent: u16le(data, 0x4A)?,
        internal_leading: u16le(data, 0x4C)?,
        external_leading: u16le(data, 0x4E)?,
        italic: *data.get(0x50)? != 0,
        underline: *data.get(0x51)? != 0,
        strikeout: *data.get(0x52)? != 0,
        weight: u16le(data, 0x53)?,
        charset: *data.get(0x55)?,
        pix_width: u16le(data, 0x56)?,
        pix_height: u16le(data, 0x58)?,
        pitch_and_family: *data.get(0x5A)?,
        avg_width: u16le(data, 0x5B)?,
        max_width: u16le(data, 0x5D)?,
        first_char: *data.get(0x5F)?,
        last_char: *data.get(0x60)?,
        default_char: *data.get(0x61)?,
        break_char: *data.get(0x62)?,
        width_bytes: u16le(data, 0x63)?,
        device_offset: u32le(data, 0x65)?,
        face_offset: u32le(data, 0x69)?,
        bits_offset: u32le(data, 0x71)?,
    })
}

/// Decode one `RT_FONT` resource.
///
/// Returns `None` for a vector font: those store drawing commands rather than
/// bitmaps, and nothing here would know what to do with them.
pub fn parse(data: &[u8]) -> Option<Font> {
    let header = parse_header(data)?;
    if !header.is_raster() {
        return None;
    }
    let height = header.pix_height as usize;
    if height == 0 || height > 512 {
        return None;
    }

    // The table has one entry per character plus a sentinel that gives the
    // end of the last glyph's data.
    let count = header.last_char as usize + 1 - header.first_char as usize + 1;
    let base = header.char_table_offset();
    let entry = header.char_entry_size();

    let mut glyphs = Vec::with_capacity(count);
    for i in 0..count {
        let o = base + i * entry;
        let width = u16le(data, o)?;
        let offset = if entry == 6 {
            u32le(data, o + 2)? as usize
        } else {
            u16le(data, o + 2)? as usize
        };
        // The sentinel entry has no character of its own.
        let Some(code) = header.first_char.checked_add(i as u8) else {
            break;
        };
        if i + 1 == count || width == 0 {
            continue;
        }
        if let Some(g) = glyph(data, offset, width, height, code) {
            glyphs.push(g);
        }
    }
    if glyphs.is_empty() {
        return None;
    }

    Some(Font {
        face: cstr(data, header.face_offset as usize),
        device: cstr(data, header.device_offset as usize),
        header,
        glyphs,
    })
}

/// Read one glyph's column-major bitmap.
fn glyph(data: &[u8], offset: usize, width: u16, height: usize, code: u8) -> Option<Glyph> {
    let w = width as usize;
    let strips = w.div_ceil(8);
    let mut rows = vec![vec![false; w]; height];
    for strip in 0..strips {
        for (y, row) in rows.iter_mut().enumerate() {
            // All of a strip's scanlines are contiguous, then the next strip.
            let byte = *data.get(offset + strip * height + y)?;
            for bit in 0..8 {
                let x = strip * 8 + bit;
                if x < w {
                    row[x] = byte & (0x80 >> bit) != 0;
                }
            }
        }
    }
    Some(Glyph { code, width, rows })
}

/// The `RT_FONTDIR` directory: how many fonts, and their resource ordinals.
///
/// Only the count and ordinals are read. The rest of each entry repeats the
/// font's own header, and reading it from the `RT_FONT` resource instead means
/// never having to guess at a layout that varied between toolchains.
pub fn parse_fontdir(data: &[u8]) -> Vec<u16> {
    let Some(count) = u16le(data, 0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut p = 2usize;
    for _ in 0..count {
        let Some(ordinal) = u16le(data, p) else { break };
        out.push(ordinal);
        // An entry is the ordinal, a copy of the FNT header up to the bits
        // pointer, then the device and face names.
        let mut q = p + 2 + 0x71;
        for _ in 0..2 {
            let Some(rest) = data.get(q..) else { break };
            q += rest.iter().position(|&b| b == 0).map(|n| n + 1).unwrap_or(rest.len());
        }
        if q <= p {
            break;
        }
        p = q;
    }
    out
}

/// Render every glyph into one sheet, sixteen to a row.
///
/// Set pixels are opaque white and the rest transparent, so the preview reads
/// against any background and shows the glyph boxes as they really are.
pub fn render_sheet(font: &Font) -> Image {
    const PER_ROW: usize = 16;
    const PAD: usize = 1;

    let cell_w = font.glyphs.iter().map(|g| g.width as usize).max().unwrap_or(1) + PAD * 2;
    let cell_h = font.height() + PAD * 2;
    let rows = font.glyphs.len().div_ceil(PER_ROW);
    let mut img = Image::new(cell_w * PER_ROW, cell_h * rows);

    for (i, g) in font.glyphs.iter().enumerate() {
        let ox = (i % PER_ROW) * cell_w + PAD;
        let oy = (i / PER_ROW) * cell_h + PAD;
        for (y, row) in g.rows.iter().enumerate() {
            for (x, on) in row.iter().enumerate() {
                if *on {
                    img.put(ox + x, oy + y, [0xFF, 0xFF, 0xFF, 0xFF]);
                }
            }
        }
    }
    img
}

/// Render a line of text, for a sample of the face at its real size.
pub fn render_text(font: &Font, text: &str) -> Image {
    let width = font.text_width(text).max(1);
    let mut img = Image::new(width, font.height().max(1));
    let mut x = 0usize;
    for ch in text.chars() {
        let Some(g) = font.glyph(ch as u32 as u8) else {
            continue;
        };
        for (y, row) in g.rows.iter().enumerate() {
            for (gx, on) in row.iter().enumerate() {
                if *on {
                    img.put(x + gx, y, [0xFF, 0xFF, 0xFF, 0xFF]);
                }
            }
        }
        x += g.width as usize;
    }
    img
}
