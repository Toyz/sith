//! Decoder tests for the structured resource types and for DIB images.

use ne_core::dib;
use ne_core::rsrc::{self, mf, NameOrOrd};

fn cstr(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

#[test]
fn menu_tree() {
    let mut m = vec![0, 0, 0, 0]; // version 0, no extra header
    m.extend_from_slice(&mf::POPUP.to_le_bytes());
    cstr(&mut m, "&File");
    m.extend_from_slice(&0u16.to_le_bytes());
    m.extend_from_slice(&100u16.to_le_bytes());
    cstr(&mut m, "&Open");
    m.extend_from_slice(&(mf::CHECKED | mf::END).to_le_bytes());
    m.extend_from_slice(&101u16.to_le_bytes());
    cstr(&mut m, "&Close");
    m.extend_from_slice(&mf::END.to_le_bytes());
    m.extend_from_slice(&200u16.to_le_bytes());
    cstr(&mut m, "&Help");

    let menu = rsrc::decode_menu(&m).expect("decodes");
    assert_eq!(menu.items.len(), 2);
    assert_eq!(menu.items[0].text, "&File");
    assert_eq!(menu.items[0].children.len(), 2);
    assert_eq!(menu.items[0].children[0].id, Some(100));
    assert_eq!(menu.items[0].children[1].text, "&Close");
    assert_eq!(menu.items[0].children[1].flag_names(), vec!["CHECKED"]);
    assert_eq!(menu.items[1].text, "&Help");
    assert_eq!(menu.items[1].id, Some(200));
}

#[test]
fn menu_separator_is_recognised() {
    let mut m = vec![0, 0, 0, 0];
    m.extend_from_slice(&mf::END.to_le_bytes());
    m.extend_from_slice(&0u16.to_le_bytes());
    cstr(&mut m, "");
    let menu = rsrc::decode_menu(&m).unwrap();
    assert!(menu.items[0].is_separator());
}

#[test]
fn dialog_template() {
    let mut d = Vec::new();
    d.extend_from_slice(&0x40u32.to_le_bytes()); // DS_SETFONT
    d.push(2); // item count
    for v in [10i16, 20, 200, 100] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.push(0); // no menu
    d.push(0); // no class
    cstr(&mut d, "Options");
    d.extend_from_slice(&8u16.to_le_bytes());
    cstr(&mut d, "MS Sans Serif");

    // A predefined BUTTON control.
    for v in [5i16, 7, 40, 14] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u16.to_le_bytes()); // IDOK
    d.extend_from_slice(&0x5001_0001u32.to_le_bytes());
    d.push(0x80); // BUTTON
    cstr(&mut d, "OK");
    d.push(0); // no creation data

    // A control with a class name string and an ordinal title.
    for v in [5i16, 30, 40, 14] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&2u16.to_le_bytes());
    d.extend_from_slice(&0x5000_0000u32.to_le_bytes());
    cstr(&mut d, "MyClass");
    d.push(0xFF);
    d.extend_from_slice(&77u16.to_le_bytes());
    d.push(0);

    let dlg = rsrc::decode_dialog(&d).expect("decodes");
    assert_eq!(dlg.caption, "Options");
    assert_eq!(dlg.font, Some((8, "MS Sans Serif".to_string())));
    assert_eq!((dlg.x, dlg.y, dlg.cx, dlg.cy), (10, 20, 200, 100));
    assert_eq!(dlg.items.len(), 2);
    assert_eq!(dlg.items[0].class_name(), "BUTTON");
    assert_eq!(dlg.items[0].text, NameOrOrd::Name("OK".into()));
    assert_eq!(dlg.items[0].id, 1);
    assert_eq!(dlg.items[1].class_name(), "MyClass");
    assert_eq!(dlg.items[1].text, NameOrOrd::Ord(77));
}

#[test]
fn accelerator_table_stops_at_the_last_entry() {
    let data = [
        0x09, 0x52, 0x00, 0x71, 0x00, // Ctrl + VIRTKEY 'R' -> 113
        0x81, 0x72, 0x00, 0x0A, 0x00, // VIRTKEY F3, last
        0x00, 0x00, 0x00, 0x00, 0x00, // must not be read
    ];
    let accels = rsrc::decode_accelerators(&data);
    assert_eq!(accels.len(), 2);
    assert_eq!(accels[0].key_name(), "Ctrl+'R'");
    assert_eq!(accels[0].id, 113);
    assert_eq!(accels[1].key_name(), "VK_F3");
}

