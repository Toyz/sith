//! The context panel.
//!
//! Reading a disassembly is mostly asking two questions about the line under
//! the cursor: what is this, and who else touches it. The panel answers them in
//! that order -- identity first, then the things that act on it, then the
//! context it sits in -- so the eye lands on the answer rather than scanning a
//! wall of label/value pairs for it.

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SegTab, SithApp};
use crate::theme::{col, dim, flow_color, mono_c};
use crate::widgets::{self, card, kv, kv_colored};
use eframe::egui::{self, Ui};
use ne_analysis::{resrefs::Confidence, Addr};

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    if app.doc().is_none() {
        return;
    }
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Values are right-aligned, so the scrollbar's strip has to be
            // kept clear or it sits on top of them.
            ui.set_width((ui.available_width() - widgets::SCROLLBAR_GUTTER).max(80.0));
            match app.nav() {
                Nav::Segment(segno) => match app.tab().and_then(|t| t.sel) {
                    Some(sel) => address(app, ui, act, segno, sel),
                    None => nothing_selected(app, ui, segno),
                },
                Nav::Resource(i) => resource(app, ui, act, i),
                // Selecting in the graph reads the function here, which is
                // what lets a click stay in the graph instead of navigating
                // away from it.
                Nav::Graph => match app.tab().and_then(|t| t.graph.selected) {
                    Some(a) => address(app, ui, act, a.segment, a.offset),
                    None => module(app, ui),
                },
                _ => module(app, ui),
            }
            ui.add_space(16.0);
        });
}

// ----------------------------------------------------------------- identity

/// The headline: what this address is, and the actions that apply to it.
fn identity(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let Some(doc) = app.doc() else { return };
    let marked = app.is_bookmarked(segment, offset);
    let name = app.user_name(segment, offset);
    let function = doc.program.function_containing(Addr { segment, offset });

    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if marked {
                    ui.label(mono_c("\u{25C6}", col::orange()));
                }
                ui.label(
                    egui::RichText::new(format!("seg{segment:02}:{offset:04X}"))
                        .monospace()
                        .size(15.0)
                        .strong()
                        .color(col::accent()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Copy, "Copy address").clicked() {
                        ui.ctx().copy_text(format!("seg{segment:02}:{offset:04X}"));
                        act.push(Action::Status("address copied".into()));
                    }
                    if icons::button(
                        ui,
                        Icon::Version,
                        if marked { "Remove bookmark (B)" } else { "Bookmark (B)" },
                    )
                    .clicked()
                    {
                        act.push(Action::ToggleBookmark { segment, offset });
                    }
                    if icons::button(ui, Icon::Font, "Name this address (N)").clicked() {
                        act.push(Action::ShowRename { segment, offset });
                    }
                });
            });

            // Colour is an annotation like naming, and belongs beside it.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for name in crate::theme::USER_COLORS {
                    let Some(c) = crate::theme::named_color(name) else {
                        continue;
                    };
                    let picked = app.user_color_name(segment, offset) == Some(*name);
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::click());
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(4),
                        c.gamma_multiply(if picked { 0.9 } else { 0.55 }),
                    );
                    if picked || resp.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            egui::CornerRadius::same(4),
                            egui::Stroke::new(if picked { 2.0 } else { 1.0 }, c),
                            egui::StrokeKind::Outside,
                        );
                    }
                    // Clicking the colour it already has clears it.
                    if resp
                        .on_hover_text(if picked {
                            format!("{name} — click to clear")
                        } else {
                            name.to_string()
                        })
                        .clicked()
                    {
                        act.push(Action::SetColor {
                            segment,
                            offset,
                            color: if picked { None } else { Some(name) },
                        });
                    }
                }
            });
            ui.add_space(3.0);

            // The name the user gave wins over the generated one, and says so.
            match (name, function) {
                (Some(n), _) => {
                    ui.horizontal(|ui| {
                        ui.label(mono_c(n, col::cyan()));
                        widgets::chip(ui, "your name", col::cyan());
                    });
                }
                (None, Some(f)) => {
                    ui.horizontal(|ui| {
                        ui.label(mono_c(f.label(), col::symbol()));
                        if f.addr.offset != offset {
                            ui.label(
                                egui::RichText::new(format!("+{:X}", offset - f.addr.offset))
                                    .monospace()
                                    .size(11.0)
                                    .color(col::faint()),
                            );
                        }
                    });
                }
                (None, None) => {
                    ui.label(dim("not inside a known function"));
                }
            }
            if let Some(seg) = doc.ne.segment(segment) {
                ui.label(
                    egui::RichText::new(format!("file {:08X}", seg.file_offset + offset as u64))
                        .monospace()
                        .size(11.0)
                        .color(col::faint()),
                );
            }
        });
    ui.add_space(10.0);
}

