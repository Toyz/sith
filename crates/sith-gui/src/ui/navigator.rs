//! The navigator: everything in the binary, one filter away.
//!
//! Sections collapse, count what they hold, and carry the same icons the tabs
//! use, so moving between the tree and an open view is not a context switch.
//! Image resources show a thumbnail, because "BITMAP #200" says far less than
//! the picture does.

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Color32, Ui};
use ne_analysis::FuncKind;
use std::collections::BTreeMap;

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.add_space(6.0);
    search_box(app, ui, act);
    ui.add_space(6.0);

    let f = app.nav_filter.to_ascii_lowercase();
    let keep = |s: &str| f.is_empty() || s.to_ascii_lowercase().contains(&f);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            // Sizes and chips are right-aligned; leave the scrollbar its strip.
            ui.set_width((ui.available_width() - widgets::SCROLLBAR_GUTTER).max(120.0));
            let Some(doc) = app.doc() else {
                crate::ui::empty(ui, "no file loaded");
                return;
            };

            views_section(app, ui, act, &keep);
            segments_section(app, ui, act, &keep);
            functions_section(app, ui, act, &keep);
            resources_section(app, ui, act, &keep);
            workspace_section(app, ui, act, &keep);
            let _ = doc;
            ui.add_space(16.0);
        });
}

fn search_box(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let mut filter = app.nav_filter.clone();
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        icons::inline(ui, Icon::Search, col::faint());
        ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("filter everything…")
                .desired_width(f32::INFINITY)
                .frame(egui::Frame::NONE),
        );
    });
    if filter != app.nav_filter {
        act.push(Action::SetNavFilter(filter));
    }
}

/// A section header: icon, name, count, and a collapse arrow.
fn header<R>(
    ui: &mut Ui,
    icon: Icon,
    title: &str,
    count: usize,
    open: bool,
    body: impl FnOnce(&mut Ui) -> R,
) {
    ui.add_space(8.0);
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("{title}   {count}"))
            .size(11.0)
            .strong()
            .color(col::dim()),
    )
    .id_salt(title)
    .icon(move |ui, openness, resp| {
        // The stock triangle is replaced by one that matches the icon set.
        let c = if resp.hovered() { col::text() } else { col::faint() };
        let r = resp.rect.shrink(3.0);
        let p = ui.painter();
        let a = r.center() + egui::vec2(-2.0, -4.0);
        let b = r.center() + egui::vec2(-2.0, 4.0);
        let tip = r.center() + egui::vec2(3.5, 0.0);
        let rot = openness * std::f32::consts::FRAC_PI_2;
        let piv = r.center();
        let rt = |q: egui::Pos2| {
            let v = q - piv;
            piv + egui::vec2(
                v.x * rot.cos() - v.y * rot.sin(),
                v.x * rot.sin() + v.y * rot.cos(),
            )
        };
        p.add(egui::Shape::convex_polygon(
            vec![rt(a), rt(b), rt(tip)],
            c,
            egui::Stroke::NONE,
        ));
    })
    .default_open(open)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        body(ui);
    });
    let _ = icon;
}

fn views_section(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, keep: &dyn Fn(&str) -> bool) {
    for (nav, label) in [
        (Nav::Overview, "Overview"),
        (Nav::Imports, "Imports"),
        (Nav::Exports, "Exports"),
        (Nav::Entries, "Entry table"),
        (Nav::Strings, "Strings"),
        (Nav::Graph, "Call graph"),
        (Nav::Xrefs(String::new()), "Cross-references"),
    ] {
        if !keep(label) {
            continue;
        }
        let selected = app.nav() == nav;
        let (_, r) = widgets::row(
            ui,
            ui.id().with(("navtop", label)),
            selected,
            false,
            |ui| {
                ui.add_space(2.0);
                icons::inline(ui, nav.icon(), if selected { col::accent() } else { col::dim() });
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(if selected { col::text() } else { Color32::from_gray(0xA8) }),
                );
            },
        );
        if r.clicked() {
            act.push(Action::Go(nav.clone()));
        }
        if r.middle_clicked() {
            act.push(Action::GoNewTab(nav));
        }
    }
}

