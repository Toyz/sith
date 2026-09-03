//! Window chrome: menu bar, toolbar, tab strip, side panels, status bar.

pub mod dialogs;
pub mod inspector;
pub mod navigator;

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::*;
use crate::views;
use eframe::egui::{self, Color32, Ui};

pub fn frame(app: &mut SithApp, ui: &mut Ui) {
    let mut act: Vec<Action> = Vec::new();
    let ctx = ui.ctx().clone();

    dropped_files(&ctx, &mut act);
    shortcuts(app, &ctx, &mut act);

    egui::Panel::top("menubar")
        .frame(egui::Frame::new().fill(RAISED).inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| menu_bar(app, ui, &mut act));

    egui::Panel::top("tabstrip")
        .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin {
            left: 6,
            right: 6,
            top: 3,
            bottom: 0,
        }))
        .show(ui, |ui| tab_strip(app, ui, &mut act));

    egui::Panel::bottom("status")
        .frame(egui::Frame::new().fill(RAISED).inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 3,
            bottom: 3,
        }))
        .show(ui, |ui| status_bar(app, ui, &mut act));

    if app.show_navigator {
        egui::Panel::left("navigator")
            .resizable(true)
            .default_size(300.0)
            .min_size(180.0)
            .show(ui, |ui| navigator::show(app, ui, &mut act));
    }

    if app.show_inspector && app.doc().is_some() {
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(320.0)
            .min_size(200.0)
            .show(ui, |ui| inspector::show(app, ui, &mut act));
    }

    egui::CentralPanel::default_margins().show(ui, |ui| {
        if app.doc().is_none() {
            views::welcome(app, ui, &mut act);
            return;
        }
        views::central(app, ui, &mut act);
    });

    dialogs::show(app, &ctx, &mut act);
    app.apply(act);
}

fn dropped_files(ctx: &egui::Context, act: &mut Vec<Action>) {
    let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
    if let Some(p) = dropped {
        act.push(Action::Open(p));
    }
}

fn shortcuts(app: &SithApp, ctx: &egui::Context, act: &mut Vec<Action>) {
    use egui::{Key, Modifiers};
    let typing = ctx.egui_wants_keyboard_input() && (app.goto_open || app.palette_open);
    ctx.input_mut(|i| {
        if i.consume_key(Modifiers::COMMAND, Key::O) {
            if let Some(p) = rfd::FileDialog::new().pick_file() {
                act.push(Action::Open(p));
            }
        }
        if i.consume_key(Modifiers::COMMAND, Key::G) {
            act.push(Action::ShowGoto);
        }
        if i.consume_key(Modifiers::COMMAND, Key::P) {
            act.push(Action::ShowPalette);
        }
        if i.consume_key(Modifiers::COMMAND, Key::W) {
            act.push(Action::CloseTab(app.active));
        }
        if i.consume_key(Modifiers::COMMAND, Key::R) {
            act.push(Action::Reload);
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowLeft) {
            act.push(Action::Back);
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowRight) {
            act.push(Action::Forward);
        }
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            act.push(Action::Dismiss);
        }
        // Listing navigation, but only when no dialog owns the keyboard.
        if !typing {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                act.push(Action::MoveSelection(1));
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                act.push(Action::MoveSelection(-1));
            }
            if i.consume_key(Modifiers::NONE, Key::PageDown) {
                act.push(Action::MoveSelection(24));
            }
            if i.consume_key(Modifiers::NONE, Key::PageUp) {
                act.push(Action::MoveSelection(-24));
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                act.push(Action::FollowSelection);
            }
        }
    });
}

