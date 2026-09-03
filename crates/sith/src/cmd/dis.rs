//! Disassembly output.

use crate::style::*;
use anyhow::{bail, Result};
use ne_analysis::{callargs, Addr, Program};
use ne_disasm::{byte_column_width, disassemble, Flow, Options, Syntax};
use ne_core::{ApiDb, NeFile};
use std::collections::BTreeSet;

#[allow(clippy::too_many_arguments)]
pub fn run(
    ne: &NeFile,
    segment: u16,
    start: Option<u32>,
    end: Option<u32>,
    func: Option<u32>,
    bits32: bool,
    syntax: &str,
) -> Result<()> {
    let Some(seg) = ne.segment(segment) else {
        bail!(
            "segment {segment} does not exist (this file has {})",
            ne.segments.len()
        );
    };
    if seg.data.is_empty() {
        bail!("segment {segment} has no file image");
    }

    // Naming the functions in the segment costs one extra decode pass and
    // makes the listing far easier to read, so it is always done.
    let mut bits = BTreeSet::new();
    if bits32 {
        bits.insert(segment);
    }
    let program = Program::analyze(ne, &bits);

    let (start, end) = match func {
        Some(off) => {
            let f = program
                .function_containing(Addr {
                    segment,
                    offset: off,
                })
                .filter(|f| f.addr.offset == off);
            match f {
                Some(f) => (f.addr.offset, Some(f.end)),
                // Not a recognised start; fall back to a fixed window so the
                // command still shows something useful.
                None => (off, Some((off + 0x600).min(seg.data.len() as u32))),
            }
        }
        None => (start.unwrap_or(0), end),
    };

    let opts = Options {
        syntax: Syntax::parse(syntax).unwrap_or_default(),
        bits: if bits32 { 32 } else { 16 },
        start,
        end,
    };
    let code = disassemble(ne, seg, &opts);
    let width = byte_column_width(&code.insns);

    let labels: std::collections::BTreeMap<u32, String> = program
        .functions
        .iter()
        .filter(|f| f.addr.segment == segment)
        .map(|f| (f.addr.offset, f.label()))
        .collect();

    println!(
        "{}",
        heading(&format!(
            "{} segment {} ({}, {}-bit) {:04X}..{:04X}",
            ne.module_name(),
            segment,
            seg.kind().as_str(),
            opts.bits,
            start,
            end.unwrap_or(seg.data.len() as u32)
        ))
    );

    let api = ApiDb::embedded();
    for (i, insn) in code.insns.iter().enumerate() {
        if let Some(name) = labels.get(&insn.offset) {
            println!();
            println!("{}", bold(&format!("{name}:")));
        }
        let mut line = format!(
            "{:04X}  {:<width$} ",
            insn.offset,
            insn.hex(),
            width = width
        );
        line.push_str(&color_text(insn.flow, &insn.text));
        if let Some(f) = &insn.fixup {
            // Where the callee's signature is known and its arguments were
            // pushed as literals, the reconstructed call says far more than
            // the symbol name alone.
            match callargs::reconstruct(&code, i, api) {
                Some(call) => {
                    line.push_str(&cyan(&format!("  ; {}.{}", call.module, call.render())));
                    if !call.complete {
                        line.push_str(&dim(" (partial)"));
                    }
                }
                None => line.push_str(&cyan(&format!("  ; {}", f.target))),
            }
            if f.additive {
                line.push_str(&yellow(" [additive]"));
            }
        } else if let Some(t) = insn.near_target {
            if let Some(name) = labels.get(&t) {
                line.push_str(&dim(&format!("  ; {name}")));
            }
        }
        println!("{line}");
    }
    Ok(())
}

fn color_text(flow: Flow, text: &str) -> String {
    match flow {
        Flow::Call | Flow::CallFar | Flow::CallIndirect => green(text),
        Flow::Jump | Flow::JumpFar | Flow::JumpIndirect | Flow::CondJump => blue(text),
        Flow::Return => magenta(text),
        Flow::Interrupt => yellow(text),
        Flow::Invalid => red(text),
        Flow::Next => text.to_string(),
    }
}
