//! What a module is, at a glance.
//!
//! The first thing you want from an unfamiliar binary is a sense of its shape:
//! how much of it is code, where that code lives, what it pulls in, and what
//! it carries with it. This view answers that and then gets out of the way --
//! everything on it is a way into somewhere else.

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::widgets::{self, human, tail};
use eframe::egui::{self, Color32, Ui};
use ne_analysis::FuncKind;
use std::collections::BTreeMap;

pub fn show(app: &SithApp, ui_: &mut Ui, act: &mut Vec<Action>) {
    if app.doc().is_none() {
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui| {
            let total = ui.available_width() - widgets::SCROLLBAR_GUTTER;
            banner(app, ui, act, total);
            ui.add_space(10.0);
            stats(app, ui, act, total);
            ui.add_space(12.0);

            // Two columns: what the module is made of on the left, what it
            // says about itself and who it talks to on the right.
            let left = (total * 0.56).clamp(340.0, total - 300.0);
            let right = total - left - 14.0;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(left);
                        segments_card(app, ui, act);
                        resources_card(app, ui, act);
                    },
                );
                ui.add_space(14.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(right, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(right);
                        header_card(app, ui, act);
                        code_card(app, ui);
                        modules_card(app, ui, act);
                        workspace_card(app, ui, act);
                    },
                );
            });
            ui.add_space(16.0);
        });
}

/// Module name, what kind of thing it is, and where it came from.
///
/// Takes its width rather than asking for it. Nested layouts inside a frame
/// do not reliably report what is left, and the failure is silent: the
/// columns collapse onto each other and the text wraps into a corner.
fn banner(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, width: f32) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let h = &ne.header;
    let library = h.is_library();
    let seg = (h.cs_ip >> 16) as u16;
    let off = (h.cs_ip & 0xFFFF) as u32;

    const PAD: f32 = 14.0;
    const ICON: f32 = 30.0;
    const ENTRY: f32 = 96.0;
    let inner = width - PAD * 2.0;
    let title_w = (inner - ICON - 10.0 - ENTRY - 14.0).max(120.0);

    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(6))
        .stroke(egui::Stroke::new(1.0, col::border()))
        .inner_margin(egui::Margin::symmetric(PAD as i8, 12))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (r, _) = ui.allocate_exact_size(egui::vec2(ICON, ICON), egui::Sense::hover());
                icons::draw(
                    ui.painter(),
                    r,
                    Icon::Module,
                    if library { col::purple() } else { col::green() },
                );
                ui.add_space(10.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(title_w, 46.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(title_w);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(ne.module_name())
                                    .size(21.0)
                                    .strong()
                                    .color(col::text()),
                            );
                            widgets::chip(
                                ui,
                                if library { "LIBRARY" } else { "APPLICATION" },
                                if library { col::purple() } else { col::green() },
                            );
                            if h.is_self_loading() {
                                widgets::chip(ui, "SELF-LOADING", col::orange());
                            }
                        });
                        let desc = ne.description();
                        if !desc.is_empty() {
                            ui.label(egui::RichText::new(desc).size(12.0).color(col::dim()));
                        }
                        let path = doc.path.display().to_string();
                        ui.label(
                            egui::RichText::new(tail(&path, (title_w / 6.2) as usize))
                                .size(11.0)
                                .monospace()
                                .color(col::faint()),
                        )
                        .on_hover_text(path);
                    },
                );

                if seg != 0 {
                    ui.add_space(14.0);
                    // The entry point is the one address everybody wants
                    // first, so it gets a corner of its own.
                    ui.allocate_ui_with_layout(
                        egui::vec2(ENTRY, 46.0),
                        egui::Layout::top_down(egui::Align::Max),
                        |ui| {
                            ui.set_width(ENTRY);
                            ui.label(
                                egui::RichText::new("ENTRY POINT")
                                    .size(9.5)
                                    .strong()
                                    .color(col::faint()),
                            );
                            if widgets::link(ui, format!("{seg:02}:{off:04X}"), col::accent())
                                .on_hover_text("go to the entry point")
                                .clicked()
                            {
                                act.push(Action::Goto(ne_analysis::Addr {
                                    segment: seg,
                                    offset: off,
                                }));
                            }
                        },
                    );
                }
            });

            let flags = h.flag_names();
            if !flags.is_empty() {
                ui.add_space(7.0);
                ui.horizontal_wrapped(|ui| {
                    for f in flags {
                        widgets::chip(ui, &f, col::dim());
                    }
                });
            }
        });
}

