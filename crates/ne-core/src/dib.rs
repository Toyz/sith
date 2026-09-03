//! Device-independent bitmap decoding.
//!
//! `RT_BITMAP`, `RT_ICON` and `RT_CURSOR` resources all store a bare DIB with
//! no `BITMAPFILEHEADER`, so they need decoding before anything can display
//! them and re-heading before they can be written out as `.bmp`.

/// Which header shape the DIB uses. Windows 3.0 introduced the 40-byte
/// `BITMAPINFOHEADER`; the 12-byte OS/2 `BITMAPCOREHEADER` still turns up in
/// resources built by older toolchains and uses 3-byte palette entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DibHeaderKind {
    Core,
    Info,
}

#[derive(Debug, Clone)]
pub struct DibInfo {
    pub kind: DibHeaderKind,
    pub header_size: u32,
    pub width: i32,
    /// As stored. For an icon or cursor this is twice the visible height,
    /// because the AND mask is appended below the color bits.
    pub height: i32,
    pub planes: u16,
    pub bit_count: u16,
    pub compression: u32,
    pub image_size: u32,
    pub colors_used: u32,
    /// Byte offset of the palette from the start of the DIB.
    pub palette_offset: usize,
    /// Palette entries actually present.
    pub palette_len: usize,
    /// Byte offset of the pixel data from the start of the DIB.
    pub bits_offset: usize,
}

pub const BI_RGB: u32 = 0;
pub const BI_RLE8: u32 = 1;
pub const BI_RLE4: u32 = 2;
pub const BI_BITFIELDS: u32 = 3;

impl DibInfo {
    pub fn parse(data: &[u8]) -> Option<DibInfo> {
        if data.len() < 12 {
            return None;
        }
        let header_size = u32::from_le_bytes(data[0..4].try_into().ok()?);
        let (kind, width, height, planes, bit_count, compression, image_size, colors_used) =
            if header_size == 12 {
                (
                    DibHeaderKind::Core,
                    i32::from(i16::from_le_bytes(data[4..6].try_into().ok()?)),
                    i32::from(i16::from_le_bytes(data[6..8].try_into().ok()?)),
                    u16::from_le_bytes(data[8..10].try_into().ok()?),
                    u16::from_le_bytes(data[10..12].try_into().ok()?),
                    BI_RGB,
                    0,
                    0,
                )
            } else {
                if header_size < 40 || data.len() < 40 {
                    return None;
                }
                (
                    DibHeaderKind::Info,
                    i32::from_le_bytes(data[4..8].try_into().ok()?),
                    i32::from_le_bytes(data[8..12].try_into().ok()?),
                    u16::from_le_bytes(data[12..14].try_into().ok()?),
                    u16::from_le_bytes(data[14..16].try_into().ok()?),
                    u32::from_le_bytes(data[16..20].try_into().ok()?),
                    u32::from_le_bytes(data[20..24].try_into().ok()?),
                    u32::from_le_bytes(data[32..36].try_into().ok()?),
                )
            };
        if bit_count > 32 || width == 0 || height == 0 {
            return None;
        }

        let entry = if kind == DibHeaderKind::Core { 3 } else { 4 };
        let max_colors = if bit_count <= 8 { 1usize << bit_count } else { 0 };
        let mut palette_len = if colors_used != 0 {
            colors_used as usize
        } else {
            max_colors
        };
        if bit_count > 8 {
            palette_len = 0;
        }
        palette_len = palette_len.min(256);

        let palette_offset = header_size as usize;
        // BI_BITFIELDS puts three 32-bit channel masks where the palette
        // would be; they are not color entries but they do shift the bits.
        let masks = if compression == BI_BITFIELDS { 12 } else { 0 };
        let bits_offset = palette_offset + palette_len * entry + masks;

        Some(DibInfo {
            kind,
            header_size,
            width,
            height,
            planes,
            bit_count,
            compression,
            image_size,
            colors_used,
            palette_offset,
            palette_len,
            bits_offset,
        })
    }

    pub fn palette_entry_size(&self) -> usize {
        if self.kind == DibHeaderKind::Core {
            3
        } else {
            4
        }
    }

    /// Bytes per pixel row, rounded up to a 4-byte boundary.
    pub fn stride(&self) -> usize {
        let w = self.width.unsigned_abs() as usize;
        ((w * self.bit_count as usize + 31) / 32) * 4
    }

    pub fn abs_width(&self) -> usize {
        self.width.unsigned_abs() as usize
    }