fn menu_bar(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open…                Ctrl+O").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_file() {
                    act.push(Action::Open(p));
                }
                ui.close();
            }
            if ui.button("Reload               Ctrl+R").clicked() {
                act.push(Action::Reload);
                ui.close();
            }
            ui.separator();
            ui.add_enabled_ui(!app.recent.is_empty(), |ui| {
                ui.menu_button("Recent", |ui| {
                    for p in &app.recent {
                        let name = p
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                            act.push(Action::Open(p.clone()));
                            ui.close();
                        }
                    }
                });
            });
            ui.separator();
            let can_save = matches!(app.nav(), Nav::Segment(_));
            ui.add_enabled_ui(can_save, |ui| {
                if ui.button("Save listing…").clicked() {
                    act.push(Action::SaveListing);
                    ui.close();
                }
            });
        });

        ui.menu_button("View", |ui| {
            let mut nav = app.show_navigator;
            if ui.checkbox(&mut nav, "Navigator").clicked() {
                act.push(Action::ToggleNavigator);
            }
            let mut insp = app.show_inspector;
            if ui.checkbox(&mut insp, "Inspector").clicked() {
                act.push(Action::ToggleInspector);
            }
            let mut bytes = app.show_bytes;
            if ui.checkbox(&mut bytes, "Instruction bytes").clicked() {
                act.push(Action::ToggleBytes);
            }
            ui.separator();
            for (nav, name) in [
                (Nav::Overview, "Overview"),
                (Nav::Imports, "Imports"),
                (Nav::Exports, "Exports"),
                (Nav::Entries, "Entry table"),
                (Nav::Strings, "Strings"),
                (Nav::Xrefs(String::new()), "Cross-references"),
            ] {
                if ui.button(name).clicked() {
                    act.push(Action::Go(nav));
                    ui.close();
                }
            }
        });

        ui.menu_button("Navigate", |ui| {
            if ui.button("Go to address…      Ctrl+G").clicked() {
                act.push(Action::ShowGoto);
                ui.close();
            }
            if ui.button("Find symbol…        Ctrl+P").clicked() {
                act.push(Action::ShowPalette);
                ui.close();
            }
            ui.separator();
            if ui.button("Back                 Alt+←").clicked() {
                act.push(Action::Back);
                ui.close();
            }
            if ui.button("Forward              Alt+→").clicked() {
                act.push(Action::Forward);
                ui.close();
            }
        });

        ui.separator();
        toolbar(app, ui, act);
    });
}

fn toolbar(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let has_back = app.tab().is_some_and(|t| !t.history.is_empty());
    let has_fwd = app.tab().is_some_and(|t| !t.forward.is_empty());
    ui.spacing_mut().item_spacing.x = 2.0;
    if icons::button(ui, Icon::Open, "Open (Ctrl+O)").clicked() {
        if let Some(p) = rfd::FileDialog::new().pick_file() {
            act.push(Action::Open(p));
        }
    }
    if icons::button(ui, Icon::Reload, "Reload (Ctrl+R)").clicked() {
        act.push(Action::Reload);
    }
    ui.add_space(6.0);
    ui.add_enabled_ui(has_back, |ui| {
        if icons::button(ui, Icon::Back, "Back (Alt+←)").clicked() {
            act.push(Action::Back);
        }
    });
    ui.add_enabled_ui(has_fwd, |ui| {
        if icons::button(ui, Icon::Forward, "Forward (Alt+→)").clicked() {
            act.push(Action::Forward);
        }
    });
    ui.add_space(6.0);
    if icons::button(ui, Icon::Target, "Go to address (Ctrl+G)").clicked() {
        act.push(Action::ShowGoto);
    }
    if icons::button(ui, Icon::Search, "Find symbol (Ctrl+P)").clicked() {
        act.push(Action::ShowPalette);
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if let Some(doc) = app.doc() {
            ui.label(mono_c(
                doc.path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                DIM,
            ));
            ui.separator();
            crate::widgets::chip(
                ui,
                if doc.ne.header.is_library() { "DLL" } else { "EXE" },
                if doc.ne.header.is_library() { PURPLE } else { GREEN },
            );
            ui.label(mono_c(doc.ne.module_name(), SYMBOL));
        }
    });
}

