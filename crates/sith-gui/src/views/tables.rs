//! Import, export and entry-table listings.

use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_analysis::Addr;
use ne_core::RelKind;
use std::collections::BTreeMap;

pub fn imports(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let ne = &doc.ne;

    // One row per imported symbol, with the number of patch sites as a usage
    // count -- that is what says which parts of the API the module leans on.
    let mut by_module: BTreeMap<String, BTreeMap<(String, Option<u16>), usize>> = BTreeMap::new();
    for seg in &ne.segments {
        for r in &seg.relocs {
            let sites = r.sites(&seg.data).len().max(1);
            let (module, symbol, ordinal) = match r.kind {
                RelKind::ImportOrdinal => (
                    ne.module_ref_name(r.target1),
                    ne.import_ordinal_name(r.target1, r.target2)
                        .unwrap_or_else(|| format!("@{}", r.target2)),
                    Some(r.target2),
                ),
                RelKind::ImportName => (
                    ne.module_ref_name(r.target1),
                    ne.imported_name(r.target2),
                    None,
                ),
                _ => continue,
            };
            *by_module
                .entry(module)
                .or_default()
                .entry((symbol, ordinal))
                .or_insert(0) += sites;
        }
    }
    for m in ne.module_ref_names() {
        by_module.entry(m).or_default();
    }

    let filter = crate::views::filter_box(app, ui, act, "filter symbols…").to_ascii_lowercase();
    crate::ui::sep(ui);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (module, syms) in &by_module {
                let known = ne
                    .export_index
                    .as_ref()
                    .and_then(|ix| ix.path_of(module))
                    .is_some();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(module).strong().color(col::symbol()));
                    widgets::chip(ui, &format!("{} symbols", syms.len()), col::dim());
                    if known && ui.small_button("open module").clicked() {
                        act.push(Action::OpenModule {
                            module: module.clone(),
                            ordinal: None,
                            name: None,
                        });
                    }
                });
                let mut rows: Vec<(&(String, Option<u16>), &usize)> = syms
                    .iter()
                    .filter(|((s, _), _)| filter.is_empty() || s.to_ascii_lowercase().contains(&filter))
                    .collect();
                rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0 .0.cmp(&b.0 .0)));
                for (i, ((sym, ordinal), count)) in rows.into_iter().enumerate() {
                    let (_, resp) = widgets::row(
                        ui,
                        ui.id().with(("imp", module, sym)),
                        false,
                        i % 2 == 1,
                        |ui| {
                            ui.label(mono_c(format!("{count:>5}"), col::faint()));
                            ui.label(mono_c(format!("{sym:<32}"), col::comment()));
                            if let Some(o) = ordinal {
                                ui.label(mono_c(format!("@{o}"), col::faint()));
                            }
                        },
                    );
                    if resp.clicked() {
                        act.push(Action::Go(Nav::Xrefs(format!("{module}.{sym}"))));
                    }
                    if resp.double_clicked() && known {
                        act.push(Action::OpenModule {
                            module: module.clone(),
                            ordinal: *ordinal,
                            name: Some(sym.clone()),
                        });
                    }
                }
                ui.add_space(8.0);
            }
        });
}

pub fn exports(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let filter = crate::views::filter_box(app, ui, act, "filter exports…").to_ascii_lowercase();
    let rows: Vec<_> = doc
        .ne
        .exports()
        .into_iter()
        .filter(|e| filter.is_empty() || e.label().to_ascii_lowercase().contains(&filter))
        .collect();
    ui.label(dim(&format!("{} exports", rows.len())));
    crate::ui::sep(ui);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, e) in rows.iter().enumerate() {
                let (_, resp) = widgets::row(
                    ui,
                    ui.id().with(("exp", e.ordinal)),
                    false,
                    i % 2 == 1,
                    |ui| {
                        ui.label(mono_c(format!("@{:<5}", e.ordinal), col::faint()));
                        ui.label(mono_c(format!("{:<30}", e.label()), col::symbol()));
                        ui.label(mono_c(format!("seg{:02}:{:04X}", e.segment, e.offset), col::addr()));
                        if e.moveable {
                            widgets::chip(ui, "moveable", col::dim());
                        }
                        if e.resident {
                            widgets::chip(ui, "resident", col::dim());
                        }
                    },
                );
                if resp.clicked() {
                    act.push(Action::Goto(Addr {
                        segment: e.segment,
                        offset: e.offset as u32,
                    }));
                }
            }
        });
}

pub fn entries(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let filter = crate::views::filter_box(app, ui, act, "filter entries…").to_ascii_lowercase();
    let rows: Vec<_> = doc
        .ne
        .entries
        .values()
        .filter(|e| {
            filter.is_empty()
                || e.label().to_ascii_lowercase().contains(&filter)
                || format!("{}", e.ordinal).contains(&filter)
        })
        .collect();
    ui.label(dim(&format!(
        "{} of {} entry slots",
        rows.len(),
        doc.ne.entries.len()
    )));
    crate::ui::sep(ui);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, e) in rows.iter().enumerate() {
                let (_, resp) = widgets::row(
                    ui,
                    ui.id().with(("ent", e.ordinal)),
                    false,
                    i % 2 == 1,
                    |ui| {
                        ui.label(mono_c(format!("@{:<5}", e.ordinal), col::faint()));
                        ui.label(mono_c(format!("seg{:02}:{:04X}", e.segment, e.offset), col::addr()));
                        ui.label(mono_c(format!("{:02X}", e.flags), col::faint()));
                        ui.label(mono_c(
                            format!("{:<28}", e.name.clone().unwrap_or_default()),
                            col::symbol(),
                        ));
                        if e.is_exported() {
                            widgets::chip(ui, "export", col::green());
                        }
                        if e.moveable {
                            widgets::chip(ui, "moveable", col::dim());
                        }
                        if e.stack_words() > 0 {
                            widgets::chip(ui, &format!("{} words", e.stack_words()), col::dim());
                        }
                    },
                );
                if resp.clicked() {
                    act.push(Action::Goto(Addr {
                        segment: e.segment,
                        offset: e.offset as u32,
                    }));
                }
            }
        });
}
