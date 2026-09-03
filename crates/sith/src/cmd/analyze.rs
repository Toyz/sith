//! Function, call graph and cross-reference reports.

use crate::style::*;
use anyhow::Result;
use ne_analysis::Program;
use ne_core::NeFile;
use serde_json::json;
use std::collections::BTreeSet;

pub fn funcs(
    ne: &NeFile,
    segment: Option<u16>,
    bits32: &BTreeSet<u16>,
    as_json: bool,
) -> Result<()> {
    let program = Program::analyze(ne, bits32);
    let selected: Vec<_> = program
        .functions
        .iter()
        .filter(|f| segment.is_none_or(|s| f.addr.segment == s))
        .collect();

    if as_json {
        let v: Vec<_> = selected
            .iter()
            .map(|f| {
                json!({
                    "segment": f.addr.segment,
                    "offset": f.addr.offset,
                    "end": f.end,
                    "size": f.size(),
                    "label": f.label(),
                    "name": f.name,
                    "ordinal": f.ordinal,
                    "kind": f.kind.as_str(),
                    "instructions": f.insn_count,
                    "far": f.frame.far,
                    "argument_bytes": f.frame.argument_bytes(),
                    "local_bytes": f.frame.local_bytes,
                    "argument_offsets": f.frame.argument_offsets,
                    "calls": f.calls.iter().map(|c| c.target.to_string()).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    let mut current = 0u16;
    for f in selected {
        if f.addr.segment != current {
            current = f.addr.segment;
            println!();
            println!("{}", heading(&format!("segment {current}")));
            println!(
                "{}",
                dim("  address       size  insns  args  kind        name")
            );
        }
        println!(
            "  {}  {:>5}  {:>5}  {:>4}  {:<10}  {}",
            f.addr,
            f.size(),
            f.insn_count,
            match f.frame.argument_bytes() {
                Some(0) | None => dim("-"),
                Some(n) => format!("{n}"),
            },
            dim(f.kind.as_str()),
            cyan(&f.label())
        );
    }
    Ok(())
}

pub fn callgraph(ne: &NeFile, segment: Option<u16>, bits32: &BTreeSet<u16>) -> Result<()> {
    let program = Program::analyze(ne, bits32);
    let mut current = 0u16;
    for f in &program.functions {
        if segment.is_some_and(|s| f.addr.segment != s) {
            continue;
        }
        if f.addr.segment != current {
            current = f.addr.segment;
            println!();
            println!("{}", heading(&format!("segment {current}")));
        }
        // Repeated calls to the same target say little; the distinct set is
        // what identifies what a function does.
        let mut targets: Vec<String> = f.calls.iter().map(|c| c.target.to_string()).collect();
        targets.sort();
        targets.dedup();
        println!("  {} {}", f.addr, cyan(&f.label()));
        for t in targets {
            println!("      {} {}", dim("->"), t);
        }
    }
    Ok(())
}

pub fn xref(ne: &NeFile, name: &str, bits32: &BTreeSet<u16>) -> Result<()> {
    let program = Program::analyze(ne, bits32);
    let hits = program.find_xrefs(name);
    if hits.is_empty() {
        println!("{}", dim(&format!("no call sites match {name:?}")));
        return Ok(());
    }
    for (target, sites) in hits {
        println!("{} {}", heading(&cyan(target)), dim(&format!("({} sites)", sites.len())));
        for a in sites {
            let owner = program
                .function_containing(*a)
                .map(|f| f.label())
                .unwrap_or_else(|| "-".into());
            println!("  {}  in {}", a, owner);
        }
        println!();
    }
    Ok(())
}