/// The counts, as one strip rather than six boxes.
///
/// They are a single fact about the module read six ways, so they are drawn
/// as one thing. The ones that lead somewhere say so by lighting up; the rest
/// are numbers, and a colored edge on a number is decoration doing no work.
fn stats(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, width: f32) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let code: u32 = ne
        .segments
        .iter()
        .filter(|s| s.is_code())
        .map(|s| s.length as u32)
        .sum();

    let items: [(&str, String, Option<Nav>); 6] = [
        ("segments", ne.segments.len().to_string(), None),
        ("functions", doc.program.functions.len().to_string(), None),
        ("exports", ne.exports().len().to_string(), Some(Nav::Exports)),
        ("imports", ne.module_ref_names().len().to_string(), Some(Nav::Imports)),
        ("resources", ne.resources.len().to_string(), None),
        ("code", human(code as u64), None),
    ];

    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(6))
        .stroke(egui::Stroke::new(1.0, col::border()))
        .inner_margin(egui::Margin::symmetric(2, 8))
        .show(ui, |ui| {
            ui.set_width(width - 4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, (label, value, nav)) in items.iter().enumerate() {
                    if i > 0 {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(1.0, 30.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, egui::CornerRadius::ZERO, col::border());
                    }
                    if stat_cell(ui, label, value, nav.is_some()) {
                        if let Some(n) = nav {
                            act.push(Action::Go(n.clone()));
                        }
                    }
                }
            });
        });
}

/// One cell of the strip. Returns whether it was clicked.
fn stat_cell(ui: &mut Ui, label: &str, value: &str, clickable: bool) -> bool {
    let sense = if clickable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let cap = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(10.5),
        col::faint(),
    );
    // Sized from the caption, which is always the wider of the two, so the
    // cells line up without a fixed width that would strand the short ones.
    let cap_w = cap.size().x;
    let w = (cap_w + 34.0).max(86.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 36.0), sense);
    let hot = clickable && resp.hovered();

    let value = ui.painter().layout_no_wrap(
        value.to_owned(),
        egui::FontId::proportional(17.0),
        if hot { col::accent() } else { col::text() },
    );
    let x = rect.min.x + 17.0;
    ui.painter()
        .galley(egui::pos2(x, rect.min.y), value, col::text());
    ui.painter()
        .galley(egui::pos2(x, rect.max.y - 14.0), cap, col::faint());
    if hot {
        // An underline under the caption, which is where a link would put it.
        let y = rect.max.y + 1.0;
        ui.painter().line_segment(
            [egui::pos2(x, y), egui::pos2(x + cap_w, y)],
            egui::Stroke::new(1.0, col::accent()),
        );
    }
    if clickable {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    } else {
        false
    }
}

/// Every segment with a bar for its size, which is the fastest way to see
/// where the module's weight actually sits.
fn segments_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let widest = ne.segments.iter().map(|s| s.length).max().unwrap_or(1).max(1) as f32;

    widgets::card(ui, "SEGMENTS", |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for s in &ne.segments {
            let color = if s.is_code() { col::code_seg() } else { col::data_seg() };
            let fns = doc.program.function_count_in(s.index);
            let (_, resp) = widgets::row_sized(
                ui,
                ui.id().with(("ovseg", s.index)),
                20.0,
                app.nav() == Nav::Segment(s.index),
                false,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(mono_c(format!("{:>2}", s.index), col::addr()));
                    icons::inline(ui, if s.is_code() { Icon::Code } else { Icon::Data }, color);
                    ui.label(mono_c(format!("{:<4}", s.kind().as_str()), color));

                    let (bar, _) =
                        ui.allocate_exact_size(egui::vec2(90.0, 7.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        bar,
                        egui::CornerRadius::same(2),
                        col::border(),
                    );
                    let mut fill = bar;
                    fill.max.x = bar.min.x + bar.width() * (s.length as f32 / widest).max(0.02);
                    ui.painter()
                        .rect_filled(fill, egui::CornerRadius::same(2), color);

                    ui.label(mono_c(format!("{:>6}", human(s.length as u64)), col::text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(mono_c(format!("{:>4} fixups", s.relocs.len()), col::faint()));
                        if fns > 0 {
                            ui.label(mono_c(format!("{fns:>4} fns"), col::dim()));
                        }
                    });
                },
            );
            let resp = widgets::hover_card(
                resp,
                Some((if s.is_code() { Icon::Code } else { Icon::Data }, color)),
                &format!("Segment {}", s.index),
                s.kind().as_str(),
                |ui| {
                    widgets::hover_row(ui, "file offset", format!("{:08X}", s.file_offset), col::text());
                    widgets::hover_row(
                        ui,
                        "length",
                        format!("{} ({} bytes)", human(s.length as u64), s.length),
                        col::text(),
                    );
                    widgets::hover_row(ui, "fixups", s.relocs.len().to_string(), col::text());
                    widgets::hover_row(ui, "functions", fns.to_string(), col::text());
                    let flags: Vec<String> =
                        s.flag_names().into_iter().skip(1).map(str::to_owned).collect();
                    widgets::hover_chips(ui, &flags, col::dim());
                },
            );
            if resp.clicked() {
                act.push(Action::Go(Nav::Segment(s.index)));
            }
            if resp.middle_clicked() {
                act.push(Action::GoNewTab(Nav::Segment(s.index)));
            }
        }
    });
}