/// The note box, which only takes space once it has something in it.
fn note(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let existing = app.user_comment(segment, offset).unwrap_or_default();
    let mut text = existing.to_string();
    let before = text.clone();
    card(ui, "NOTE", |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut text)
                .hint_text("what is this for?")
                .desired_rows(if existing.is_empty() { 1 } else { 3 })
                .desired_width(f32::INFINITY)
                .frame(egui::Frame::NONE),
        );
    });
    if text != before {
        act.push(Action::SetComment {
            segment,
            offset,
            text,
        });
    }
}

// ---------------------------------------------------------------- selection

fn address(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let Some(doc) = app.doc() else { return };
    identity(app, ui, act, segment, offset);
    note(app, ui, act, segment, offset);

    let seg_tab = app.tab().map(|t| t.seg_tab).unwrap_or(SegTab::Disasm);
    let insn = if seg_tab == SegTab::Disasm {
        doc.program
            .code
            .get(&segment)
            .and_then(|c| c.insns.iter().enumerate().find(|(_, i)| i.offset == offset))
    } else {
        None
    };

    if let Some((index, insn)) = insn {
        card(ui, "INSTRUCTION", |ui| {
            ui.label(mono_c(&insn.text, flow_color(insn.flow)));
            ui.add_space(4.0);
            kv_colored(ui, "bytes", insn.hex(), col::bytes());
            kv(ui, "length", format!("{} bytes", insn.len));
            kv_colored(ui, "flow", format!("{:?}", insn.flow), flow_color(insn.flow));
        });

        if let Some(f) = &insn.fixup {
            card(ui, "FIXUP", |ui| {
                kv_colored(ui, "kind", f.addr_type.as_str(), col::dim());
                if f.additive {
                    kv_colored(ui, "additive", "yes", col::orange());
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("target")
                            .size(11.0)
                            .monospace()
                            .color(col::faint()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::link(ui, f.target.to_string(), col::comment()).clicked() {
                            act.push(crate::views::disasm::target_action(&f.target));
                        }
                    });
                });
            });
        }

        api_call(app, ui, segment, index);
    }

    // A loading call points at artwork; that is usually what you wanted.
    if let Some(res) = doc.res_links.resource_at(Addr { segment, offset }) {
        if let Some(r) = doc.ne.resources.get(res) {
            card(ui, "LOADS", |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, icons::for_resource(r.type_id.as_id()), col::orange());
                    if widgets::link(
                        ui,
                        format!("{} {}", r.type_name(), r.res_id),
                        col::orange(),
                    )
                    .clicked()
                    {
                        act.push(Action::Go(Nav::Resource(res)));
                    }
                });
            });
        }
    }

    bytes_at(app, ui, segment, offset);
    references(app, ui, act, segment, offset);
    function_card(app, ui, act, segment, offset);
    segment_card(app, ui, segment, true);
}

/// The reconstructed call: signature, then one row per argument.
fn api_call(app: &SithApp, ui: &mut Ui, segment: u16, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(code) = doc.program.code.get(&segment) else {
        return;
    };
    let Some(call) = ne_analysis::callargs::reconstruct(code, index, ne_core::ApiDb::embedded())
    else {
        return;
    };

    card(ui, "API CALL", |ui| {
        ui.horizontal(|ui| {
            ui.label(mono_c(&call.module, col::purple()));
            ui.label(mono_c(&call.signature.name, col::symbol()).strong());
            if !call.complete {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::chip(ui, "partial", col::orange());
                });
            }
        });
        if let Some(ret) = &call.signature.ret {
            ui.label(
                egui::RichText::new(format!("returns {ret}"))
                    .size(10.5)
                    .monospace()
                    .color(col::faint()),
            );
        }
        ui.add_space(6.0);

        for (i, a) in call.args.iter().enumerate() {
            let label = call
                .signature
                .param_name(i)
                .map(str::to_string)
                .unwrap_or_else(|| format!("arg{i}"));
            // The argument's width lives in the tooltip rather than in a
            // column: an operand like `word [bp+0Ah]:word [bp+8]` already says
            // it, and a third column collides with the value in a narrow panel.
            let value = a.render();
            let colour = if a.name.is_some() {
                col::green()
            } else if a.value.is_some() {
                col::text()
            } else {
                col::faint()
            };
            let r = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    egui::RichText::new(&label)
                        .size(11.0)
                        .monospace()
                        .color(col::cyan()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(mono_c(elide(&strip_size(&value), 22), colour));
                });
            });
            r.response.on_hover_text(format!(
                "{} {}\n{}",
                a.kind.as_str(),
                label,
                value
            ));
        }
        if !call.complete {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("some arguments were not literal pushes")
                    .size(10.5)
                    .color(col::faint()),
            );
        }
    });
}

