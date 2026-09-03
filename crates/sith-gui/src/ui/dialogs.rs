//! Go-to-address and symbol-finder overlays.

use crate::icons::{self, Icon};
use crate::state::{Action, SithApp};
use crate::theme::{col, *};
use eframe::egui::{self, Context, Key};
use crate::widgets::tail;

pub fn show(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    if app.goto_open {
        goto(app, ctx, act);
    }
    if !app.missing.is_empty() {
        missing(app, ctx, act);
    }
    if let Some((segment, offset)) = app.rename_at {
        rename(app, ctx, act, segment, offset);
    }
    if app.keys_open {
        keys(app, ctx, act);
    }
    if app.palette_open {
        palette(app, ctx, act);
        // The forced scroll applies for one frame; holding it would stop the
        // user scrolling the list by hand.
        act.push(Action::PaletteScrolled);
    }
}

fn goto(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    let mut text = app.goto_text.clone();
    egui::Modal::new(egui::Id::new("goto")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(egui::RichText::new("Go to address").strong());
        ui.label(dim("seg02:1A40, a bare offset in the current segment, or a symbol name"));
        ui.add_space(6.0);
        let r = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if app.focus_input {
            r.request_focus();
        }
        match app.resolve(&text) {
            Some(addr) => ui.label(mono_c(format!("→ {addr}"), col::green())),
            None if text.trim().is_empty() => ui.label(dim("…")),
            None => ui.label(mono_c("no match", col::red())),
        };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let go = ui.button("Go").clicked()
                || ui.input(|i| i.key_pressed(Key::Enter));
            if go {
                if let Some(addr) = app.resolve(&text) {
                    act.push(Action::Goto(addr));
                    act.push(Action::Dismiss);
                }
            }
            if ui.button("Cancel").clicked() {
                act.push(Action::Dismiss);
            }
        });
    });
    if text != app.goto_text {
        act.push(Action::SetGotoText(text));
    }
}


/// Binaries a project points at that are not on disk.
///
/// This is not an error -- a project is often opened on a machine that has
/// only some of the binaries -- but it does need saying, and the choice of
/// what to do about it belongs to the user, because the annotations for a
/// removed binary go with it.
fn missing(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    egui::Modal::new(egui::Id::new("missing")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Module, col::orange());
            ui.label(
                egui::RichText::new(format!(
                    "{} binar{} in this project {} not there",
                    app.missing.len(),
                    if app.missing.len() == 1 { "y" } else { "ies" },
                    if app.missing.len() == 1 { "is" } else { "are" }
                ))
                .strong(),
            );
        });
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(col::bg())
            .corner_radius(egui::CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for p in &app.missing {
                    let annotations = app
                        .project
                        .binaries
                        .iter()
                        .find(|b| app.project.resolve(&b.path) == *p)
                        .map(|b| b.names.len() + b.comments.len() + b.bookmarks.len())
                        .unwrap_or(0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(mono_c(
                            p.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            col::text(),
                        ));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if annotations > 0 {
                                crate::widgets::chip(
                                    ui,
                                    &format!("{annotations} annotations"),
                                    col::orange(),
                                );
                            }
                            // The directory is context, not identity, so it
                            // gives way rather than running over the name.
                            ui.label(
                                egui::RichText::new(tail(
                                    &p.parent()
                                        .map(|d| d.display().to_string())
                                        .unwrap_or_default(),
                                    38,
                                ))
                                .size(10.5)
                                .color(col::faint()),
                            );
                        });
                    });
                }
            });
        ui.add_space(8.0);
        ui.label(dim(
            "The rest of the project opened normally. Removing these drops the \
             names and notes recorded against them, so it is worth being sure \
             the files are gone rather than moved.",
        ));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Keep them").clicked() {
                act.push(Action::DismissMissing);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Remove from project")
                    .on_hover_text("Deletes their annotations too")
                    .clicked()
                {
                    act.push(Action::DropMissingBinaries);
                }
            });
        });
    });
}