fn segments_section(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    keep: &dyn Fn(&str) -> bool,
) {
    let Some(doc) = app.doc() else { return };
    let rows: Vec<_> = doc
        .ne
        .segments
        .iter()
        .filter(|s| {
            keep(&format!(
                "{} {} {}",
                s.index,
                s.kind().as_str(),
                s.flag_names().join(" ")
            ))
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let biggest = doc.ne.segments.iter().map(|s| s.length).max().unwrap_or(1).max(1);

    header(ui, Icon::Segment, "SEGMENTS", rows.len(), true, |ui| {
        for s in rows {
            let selected = app.nav() == Nav::Segment(s.index);
            let color = if s.is_code() { col::code_seg() } else { col::data_seg() };
            let (_, resp) = widgets::row(
                ui,
                ui.id().with(("navseg", s.index)),
                selected,
                false,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(2.0);
                    icons::inline(ui, if s.is_code() { Icon::Code } else { Icon::Data }, color);
                    ui.label(mono_c(format!("{:>2}", s.index), color));
                    ui.label(mono_c(format!("{:<4}", s.kind().as_str()), col::dim()));
                    // A size bar makes the shape of the binary obvious at a
                    // glance: which segment holds the bulk of the code.
                    bar(ui, s.length as f32 / biggest as f32, color);
                    ui.label(mono_c(format!("{:>6}", human(s.length as usize)), col::faint()));
                    if !s.relocs.is_empty() {
                        widgets::chip(ui, &format!("{}", s.relocs.len()), col::orange());
                    }
                },
            );
            let flags: Vec<&str> = s.flag_names().into_iter().skip(1).collect();
            let resp = widgets::hover_card(
                resp,
                Some((if s.is_code() { Icon::Code } else { Icon::Data }, color)),
                &format!("Segment {}", s.index),
                s.kind().as_str(),
                |ui| {
                    widgets::hover_row(ui, "file offset", format!("{:08X}", s.file_offset), col::text());
                    widgets::hover_row(ui, "size", format!("{} bytes", s.length), col::text());
                    widgets::hover_row(ui, "alloc", format!("{} bytes", s.min_alloc), col::text());
                    widgets::hover_row(ui, "fixups", s.relocs.len().to_string(), col::text());
                    if !flags.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for f in flags {
                                widgets::chip(ui, f, col::dim());
                            }
                        });
                    }
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

fn functions_section(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    keep: &dyn Fn(&str) -> bool,
) {
    let Some(doc) = app.doc() else { return };
    let matching: Vec<_> = doc
        .program
        .functions
        .iter()
        .filter(|f| keep(&f.label()) || keep(&f.addr.to_string()))
        .collect();
    if matching.is_empty() {
        return;
    }
    let named = matching.iter().filter(|f| f.name.is_some()).count();

    header(
        ui,
        Icon::Code,
        "FUNCTIONS",
        matching.len(),
        !app.nav_filter.is_empty(),
        |ui| {
            if named > 0 {
                ui.label(
                    egui::RichText::new(format!("  {named} named"))
                        .size(10.0)
                        .color(col::faint()),
                );
            }
            // Grouped by segment, mirroring how the listing is organised.
            let mut by_seg: BTreeMap<u16, Vec<_>> = BTreeMap::new();
            for f in &matching {
                by_seg.entry(f.addr.segment).or_default().push(*f);
            }
            for (segno, mut fns) in by_seg {
                fns.sort_by_key(|f| (f.name.is_none(), f.label()));
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("segment {segno}   {}", fns.len()))
                        .size(11.0)
                        .color(col::faint()),
                )
                .id_salt(("fnseg", segno))
                .default_open(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for f in fns {
                        function_row(app, ui, act, f);
                    }
                });
            }
        },
    );
}

fn function_row(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    f: &ne_analysis::Function,
) {
    let selected = app.tab().and_then(|t| t.sel) == Some(f.addr.offset)
        && app.nav() == Nav::Segment(f.addr.segment);
    let (kind_label, kind_color) = match f.kind {
        FuncKind::Export => ("exp", col::green()),
        FuncKind::Entry => ("ent", col::cyan()),
        FuncKind::EntryPoint => ("main", col::orange()),
        FuncKind::Relocated => ("ref", col::dim()),
        FuncKind::Called => ("sub", col::dim()),
        FuncKind::Prologue => ("?", col::faint()),
    };
    let (_, resp) = widgets::row(
        ui,
        ui.id().with(("navfn", f.addr.segment, f.addr.offset)),
        selected,
        false,
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(4.0);
            ui.label(mono_c(format!("{:04X}", f.addr.offset), col::addr()));
            widgets::chip(ui, kind_label, kind_color);
            let user_named = app.user_name(f.addr.segment, f.addr.offset).is_some();
            let tint = app.user_color(f.addr.segment, f.addr.offset);
            if let Some(c) = tint {
                // A color the user chose is the strongest signal on the row.
                let (dot, _) =
                    ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.5, c);
            }
            if app.is_bookmarked(f.addr.segment, f.addr.offset) {
                ui.label(mono_c("\u{25C6}", col::orange()));
            }
            ui.label(mono_c(
                app.label(f),
                if let Some(c) = tint {
                    c
                } else if user_named {
                    col::cyan()
                } else {
                    match f.kind {
                        FuncKind::Export | FuncKind::EntryPoint => col::symbol(),
                        FuncKind::Prologue => col::dim(),
                        _ => col::text(),
                    }
                },
            ));
        },
    );
    let external: Vec<String> = app
        .doc()
        .map(|d| d.program.external_calls_of(f))
        .unwrap_or_default();
    let resp = widgets::hover_card(
        resp,
        Some((Icon::Code, kind_color)),
        &app.label(f),
        &f.addr.to_string(),
        |ui| {
            widgets::hover_row(ui, "size", format!("{} bytes", f.size()), col::text());
            widgets::hover_row(ui, "instructions", f.insn_count.to_string(), col::text());
            widgets::hover_row(ui, "calls", f.calls.len().to_string(), col::text());
            widgets::hover_row(ui, "frame", f.frame.describe(), col::text());
            if app.user_name(f.addr.segment, f.addr.offset).is_some() {
                widgets::hover_row(ui, "generated", f.label(), col::faint());
            }
            if !external.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("calls out to")
                        .size(10.0)
                        .color(col::faint()),
                );
                ui.horizontal_wrapped(|ui| {
                    for name in external.iter().take(8) {
                        widgets::chip(ui, name, col::comment());
                    }
                    if external.len() > 8 {
                        ui.label(
                            egui::RichText::new(format!("+{}", external.len() - 8))
                                .size(10.0)
                                .color(col::faint()),
                        );
                    }
                });
            }
            widgets::hover_note(ui, f.kind.describe());
        },
    );
    if resp.clicked() {
        act.push(Action::Goto(f.addr));
    }
    if resp.middle_clicked() {
        act.push(Action::GotoNewTab(f.addr));
    }
    if resp.double_clicked() {
        act.push(Action::ShowRename {
            segment: f.addr.segment,
            offset: f.addr.offset,
        });
    }
    resp.context_menu(|ui| {
        if ui.button("Name this function…").clicked() {
            act.push(Action::ShowRename {
                segment: f.addr.segment,
                offset: f.addr.offset,
            });
            ui.close();
        }
        if ui.button("Show in call graph").clicked() {
            act.push(Action::SetGraphRoot(f.addr));
            ui.close();
        }
    });
}

