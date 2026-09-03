//! The theme editor.
//!
//! Editing applies to the running window immediately. A palette is judged by
//! how a listing reads under it, not by how the swatches look beside each
//! other, so the only honest preview is the application itself; the sample
//! here covers the roles you cannot see from wherever the editor was opened.

use crate::icons::{self, Icon};
use crate::state::{Action, SithApp};
use crate::theme::{self, col, Theme, ROLE_GROUPS};
use crate::widgets;
use eframe::egui::{self, Color32, Context, Ui};

/// What the editor is working on.
#[derive(Clone)]
pub struct ThemeEditor {
    pub working: Theme,
    /// The theme that was active when the editor opened, to put back on cancel.
    pub original_name: String,
    /// What the working theme started from, for per-role reverts.
    pub base: Theme,
}

impl ThemeEditor {
    pub fn open(from: Theme, original_name: String) -> ThemeEditor {
        let base = from.clone();
        let mut working = from;
        // A built-in cannot be written over, so editing one starts a copy and
        // says so in the name rather than failing at save time.
        if working.builtin {
            working.name = format!("{} copy", working.name);
            working.builtin = false;
        }
        ThemeEditor {
            working,
            original_name,
            base,
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
        ui.set_width(900.0);
        header(ui, act, &mut working, &mut changed);
        crate::ui::sep(ui);

        ui.horizontal_top(|ui| {
            // An explicit top-down layout: allocating inside a horizontal row
            // otherwise inherits that row's direction and the cards lay
            // themselves out sideways, on top of each other.
            ui.allocate_ui_with_layout(
                egui::vec2(430.0, 470.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                ui.set_width(430.0);
                egui::ScrollArea::vertical()
                    .id_salt("theme-roles")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(430.0 - widgets::SCROLLBAR_GUTTER);
                        for (group, roles) in ROLE_GROUPS {
                            widgets::card(ui, group, |ui| {
                                for (role, what) in *roles {
                                    role_row(
                                        ui,
                                        role,
                                        what,
                                        &editor.base,
                                        &mut working,
                                        &mut changed,
                                    );
                                }
                            });
                        }
                    });
                },
            );

            ui.add_space(10.0);
            ui.allocate_ui_with_layout(
                egui::vec2(430.0, 470.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                ui.set_width(430.0);
                    sample(ui, &working);
                    readability(ui, &working);
                },
            );
        });

        crate::ui::sep(ui);
        footer(app, ui, act, &working);
    });

    if changed {
        act.push(Action::EditTheme(Box::new(working)));
    }
}

fn header(ui: &mut Ui, act: &mut Vec<Action>, working: &mut Theme, changed: &mut bool) {
    ui.horizontal(|ui| {
        icons::inline_sized(ui, Icon::Resource, col::accent(), 18.0);
        ui.label(egui::RichText::new("Theme editor").size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("  Save  ").clicked() {
                act.push(Action::SaveTheme);
            }
            if ui.button("Cancel").clicked() {
                act.push(Action::CancelThemeEdit);
            }
        });
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            egui::RichText::new("name")
                .monospace()
                .size(11.0)
                .color(col::faint()),
        );
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
            *changed = true;
        }

        // Starting from another palette is the usual way to make one, so it is
        // a control rather than something to do before opening the editor.
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("start from")
                .monospace()
                .size(11.0)
                .color(col::faint()),
        );
        egui::ComboBox::from_id_salt("theme-base")
            .selected_text("choose…")
            .width(190.0)
            .show_ui(ui, |ui| {
                for t in theme::themes() {
                    if ui.selectable_label(false, &t.name).clicked() {
                        working.colors = t.colors;
                        *changed = true;
                    }
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut dark = working.colors.dark;
            if ui
                .checkbox(&mut dark, "dark")
                .on_hover_text(
                    "Tells egui which way to shade the widgets it draws for itself",
                )
                .changed()
            {
                working.colors.dark = dark;
                *changed = true;
            }
        });
    });
}

/// One role: swatch, name, hex, what it is for, and a way back.
fn role_row(
    ui: &mut Ui,
    role: &str,
    what: &str,
    base: &Theme,
    working: &mut Theme,
    changed: &mut bool,
) {
    let mut c = working.colors.role(role);
    let original = base.colors.role(role);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        // A large swatch with a border: a small one on a dark ground cannot be
        // judged, and judging is the whole task here.
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 20.0), egui::Sense::click());
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), c);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, col::border()),
            egui::StrokeKind::Inside,
        );
        let mut picker = c;
        let popup = egui::Popup::menu(&resp).show(|ui| {
            ui.spacing_mut().slider_width = 200.0;
            egui::color_picker::color_picker_color32(
                ui,
                &mut picker,
                egui::color_picker::Alpha::Opaque,
            );
        });
        if popup.is_some() && picker != c {
            c = picker;
            working.colors.set_role(role, c);
            *changed = true;
        }

        ui.label(
            egui::RichText::new(format!("{role:<7}"))
                .monospace()
                .size(12.0)
                .color(col::text()),
        );

        // The hex is editable so a palette can be pasted in from wherever it
        // was designed.
        let mut hex = theme::hex_of(c);
        if ui
            .add(
                egui::TextEdit::singleline(&mut hex)
                    .desired_width(78.0)
                    .font(egui::TextStyle::Monospace),
            )
            .changed()
        {
            if let Some(parsed) = theme::color_of(&hex) {
                working.colors.set_role(role, parsed);
                *changed = true;
            }
        }

        ui.label(egui::RichText::new(what).size(10.5).color(col::faint()));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if c != original {
                let (r, resp) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                icons::draw(
                    ui.painter(),
                    r,
                    Icon::Reload,
                    if resp.hovered() { col::accent() } else { col::faint() },
                );
                if resp
                    .on_hover_text(format!("back to {}", theme::hex_of(original)))
                    .clicked()
                {
                    working.colors.set_role(role, original);
                    *changed = true;
                }
            }
        });
    });
}