fn tab_strip(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, tab) in app.tabs.iter().enumerate() {
                    let active = i == app.active;
                    let title = tab.nav.title(app.docs.get(tab.doc));
                    let fill = if active { BG } else { RAISED };
                    let text_col = if active { TEXT } else { DIM };
                    // The close target is recorded rather than interacted
                    // with: the tab's own click area is created afterwards
                    // and covers it, so the two must be told apart by
                    // position rather than by widget order.
                    let mut close_rect = egui::Rect::NOTHING;
                    let r = egui::Frame::new()
                        .fill(fill)
                        .corner_radius(egui::CornerRadius {
                            nw: 5,
                            ne: 5,
                            sw: 0,
                            se: 0,
                        })
                        .inner_margin(egui::Margin {
                            left: 10,
                            right: 6,
                            top: 4,
                            bottom: 4,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                icons::inline(ui, tab.nav.icon(), if active { ACCENT } else { DIM });
                                ui.label(egui::RichText::new(&title).color(text_col).size(12.0));
                                // A tab from another file says so: with several
                                // modules open the title alone is ambiguous.
                                if app.docs.len() > 1 {
                                    if let Some(d) = app.docs.get(tab.doc) {
                                        ui.label(
                                            egui::RichText::new(d.ne.module_name())
                                                .color(FAINT)
                                                .size(10.0),
                                        );
                                    }
                                }
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(12.0, 12.0),
                                    egui::Sense::hover(),
                                );
                                close_rect = rect;
                            });
                        });

                    let resp = ui.interact(
                        r.response.rect,
                        ui.id().with(("tab", i)),
                        egui::Sense::click(),
                    );
                    let over_close = ui
                        .ctx()
                        .pointer_latest_pos()
                        .is_some_and(|p| close_rect.contains(p));
                    icons::draw(
                        ui.painter(),
                        close_rect,
                        Icon::Close,
                        if over_close { RED } else { FAINT },
                    );
                    if resp.clicked() {
                        act.push(if over_close {
                            Action::CloseTab(i)
                        } else {
                            Action::SelectTab(i)
                        });
                    }
                    // Middle-click closes, as it does in every editor.
                    if resp.middle_clicked() {
                        act.push(Action::CloseTab(i));
                    }
                    if active {
                        let mut bar = r.response.rect;
                        bar.min.y = bar.max.y - 2.0;
                        ui.painter()
                            .rect_filled(bar, egui::CornerRadius::ZERO, ACCENT);
                    }
                }
                if app.tab().is_some() && icons::button(ui, Icon::Plus, "New tab").clicked() {
                    act.push(Action::NewTab(app.nav()));
                }
            });
        });
}

fn status_bar(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if let Some(err) = &app.error {
            ui.label(egui::RichText::new("!").color(RED).monospace().strong());
            ui.label(egui::RichText::new(err).color(RED).size(12.0));
            if ui.small_button("dismiss").clicked() {
                act.push(Action::Dismiss);
            }
            return;
        }
        ui.label(egui::RichText::new(&app.status).color(DIM).size(12.0));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(doc) = app.doc() {
                let sel = app.tab().and_then(|t| t.sel);
                if let (Nav::Segment(segno), Some(sel)) = (app.nav(), sel) {
                    if let Some(seg) = doc.ne.segment(segno) {
                        ui.label(mono_c(
                            format!("file {:08X}", seg.file_offset + sel as u64),
                            FAINT,
                        ));
                        ui.separator();
                        ui.label(mono_c(format!("seg{segno:02}:{sel:04X}"), ACCENT));
                    }
                }
                ui.separator();
                ui.label(mono_c(
                    format!(
                        "{} fn  {} decoded  {} strings",
                        doc.program.functions.len(),
                        human(doc.decoded_bytes()),
                        doc.ne
                            .segments
                            .iter()
                            .map(|s| ne_core::strings::scan(&s.data, app.min_string_len).len())
                            .sum::<usize>()
                    ),
                    FAINT,
                ));
            }
        });
    });
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

/// Shared empty-state message.
pub fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(text).color(FAINT));
    });
}

pub fn sep(ui: &mut Ui) {
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter().hline(
        rect.min.x..=rect.max.x,
        y,
        egui::Stroke::new(1.0, Color32::from_rgb(0x24, 0x2C, 0x38)),
    );
    ui.add_space(5.0);
}