/// What the module carries, grouped by type rather than listed one by one --
/// the tree on the left already lists them one by one.
fn resources_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    if doc.ne.resources.is_empty() {
        return;
    }
    let mut by_type: BTreeMap<String, (usize, u64, Option<u16>, usize)> = BTreeMap::new();
    for (i, r) in doc.ne.resources.iter().enumerate() {
        let e = by_type
            .entry(r.type_name())
            .or_insert((0, 0, r.type_id.as_id(), i));
        e.0 += 1;
        e.1 += r.length as u64;
    }
    let heaviest = by_type.values().map(|v| v.1).max().unwrap_or(1).max(1) as f32;

    widgets::card(ui, "RESOURCES", |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for (name, (count, bytes, type_id, first)) in by_type {
            let (_, resp) = widgets::row_sized(
                ui,
                ui.id().with(("ovres", &name)),
                20.0,
                false,
                false,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    icons::inline(ui, icons::for_resource(type_id), col::orange());
                    ui.label(mono_c(format!("{name:<13}"), col::text()));
                    ui.label(mono_c(format!("{count:>3}"), col::dim()));

                    let (bar, _) =
                        ui.allocate_exact_size(egui::vec2(90.0, 7.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(bar, egui::CornerRadius::same(2), col::border());
                    let mut fill = bar;
                    fill.max.x = bar.min.x + bar.width() * (bytes as f32 / heaviest).max(0.02);
                    ui.painter()
                        .rect_filled(fill, egui::CornerRadius::same(2), col::orange());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(mono_c(human(bytes as u64), col::faint()));
                    });
                },
            );
            if resp
                .on_hover_text("show the first of these")
                .clicked()
            {
                act.push(Action::Go(Nav::Resource(first)));
            }
        }
    });
}

/// The header fields, which are facts about how the loader will treat this
/// file rather than facts about the program.
fn header_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let h = &ne.header;

    widgets::card(ui, "HEADER", |ui| {
        widgets::kv(ui, "target", h.target_os.name());
        widgets::kv(
            ui,
            "expects",
            format!("Windows {}.{}", h.expected_version >> 8, h.expected_version & 0xFF),
        );
        widgets::kv(
            ui,
            "linker",
            format!("{}.{}", h.linker_version.0, h.linker_version.1),
        );
        widgets::kv(ui, "flags", format!("{:04X}", h.flags));
        crate::ui::sep(ui);
        widgets::kv_colored(
            ui,
            "entry",
            format!("{:02}:{:04X}", h.cs_ip >> 16, h.cs_ip & 0xFFFF),
            col::accent(),
        );
        widgets::kv(
            ui,
            "stack",
            format!("{:02}:{:04X}", h.ss_sp >> 16, h.ss_sp & 0xFFFF),
        );
        widgets::kv(ui, "stack size", human(h.stack_size as u64));
        widgets::kv(ui, "heap", human(h.heap_size as u64));
        crate::ui::sep(ui);
        let auto = h.auto_data_segment;
        widgets::kv_colored(
            ui,
            "autodata",
            if auto == 0 {
                "none".to_owned()
            } else {
                format!("segment {auto}")
            },
            if auto == 0 { col::faint() } else { col::text() },
        );
        widgets::kv(ui, "alignment", format!("1 << {}", h.align_shift_or_default()));
        widgets::kv(ui, "file size", human(ne.buf.len() as u64));
    });
    let _ = act;
}

