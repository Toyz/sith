//! Module summary.

use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::icons::{self, Icon};
use crate::widgets;
use eframe::egui::{self, Ui};

pub fn show(app: &SithApp, ui_: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let h = &ne.header;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(ne.module_name()).size(22.0).strong());
                widgets::chip(
                    ui,
                    if h.is_library() { "LIBRARY" } else { "APPLICATION" },
                    if h.is_library() { col::purple() } else { col::green() },
                );
                if h.is_self_loading() {
                    widgets::chip(ui, "SELF-LOADING", col::orange());
                }
                for f in h.flag_names() {
                    widgets::chip(ui, f, col::dim());
                }
            });
            if !ne.description().is_empty() {
                ui.label(egui::RichText::new(ne.description()).color(col::dim()));
            }

            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                stat(ui, "segments", &ne.segments.len().to_string());
                stat(ui, "functions", &doc.program.functions.len().to_string());
                stat(ui, "exports", &ne.exports().len().to_string());
                stat(ui, "imports", &ne.module_ref_names().len().to_string());
                stat(ui, "resources", &ne.resources.len().to_string());
                stat(ui, "size", &format!("{} B", ne.buf.len()));
            });

            widgets::section(ui, "HEADER");
            egui::Grid::new("hdr")
                .num_columns(4)
                .spacing([24.0, 3.0])
                .show(ui, |ui| {
                    kv(ui, "target", &h.target_os.name());
                    kv(
                        ui,
                        "expects",
                        &format!("Windows {}.{}", h.expected_version >> 8, h.expected_version & 0xFF),
                    );
                    ui.end_row();
                    kv(
                        ui,
                        "linker",
                        &format!("{}.{}", h.linker_version.0, h.linker_version.1),
                    );
                    kv(ui, "flags", &format!("{:04X}", h.flags));
                    ui.end_row();
                    kv(
                        ui,
                        "entry",
                        &format!("CS:IP {:04X}:{:04X}", h.cs_ip >> 16, h.cs_ip & 0xFFFF),
                    );
                    kv(
                        ui,
                        "stack",
                        &format!(
                            "SS:SP {:04X}:{:04X}  {} B",
                            h.ss_sp >> 16,
                            h.ss_sp & 0xFFFF,
                            h.stack_size
                        ),
                    );
                    ui.end_row();
                    kv(ui, "autodata", &format!("segment {}", h.auto_data_segment));
                    kv(ui, "heap", &format!("{} B", h.heap_size));
                    ui.end_row();
                    kv(ui, "alignment", &format!("1 << {}", h.align_shift_or_default()));
                    kv(ui, "file", &doc.path.display().to_string());
                    ui.end_row();
                });

            widgets::section(ui, "SEGMENTS");
            for s in &ne.segments {
                let (_, resp) = widgets::row(
                    ui,
                    ui.id().with(("ovseg", s.index)),
                    false,
                    s.index % 2 == 0,
                    |ui| {
                        ui.label(mono_c(format!("{:>3}", s.index), col::addr()));
                        ui.label(mono_c(
                            format!("{:<5}", s.kind().as_str()),
                            if s.is_code() { col::code_seg() } else { col::data_seg() },
                        ));
                        ui.label(mono_c(format!("{:08X}", s.file_offset), col::faint()));
                        ui.label(mono_c(format!("{:>7} B", s.length), col::text()));
                        ui.label(mono_c(format!("{:>4} fixups", s.relocs.len()), col::dim()));
                        ui.label(mono_c(s.flag_names()[1..].join(" "), col::faint()));
                    },
                );
                if resp.clicked() {
                    act.push(Action::Go(Nav::Segment(s.index)));
                }
            }

            widgets::section(ui, "MODULES REFERENCED");
            ui.horizontal_wrapped(|ui| {
                for m in ne.module_ref_names() {
                    let known = ne
                        .export_index
                        .as_ref()
                        .and_then(|ix| ix.path_of(&m))
                        .is_some();
                    let color = if known { col::accent() } else { col::comment() };
                    let r = widgets::link(ui, format!("[{m}]"), color);
                    let r = if known {
                        r.on_hover_text("open this module")
                    } else {
                        r.on_hover_text("system module: show its call sites")
                    };
                    if r.clicked() {
                        act.push(if known {
                            Action::OpenModule {
                                module: m.clone(),
                                ordinal: None,
                                name: None,
                            }
                        } else {
                            Action::Go(Nav::Xrefs(format!("{m}.")))
                        });
                    }
                }
            });

            if let Some(ix) = &ne.export_index {
                widgets::section(
                    ui,
                    &format!("WORKSPACE ({} modules found beside this file)", ix.module_count()),
                );
                let mut mods: Vec<_> = ix.modules().collect();
                mods.sort_by(|a, b| a.module.cmp(&b.module));
                for m in mods {
                    let (_, resp) = widgets::row(
                        ui,
                        ui.id().with(("wsmod", &m.module)),
                        m.module.eq_ignore_ascii_case(ne.module_name()),
                        false,
                        |ui| {
                            icons::inline(ui, Icon::Module, if m.is_library { col::purple() } else { col::green() });
                            ui.label(mono_c(format!("{:<12}", m.module), col::symbol()));
                            widgets::chip(
                                ui,
                                if m.is_library { "DLL" } else { "EXE" },
                                if m.is_library { col::purple() } else { col::green() },
                            );
                            ui.label(mono_c(format!("{:>4} exports", m.exports.len()), col::dim()));
                            ui.label(mono_c(&m.description, col::faint()));
                        },
                    );
                    if resp.clicked() {
                        act.push(Action::OpenModule {
                            module: m.module.clone(),
                            ordinal: None,
                            name: None,
                        });
                    }
                }
            }
            ui.add_space(20.0);
        });
}

fn stat(ui: &mut Ui, label: &str, value: &str) {
    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(value).size(16.0).strong().color(col::text()));
                ui.label(egui::RichText::new(label).size(10.0).color(col::dim()));
            });
        });
}

fn kv(ui: &mut Ui, key: &str, value: &str) {
    ui.label(mono_c(format!("{key:<10}"), col::faint()));
    ui.label(mono_c(value, col::text()));
}