/// The bytes at the cursor, read as the widths a 16-bit structure uses.
fn bytes_at(app: &SithApp, ui: &mut Ui, segment: u16, offset: u32) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segment) else { return };
    let start = offset as usize;
    let Some(b) = seg.data.get(start..(start + 8).min(seg.data.len())) else {
        return;
    };
    if b.is_empty() {
        return;
    }

    card(ui, "DATA AT CURSOR", |ui| {
        ui.label(mono_c(
            b.iter().map(|x| format!("{x:02X} ")).collect::<String>(),
            col::bytes(),
        ));
        ui.add_space(4.0);
        kv(ui, "u8", format!("{:#04X}   {}", b[0], b[0]));
        if b.len() >= 2 {
            let w = u16::from_le_bytes([b[0], b[1]]);
            kv(ui, "u16", format!("{w:#06X}   {w}"));
            kv_colored(ui, "i16", format!("{}", w as i16), col::dim());
        }
        if b.len() >= 4 {
            let d = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            kv(ui, "u32", format!("{d:#010X}   {d}"));
            // A far pointer is the other thing four bytes usually mean here.
            kv_colored(
                ui,
                "seg:off",
                format!("{:04X}:{:04X}", d >> 16, d & 0xFFFF),
                col::dim(),
            );
        }
        let ascii: String = b
            .iter()
            .map(|&x| if (0x20..0x7F).contains(&x) { x as char } else { '·' })
            .collect();
        kv_colored(ui, "ascii", ascii, col::dim());
    });
}

/// Everything that calls this address.
fn references(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let Some(doc) = app.doc() else { return };
    let label = format!("seg{segment:02X}:{offset:04X}");
    let sites = doc.program.xrefs.get(&label).cloned().unwrap_or_default();
    if sites.is_empty() {
        return;
    }
    card(ui, &format!("REFERENCED FROM ({})", sites.len()), |ui| {
        for a in sites.iter().take(40) {
            let owner = doc
                .program
                .function_containing(*a)
                .map(|f| app.label(f))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if widgets::link(ui, a.to_string(), col::accent()).clicked() {
                    act.push(Action::Goto(*a));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(mono_c(owner, col::symbol()));
                });
            });
        }
        if sites.len() > 40 {
            ui.label(dim(format!("and {} more", sites.len() - 40)));
        }
    });
}

fn function_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let Some(doc) = app.doc() else { return };
    let Some(f) = doc.program.function_containing(Addr { segment, offset }) else {
        return;
    };
    card(ui, "FUNCTION", |ui| {
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Code, col::symbol());
            if widgets::link(ui, app.label(f), col::symbol()).clicked() {
                act.push(Action::Goto(f.addr));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::chip(ui, f.kind.as_str(), col::dim());
            });
        });
        ui.add_space(4.0);
        kv(ui, "start", f.addr.to_string());
        kv(ui, "size", format!("{} bytes", f.size()));
        kv(ui, "instructions", f.insn_count.to_string());
        kv(ui, "calls", f.calls.len().to_string());
        // Recovered from the frame, without symbols: a pascal callee pops its
        // own arguments, so the return instruction states their size.
        kv_colored(ui, "called", if f.frame.far { "far" } else { "near" }, col::dim());
        match f.frame.argument_bytes() {
            Some(n) if n > 0 => {
                kv_colored(ui, "arguments", format!("{n} bytes"), col::green());
                if !f.frame.argument_offsets.is_empty() {
                    kv_colored(
                        ui,
                        "at",
                        f.frame
                            .argument_offsets
                            .iter()
                            .map(|d| format!("bp+{d:X}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                        col::dim(),
                    );
                }
            }
            _ => kv_colored(ui, "arguments", "none", col::faint()),
        }
        if let Some(l) = f.frame.local_bytes.filter(|l| *l > 0) {
            kv(ui, "locals", format!("{l} bytes"));
        }
        ui.add_space(6.0);
        if ui.small_button("Show in call graph").clicked() {
            act.push(Action::SetGraphRoot(f.addr));
        }
    });
}

/// The segment is context rather than selection, so it sits last and folded.
fn segment_card(app: &SithApp, ui: &mut Ui, segno: u16, collapsed: bool) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else { return };
    widgets::collapsing_card(
        ui,
        "inspector-segment",
        &format!("SEGMENT {segno}"),
        !collapsed,
        |ui| {
            kv_colored(
                ui,
                "kind",
                seg.kind().as_str(),
                if seg.is_code() {
                    col::code_seg()
                } else {
                    col::data_seg()
                },
            );
            kv(ui, "file offset", format!("{:08X}", seg.file_offset));
            kv(ui, "size", format!("{} bytes", seg.length));
            kv(ui, "alloc", format!("{} bytes", seg.min_alloc));
            kv(ui, "fixups", seg.relocs.len().to_string());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for f in seg.flag_names().iter().skip(1) {
                    widgets::chip(ui, f, col::dim());
                }
            });
        },
    );
}