fn resources_section(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    keep: &dyn Fn(&str) -> bool,
) {
    let Some(doc) = app.doc() else { return };
    if doc.ne.resources.is_empty() {
        return;
    }
    let mut by_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, r) in doc.ne.resources.iter().enumerate() {
        if keep(&r.type_name()) || keep(&r.res_id.to_string()) {
            by_type.entry(r.type_name()).or_default().push(i);
        }
    }
    let total: usize = by_type.values().map(Vec::len).sum();
    if total == 0 {
        return;
    }

    header(ui, Icon::Resource, "RESOURCES", total, true, |ui| {
        for (t, idxs) in by_type {
            let type_id = doc
                .ne
                .resources
                .get(idxs[0])
                .and_then(|r| r.type_id.as_id());
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{t}   {}", idxs.len()))
                    .size(11.0)
                    .color(col::faint()),
            )
            .id_salt(("restype", &t))
            .icon(move |ui, openness, resp| {
                let c = if resp.hovered() { col::text() } else { col::faint() };
                let r = resp.rect;
                // The group's own type icon, rotating out of the way as it
                // opens, rather than a nondescript triangle.
                if openness > 0.5 {
                    icons::draw(ui.painter(), r.shrink(2.0), icons::for_resource(type_id), c);
                } else {
                    icons::draw(ui.painter(), r.shrink(3.0), Icon::Forward, c);
                }
            })
            .default_open(idxs.len() <= 16)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for i in idxs {
                    resource_row(app, ui, act, i);
                }
            });
        }
    });
}

