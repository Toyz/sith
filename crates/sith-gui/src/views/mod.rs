//! Central-panel views.
//!
//! Every view renders from an immutable borrow and pushes [`Action`]s for
//! anything the user asked for.

pub mod disasm;
pub mod graph;
pub mod hex;
pub mod overview;
pub mod resource;
pub mod segment;
pub mod strings;
pub mod tables;
pub mod xrefs;

use crate::state::{Action, Nav, SithApp};
use crate::theme::*;
use eframe::egui::{self, Ui};

pub fn welcome(_app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.label(egui::RichText::new("sith").size(40.0).strong().color(ACCENT));
        ui.label(
            egui::RichText::new("browser for 16-bit Windows NE executables")
                .size(14.0)
                .color(DIM),
        );
        ui.add_space(24.0);
        if ui.button("Open a binary…").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Executables", &["exe", "dll", "drv", "EXE", "DLL", "DRV"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                act.push(Action::Open(p));
            }
        }
        ui.add_space(8.0);
        ui.label(dim("…or drop a .EXE, .DLL or .DRV onto this window"));
        ui.add_space(28.0);
        for (k, what) in [
            ("Ctrl+O", "open"),
            ("Ctrl+G", "go to address"),
            ("Ctrl+P", "find symbol"),
            ("Alt+←/→", "back and forward"),
            ("↑ ↓ Enter", "move and follow"),
        ] {
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 120.0);
                ui.label(mono_c(format!("{k:<12}"), ACCENT));
                ui.label(dim(what));
            });
        }
    });
}

pub fn central(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    // A pending scroll target applies to one frame only; holding it would pin
    // the listing and stop the user scrolling by hand.
    match app.nav() {
        Nav::Overview => overview::show(app, ui, act),
        Nav::Segment(n) => segment::show(app, ui, act, n),
        Nav::Resource(i) => resource::show(app, ui, act, i),
        Nav::Imports => tables::imports(app, ui, act),
        Nav::Exports => tables::exports(app, ui, act),
        Nav::Entries => tables::entries(app, ui, act),
        Nav::Strings => strings::show(app, ui, act),
        Nav::Graph => graph::show(app, ui, act),
        Nav::Xrefs(filter) => xrefs::show(app, ui, act, &filter),
    }
    // Applied before anything the frame queued, so a fresh jump survives.
    act.insert(0, Action::ConsumeScroll);
}

/// A search box that writes back through an action.
pub fn filter_box(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, hint: &str) -> String {
    let current = app.tab().map(|t| t.filter.clone()).unwrap_or_default();
    let mut text = current.clone();
    ui.add(
        egui::TextEdit::singleline(&mut text)
            .hint_text(hint)
            .desired_width(240.0),
    );
    if text != current {
        act.push(Action::SetViewFilter(text.clone()));
    }
    text
}
