//! Window chrome: menu bar, toolbar, tab strip, side panels, status bar.

pub mod dialogs;
pub mod theme_editor;
pub mod inspector;
pub mod navigator;

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::views;
use eframe::egui::{self, Ui};
use crate::widgets::human;

pub fn frame(app: &mut SithApp, ui: &mut Ui) {
    let mut act: Vec<Action> = Vec::new();
    let ctx = ui.ctx().clone();

    // A theme change has to rebuild egui's style, which cannot happen while
    // the frame that requested it is still being laid out.
    if app.restyle {
        app.restyle = false;
        crate::theme::install(&ctx);
    }

    dropped_files(&ctx, &mut act);
    shortcuts(app, &ctx, &mut act);

    egui::Panel::top("menubar")
        .frame(egui::Frame::new().fill(col::raised()).inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| menu_bar(app, ui, &mut act));

    egui::Panel::top("tabstrip")
        .frame(egui::Frame::new().fill(col::panel()).inner_margin(egui::Margin {
            left: 6,
            right: 6,
            top: 3,
            bottom: 0,
        }))
        .show(ui, |ui| tab_strip(app, ui, &mut act));

    egui::Panel::bottom("status")
        .frame(egui::Frame::new().fill(col::raised()).inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 3,
            bottom: 3,
        }))
        .show(ui, |ui| status_bar(app, ui, &mut act));

    if app.show_navigator {
        // Both bounds matter. Without a maximum a panel can be dragged over
        // the listing until there is nothing left to read, and the listing is
        // the point of the window.
        egui::Panel::left("navigator")
            .resizable(true)
            .default_size(300.0)
            .size_range(220.0..=(ui.available_width() * 0.4).max(220.0))
            .show(ui, |ui| navigator::show(app, ui, &mut act));
    }

    if app.show_inspector && app.doc().is_some() {
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(320.0)
            .size_range(240.0..=(ui.available_width() * 0.4).max(240.0))
            .show(ui, |ui| inspector::show(app, ui, &mut act));
    }

    // The listing sits on `bg` and the panels either side on `panel`, which is
    // what the theme editor says those roles mean.
    egui::CentralPanel::default_margins()
        .frame(
            egui::Frame::new()
                .fill(col::bg())
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ui, |ui| {
        if app.doc().is_none() {
            // An open project with nothing in it is not a cold start, and
            // should not look like one.
            if app.project.path.is_some() {
                views::empty_project(app, ui, &mut act);
            } else {
                views::welcome(app, ui, &mut act);
            }
            return;
        }
        views::central(app, ui, &mut act);
    });

    if let Some(w) = &app.wizard {
        crate::wizard::show(w, &ctx, &mut act);
    }
    theme_editor::show(app, &ctx, &mut act);
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
    // Any open dialog owns the keyboard. Listing shortcuts are single letters,
    // so leaving them live while a text box has focus types nothing and
    // renames something instead.
    let modal_open =
        app.goto_open || app.palette_open || app.wizard.is_some() || app.rename_at.is_some();
    let typing = modal_open || ctx.egui_wants_keyboard_input();
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
        if i.consume_key(Modifiers::COMMAND, Key::S) {
            act.push(Action::SaveProject);
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
            if let (Nav::Segment(segment), Some(offset)) =
                (app.nav(), app.tab().and_then(|t| t.sel))
            {
                if i.consume_key(Modifiers::NONE, Key::N) {
                    act.push(Action::ShowRename { segment, offset });
                }
                if i.consume_key(Modifiers::NONE, Key::B) {
                    act.push(Action::ToggleBookmark { segment, offset });
                }
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
                    for r in &app.recent {
                        let gone = !r.exists();
                        let label = format!(
                            "{}{}{}",
                            r.file_name(),
                            if r.is_project { "  (project)" } else { "" },
                            if gone { "  — missing" } else { "" }
                        );
                        ui.horizontal(|ui| {
                            let btn = ui.add_enabled(!gone, egui::Button::new(label));
                            if btn.on_hover_text(r.path.display().to_string()).clicked() {
                                act.push(if r.is_project {
                                    Action::OpenProjectAt(r.path.clone())
                                } else {
                                    Action::Open(r.path.clone())
                                });
                                ui.close();
                            }
                            if gone && ui.small_button("forget").clicked() {
                                act.push(Action::ForgetRecent(r.path.clone()));
                            }
                        });
                    }
                });
            });
            ui.separator();
            if ui.button("New project…").clicked() {
                act.push(Action::ShowWizard);
                ui.close();
            }
            if ui.button("Open project…").clicked() {
                act.push(Action::OpenProject);
                ui.close();
            }
            if ui.button("Save project           Ctrl+S").clicked() {
                act.push(Action::SaveProject);
                ui.close();
            }
            if ui.button("Save project as…").clicked() {
                act.push(Action::SaveProjectAs);
                ui.close();
            }
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
            ui.menu_button("Theme", |ui| {
                if ui.button("Edit this theme…").clicked() {
                    act.push(Action::OpenThemeEditor);
                    ui.close();
                }
                ui.separator();
                for p in crate::theme::themes() {
                    let active = app.theme == p.name;
                    let c = p.colors;
                    // A swatch beside each name says what the theme looks like
                    // without having to try it.
                    let r = ui.horizontal(|ui| {
                        for c in [c.accent, c.green, c.yellow, c.red, c.purple] {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 12.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rect, egui::CornerRadius::same(2), c);
                        }
                        ui.selectable_label(active, &p.name)
                    });
                    if r.inner.clicked() {
                        act.push(Action::SetTheme(p.name.clone()));
                        ui.close();
                    }
                    // Only a saved theme can be removed; the shipped ones come
                    // back on the next launch anyway.
                    if !p.builtin {
                        r.response.context_menu(|ui| {
                            if ui.button(format!("Delete {}", p.name)).clicked() {
                                act.push(Action::DeleteTheme(p.name.clone()));
                                ui.close();
                            }
                        });
                    }
                }
            });
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
                col::dim(),
            ));
            ui.separator();
            crate::widgets::chip(
                ui,
                if doc.ne.header.is_library() { "DLL" } else { "EXE" },
                if doc.ne.header.is_library() { col::purple() } else { col::green() },
            );
            ui.label(mono_c(doc.ne.module_name(), col::symbol()));
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
                    let fill = if active { col::bg() } else { col::raised() };
                    let text_col = if active { col::text() } else { col::dim() };
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
                                icons::inline(
                                    ui,
                                    tab.nav.icon_for(app.docs.get(tab.doc)),
                                    if active { col::accent() } else { col::dim() },
                                );
                                ui.label(egui::RichText::new(&title).color(text_col).size(12.0));
                                // A tab from another file says so: with several
                                // modules open the title alone is ambiguous.
                                if app.docs.len() > 1 {
                                    if let Some(d) = app.docs.get(tab.doc) {
                                        ui.label(
                                            egui::RichText::new(d.ne.module_name())
                                                .color(col::faint())
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
                        if over_close { col::red() } else { col::faint() },
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
                            .rect_filled(bar, egui::CornerRadius::ZERO, col::accent());
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
            ui.label(egui::RichText::new("!").color(col::red()).monospace().strong());
            ui.label(egui::RichText::new(err).color(col::red()).size(12.0));
            // A path that has gone is usually worth dropping from the list, so
            // the offer sits beside the message that revealed it.
            if let Some(p) = &app.forget_candidate {
                if ui
                    .small_button("remove from recent")
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    act.push(Action::ForgetRecent(p.clone()));
                }
            }
            if ui.small_button("dismiss").clicked() {
                act.push(Action::Dismiss);
            }
            return;
        }
        ui.label(egui::RichText::new(&app.status).color(col::dim()).size(12.0));
        if app.project.annotation_count() > 0 || app.project.path.is_some() {
            ui.separator();
            let name = if app.project.name.is_empty() {
                "untitled".to_string()
            } else {
                app.project.name.clone()
            };
            ui.label(
                egui::RichText::new(format!(
                    "{name}{}  ·  {} annotations",
                    if app.project.dirty { "*" } else { "" },
                    app.project.annotation_count()
                ))
                .color(if app.project.dirty { col::orange() } else { col::faint() })
                .size(11.5),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(doc) = app.doc() {
                let sel = app.tab().and_then(|t| t.sel);
                if let (Nav::Segment(segno), Some(sel)) = (app.nav(), sel) {
                    if let Some(seg) = doc.ne.segment(segno) {
                        ui.label(mono_c(
                            format!("file {:08X}", seg.file_offset + sel as u64),
                            col::faint(),
                        ));
                        ui.separator();
                        ui.label(mono_c(format!("seg{segno:02}:{sel:04X}"), col::accent()));
                    }
                }
                ui.separator();
                ui.label(mono_c(
                    format!(
                        "{} fn  {} decoded  {} strings",
                        doc.program.functions.len(),
                        human(doc.decoded_bytes() as u64),
                        doc.ne
                            .segments
                            .iter()
                            .map(|s| ne_core::strings::scan(&s.data, app.min_string_len).len())
                            .sum::<usize>()
                    ),
                    col::faint(),
                ));
            }
        });
    });
}


/// Shared empty-state message.
pub fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(text).color(col::faint()));
    });
}

pub fn sep(ui: &mut Ui) {
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter()
        .hline(rect.min.x..=rect.max.x, y, egui::Stroke::new(1.0, col::border()));
    ui.add_space(5.0);
}
