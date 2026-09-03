//! Context panel for whatever is selected.
//!
//! The point of this panel is that reading a listing is mostly asking "what is
//! this and who else touches it"; having the answer beside the line means not
//! losing your place to go and find out.

use crate::state::{Action, Nav, SegTab, SithApp};
use crate::theme::*;
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_analysis::Addr;


pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match app.nav() {
                Nav::Segment(segno) => segment_context(app, ui, act, segno),
                Nav::Resource(i) => resource_context(app, ui, i),
                _ => {
                    widgets::section(ui, "MODULE");
                    kv(ui, "module", doc.ne.module_name());
                    kv(ui, "segments", &doc.ne.segments.len().to_string());
                    kv(ui, "functions", &doc.program.functions.len().to_string());
                    kv(ui, "resources", &doc.ne.resources.len().to_string());
                    kv(
                        ui,
                        "imports",
                        &doc.ne.module_ref_names().len().to_string(),
                    );
                    widgets::section(ui, "HINT");
                    ui.label(dim(
                        "Select a line in a listing to see its bytes, its fixup \
                         and everything that references it.",
                    ));
                }
            }
        });
}

fn segment_context(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else { return };
    let (sel_opt, seg_tab) = match app.tab() {
        Some(t) => (t.sel, t.seg_tab),
        None => return,
    };

    widgets::section(ui, "SEGMENT");
    kv(ui, "index", &segno.to_string());
    kv(ui, "kind", seg.kind().as_str());
    kv(ui, "file offset", &format!("{:08X}", seg.file_offset));
    kv(ui, "size", &format!("{} bytes", seg.length));
    kv(ui, "alloc", &format!("{} bytes", seg.min_alloc));
    kv(ui, "fixups", &seg.relocs.len().to_string());
    ui.horizontal_wrapped(|ui| {
        for f in seg.flag_names().iter().skip(1) {
            widgets::chip(ui, f, DIM);
        }
    });

    let Some(sel) = sel_opt else { return };

    widgets::section(ui, "SELECTION");
    kv(ui, "address", &format!("seg{segno:02}:{sel:04X}"));
    kv(
        ui,
        "file offset",
        &format!("{:08X}", seg.file_offset + sel as u64),
    );

    if let Some(f) = doc.program.function_containing(Addr {
        segment: segno,
        offset: sel,
    }) {
        kv(ui, "function", &f.label());
        kv(ui, "func kind", f.kind.as_str());
        kv(ui, "func size", &format!("{} bytes", f.size()));
        if ui.small_button("Show in call graph").clicked() {
            act.push(Action::SetGraphRoot(f.addr));
        }
    }

    // Bytes at the selection, read as the common widths. A 16-bit binary is
    // full of packed structures and this saves a trip to a calculator.
    if let Some(bytes) = seg.data.get(sel as usize..(sel as usize + 8).min(seg.data.len())) {
        widgets::section(ui, "BYTES");
        ui.label(mono_c(
            bytes.iter().map(|b| format!("{b:02X} ")).collect::<String>(),
            BYTES,
        ));
        if bytes.len() >= 2 {
            let w = u16::from_le_bytes([bytes[0], bytes[1]]);
            kv(ui, "u16", &format!("{w:#06X}  {w}"));
        }
        if bytes.len() >= 4 {
            let d = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            kv(ui, "u32", &format!("{d:#010X}  {d}"));
        }
        let printable: String = bytes
            .iter()
            .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
            .collect();
        kv(ui, "ascii", &printable);
    }

    if seg_tab == SegTab::Disasm {
        if let Some(code) = doc.program.code.get(&segno) {
            if let Some((idx, insn)) = code
                .insns
                .iter()
                .enumerate()
                .find(|(_, i)| i.offset == sel)
            {
                widgets::section(ui, "INSTRUCTION");
                ui.label(mono_c(&insn.text, flow_color(insn.flow)));
                kv(ui, "length", &format!("{} bytes", insn.len));
                kv(ui, "flow", &format!("{:?}", insn.flow));
                if let Some(call) =
                    ne_analysis::callargs::reconstruct(code, idx, ne_core::ApiDb::embedded())
                {
                    widgets::section(ui, "API CALL");
                    ui.label(mono_c(
                        format!("{}.{}", call.module, call.signature.render()),
                        SYMBOL,
                    ));
                    ui.add_space(2.0);
                    for (i, a) in call.args.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(mono_c(format!("{:<8}", a.kind.as_str()), FAINT));
                            ui.label(mono_c(
                                a.render(),
                                if a.name.is_some() { GREEN } else { TEXT },
                            ));
                        });
                        let _ = i;
                    }
                    if !call.complete {
                        ui.label(dim("some arguments were not literal pushes"));
                    }
                }
                if let Some(f) = &insn.fixup {
                    widgets::section(ui, "FIXUP");
                    kv(ui, "kind", f.addr_type.as_str());
                    if f.additive {
                        kv(ui, "additive", "yes");
                    }
                    if widgets::link(ui, f.target.to_string(), COMMENT).clicked() {
                        act.push(crate::views::disasm::target_action(&f.target));
                    }
                }
            }
        }
    }

    // Anything that calls the selected address.
    let label = format!("seg{segno:02X}:{sel:04X}");
    let sites: Vec<Addr> = doc
        .program
        .xrefs
        .get(&label)
        .cloned()
        .unwrap_or_default();
    if !sites.is_empty() {
        widgets::section(ui, &format!("REFERENCED FROM ({})", sites.len()));
        for a in sites.iter().take(40) {
            let owner = doc
                .program
                .function_containing(*a)
                .map(|f| f.label())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                if widgets::link(ui, a.to_string(), ACCENT).clicked() {
                    act.push(Action::Goto(*a));
                }
                ui.label(mono_c(owner, DIM));
            });
        }
    }
}

fn resource_context(app: &SithApp, ui: &mut Ui, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(r) = doc.ne.resources.get(index) else {
        return;
    };
    widgets::section(ui, "RESOURCE");
    kv(ui, "type", &r.type_name());
    kv(ui, "id", &r.res_id.to_string());
    kv(ui, "file offset", &format!("{:08X}", r.offset));
    kv(ui, "size", &format!("{} bytes", r.length));
    ui.horizontal_wrapped(|ui| {
        for f in r.flag_names() {
            widgets::chip(ui, f, DIM);
        }
    });
    if let Some(info) = ne_core::dib::DibInfo::parse(doc.ne.resource_data(r)) {
        widgets::section(ui, "BITMAP");
        kv(
            ui,
            "size",
            &format!("{} x {}", info.abs_width(), info.abs_height()),
        );
        kv(ui, "depth", &format!("{} bpp", info.bit_count));
        kv(ui, "compression", info.compression_name());
        kv(ui, "palette", &info.palette_len.to_string());
    }
}

/// A label/value line, aligned so the panel reads as a table.
fn kv(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(mono_c(format!("{key:<12}"), FAINT));
        ui.label(mono_c(value, TEXT));
    });
}