    pub fn abs_height(&self) -> usize {
        self.height.unsigned_abs() as usize
    }

    /// Rows are stored bottom-up unless the height is negative.
    pub fn bottom_up(&self) -> bool {
        self.height > 0
    }

    pub fn compression_name(&self) -> &'static str {
        match self.compression {
            BI_RGB => "BI_RGB",
            BI_RLE8 => "BI_RLE8",
            BI_RLE4 => "BI_RLE4",
            BI_BITFIELDS => "BI_BITFIELDS",
            4 => "BI_JPEG",
            5 => "BI_PNG",
            _ => "?",
        }
    }
}

/// A decoded image: 8-bit RGBA, top row first.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Image {
        Image {
            width,
            height,
            rgba: vec![0; width * height * 4],
        }
    }

    /// Paint every pixel one color, used as the RLE background.
    pub fn fill(&mut self, px: [u8; 4]) {
        for c in self.rgba.chunks_mut(4) {
            c.copy_from_slice(&px);
        }
    }

    #[inline]
    pub fn put(&mut self, x: usize, y: usize, px: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = (y * self.width + x) * 4;
        self.rgba[i..i + 4].copy_from_slice(&px);
    }
}

/// The DIB's color table, as RGBA.
///
/// Worth having on its own: which index a bitmap treats as transparent, and
/// whether two resources share a palette, are questions you cannot answer
/// from the decoded picture.
pub fn palette(data: &[u8], info: &DibInfo) -> Vec<[u8; 4]> {
    let esz = info.palette_entry_size();
    (0..info.palette_len)
        .map(|i| {
            let o = info.palette_offset + i * esz;
            match data.get(o..o + 3) {
                // Palette entries are stored B,G,R.
                Some(e) => [e[2], e[1], e[0], 0xFF],
                None => [0, 0, 0, 0xFF],
            }
        })
        .collect()
}

/// Decode a bare DIB.
///
/// `height_override` forces the visible height, which icon and cursor
/// resources need: their header height covers the color bits *and* the AND
/// mask stacked below them.
pub fn decode(data: &[u8], height_override: Option<usize>) -> Option<Image> {
    let info = DibInfo::parse(data)?;
    let w = info.abs_width();
    let h = height_override.unwrap_or_else(|| info.abs_height());
    if w == 0 || h == 0 || w > 1 << 15 || h > 1 << 15 {
        return None;
    }
    let pal = palette(data, &info);
    let bits = data.get(info.bits_offset..)?;
    let mut img = Image::new(w, h);

    match info.compression {
        BI_RLE8 | BI_RLE4 => {
            // An RLE stream need not cover every pixel: delta and
            // end-of-line codes skip forward, and the encoder may stop short
            // of the last row. Windows leaves whatever was already in the
            // destination there; for a standalone decode, color index 0 is
            // the conventional fill and matches what other readers produce.
            img.fill(idx(&pal, 0));
            if info.compression == BI_RLE8 {
                decode_rle8(bits, &pal, &info, &mut img);
            } else {
                decode_rle4(bits, &pal, &info, &mut img);
            }
        }
        _ => decode_uncompressed(bits, &pal, &info, h, &mut img),
    }
    Some(img)
}

fn row_y(info: &DibInfo, h: usize, row: usize) -> usize {
    if info.bottom_up() {
        h - 1 - row
    } else {
        row
    }
}

fn decode_uncompressed(bits: &[u8], pal: &[[u8; 4]], info: &DibInfo, h: usize, img: &mut Image) {
    let stride = info.stride();
    let w = info.abs_width();
    for row in 0..h {
        let Some(line) = bits.get(row * stride..row * stride + stride) else {
            break;
        };
        let y = row_y(info, h, row);
        for x in 0..w {
            let px = match info.bit_count {
                1 => idx(pal, ((line[x / 8] >> (7 - (x % 8))) & 1) as usize),
                4 => {
                    let b = line[x / 2];
                    idx(pal, if x % 2 == 0 { (b >> 4) as usize } else { (b & 0xF) as usize })
                }
                8 => idx(pal, line[x] as usize),
                16 => {
                    // Default 16-bit layout with no BI_BITFIELDS is XRGB1555.
                    let v = u16::from_le_bytes([line[x * 2], line[x * 2 + 1]]);
                    let r = ((v >> 10) & 0x1F) as u8;
                    let g = ((v >> 5) & 0x1F) as u8;
                    let b = (v & 0x1F) as u8;
                    [r << 3 | r >> 2, g << 3 | g >> 2, b << 3 | b >> 2, 0xFF]
                }
                24 => {
                    let o = x * 3;
                    [line[o + 2], line[o + 1], line[o], 0xFF]
                }
                32 => {
                    let o = x * 4;
                    [line[o + 2], line[o + 1], line[o], 0xFF]
                }
                _ => [0, 0, 0, 0xFF],
            };
            img.put(x, y, px);
        }
    }
}

