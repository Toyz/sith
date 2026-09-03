//! Tests for the Win16 signature and constant tables.

use ne_core::api::{ApiDb, ArgKind, CallConv};

#[test]
fn embedded_table_has_the_core_modules() {
    let db = ApiDb::embedded();
    assert!(db.len() > 1000, "expected a substantial table, got {}", db.len());
    for (module, ordinal, name) in [
        ("KERNEL", 15u16, "GlobalAlloc"),
        ("USER", 1, "MessageBox"),
        ("GDI", 34, "BitBlt"),
    ] {
        let sig = db
            .signature(module, ordinal)
            .unwrap_or_else(|| panic!("{module}.{ordinal} missing"));
        assert_eq!(sig.name, name);
        assert_eq!(sig.conv, CallConv::Pascal);
    }
}

#[test]
fn signatures_describe_the_stack_shape() {
    let db = ApiDb::embedded();
    // GlobalAlloc(word flags, long bytes): one word plus one long.
    let sig = db.signature("KERNEL", 15).unwrap();
    assert_eq!(sig.args, vec![ArgKind::Word, ArgKind::Long]);
    assert_eq!(sig.stack_words(), 3);
    assert_eq!(sig.render(), "GlobalAlloc(word, long)");

    // MessageBox(word, str, str, word): two words and two far pointers.
    let sig = db.signature("USER", 1).unwrap();
    assert_eq!(sig.stack_words(), 6);
}

#[test]
fn lookup_is_case_insensitive_and_works_by_name() {
    let db = ApiDb::embedded();
    assert!(db.signature("kernel", 15).is_some());
    let by_name = db.signature_by_name("KERNEL", "globalalloc").unwrap();
    assert_eq!(by_name.name, "GlobalAlloc");
}

#[test]
fn flag_sets_decode_to_combined_names() {
    let db = ApiDb::embedded();
    let gmem = db.param_set("KERNEL", "GlobalAlloc", 0).expect("GMEM bound");
    assert!(gmem.flags);
    assert_eq!(gmem.decode(0x0002 | 0x0040).as_deref(), Some("GMEM_MOVEABLE|GMEM_ZEROINIT"));
    // A named zero is reported rather than left blank.
    assert_eq!(gmem.decode(0).as_deref(), Some("GMEM_FIXED"));
    // Bits with no name are kept, so nothing is silently dropped.
    let mixed = gmem.decode(0x0002 | 0x4000).unwrap();
    assert!(mixed.contains("GMEM_MOVEABLE"), "{mixed}");
    assert!(mixed.contains("4000"), "{mixed}");
}

#[test]
fn enumerations_match_exactly() {
    let db = ApiDb::embedded();
    let rop = db.param_set("GDI", "BitBlt", 8).expect("ROP bound");
    assert!(!rop.flags);
    assert_eq!(rop.decode(0x00CC0020).as_deref(), Some("SRCCOPY"));
    assert_eq!(rop.decode(0x00550009).as_deref(), Some("DSTINVERT"));
    // An enumeration does not invent a combination for an unknown value.
    assert_eq!(rop.decode(0x1234_5678), None);

    let stock = db.param_set("GDI", "GetStockObject", 0).expect("STOCK bound");
    assert_eq!(stock.decode(4).as_deref(), Some("BLACK_BRUSH"));

    let idc = db.param_set("USER", "LoadCursor", 1).expect("IDC bound");
    assert_eq!(idc.decode(32512).as_deref(), Some("IDC_ARROW"));
}

#[test]
fn unbound_parameters_have_no_set() {
    let db = ApiDb::embedded();
    assert!(db.param_set("KERNEL", "GlobalAlloc", 1).is_none());
    assert!(db.param_set("KERNEL", "NoSuchFunction", 0).is_none());
}