/// The shapes the tool actually draws, in the theme being edited.
fn sample(ui: &mut Ui, t: &Theme) {
    let c = t.colors;
    widgets::card(ui, "PREVIEW", |ui| {
        // A toolbar strip, so `raised` and `border` can be judged.
        egui::Frame::new()
            .fill(c.raised)
            .corner_radius(egui::CornerRadius::same(5))
            .stroke(egui::Stroke::new(1.0, c.border))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(mono(11.0, "Segment 2", c.text));
                    pill(ui, "CODE", c.green);
                    pill(ui, "87 fixups", c.orange);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(mono(10.5, "MAINWNDPROC", c.yellow));
                    });
                });
            });
        ui.add_space(6.0);

        // The listing, which is what the palette is really for.
        egui::Frame::new()
            .fill(c.bg)
            .corner_radius(egui::CornerRadius::same(5))
            .stroke(egui::Stroke::new(1.0, c.border))
            .inner_margin(egui::Margin::symmetric(8, 7))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(mono(11.5, "MAINWNDPROC:", c.yellow));
                for (addr, bytes, mnem, ops, note, flow) in [
                    ("225C", "8CD8", "mov", "ax,ds", "", c.text),
                    ("225E", "55", "push", "bp", "", c.text),
                    ("2263", "9AFFFF0000", "call", "0:0FFFFh", "; USER.BeginPaint", c.green),
                    ("2268", "7439", "je", "short 2276h", "", c.cyan),
                    ("226A", "CD3F", "int", "3Fh", "", c.orange),
                    ("226C", "CA0A00", "retf", "0Ah", "; 10 bytes of arguments", c.purple),
                    ("226F", "FF", "(bad)", "", "", c.red),
                ] {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(mono(11.0, addr, c.faint));
                        ui.label(mono(11.0, &format!("{bytes:<11}"), c.faint));
                        ui.label(mono(11.0, &format!("{mnem:<6}"), flow));
                        ui.label(mono(11.0, ops, c.text));
                        if !note.is_empty() {
                            ui.label(mono(11.0, note, c.cyan));
                        }
                    });
                }
            });
        ui.add_space(6.0);

        // A panel card, so `panel` and `dim` can be judged against the rest.
        egui::Frame::new()
            .fill(c.panel)
            .corner_radius(egui::CornerRadius::same(5))
            .stroke(egui::Stroke::new(1.0, c.border))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(mono(10.0, "SELECTION", c.faint));
                ui.horizontal(|ui| {
                    ui.label(mono(11.0, "address", c.faint));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(mono(11.0, "seg02:225C", c.accent));
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(mono(11.0, "note", c.faint));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(mono(11.0, "the window procedure", c.dim));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, color) in [
                        ("export", c.green),
                        ("named", c.cyan),
                        ("partial", c.orange),
                        ("error", c.red),
                        ("library", c.purple),
                    ] {
                        pill(ui, label, color);
                    }
                });
            });
    });
}

/// Whether the palette can actually be read.
///
/// A tool whose job is dense text has to say when a palette fails at it; the
/// swatches can look pleasant and the listing still be unreadable.
fn readability(ui: &mut Ui, t: &Theme) {
    let c = t.colors;
    widgets::card(ui, "READABILITY", |ui| {
        let checks = [
            ("text on the listing", c.text, c.bg),
            ("faint on the listing", c.faint, c.bg),
            ("text on panels", c.text, c.panel),
            ("accent on panels", c.accent, c.panel),
        ];
        for (what, fg, bg) in checks {
            let ratio = theme::contrast(fg, bg);
            // 4.5:1 is the usual floor for body text; 3:1 is the floor for
            // anything larger or purely decorative.
            let (mark, color) = if ratio >= 4.5 {
                ("good", col::green())
            } else if ratio >= 3.0 {
                ("thin", col::orange())
            } else {
                ("unreadable", col::red())
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{what:<22}"))
                        .monospace()
                        .size(11.0)
                        .color(col::faint()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::chip(ui, mark, color);
                    ui.label(
                        egui::RichText::new(format!("{ratio:.1}:1"))
                            .monospace()
                            .size(11.0)
                            .color(col::dim()),
                    );
                });
            });
        }
    });
}

fn footer(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, working: &Theme) {
    ui.horizontal(|ui| {
        if let Some(dir) = theme::themes_dir() {
            ui.label(
                egui::RichText::new(format!("saves to {}", dir.display()))
                    .size(10.5)
                    .color(col::faint()),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Deleting is only offered for a theme that exists on disk.
            let saved = theme::theme(&working.name).is_some_and(|t| !t.builtin);
            if saved && ui.small_button("Delete this theme").clicked() {
                act.push(Action::DeleteTheme(working.name.clone()));
                act.push(Action::CancelThemeEdit);
            }
            let _ = app;
        });
    });
}

fn mono(size: f32, text: &str, color: Color32) -> egui::RichText {
    egui::RichText::new(text).monospace().size(size).color(color)
}

/// A chip drawn in the theme being edited, rather than the active one.
fn pill(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(10.0),
        color,
    );
    let pad = egui::vec2(5.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(3),
        color.gamma_multiply(0.18),
    );
    ui.painter().galley(rect.min + pad, galley, color);
}
