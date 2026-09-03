//! Tests over a hand-assembled NE image.
//!
//! Building the bytes here rather than checking in a binary keeps the
//! expectations visible: every offset in the file is written by the test, so a
//! failure points at the field that moved rather than at an opaque fixture.

use ne_core::reloc::{AddrType, RelKind, Target};
use ne_core::resource::rt;
use ne_core::{rsrc, NeFile, ResId, SegKind};

const NE_OFF: usize = 0x40;
const SEG_TABLE: usize = 0x80;
const RES_TABLE: usize = 0x90;
const MOD_TABLE: usize = 0xA8;
const IMP_TABLE: usize = 0xB0;
const RESIDENT: usize = 0xC0;
const ENTRY_TABLE: usize = 0xE0;
const NONRESIDENT: usize = 0x100;
const SEG1: usize = 0x400;
const SEG1_LEN: usize = 0x20;
const SEG2: usize = 0x480;
const SEG2_LEN: usize = 0x10;
const RES_DATA: usize = 0x500;
const ALIGN_SHIFT: u16 = 4;

fn w(buf: &mut [u8], at: usize, v: u16) {
    buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn d(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn pascal(buf: &mut [u8], at: usize, s: &str) -> usize {
    buf[at] = s.len() as u8;
    buf[at + 1..at + 1 + s.len()].copy_from_slice(s.as_bytes());
    1 + s.len()
}

/// A two-segment module exporting one function, importing one system ordinal
/// and one intersegment address, with a single string resource.
fn build() -> Vec<u8> {
    let mut b = vec![0u8; 0x600];
    b[0] = b'M';
    b[1] = b'Z';
    d(&mut b, 0x3C, NE_OFF as u32);

    // --- NE header ---
    b[NE_OFF] = b'N';
    b[NE_OFF + 1] = b'E';
    b[NE_OFF + 2] = 5; // linker major
    b[NE_OFF + 3] = 30; // linker minor
    w(&mut b, NE_OFF + 0x04, (ENTRY_TABLE - NE_OFF) as u16);
    w(&mut b, NE_OFF + 0x06, 6); // entry table length
    w(&mut b, NE_OFF + 0x0C, 0x0001); // SINGLEDATA
    w(&mut b, NE_OFF + 0x0E, 2); // auto data segment
    w(&mut b, NE_OFF + 0x10, 0x0400); // heap
    w(&mut b, NE_OFF + 0x12, 0x0800); // stack
    d(&mut b, NE_OFF + 0x14, 0x0001_0010); // CS:IP = 0001:0010
    d(&mut b, NE_OFF + 0x18, 0x0002_0000); // SS:SP
    w(&mut b, NE_OFF + 0x1C, 2); // segment count
    w(&mut b, NE_OFF + 0x1E, 2); // module reference count
    w(&mut b, NE_OFF + 0x20, 15); // non-resident name table length
    w(&mut b, NE_OFF + 0x22, (SEG_TABLE - NE_OFF) as u16);
    w(&mut b, NE_OFF + 0x24, (RES_TABLE - NE_OFF) as u16);
    w(&mut b, NE_OFF + 0x26, (RESIDENT - NE_OFF) as u16);
    w(&mut b, NE_OFF + 0x28, (MOD_TABLE - NE_OFF) as u16);
    w(&mut b, NE_OFF + 0x2A, (IMP_TABLE - NE_OFF) as u16);
    d(&mut b, NE_OFF + 0x2C, NONRESIDENT as u32);
    w(&mut b, NE_OFF + 0x32, ALIGN_SHIFT);
    w(&mut b, NE_OFF + 0x34, 1); // resource count
    b[NE_OFF + 0x36] = 2; // Windows
    w(&mut b, NE_OFF + 0x3E, 0x030A); // expects Windows 3.10

    // --- segment table ---
    w(&mut b, SEG_TABLE, (SEG1 >> ALIGN_SHIFT) as u16);
    w(&mut b, SEG_TABLE + 2, SEG1_LEN as u16);
    w(&mut b, SEG_TABLE + 4, 0x0110); // MOVEABLE | RELOCINFO, code
    w(&mut b, SEG_TABLE + 6, SEG1_LEN as u16);
    w(&mut b, SEG_TABLE + 8, (SEG2 >> ALIGN_SHIFT) as u16);
    w(&mut b, SEG_TABLE + 10, SEG2_LEN as u16);
    w(&mut b, SEG_TABLE + 12, 0x0001); // DATA
    w(&mut b, SEG_TABLE + 14, SEG2_LEN as u16);

    // --- resource table: one RT_STRING, id 1 ---
    w(&mut b, RES_TABLE, ALIGN_SHIFT);
    w(&mut b, RES_TABLE + 2, 0x8000 | rt::STRING);
    w(&mut b, RES_TABLE + 4, 1); // one resource of this type
    w(&mut b, RES_TABLE + 10, (RES_DATA >> ALIGN_SHIFT) as u16);
    w(&mut b, RES_TABLE + 12, 1); // length, in alignment units
    w(&mut b, RES_TABLE + 14, 0x0010); // MOVEABLE
    w(&mut b, RES_TABLE + 16, 0x8000 | 1); // id 1
    w(&mut b, RES_TABLE + 22, 0); // end of type list

    // --- module references and imported names ---
    w(&mut b, MOD_TABLE, 1);
    w(&mut b, MOD_TABLE + 2, 8);
    pascal(&mut b, IMP_TABLE + 1, "KERNEL");
    pascal(&mut b, IMP_TABLE + 8, "TESTLIB");

    // --- resident names: module name, then one export ---
    let mut p = RESIDENT;
    p += pascal(&mut b, p, "TESTMOD");
    w(&mut b, p, 0);
    p += 2;
    p += pascal(&mut b, p, "FOO");
    w(&mut b, p, 1);

    // --- non-resident names: description ---
    let mut p = NONRESIDENT;
    p += pascal(&mut b, p, "test module");
    w(&mut b, p, 0);

    // --- entry table: one fixed entry in segment 1, exported ---
    b[ENTRY_TABLE] = 1; // bundle of one
    b[ENTRY_TABLE + 1] = 1; // fixed, segment 1
    b[ENTRY_TABLE + 2] = 0x01; // exported
    w(&mut b, ENTRY_TABLE + 3, 0x0010);

    // --- segment 1 code and its fixup chains ---
    // An import fixup chaining two sites, and an intersegment reference whose
    // target offset lives in the code rather than in the record.
    w(&mut b, SEG1 + 0x04, 0x000C); // link to the next site
    w(&mut b, SEG1 + 0x0C, 0xFFFF); // end of chain
    w(&mut b, SEG1 + 0x12, 0x0008); // target offset for the segment fixup
    w(&mut b, SEG1 + 0x14, 0xFFFF); // single-site chain

    let rel = SEG1 + SEG1_LEN;
    w(&mut b, rel, 2); // two relocation records
    b[rel + 2] = 3; // ADDR_FAR
    b[rel + 3] = 1; // import by ordinal
    w(&mut b, rel + 4, 0x0004);
    w(&mut b, rel + 6, 1); // KERNEL
    w(&mut b, rel + 8, 5); // ordinal 5
    b[rel + 10] = 2; // ADDR_SEGMENT
    b[rel + 11] = 0; // internal
    w(&mut b, rel + 12, 0x0014);
    w(&mut b, rel + 14, 2); // segment 2
    w(&mut b, rel + 16, 0); // no offset in the record

    // --- resource body: a string block holding one string ---
    pascal(&mut b, RES_DATA, "hello");
    b
}

fn load() -> NeFile {
    NeFile::from_bytes("synthetic.exe".into(), build()).expect("parses")
}

#[test]
fn header_fields() {
    let ne = load();
    assert_eq!(ne.module_name(), "TESTMOD");
    assert_eq!(ne.description(), "test module");
    assert_eq!(ne.header.linker_version, (5, 30));
    assert_eq!(ne.header.expected_version, 0x030A);
    assert_eq!(ne.header.align_shift_or_default(), ALIGN_SHIFT);
    assert!(!ne.header.is_library());
    assert_eq!(ne.header.target_os.name(), "Windows");
    assert_eq!(ne.header.cs_ip, 0x0001_0010);
}

#[test]
fn segments_and_data() {
    let ne = load();
    assert_eq!(ne.segments.len(), 2);
    let s1 = ne.segment(1).unwrap();
    assert_eq!(s1.kind(), SegKind::Code);
    assert_eq!(s1.file_offset, SEG1 as u64);
    assert_eq!(s1.data.len(), SEG1_LEN);
    assert!(s1.has_relocs());
    let s2 = ne.segment(2).unwrap();
    assert_eq!(s2.kind(), SegKind::Data);
    assert!(!s2.has_relocs());
    assert!(ne.segment(3).is_none());
}

#[test]
fn module_and_import_names() {
    let ne = load();
    assert_eq!(ne.module_ref_names(), vec!["KERNEL", "TESTLIB"]);
    // KERNEL.5 comes from the built-in Win16 table.
    assert_eq!(
        ne.import_ordinal_name(1, 5).as_deref(),
        Some("LocalAlloc"),
        "the embedded ordinal database should name KERNEL.5"
    );
}

#[test]
fn exports() {
    let ne = load();
    let exports = ne.exports();
    assert_eq!(exports.len(), 1);
    let e = exports[0];
    assert_eq!(e.ordinal, 1);
    assert_eq!(e.name.as_deref(), Some("FOO"));
    assert_eq!(e.segment, 1);
    assert_eq!(e.offset, 0x0010);
    assert!(e.is_exported());
    assert!(!e.moveable);
    assert!(e.resident);
}

#[test]
fn fixup_chains_expand_to_every_site() {
    let ne = load();
    let seg = ne.segment(1).unwrap();
    assert_eq!(seg.relocs.len(), 2);

    let import = seg.relocs[0];
    assert_eq!(import.addr_type, AddrType::Far);
    assert_eq!(import.kind, RelKind::ImportOrdinal);
    assert!(!import.additive);
    assert_eq!(
        import.sites(&seg.data),
        vec![0x0004, 0x000C],
        "the chain is threaded through the code, so both sites must be found"
    );

    let internal = seg.relocs[1];
    assert_eq!(internal.addr_type, AddrType::Segment);
    assert_eq!(internal.sites(&seg.data), vec![0x0014]);
}

#[test]
fn fixup_targets_are_named() {
    let ne = load();
    let seg = ne.segment(1).unwrap();
    let fixups = ne.fixups(seg);
    assert_eq!(fixups.len(), 3, "two import sites plus one internal site");

    let import = &fixups[0];
    assert_eq!(import.site, 0x0004);
    assert_eq!(
        import.target,
        Target::ImportOrdinal {
            module: "KERNEL".into(),
            ordinal: 5,
            name: Some("LocalAlloc".into()),
        }
    );
    assert_eq!(import.target.to_string(), "KERNEL.LocalAlloc");

    // The record carries target2 == 0; the real offset is the word below the
    // patch site, and it is inside segment 2, so it is trusted.
    let internal = fixups.iter().find(|f| f.site == 0x0014).unwrap();
    assert_eq!(
        internal.target,
        Target::Internal {
            segment: 2,
            offset: Some(0x0008)
        }
    );
    assert_eq!(internal.target.to_string(), "seg02:0008");
}

#[test]
fn implausible_recovered_offsets_are_rejected() {
    // Point the recovered offset past the end of segment 2. Recovery is a
    // heuristic, and an out-of-range result means the fixup was not the
    // segment half of a far pointer after all.
    let mut bytes = build();
    w(&mut bytes, SEG1 + 0x12, 0x7FFF);
    let ne = NeFile::from_bytes("synthetic.exe".into(), bytes).unwrap();
    let seg = ne.segment(1).unwrap();
    let internal = ne
        .fixups(seg)
        .into_iter()
        .find(|f| f.site == 0x0014)
        .unwrap();
    assert_eq!(
        internal.target,
        Target::Internal {
            segment: 2,
            offset: None
        }
    );
    assert_eq!(internal.target.to_string(), "seg02:????");
}

#[test]
fn resources_and_string_block() {
    let ne = load();
    assert_eq!(ne.resources.len(), 1);
    let r = &ne.resources[0];
    assert_eq!(r.type_id, ResId::Id(rt::STRING));
    assert_eq!(r.res_id, ResId::Id(1));
    assert_eq!(r.type_name(), "STRING");
    assert_eq!(r.offset, RES_DATA as u64);
    assert_eq!(r.length, 1 << ALIGN_SHIFT);

    let strings = rsrc::decode_string_block(ne.resource_data(r), 1);
    assert_eq!(strings, vec![(0u16, "hello".to_string())]);
}

#[test]
fn rejects_non_ne_input() {
    assert!(NeFile::from_bytes("x".into(), vec![0; 64]).is_err());
    let mut mz = vec![0u8; 0x100];
    mz[0] = b'M';
    mz[1] = b'Z';
    d(&mut mz, 0x3C, 0x40);
    // MZ header but "PE" where the NE signature should be.
    mz[0x40] = b'P';
    mz[0x41] = b'E';
    assert!(NeFile::from_bytes("x".into(), mz).is_err());
}

#[test]
fn truncated_file_does_not_panic() {
    // Every prefix of a valid image must either parse or return an error.
    let full = build();
    for len in (0x40..full.len()).step_by(7) {
        let _ = NeFile::from_bytes("x".into(), full[..len].to_vec());
    }
}
