//! Header, segment, import, export and entry-table reports.

use crate::style::*;
use anyhow::Result;
use ne_core::{NeFile, RelKind};
use serde_json::json;
use std::collections::BTreeMap;

pub fn run(ne: &NeFile, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary_json(ne))?);
        return Ok(());
    }
    let h = &ne.header;
    println!(
        "{} {}",
        bold("file       "),
        format!("{} ({} bytes)", ne.path.display(), ne.buf.len())
    );
    println!("{} {}", bold("module     "), cyan(ne.module_name()));
    if !ne.description().is_empty() {
        println!("{} {}", bold("description"), ne.description());
    }
    println!(
        "{} {}  {}  {}",
        bold("kind       "),
        if h.is_library() { "DLL" } else { "application" },
        h.target_os.name(),
        if h.is_self_loading() {
            yellow("self-loading")
        } else {
            String::new()
        }
    );
    println!(
        "{} linker {}.{}   flags {:04X} [{}]   expects Windows {}.{}",
        bold("build      "),
        h.linker_version.0,
        h.linker_version.1,
        h.flags,
        h.flag_names().join(" "),
        h.expected_version >> 8,
        h.expected_version & 0xFF
    );
    println!(
        "{} CS:IP {:04X}:{:04X}   SS:SP {:04X}:{:04X}",
        bold("entry      "),
        h.cs_ip >> 16,
        h.cs_ip & 0xFFFF,
        h.ss_sp >> 16,
        h.ss_sp & 0xFFFF
    );
    println!(
        "{} segment {}   heap {}   stack {}   align 1<<{}",
        bold("autodata   "),
        h.auto_data_segment,
        h.heap_size,
        h.stack_size,
        h.align_shift_or_default()
    );
    println!();

    print_segments(ne);
    println!();
    print_module_refs(ne);
    println!();
    print_exports(ne);
    if !ne.resources.is_empty() {
        println!();
        print_resource_summary(ne);
    }
    Ok(())
}

pub fn segments(ne: &NeFile, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(&segments_json(ne))?);
        return Ok(());
    }
    print_segments(ne);
    Ok(())
}

fn print_segments(ne: &NeFile) {
    println!(
        "{}",
        heading(&format!("segments ({})", ne.segments.len()))
    );
    println!(
        "{}",
        dim("  #   file       size    alloc  relocs  flags")
    );
    for s in &ne.segments {
        let kind = if s.is_code() {
            green(s.kind().as_str())
        } else {
            blue(s.kind().as_str())
        };
        let flags: Vec<&str> = s.flag_names().into_iter().skip(1).collect();
        println!(
            "  {:<3} {:08X}  {:6}  {:6}  {:6}  {} {}",
            s.index,
            s.file_offset,
            s.length,
            s.min_alloc,
            s.relocs.len(),
            kind,
            dim(&flags.join(" "))
        );
    }
}

