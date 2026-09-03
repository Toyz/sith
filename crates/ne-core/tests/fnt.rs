//! Bitmap font decoding, driven from a hand-built FNT resource.

use ne_core::fnt;

const HEIGHT: usize = 8;
const CHAR_TABLE: usize = 0x94; // version 3.0 header length
const ENTRY: usize = 6; // width word plus a 32-bit offset

/// Two 8x8 glyphs, 'A' and 'B', stored the way Windows stores them: column
/// strips, all of a strip's scanlines together.
fn build() -> Vec<u8> {
    let glyph_a: [u8; HEIGHT] = [
        0b0001_1000,
        0b0010_0100,
        0b0100_0010,
        0b0100_0010,
        0b0111_1110,
        0b0100_0010,
        0b0100_0010,
        0b0000_0000,
    ];
    let glyph_b: [u8; HEIGHT] = [0xFF, 0x81, 0x81, 0xFF, 0x81, 0x81, 0xFF, 0x00];

    let table_len = 3 * ENTRY; // two characters plus the sentinel
    let bits_at = CHAR_TABLE + table_len;
    let face_at = bits_at + HEIGHT * 2;

    let mut b = vec![0u8; face_at + 16];
    let w = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    let d = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());

    w(&mut b, 0x00, fnt::VERSION_3);
    d(&mut b, 0x02, face_at as u32 + 16);
    b[0x06..0x06 + 9].copy_from_slice(b"Test font");
    w(&mut b, 0x42, 0); // raster
    w(&mut b, 0x44, 8); // points
    w(&mut b, 0x46, 96); // vertical resolution
    w(&mut b, 0x48, 96); // horizontal resolution
    w(&mut b, 0x4A, 7); // ascent
    w(&mut b, 0x53, 400); // weight
    b[0x55] = 0; // ANSI
    w(&mut b, 0x56, 8); // fixed pitch, 8 px wide
    w(&mut b, 0x58, HEIGHT as u16);
    b[0x5A] = 0x30; // modern family
    w(&mut b, 0x5B, 8); // average width
    w(&mut b, 0x5D, 8); // widest
    b[0x5F] = b'A';
    b[0x60] = b'B';
    b[0x61] = b'A'; // default character
    b[0x62] = b' '; // break character
    d(&mut b, 0x69, face_at as u32);
    d(&mut b, 0x71, bits_at as u32);

    for (i, off) in [bits_at, bits_at + HEIGHT, bits_at + HEIGHT * 2]
        .iter()
        .enumerate()
    {
        let o = CHAR_TABLE + i * ENTRY;
        w(&mut b, o, 8);
        d(&mut b, o + 2, *off as u32);
    }
    b[bits_at..bits_at + HEIGHT].copy_from_slice(&glyph_a);
    b[bits_at + HEIGHT..bits_at + HEIGHT * 2].copy_from_slice(&glyph_b);
    b[face_at..face_at + 6].copy_from_slice(b"Tester");
    b
}

#[test]
fn header_fields() {
    let data = build();
    let h = fnt::parse_header(&data).expect("header parses");
    assert_eq!(h.version, fnt::VERSION_3);
    assert!(h.is_raster());
    assert!(!h.is_proportional());
    assert_eq!(h.points, 8);
    assert_eq!(h.pix_height, HEIGHT as u16);
    assert_eq!((h.first_char, h.last_char), (b'A', b'B'));
    assert_eq!(h.weight_name(), "regular");
    assert_eq!(h.charset_name(), "ANSI");
    assert_eq!(h.family_name(), "modern");
}

#[test]
fn glyphs_decode_column_major() {
    let font = fnt::parse(&build()).expect("font parses");
    assert_eq!(font.face, "Tester");
    assert_eq!(font.header.copyright, "Test font");
    assert_eq!(font.glyphs.len(), 2, "the sentinel entry is not a glyph");

    let a = font.glyph(b'A').expect("has an A");
    assert_eq!(a.width, 8);
    assert_eq!(a.rows.len(), HEIGHT);
    // Row 4 of the 'A' is its crossbar: 0b0111_1110.
    let expected = [false, true, true, true, true, true, true, false];
    assert_eq!(a.rows[4], expected, "row-major read of a column-major glyph");
    // Row 0 has the apex only.
    assert_eq!(
        a.rows[0],
        [false, false, false, true, true, false, false, false]
    );
}

#[test]
fn a_wide_glyph_spans_several_column_strips() {
    // A 12-pixel glyph occupies two strips: the second holds the last four
    // columns, and reading it as if the rows were contiguous would shear it.
    let mut data = build();
    let bits_at = CHAR_TABLE + 3 * ENTRY;
    // Widen 'A' and give it a second strip that is solid.
    data[CHAR_TABLE..CHAR_TABLE + 2].copy_from_slice(&12u16.to_le_bytes());
    let second_strip = bits_at + HEIGHT;
    for y in 0..HEIGHT {
        data[second_strip + y] = 0xF0;
    }
    // Move 'B' out of the way so the strips do not overlap.
    let b_at = bits_at + HEIGHT * 2;
    let o = CHAR_TABLE + ENTRY;
    data[o + 2..o + 6].copy_from_slice(&(b_at as u32).to_le_bytes());

    let font = fnt::parse(&data).expect("parses");
    let a = font.glyph(b'A').unwrap();
    assert_eq!(a.width, 12);
    // Columns 8..12 come from the second strip's high nibble.
    assert!(a.rows[0][8] && a.rows[0][9] && a.rows[0][10] && a.rows[0][11]);
}

#[test]
fn rendering_produces_a_sheet_and_a_line() {
    let font = fnt::parse(&build()).unwrap();
    let sheet = fnt::render_sheet(&font);
    assert!(sheet.width > 0 && sheet.height > 0);

    let line = fnt::render_text(&font, "AB");
    assert_eq!(line.width, 16, "two fixed-width glyphs");
    assert_eq!(line.height, HEIGHT);
    // The 'B' starts at x = 8 and its top row is solid.
    let px = |x: usize, y: usize| line.rgba[(y * line.width + x) * 4 + 3];
    assert!(px(8, 0) > 0 && px(15, 0) > 0);
}

#[test]
fn a_vector_font_is_declined_rather_than_misread() {
    let mut data = build();
    data[0x42] = 1; // dfType bit 0 set: vector
    assert!(fnt::parse(&data).is_none());
    // The header still parses, which is what lets the UI describe it.
    assert!(fnt::parse_header(&data).is_some());
}

#[test]
fn malformed_input_is_rejected_rather_than_panicking() {
    assert!(fnt::parse_header(&[]).is_none());
    assert!(fnt::parse_header(&[0; 32]).is_none());
    let full = build();
    for len in 0..full.len() {
        let _ = fnt::parse(&full[..len]);
    }
}