/// How much of the code the analysis actually accounted for.
fn code_card(app: &SithApp, ui: &mut Ui) {
    let Some(doc) = app.doc() else { return };
    let fns = &doc.program.functions;
    if fns.is_empty() {
        return;
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in fns {
        *counts.entry(kind_label(f.kind)).or_default() += 1;
    }
    // Strongest evidence first: the entry point and the export table are
    // stated by the file, a prologue match is a guess.
    const ORDER: [&str; 6] = [
        "entry point",
        "exported",
        "entry table",
        "referenced",
        "called",
        "guessed",
    ];
    let named = fns.iter().filter(|f| f.name.is_some()).count();
    let with_args = fns.iter().filter(|f| f.frame.takes_arguments()).count();

    widgets::card(ui, "CODE", |ui| {
        for label in ORDER {
            if let Some(n) = counts.get(label) {
                widgets::kv_colored(ui, label, n.to_string(), kind_color(label));
            }
        }
        crate::ui::sep(ui);
        // Naming is the whole job, so the view should say how far along it is.
        widgets::kv_colored(
            ui,
            "named",
            format!("{named} of {}", fns.len()),
            if named == 0 { col::faint() } else { col::cyan() },
        );
        widgets::kv(ui, "take arguments", with_args.to_string());
    });
}

fn kind_label(k: FuncKind) -> &'static str {
    match k {
        FuncKind::Export => "exported",
        FuncKind::Entry => "entry table",
        FuncKind::EntryPoint => "entry point",
        FuncKind::Relocated => "referenced",
        FuncKind::Called => "called",
        FuncKind::Prologue => "guessed",
    }
}

fn kind_color(label: &str) -> Color32 {
    match label {
        "exported" => col::green(),
        "entry table" => col::cyan(),
        "entry point" => col::orange(),
        "guessed" => col::faint(),
        _ => col::text(),
    }
}

/// What this module needs from elsewhere.
fn modules_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;
    let names = ne.module_ref_names();
    if names.is_empty() {
        return;
    }

    widgets::card(ui, "IMPORTS FROM", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
            for m in names {
                // A module we have a file for opens; one we only have a
                // signature database for goes to its call sites instead.
                let known = ne
                    .export_index
                    .as_ref()
                    .and_then(|ix| ix.path_of(&m))
                    .is_some();
                let color = if known { col::accent() } else { col::comment() };
                let galley = ui.painter().layout_no_wrap(
                    m.clone(),
                    egui::FontId::monospace(11.0),
                    color,
                );
                let (rect, resp) = ui.allocate_exact_size(
                    galley.size() + egui::vec2(14.0, 7.0),
                    egui::Sense::click(),
                );
                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(4),
                    if resp.hovered() {
                        color.gamma_multiply(0.22)
                    } else {
                        color.gamma_multiply(0.12)
                    },
                    egui::Stroke::new(
                        1.0,
                        if known { color.gamma_multiply(0.5) } else { Color32::TRANSPARENT },
                    ),
                    egui::StrokeKind::Inside,
                );
                ui.painter()
                    .galley(rect.min + egui::vec2(7.0, 3.5), galley, color);
                let resp = resp
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(if known {
                        "open this module"
                    } else {
                        "system module: show its call sites"
                    });
                if resp.clicked() {
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
    });
}

/// The other binaries sitting beside this one, which is where a cross-module
/// jump will land.
fn workspace_card(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let Some(ix) = &doc.ne.export_index else { return };
    if ix.module_count() == 0 {
        return;
    }
    let mut mods: Vec<_> = ix.modules().collect();
    mods.sort_by(|a, b| a.module.cmp(&b.module));

    widgets::card(ui, "BESIDE THIS FILE", |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for m in mods {
            let here = m.module.eq_ignore_ascii_case(doc.ne.module_name());
            let color = if m.is_library { col::purple() } else { col::green() };
            let (_, resp) = widgets::row_sized(
                ui,
                ui.id().with(("ovws", &m.module)),
                20.0,
                here,
                false,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    icons::inline(ui, Icon::Module, color);
                    ui.label(mono_c(format!("{:<12}", m.module), col::symbol()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(2.0);
                        ui.label(mono_c(format!("{} exports", m.exports.len()), col::faint()));
                        if here {
                            widgets::chip(ui, "this file", col::dim());
                        }
                    });
                },
            );
            let resp = widgets::hover_card(
                resp,
                Some((Icon::Module, color)),
                &m.module,
                if m.is_library { "library" } else { "application" },
                |ui| {
                    widgets::hover_row(ui, "exports", m.exports.len().to_string(), col::text());
                    if !m.description.is_empty() {
                        widgets::hover_note(ui, &m.description);
                    }
                },
            );
            if resp.clicked() && !here {
                act.push(Action::OpenModule {
                    module: m.module.clone(),
                    ordinal: None,
                    name: None,
                });
            }
        }
    });
}