pub fn imports(ne: &NeFile, as_json: bool) -> Result<()> {
    // Group every ordinal and named import by the module it comes from, so
    // the report reads as an API surface rather than a list of fixups.
    let mut by_module: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for seg in &ne.segments {
        for r in &seg.relocs {
            let sites = r.sites(&seg.data).len().max(1);
            let (module, symbol) = match r.kind {
                RelKind::ImportOrdinal => (
                    ne.module_ref_name(r.target1),
                    match ne.import_ordinal_name(r.target1, r.target2) {
                        Some(n) => format!("{n} @{}", r.target2),
                        None => format!("@{}", r.target2),
                    },
                ),
                RelKind::ImportName => {
                    (ne.module_ref_name(r.target1), ne.imported_name(r.target2))
                }
                _ => continue,
            };
            *by_module
                .entry(module)
                .or_default()
                .entry(symbol)
                .or_insert(0) += sites;
        }
    }
    // A module can be referenced without any surviving fixup, so seed the map
    // from the module reference table too.
    for m in ne.module_ref_names() {
        by_module.entry(m).or_default();
    }

    if as_json {
        let v: serde_json::Value = by_module
            .iter()
            .map(|(m, syms)| {
                (
                    m.clone(),
                    json!(syms
                        .iter()
                        .map(|(s, n)| json!({"symbol": s, "sites": n}))
                        .collect::<Vec<_>>()),
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    for (module, syms) in &by_module {
        println!(
            "{} {}",
            heading(&magenta(module)),
            dim(&format!("({} symbols)", syms.len()))
        );
        let mut rows: Vec<(&String, &usize)> = syms.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (sym, sites) in rows {
            println!("  {:>5}  {}", dim(&sites.to_string()), sym);
        }
        println!();
    }
    Ok(())
}

fn print_module_refs(ne: &NeFile) {
    let names = ne.module_ref_names();
    println!("{}", heading(&format!("module references ({})", names.len())));
    for (i, n) in names.iter().enumerate() {
        println!("  {:<3} {}", i + 1, magenta(n));
    }
}

pub fn exports(ne: &NeFile, as_json: bool) -> Result<()> {
    if as_json {
        let v: Vec<_> = ne
            .exports()
            .iter()
            .map(|e| {
                json!({
                    "ordinal": e.ordinal,
                    "name": e.name,
                    "segment": e.segment,
                    "offset": e.offset,
                    "moveable": e.moveable,
                    "resident": e.resident,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    print_exports(ne);
    Ok(())
}

fn print_exports(ne: &NeFile) {
    let exports = ne.exports();
    println!(
        "{}",
        heading(&format!(
            "exports ({} of {} entry slots)",
            exports.len(),
            ne.entries.len()
        ))
    );
    for e in exports {
        println!(
            "  @{:<5} {:<28} seg{:02X}:{:04X} {} {}",
            e.ordinal,
            cyan(&e.label()),
            e.segment,
            e.offset,
            if e.moveable { dim("moveable") } else { String::new() },
            if e.resident { dim("resident") } else { String::new() }
        );
    }
}

pub fn entries(ne: &NeFile, as_json: bool) -> Result<()> {
    if as_json {
        let v: Vec<_> = ne
            .entries
            .values()
            .map(|e| {
                json!({
                    "ordinal": e.ordinal,
                    "name": e.name,
                    "segment": e.segment,
                    "offset": e.offset,
                    "flags": e.flags,
                    "exported": e.is_exported(),
                    "moveable": e.moveable,
                    "stack_words": e.stack_words(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    println!("{}", heading(&format!("entry table ({})", ne.entries.len())));
    println!(
        "{}",
        dim("  ord    address        flags  name")
    );
    for e in ne.entries.values() {
        let mut marks = Vec::new();
        if e.is_exported() {
            marks.push(green("export"));
        }
        if e.moveable {
            marks.push(dim("moveable"));
        }
        if e.uses_shared_data() {
            marks.push(dim("shareddata"));
        }
        println!(
            "  @{:<5} seg{:02X}:{:04X}   {:<5}  {}  {}",
            e.ordinal,
            e.segment,
            e.offset,
            format!("{:02X}", e.flags),
            e.name.as_deref().unwrap_or("-"),
            marks.join(" ")
        );
    }
    Ok(())
}

fn print_resource_summary(ne: &NeFile) {
    let mut by_type: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for r in &ne.resources {
        let e = by_type.entry(r.type_name()).or_insert((0, 0));
        e.0 += 1;
        e.1 += r.length as u64;
    }
    println!(
        "{}",
        heading(&format!("resources ({})", ne.resources.len()))
    );
    for (t, (n, bytes)) in by_type {
        println!("  {:<16} {:>4}  {:>9} bytes", yellow(&t), n, bytes);
    }
}

fn segments_json(ne: &NeFile) -> serde_json::Value {
    json!(ne
        .segments
        .iter()
        .map(|s| json!({
            "index": s.index,
            "file_offset": s.file_offset,
            "length": s.length,
            "min_alloc": s.min_alloc,
            "flags": s.flags,
            "kind": s.kind().as_str(),
            "flag_names": s.flag_names(),
            "relocs": s.relocs.len(),
        }))
        .collect::<Vec<_>>())
}

fn summary_json(ne: &NeFile) -> serde_json::Value {
    let h = &ne.header;
    json!({
        "path": ne.path.display().to_string(),
        "size": ne.buf.len(),
        "module": ne.module_name(),
        "description": ne.description(),
        "is_library": h.is_library(),
        "self_loading": h.is_self_loading(),
        "target_os": h.target_os.name(),
        "linker": format!("{}.{}", h.linker_version.0, h.linker_version.1),
        "flags": h.flags,
        "flag_names": h.flag_names(),
        "expected_windows": format!("{}.{}", h.expected_version >> 8, h.expected_version & 0xFF),
        "cs_ip": format!("{:04X}:{:04X}", h.cs_ip >> 16, h.cs_ip & 0xFFFF),
        "ss_sp": format!("{:04X}:{:04X}", h.ss_sp >> 16, h.ss_sp & 0xFFFF),
        "auto_data_segment": h.auto_data_segment,
        "heap": h.heap_size,
        "stack": h.stack_size,
        "align_shift": h.align_shift_or_default(),
        "segments": segments_json(ne),
        "modules": ne.module_ref_names(),
        "exports": ne.exports().iter().map(|e| json!({
            "ordinal": e.ordinal,
            "name": e.name,
            "segment": e.segment,
            "offset": e.offset,
        })).collect::<Vec<_>>(),
        "resources": ne.resources.iter().map(|r| json!({
            "type": r.type_name(),
            "id": r.res_id.to_string(),
            "offset": r.offset,
            "length": r.length,
        })).collect::<Vec<_>>(),
    })
}
