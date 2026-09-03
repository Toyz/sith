//! Call-site argument reconstruction, driven from assembled bytes.

use ne_analysis::callargs;
use ne_core::api::ApiDb;
use ne_disasm::{Options, SegmentCode};

const CALL_FAR: [u8; 5] = [0x9A, 0x00, 0x00, 0x00, 0x00];

/// Assemble a run of pushes followed by a far call, then attach the fixup that
/// makes the call look like an import of `MODULE.ordinal`.
fn call_site(pushes: &[&[u8]], module: &str, ordinal: u16) -> SegmentCode {
    let mut bytes = Vec::new();
    for p in pushes {
        bytes.extend_from_slice(p);
    }
    bytes.extend_from_slice(&CALL_FAR);

    let mut code = ne_disasm::disassemble_raw(1, &bytes, &Options::default());
    let last = code.insns.len() - 1;
    code.insns[last].fixup = Some(ne_core::Fixup {
        site: code.insns[last].offset as u16 + 1,
        addr_type: ne_core::AddrType::Far,
        additive: false,
        target: ne_core::Target::ImportOrdinal {
            module: module.to_string(),
            ordinal,
            name: None,
        },
    });
    code
}

fn reconstruct(code: &SegmentCode) -> Option<callargs::CallArgs> {
    callargs::reconstruct(code, code.insns.len() - 1, ApiDb::embedded())
}

#[test]
fn a_single_word_argument_gets_its_constant_name() {
    // GDI.87 is GetStockObject(word), whose argument is a named enumeration.
    let code = call_site(&[&[0x6A, 0x04]], "GDI", 87); // push 4
    let call = reconstruct(&code).expect("GetStockObject has a signature");
    assert_eq!(call.function, "GetStockObject");
    assert_eq!(call.args.len(), 1);
    assert_eq!(call.args[0].value, Some(4));
    assert_eq!(call.args[0].name.as_deref(), Some("BLACK_BRUSH"));
    assert!(call.complete);
    assert_eq!(call.render(), "GetStockObject(BLACK_BRUSH)");
}

#[test]
fn long_arguments_join_two_pushed_words_high_first() {
    // GlobalAlloc(GMEM_MOVEABLE|GMEM_ZEROINIT, 0x1000):
    //   push 42h    ; flags
    //   push 0      ; high word of the size
    //   push 1000h  ; low word of the size
    let code = call_site(
        &[&[0x6A, 0x42], &[0x6A, 0x00], &[0x68, 0x00, 0x10]],
        "KERNEL",
        15,
    );
    let call = reconstruct(&code).unwrap();
    assert_eq!(call.function, "GlobalAlloc");
    assert_eq!(
        call.args[0].name.as_deref(),
        Some("GMEM_MOVEABLE|GMEM_ZEROINIT")
    );
    assert_eq!(call.args[1].value, Some(0x0000_1000));
    assert!(call.complete);
}

#[test]
fn a_memory_operand_is_not_read_as_a_literal() {
    // `push word [bp+6]` must not be mistaken for the constant 6: the
    // displacement belongs to an address, not to the pushed value.
    let code = call_site(&[&[0xFF, 0x76, 0x06]], "GDI", 87);
    let call = reconstruct(&code).unwrap();
    assert_eq!(call.args[0].value, None);
    assert_eq!(call.args[0].name, None);
    assert!(!call.complete);
    assert!(
        call.render().contains("bp"),
        "the operand text should survive: {}",
        call.render()
    );
}

#[test]
fn a_non_push_ends_the_argument_run() {
    // The `push 63h` belongs to earlier work; the `xor` between it and the
    // call must stop the walk rather than let it be harvested.
    let mut bytes = vec![0x6A, 0x63, 0x31, 0xC0];
    bytes.extend_from_slice(&CALL_FAR);
    let mut code = ne_disasm::disassemble_raw(1, &bytes, &Options::default());
    let last = code.insns.len() - 1;
    code.insns[last].fixup = Some(ne_core::Fixup {
        site: 0,
        addr_type: ne_core::AddrType::Far,
        additive: false,
        target: ne_core::Target::ImportOrdinal {
            module: "GDI".into(),
            ordinal: 87,
            name: None,
        },
    });
    let call = reconstruct(&code).unwrap();
    assert_eq!(call.args[0].value, None, "must not reach past the xor");
    assert!(!call.complete);
}

#[test]
fn calls_without_a_known_signature_are_left_alone() {
    let code = call_site(&[&[0x6A, 0x01]], "NOSUCHMOD", 1);
    assert!(reconstruct(&code).is_none());
}

#[test]
fn a_call_with_no_fixup_is_not_an_api_call() {
    let code = ne_disasm::disassemble_raw(1, &CALL_FAR, &Options::default());
    assert!(reconstruct(&code).is_none());
}
