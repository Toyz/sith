//! The theme editor.
//!
//! Editing applies to the running window immediately. A palette is judged by
//! how a listing reads under it, not by how the swatches look beside each
//! other, so the only honest preview is the application itself; the sample
//! block here is a convenience for the roles you cannot see from wherever you
//! happened to open the editor.

use crate::icons::{self, Icon};
use crate::state::{Action, SithApp};
use crate::theme::{self, col, Theme, ROLES};
use crate::widgets;
use eframe::egui::{self, Context, Ui};

/// What the editor is working on.
#[derive(Clone)]
pub struct ThemeEditor {
    pub working: Theme,
    /// The theme that was active when the editor opened, to put back on cancel.
    pub original_name: String,
}

impl ThemeEditor {
    pub fn open(from: Theme, original_name: String) -> ThemeEditor {
        let mut working = from;
        // A built-in cannot be written over, so editing one starts a copy and
        // says so in the name rather than failing at save time.
        if working.builtin {
            working.name = format!("{} (edited)", working.name);
            working.builtin = false;
        }
        ThemeEditor {
            working,
            original_name,
        }
    }
}

pub fn show(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    let Some(editor) = &app.theme_editor else {
        return;
    };
    let mut working = editor.working.clone();
    let mut changed = false;

    egui::Modal::new(egui::Id::new("theme-editor")).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Resource, col::accent());
            ui.label(egui::RichText::new("Theme").size(16.0).strong());
            ui.add_space(6.0);
            let mut name = working.name.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(240.0)
                        .font(egui::TextStyle::Monospace),
                )
                .changed()
            {
                working.name = name;
                changed = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let known = theme::theme(&working.name).is_some_and(|t| !t.builtin);
                if ui
                    .button(if known { "Save" } else { "Save as new" })
                    .clicked()
                {
                    act.push(Action::SaveTheme);
                }
                if ui.button("Cancel").clicked() {
                    act.push(Action::CancelThemeEdit);
                }
            });
        });
        if let Some(dir) = theme::themes_dir() {
            ui.label(
                egui::RichText::new(format!("saved as JSON in {}", dir.display()))
                    .size(10.5)
                    .color(col::faint()),
            );
        }
        crate::ui::sep(ui);

        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            let mut dark = working.colors.dark;
            if ui
                .checkbox(&mut dark, "dark theme")
                .on_hover_text("Sets how egui picks its own shades for widgets")
                .changed()
            {
                working.colors.dark = dark;
                changed = true;
            }
            ui.add_space(6.0);

            for (role, what) in ROLES {
                let mut c = working.colors.role(role);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        working.colors.set_role(role, c);
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(format!("{role:<8}"))
                            .monospace()
                            .size(11.5)
                            .color(col::text()),
                    );
                    // The hex is editable, so a palette can be pasted in from
                    // wherever it was designed.
                    let mut hex = theme::hex_of(c);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut hex)
                            .desired_width(74.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if r.changed() {
                        if let Some(parsed) = theme::color_of(&hex) {
                            working.colors.set_role(role, parsed);
                            changed = true;
                        }
                    }
                    ui.label(
                        egui::RichText::new(*what)
                            .size(10.5)
                            .color(col::faint()),
                    );
                });
            }

            // The right column repeats the shapes the tool actually draws,
            // because that is what the colors have to work in.
            sample(&mut cols[1]);
        });
    });

    if changed {
        act.push(Action::EditTheme(Box::new(working)));
    }
}

/// A miniature of the listing, drawn in the theme being edited.
pub fn sample(ui: &mut Ui) {
    widgets::card(ui, "SAMPLE", |ui| {
        ui.label(
            egui::RichText::new("MAINWNDPROC:")
                .monospace()
                .color(col::symbol()),
        );
        for (addr, bytes, mnem, ops, note) in [
            ("225C", "8CD8", "mov", "ax,ds", ""),
            ("2260", "55", "push", "bp", ""),
            ("2263", "9AFFFF0000", "call", "0:0FFFFh", "; USER.BeginPaint"),
            ("2268", "7439", "je", "short 2276h", ""),
            ("226A", "CA0A00", "retf", "0Ah", ""),
        ] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(egui::RichText::new(addr).monospace().size(11.5).color(col::addr()));
                ui.label(
                    egui::RichText::new(format!("{bytes:<12}"))
                        .monospace()
                        .size(11.5)
                        .color(col::bytes()),
                );
                let flow = match mnem {
                    "call" => col::green(),
                    "je" => col::cyan(),
                    "retf" => col::purple(),
                    _ => col::text(),
                };
                ui.label(
                    egui::RichText::new(format!("{mnem:<6}"))
                        .monospace()
                        .size(11.5)
                        .color(flow),
                );
                ui.label(egui::RichText::new(ops).monospace().size(11.5).color(col::mnemonic()));
                if !note.is_empty() {
                    ui.label(
                        egui::RichText::new(note)
                            .monospace()
                            .size(11.5)
                            .color(col::comment()),
                    );
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            widgets::chip(ui, "CODE", col::code_seg());
            widgets::chip(ui, "DATA", col::data_seg());
            widgets::chip(ui, "export", col::green());
            widgets::chip(ui, "partial", col::orange());
            widgets::chip(ui, "error", col::red());
            widgets::chip(ui, "named", col::cyan());
        });
    });
}