fn resource_row(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(r) = doc.ne.resources.get(index) else {
        return;
    };
    let selected = app.nav() == Nav::Resource(index);
    let (_, resp) = widgets::row(
        ui,
        ui.id().with(("navres", index)),
        selected,
        false,
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(4.0);
            thumbnail(app, ui, index);
            ui.label(mono_c(r.res_id.to_string(), col::text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono_c(human(r.length as usize), col::faint()));
            });
        },
    );
    if resp.clicked() {
        act.push(Action::Go(Nav::Resource(index)));
    }
    if resp.middle_clicked() {
        act.push(Action::GoNewTab(Nav::Resource(index)));
    }
}

/// A 16px preview for image resources, falling back to a type icon.
fn thumbnail(app: &SithApp, ui: &mut Ui, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(r) = doc.ne.resources.get(index) else {
        return;
    };
    let size = egui::vec2(16.0, 16.0);

    let mut cache = app.textures.borrow_mut();
    if !cache.contains_key(&index) {
        // Decoding is only done once per resource and the texture is shared
        // with the preview pane, so the tree costs nothing extra to draw.
        if let Some(img) = doc.ne.resource_image(r) {
            let color = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
            let tex = ui.ctx().load_texture(
                format!("res{index}"),
                color,
                egui::TextureOptions::NEAREST,
            );
            cache.insert(index, tex);
        }
    }
    match cache.get(&index) {
        Some(tex) => {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            let icon = match r.type_id.as_id() {
                Some(4) => Icon::Entries,
                Some(5) => Icon::Overview,
                Some(6) => Icon::Strings,
                _ => Icon::Resource,
            };
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            icons::draw(ui.painter(), rect.shrink(2.0), icon, col::dim());
        }
    }
}

fn workspace_section(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    keep: &dyn Fn(&str) -> bool,
) {
    if app.index.module_count() == 0 {
        return;
    }
    let mut mods: Vec<_> = app.index.modules().filter(|m| keep(&m.module)).collect();
    if mods.is_empty() {
        return;
    }
    mods.sort_by(|a, b| a.module.cmp(&b.module));
    let current = app.doc().map(|d| d.ne.module_name().to_ascii_uppercase());

    header(ui, Icon::Module, "WORKSPACE", mods.len(), true, |ui| {
        for m in mods {
            let is_current = current.as_deref() == Some(m.module.as_str());
            let open_here = app.docs.iter().any(|d| d.path == m.path);
            let (_, resp) = widgets::row(
                ui,
                ui.id().with(("ws", &m.module)),
                is_current,
                false,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(2.0);
                    icons::inline(
                        ui,
                        Icon::Module,
                        if m.is_library { col::purple() } else { col::green() },
                    );
                    ui.label(mono_c(&m.module, if is_current { col::symbol() } else { col::text() }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if open_here {
                            widgets::chip(ui, "open", col::accent());
                        }
                        ui.label(mono_c(format!("{}", m.exports.len()), col::faint()));
                    });
                },
            );
            let resp = widgets::hover_card(
                resp,
                Some((
                    Icon::Module,
                    if m.is_library { col::purple() } else { col::green() },
                )),
                &m.module,
                if m.is_library { "library" } else { "application" },
                |ui| {
                    widgets::hover_row(ui, "exports", m.exports.len().to_string(), col::text());
                    widgets::hover_row(
                        ui,
                        "file",
                        m.path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        col::text(),
                    );
                    if !m.description.is_empty() {
                        widgets::hover_note(ui, &m.description);
                    }
                    if is_current {
                        widgets::hover_note(ui, "currently open");
                    } else if open_here {
                        widgets::hover_note(ui, "open in another tab");
                    } else {
                        widgets::hover_note(ui, "click to open this module");
                    }
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
    });
}

/// A small proportional bar, used for relative sizes.
fn bar(ui: &mut Ui, frac: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 5.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(2), Color32::from_gray(0x22));
    let mut fill = rect;
    fill.max.x = rect.min.x + rect.width() * frac.clamp(0.02, 1.0);
    p.rect_filled(fill, egui::CornerRadius::same(2), color.gamma_multiply(0.6));
}

fn human(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1}M", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1}K", n as f64 / 1024.0)
    } else {
        format!("{n}")
    }
}
