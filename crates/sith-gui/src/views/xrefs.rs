//! Cross-references: what calls what, across the whole module.

use crate::state::{Action, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Ui};

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, preset: &str) {
    let Some(doc) = app.doc() else { return };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Cross-references").size(15.0).strong());
        if !preset.is_empty() {
            widgets::chip(ui, preset, col::accent());
        }
    });
    let typed = crate::views::filter_box(app, ui, act, "filter targets…");
    crate::ui::sep(ui);

    // The typed filter narrows within the preset rather than replacing it, so
    // arriving from an import and then refining does what you would expect.
    let needle = if typed.is_empty() {
        preset.to_string()
    } else {
        typed
    };
    let hits = doc.program.find_xrefs(&needle);
    if hits.is_empty() {
        crate::ui::empty(ui, &format!("no call sites match {needle:?}"));
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (target, sites) in hits {
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("{target}   ({} sites)", sites.len()))
                        .monospace()
                        .color(col::comment()),
                )
                .id_salt(target)
                .default_open(sites.len() <= 40)
                .show(ui, |ui| {
                    for (i, a) in sites.iter().enumerate() {
                        let owner = doc.program.function_containing(*a);
                        // A color the user gave a function follows its name.
                        let tint =
                            owner.and_then(|f| app.user_color(f.addr.segment, f.addr.offset));
                        let label = owner.map(|f| app.label(f)).unwrap_or_default();
                        let (_, resp) = widgets::row(
                            ui,
                            ui.id().with(("xr", target, i)),
                            false,
                            i % 2 == 1,
                            |ui| {
                                ui.label(mono_c(a.to_string(), col::addr()));
                                ui.label(mono_c(label, tint.unwrap_or(col::symbol())));
                            },
                        );
                        if resp.clicked() {
                            act.push(Action::Goto(*a));
                        }
                    }
                });
            }
        });
}