#[test]
fn version_info_fixed_block_and_strings() {
    // VS_VERSIONINFO in its 16-bit ANSI form: DWORD-aligned nodes.
    fn node(key: &str, value: &[u8], children: Vec<Vec<u8>>) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(key.as_bytes());
        body.push(0);
        while (4 + body.len()) % 4 != 0 {
            body.push(0);
        }
        body.extend_from_slice(value);
        while (4 + body.len()) % 4 != 0 {
            body.push(0);
        }
        for c in children {
            body.extend_from_slice(&c);
            while (4 + body.len()) % 4 != 0 {
                body.push(0);
            }
        }
        let total = 4 + body.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&(total as u16).to_le_bytes());
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    let mut fixed = Vec::new();
    fixed.extend_from_slice(&rsrc::VS_FFI_SIGNATURE.to_le_bytes());
    fixed.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // struct version
    fixed.extend_from_slice(&0x0001_0002u32.to_le_bytes()); // file version ms
    fixed.extend_from_slice(&0x0003_0004u32.to_le_bytes()); // file version ls
    fixed.extend_from_slice(&0x0005_0006u32.to_le_bytes()); // product version ms
    fixed.extend_from_slice(&0x0007_0008u32.to_le_bytes()); // product version ls
    // mask, flags, os, type, subtype and the two timestamp words: VS_FIXEDFILEINFO is 52 bytes.
    fixed.extend_from_slice(&[0u8; 28]);

    let company = node("CompanyName", b"Acme\0", vec![]);
    let table = node("040904E4", &[], vec![company]);
    let sfi = node("StringFileInfo", &[], vec![table]);
    let root = node("VS_VERSION_INFO", &fixed, vec![sfi]);

    let v = rsrc::decode_version(&root).expect("decodes");
    let f = v.fixed.as_ref().expect("has a fixed block");
    assert_eq!(f.file_version, (1, 2, 3, 4));
    assert_eq!(f.file_version_string(), "1.2.3.4");
    assert_eq!(f.product_version_string(), "5.6.7.8");
    assert_eq!(
        v.strings(),
        vec![("CompanyName".to_string(), "Acme".to_string())]
    );
}

/// A 2x2 8bpp bottom-up DIB with a four-entry palette.
fn small_dib() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
    b.extend_from_slice(&2i32.to_le_bytes()); // width
    b.extend_from_slice(&2i32.to_le_bytes()); // height, positive = bottom-up
    b.extend_from_slice(&1u16.to_le_bytes()); // planes
    b.extend_from_slice(&8u16.to_le_bytes()); // bits per pixel
    b.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    b.extend_from_slice(&0u32.to_le_bytes()); // image size
    b.extend_from_slice(&[0u8; 8]); // pixels per metre
    b.extend_from_slice(&4u32.to_le_bytes()); // colors used
    b.extend_from_slice(&0u32.to_le_bytes()); // colors important
    for bgr in [[0, 0, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255]] {
        b.extend_from_slice(&bgr);
        b.push(0);
    }
    // Rows are padded to four bytes and stored bottom-up.
    b.extend_from_slice(&[2, 3, 0, 0]); // bottom row: green, red
    b.extend_from_slice(&[0, 1, 0, 0]); // top row: black, blue
    b
}

#[test]
fn dib_header_and_pixels() {
    let data = small_dib();
    let info = dib::DibInfo::parse(&data).expect("header parses");
    assert_eq!((info.abs_width(), info.abs_height()), (2, 2));
    assert_eq!(info.bit_count, 8);
    assert_eq!(info.palette_len, 4);
    assert_eq!(info.stride(), 4);
    assert!(info.bottom_up());
    assert_eq!(info.bits_offset, 40 + 4 * 4);

    let img = dib::decode(&data, None).expect("decodes");
    assert_eq!((img.width, img.height), (2, 2));
    let px = |x: usize, y: usize| {
        let i = (y * img.width + x) * 4;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
    };
    // Palette entries are stored BGR, and the first stored row is the bottom.
    assert_eq!(px(0, 0), [0, 0, 0, 255], "top-left is palette 0");
    assert_eq!(px(1, 0), [0, 0, 255, 255], "top-right is palette 3");
    assert_eq!(px(0, 1), [0, 255, 0, 255], "bottom-left is palette 2");
    assert_eq!(px(1, 1), [255, 0, 0, 255], "bottom-right is palette 1");
}

#[test]
fn bmp_file_header_points_at_the_bits() {
    let data = small_dib();
    let bmp = dib::to_bmp_file(&data).expect("wraps");
    assert_eq!(&bmp[..2], b"BM");
    assert_eq!(
        u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize,
        bmp.len()
    );
    let off_bits = u32::from_le_bytes(bmp[10..14].try_into().unwrap()) as usize;
    assert_eq!(off_bits, 14 + 40 + 16);
    assert_eq!(&bmp[off_bits..off_bits + 2], &[2, 3]);
}

#[test]
fn malformed_dibs_are_rejected_rather_than_panicking() {
    assert!(dib::DibInfo::parse(&[]).is_none());
    assert!(dib::DibInfo::parse(&[0; 8]).is_none());
    let mut truncated = small_dib();
    truncated.truncate(45);
    // The header is intact but the bits are gone; decoding must not panic.
    let _ = dib::decode(&truncated, None);
    for len in 0..small_dib().len() {
        let _ = dib::decode(&small_dib()[..len], None);
    }
}