fn nothing_selected(app: &SithApp, ui: &mut Ui, segno: u16) {
    segment_card(app, ui, segno, false);
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        icons::inline(ui, Icon::Target, col::faint());
        ui.label(dim("select a line"));
        ui.label(
            egui::RichText::new("its bytes, its fixup and everything that references it")
                .size(10.5)
                .color(col::faint()),
        );
    });
}

/// An operand's size prefix repeats what the argument type already says.
fn strip_size(text: &str) -> String {
    let mut out = text.to_string();
    for prefix in ["word ", "byte ", "dword ", "qword "] {
        out = out.replace(prefix, "");
    }
    out
}

/// Keep a value inside the panel; the full text stays in the tooltip.
fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max - 1).collect();
    format!("{head}\u{2026}")
}

// ----------------------------------------------------------------- resource

fn resource(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(r) = doc.ne.resources.get(index) else {
        return;
    };

    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, icons::for_resource(r.type_id.as_id()), col::orange());
                ui.label(
                    egui::RichText::new(r.res_id.to_string())
                        .monospace()
                        .size(15.0)
                        .strong()
                        .color(col::text()),
                );
            });
            ui.label(mono_c(r.type_name(), col::orange()));
        });
    ui.add_space(10.0);

    card(ui, "STORED", |ui| {
        kv(ui, "file offset", format!("{:08X}", r.offset));
        kv(ui, "size", format!("{} bytes", r.length));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for f in r.flag_names() {
                widgets::chip(ui, f, col::dim());
            }
        });
    });

    let data = doc.ne.resource_data(r);
    if let Some(info) = ne_core::dib::DibInfo::parse(data) {
        card(ui, "BITMAP", |ui| {
            kv(ui, "size", format!("{} x {}", info.abs_width(), info.abs_height()));
            kv(ui, "depth", format!("{} bpp", info.bit_count));
            kv(ui, "compression", info.compression_name());
            kv(ui, "palette", info.palette_len.to_string());
        });
    }
    if let Some(font) = ne_core::fnt::parse(data) {
        card(ui, "FONT", |ui| {
            kv(ui, "face", &font.face);
            kv(ui, "size", format!("{} point", font.header.points));
            kv(
                ui,
                "cell",
                format!("{} x {}", font.header.pix_width, font.header.pix_height),
            );
            kv(ui, "weight", font.header.weight_name());
            kv(ui, "charset", font.header.charset_name());
            kv(ui, "glyphs", font.glyphs.len().to_string());
        });
    }

    let uses = doc.res_links.uses(index);
    if !uses.is_empty() {
        card(ui, &format!("LOADED BY ({})", uses.len()), |ui| {
            for u in uses {
                let owner = doc
                    .program
                    .function_containing(u.addr)
                    .map(|f| app.label(f))
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if widgets::link(ui, u.addr.to_string(), col::accent()).clicked() {
                        act.push(Action::Goto(u.addr));
                    }
                    ui.label(mono_c(owner, col::symbol()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::chip(
                            ui,
                            &u.api,
                            match u.confidence {
                                Confidence::Id => col::green(),
                                Confidence::Name => col::faint(),
                            },
                        );
                    });
                });
            }
        });
    }
}

// ------------------------------------------------------------------ module

fn module(app: &SithApp, ui: &mut Ui) {
    let Some(doc) = app.doc() else { return };
    card(ui, "MODULE", |ui| {
        ui.horizontal(|ui| {
            icons::inline(
                ui,
                Icon::Module,
                if doc.ne.header.is_library() {
                    col::purple()
                } else {
                    col::green()
                },
            );
            ui.label(mono_c(doc.ne.module_name(), col::symbol()).strong());
        });
        if !doc.ne.description().is_empty() {
            ui.label(
                egui::RichText::new(doc.ne.description())
                    .size(11.0)
                    .color(col::dim()),
            );
        }
        ui.add_space(6.0);
        kv(ui, "segments", doc.ne.segments.len().to_string());
        kv(ui, "functions", doc.program.functions.len().to_string());
        kv(ui, "exports", doc.ne.exports().len().to_string());
        kv(ui, "imports", doc.ne.module_ref_names().len().to_string());
        kv(ui, "resources", doc.ne.resources.len().to_string());
    });

    if app.project.annotation_count() > 0 {
        card(ui, "PROJECT", |ui| {
            kv(
                ui,
                "name",
                if app.project.name.is_empty() {
                    "untitled".to_string()
                } else {
                    app.project.name.clone()
                },
            );
            kv(ui, "annotations", app.project.annotation_count().to_string());
            kv(ui, "binaries", app.project.binaries.len().to_string());
        });
    }

    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("select a line in a listing for its details")
                .size(11.0)
                .color(col::faint()),
        );
    });
}