/// Name an address.
///
/// A name is the single most valuable annotation in a disassembly, so it is
/// one keystroke away (`N`) and lands wherever the selection is.
fn rename(app: &SithApp, ctx: &Context, act: &mut Vec<Action>, segment: u16, offset: u32) {
    let mut text = app.rename_text.clone();
    egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Code, col::symbol());
            ui.label(egui::RichText::new("Name this address").strong());
            ui.label(mono_c(format!("seg{segment:02}:{offset:04X}"), col::faint()));
        });
        ui.label(dim("clearing the box removes the name and restores the generated one"));
        ui.add_space(6.0);
        let r = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if app.focus_input {
            r.request_focus();
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let commit = ui.button("Save").clicked() || ui.input(|i| i.key_pressed(Key::Enter));
            if commit {
                act.push(Action::SetName {
                    segment,
                    offset,
                    name: text.clone(),
                });
            }
            if ui.button("Cancel").clicked() {
                act.push(Action::Dismiss);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let marked = app.is_bookmarked(segment, offset);
                if ui
                    .button(if marked { "Un-bookmark" } else { "Bookmark" })
                    .clicked()
                {
                    act.push(Action::ToggleBookmark { segment, offset });
                }
            });
        });
    });
    if text != app.rename_text {
        act.push(Action::SetRenameText(text));
    }
}

/// The command palette.
///
/// Everything addressable in the binary lives in one list: views, functions,
/// segments, resources, imports, strings and sibling modules. The list is
/// built by [`crate::palette`] and consulted again by the action handler, so
/// what is shown and what Enter does cannot drift apart.
fn palette(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    let mut text = app.palette_text.clone();
    let hits = crate::palette::candidates(app, &text);
    let sel = app.palette_sel.min(hits.len().saturating_sub(1));

    egui::Modal::new(egui::Id::new("palette")).show(ctx, |ui| {
        ui.set_width(700.0);
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Search, col::accent());
            let r = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(f32::INFINITY)
                    .hint_text("go to anything…")
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace),
            );
            if app.focus_input {
                r.request_focus();
            }
        });
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(crate::palette::hint())
                .size(10.5)
                .color(col::faint())
                .monospace(),
        );
        ui.add_space(4.0);

        if hits.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| ui.label(dim("nothing matches")));
            ui.add_space(12.0);
            return;
        }

        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 12.0;
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut area = egui::ScrollArea::vertical()
            .max_height(420.0)
            .auto_shrink([false, false]);
        // Keep the highlighted row in view as the arrow keys move it.
        if app.palette_scroll {
            area = area.vertical_scroll_offset((sel as f32 * row_h - 160.0).max(0.0));
        }
        area.show_rows(ui, row_h, hits.len(), |ui, range| {
            for i in range {
                let c = &hits[i];
                let (_, resp) = crate::widgets::row_sized(
                    ui,
                    ui.id().with(("pal", i)),
                    row_h,
                    i == sel,
                    false,
                    |ui| {
                        // Every cell is width-bounded -- a palette that grows
                        // to fit its longest string resource is unusable --
                        // but the bounds come from the row, not from a
                        // character count that ignores how wide it is.
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add_space(4.0);
                        icons::inline(ui, c.kind.icon(), c.kind.color());
                        ui.allocate_ui_with_layout(
                            egui::vec2(78.0, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| crate::widgets::chip(ui, c.kind.label(), c.kind.color()),
                        );
                        let rest = ui.available_width();
                        let title_w = rest * 0.58;
                        let detail_w = rest - title_w - 16.0;
                        let title = crate::widgets::fit(
                            ui,
                            &one_line(&c.title, 400),
                            egui::FontId::proportional(13.0),
                            title_w,
                            false,
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(title_w, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| highlighted(ui, &title, &c.hits, c.kind.color()),
                        );
                        // A detail is usually a path, so it keeps its end.
                        let detail = crate::widgets::fit(
                            ui,
                            &one_line(&c.detail, 400),
                            egui::FontId::monospace(13.0),
                            detail_w,
                            c.detail.contains('/') || c.detail.contains('\\'),
                        );
                        ui.label(mono_c(detail, col::faint()));
                    },
                );
                if resp.clicked() {
                    act.push(Action::PaletteChoose(i));
                }
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} matches", hits.len()))
                    .size(10.5)
                    .color(col::faint()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("up/down move     enter open     esc close")
                        .size(10.5)
                        .color(col::faint()),
                );
            });
        });
    });

    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
            act.push(Action::PaletteMove(1));
        }
        if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
            act.push(Action::PaletteMove(-1));
        }
        if i.consume_key(egui::Modifiers::NONE, Key::Enter) {
            act.push(Action::PaletteChoose(sel));
        }
    });

    if text != app.palette_text {
        act.push(Action::SetPaletteText(text));
    }
}

