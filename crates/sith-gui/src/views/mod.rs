//! Central-panel views.
//!
//! Every view renders from an immutable borrow and pushes [`Action`]s for
//! anything the user asked for.

pub mod disasm;
pub mod graph;
pub mod highlight;
pub mod hex;
pub mod overview;
pub mod resource;
pub mod segment;
pub mod strings;
pub mod tables;
pub mod xrefs;

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use eframe::egui::{self, Ui};

pub fn welcome(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(48.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("sith").size(44.0).strong().color(col::accent()));
                ui.label(
                    egui::RichText::new("browser for 16-bit Windows NE executables")
                        .size(14.0)
                        .color(col::dim()),
                );
            });
            ui.add_space(28.0);

            // The two starting moves, given equal weight: open a binary to
            // look at, or reopen the project where the work already lives.
            let width = 860.0_f32.min(ui.available_width() - 40.0);
            centered(ui, width, |ui| {
                ui.columns(3, |cols| {
                    start_card(
                        &mut cols[0],
                        Icon::Plus,
                        "New project",
                        "scan a folder, pick the modules",
                        col::green(),
                        act,
                        || Some(Action::ShowWizard),
                    );
                    start_card(
                        &mut cols[1],
                        Icon::Open,
                        "Open a binary",
                        ".EXE, .DLL or .DRV from Windows 3.x",
                        col::accent(),
                        act,
                        || {
                            rfd::FileDialog::new()
                                .add_filter("16-bit executables", &["exe", "dll", "drv", "EXE", "DLL", "DRV"])
                                .add_filter("All files", &["*"])
                                .pick_file()
                                .map(Action::Open)
                        },
                    );
                    start_card(
                        &mut cols[2],
                        Icon::Overview,
                        "Open a project",
                        "your names, notes and bookmarks",
                        col::purple(),
                        act,
                        || {
                            rfd::FileDialog::new()
                                .add_filter("sith project", &["sith", "json"])
                                .pick_file()
                                .map(Action::OpenProjectAt)
                        },
                    );
                });

                ui.add_space(18.0);
                // Missing entries are shown rather than hidden: a project that
                // has moved is worth knowing about, and there has to be a way
                // to stop it being listed.
                let existing: Vec<_> = app.recent.iter().collect();
                if existing.is_empty() {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.label(dim("drop a file onto this window to open it"));
                    });
                } else {
                    crate::widgets::section(ui, "RECENT");
                    for r in existing.iter().take(10) {
                        let gone = !r.exists();
                        let mut forget = false;
                        let (_, resp) = crate::widgets::row(
                            ui,
                            ui.id().with(("recent", &r.path)),
                            false,
                            false,
                            |ui| {
                                ui.add_space(4.0);
                                icons::inline(
                                    ui,
                                    if r.is_project { Icon::Overview } else { Icon::Module },
                                    if gone {
                                        col::faint()
                                    } else if r.is_project {
                                        col::purple()
                                    } else {
                                        col::accent()
                                    },
                                );
                                ui.label(mono_c(
                                    r.file_name(),
                                    if gone { col::faint() } else { col::text() },
                                ));
                                if gone {
                                    crate::widgets::chip(ui, "missing", col::orange());
                                } else if r.is_project {
                                    crate::widgets::chip(ui, "project", col::purple());
                                } else if !r.label.is_empty() {
                                    crate::widgets::chip(ui, &r.label, col::dim());
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if gone {
                                            let (rect, cross) = ui.allocate_exact_size(
                                                egui::vec2(14.0, 14.0),
                                                egui::Sense::click(),
                                            );
                                            icons::draw(
                                                ui.painter(),
                                                rect,
                                                Icon::Close,
                                                if cross.hovered() {
                                                    col::red()
                                                } else {
                                                    col::faint()
                                                },
                                            );
                                            if cross.on_hover_text("Remove from this list").clicked()
                                            {
                                                forget = true;
                                            }
                                        }
                                        ui.label(
                                            egui::RichText::new(
                                                r.path
                                                    .parent()
                                                    .map(|d| d.display().to_string())
                                                    .unwrap_or_default(),
                                            )
                                            .color(col::faint())
                                            .size(11.0),
                                        );
                                    },
                                );
                            },
                        );
                        if forget {
                            act.push(Action::ForgetRecent(r.path.clone()));
                        } else if resp.clicked() {
                            if gone {
                                act.push(Action::Status(format!(
                                    "{} is no longer there",
                                    r.path.display()
                                )));
                            } else {
                                act.push(if r.is_project {
                                    Action::OpenProjectAt(r.path.clone())
                                } else {
                                    Action::Open(r.path.clone())
                                });
                            }
                        }
                    }
                }

                ui.add_space(24.0);
                crate::widgets::section(ui, "KEYS");
                for (k, what) in [
                    ("Ctrl+O", "open a binary"),
                    ("Ctrl+S", "save the project"),
                    ("Ctrl+P", "find anything"),
                    ("Ctrl+G", "go to an address"),
                    ("N", "name the selected address"),
                    ("B", "bookmark the selected address"),
                    ("Alt+left / right", "back and forward"),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        ui.label(mono_c(format!("{k:<18}"), col::accent()));
                        ui.label(dim(what));
                    });
                }
                ui.add_space(32.0);
            });
        });
}

/// Lay `content` out in a fixed-width column centred in the available space.
///
/// The inner ui must be given an explicit top-down layout: allocating inside a
/// horizontal row otherwise inherits that row's direction and every label ends
/// up one character wide.
fn centered(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let pad = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.add_space(pad);
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                content(ui);
            },
        );
    });
}

fn start_card(
    ui: &mut Ui,
    icon: Icon,
    title: &str,
    subtitle: &str,
    color: egui::Color32,
    act: &mut Vec<Action>,
    pick: impl FnOnce() -> Option<Action>,
) {
    let r = egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, icon, color);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).size(14.0).strong().color(col::text()));
                    ui.label(egui::RichText::new(subtitle).size(11.0).color(col::dim()));
                });
            });
        });
    let resp = ui.interact(
        r.response.rect,
        ui.id().with(("startcard", title)),
        egui::Sense::click(),
    );
    if resp.hovered() {
        ui.painter().rect_stroke(
            r.response.rect,
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        if let Some(a) = pick() {
            act.push(a);
        }
    }
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