#[inline]
fn idx(pal: &[[u8; 4]], i: usize) -> [u8; 4] {
    pal.get(i).copied().unwrap_or([0, 0, 0, 0xFF])
}

fn decode_rle8(bits: &[u8], pal: &[[u8; 4]], info: &DibInfo, img: &mut Image) {
    let h = img.height;
    let (mut x, mut row) = (0usize, 0usize);
    let mut p = 0usize;
    while p + 1 < bits.len() {
        let (count, val) = (bits[p], bits[p + 1]);
        p += 2;
        if count > 0 {
            let px = idx(pal, val as usize);
            for _ in 0..count {
                img.put(x, row_y(info, h, row), px);
                x += 1;
            }
            continue;
        }
        match val {
            0 => {
                x = 0;
                row += 1;
            }
            1 => break,
            2 => {
                if p + 1 >= bits.len() {
                    break;
                }
                x += bits[p] as usize;
                row += bits[p + 1] as usize;
                p += 2;
            }
            n => {
                let n = n as usize;
                for i in 0..n {
                    let Some(&v) = bits.get(p + i) else { break };
                    img.put(x, row_y(info, h, row), idx(pal, v as usize));
                    x += 1;
                }
                p += n + (n & 1); // runs are word-aligned
            }
        }
        if row >= h {
            break;
        }
    }
}

fn decode_rle4(bits: &[u8], pal: &[[u8; 4]], info: &DibInfo, img: &mut Image) {
    let h = img.height;
    let (mut x, mut row) = (0usize, 0usize);
    let mut p = 0usize;
    while p + 1 < bits.len() {
        let (count, val) = (bits[p], bits[p + 1]);
        p += 2;
        if count > 0 {
            for i in 0..count as usize {
                let nib = if i % 2 == 0 { val >> 4 } else { val & 0xF };
                img.put(x, row_y(info, h, row), idx(pal, nib as usize));
                x += 1;
            }
            continue;
        }
        match val {
            0 => {
                x = 0;
                row += 1;
            }
            1 => break,
            2 => {
                if p + 1 >= bits.len() {
                    break;
                }
                x += bits[p] as usize;
                row += bits[p + 1] as usize;
                p += 2;
            }
            n => {
                let n = n as usize;
                let bytes = (n + 1) / 2;
                for i in 0..n {
                    let Some(&b) = bits.get(p + i / 2) else { break };
                    let nib = if i % 2 == 0 { b >> 4 } else { b & 0xF };
                    img.put(x, row_y(info, h, row), idx(pal, nib as usize));
                    x += 1;
                }
                p += bytes + (bytes & 1);
            }
        }
        if row >= h {
            break;
        }
    }
}

/// Apply an icon/cursor AND mask as alpha. The mask is a 1bpp bitmap of the
/// same width stacked below the color bits; a set bit means transparent.
pub fn apply_and_mask(img: &mut Image, data: &[u8], info: &DibInfo) {
    let color_rows = img.height;
    let color_bytes = info.stride() * color_rows;
    let mask_stride = ((img.width + 31) / 32) * 4;
    let base = info.bits_offset + color_bytes;
    for row in 0..color_rows {
        let y = if info.bottom_up() {
            color_rows - 1 - row
        } else {
            row
        };
        let Some(line) = data.get(base + row * mask_stride..base + row * mask_stride + mask_stride)
        else {
            return;
        };
        for x in 0..img.width {
            if (line[x / 8] >> (7 - (x % 8))) & 1 == 1 {
                let i = (y * img.width + x) * 4;
                img.rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
}

/// Wrap a bare DIB in a `BITMAPFILEHEADER` so it can be written as `.bmp`.
pub fn to_bmp_file(data: &[u8]) -> Option<Vec<u8>> {
    let info = DibInfo::parse(data)?;
    let off_bits = 14 + info.bits_offset;
    let size = 14 + data.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved1
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved2
    out.extend_from_slice(&(off_bits as u32).to_le_bytes());
    out.extend_from_slice(data);
    Some(out)
}