/// Collapse a candidate title to a single bounded line.
///
/// String resources run to hundreds of characters and contain newlines; left
/// alone they set the width of the whole dialog.
fn one_line(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        return flat.to_string();
    }
    let head: String = flat.chars().take(max - 1).collect();
    format!("{head}\u{2026}")
}

/// Draw `text` with the matched characters picked out.
///
/// Seeing which letters matched is what makes a fuzzy list trustworthy: it
/// explains why an entry is in the list at all.
fn highlighted(ui: &mut egui::Ui, text: &str, hits: &[usize], color: egui::Color32) {
    if hits.is_empty() {
        ui.label(mono_c(text, col::text()));
        return;
    }
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let font = egui::FontId::monospace(13.0);
    for (i, ch) in text.chars().enumerate() {
        let matched = hits.contains(&i);
        job.append(
            &ch.to_string(),
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: if matched { color } else { col::text() },
                underline: if matched {
                    egui::Stroke::new(1.0, color)
                } else {
                    egui::Stroke::NONE
                },
                ..Default::default()
            },
        );
    }
    ui.label(job);
}

/// The key binding editor.
pub fn keys(app: &SithApp, ctx: &egui::Context, act: &mut Vec<Action>) {
    use crate::keys::{Command, Scope};
    if !app.keys_open {
        return;
    }

    egui::Modal::new(egui::Id::new("keys")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            icons::inline_sized(ui, Icon::Overview, col::accent(), 18.0);
            ui.label(egui::RichText::new("Key bindings").size(16.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("  Done  ").clicked() {
                    act.push(Action::ShowKeys);
                }
                if ui.button("Reset all").clicked() {
                    act.push(Action::ResetAllKeys);
                }
            });
        });
        crate::ui::sep(ui);

        if let Some(c) = app.capturing {
            ui.label(
                egui::RichText::new(format!(
                    "Press a key for \u{201C}{}\u{201D}, or Escape to leave it alone.",
                    c.label()
                ))
                .size(12.0)
                .color(col::accent()),
            );
            ui.add_space(4.0);
        } else if let Some((wanted, held_by)) = app.key_clash {
            // Refusing is deliberate: silently unbinding the other command
            // would take away a shortcut the user never touched.
            ui.label(
                egui::RichText::new(format!(
                    "That key already runs \u{201C}{}\u{201D}, so \u{201C}{}\u{201D} was left as it was.",
                    held_by.label(),
                    wanted.label()
                ))
                .size(12.0)
                .color(col::orange()),
            );
            ui.add_space(4.0);
        }

        egui::ScrollArea::vertical()
            .id_salt("keylist")
            .max_height(420.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for scope in [Scope::Global, Scope::Listing, Scope::Selection] {
                    let group: Vec<&Command> = Command::ALL
                        .iter()
                        .filter(|c| c.scope() == scope)
                        .collect();
                    if group.is_empty() {
                        continue;
                    }
                    crate::widgets::card(ui, &scope.label().to_uppercase(), |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for c in group {
                            key_row(app, ui, act, *c);
                        }
                    });
                }
            });

        crate::ui::sep(ui);
        if let Some(dir) = crate::paths::config_dir() {
            ui.label(
                egui::RichText::new(format!("saves to {}", dir.join("keys.json").display()))
                    .size(10.5)
                    .color(col::faint()),
            );
        }
    });
}

fn key_row(app: &SithApp, ui: &mut egui::Ui, act: &mut Vec<Action>, c: crate::keys::Command) {
    let capturing = app.capturing == Some(c);
    let (_, resp) = crate::widgets::row_sized(
        ui,
        ui.id().with(("keyrow", c.id())),
        24.0,
        capturing,
        false,
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(egui::RichText::new(c.label()).size(12.0).color(col::text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !app.keys.is_default(c) {
                    let (r, rr) =
                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                    icons::draw(
                        ui.painter(),
                        r,
                        Icon::Reload,
                        if rr.hovered() { col::accent() } else { col::faint() },
                    );
                    if rr.on_hover_text("back to the default").clicked() {
                        act.push(Action::ResetKey(c));
                    }
                }
                crate::widgets::chip(
                    ui,
                    &if capturing {
                        "press a key\u{2026}".to_owned()
                    } else {
                        app.keys.shortcut(c)
                    },
                    if capturing {
                        col::accent()
                    } else if app.keys.is_default(c) {
                        col::dim()
                    } else {
                        col::cyan()
                    },
                );
            });
        },
    );
    if resp.clicked() && !capturing {
        act.push(Action::CaptureKey(c));
    }
}
