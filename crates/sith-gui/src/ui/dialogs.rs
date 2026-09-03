//! Go-to-address and symbol-finder overlays.

use crate::state::{Action, Nav, SithApp};
use crate::theme::*;
use eframe::egui::{self, Context, Key};

pub fn show(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    if app.goto_open {
        goto(app, ctx, act);
    }
    if app.palette_open {
        palette(app, ctx, act);
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
            Some(addr) => ui.label(mono_c(format!("→ {addr}"), GREEN)),
            None if text.trim().is_empty() => ui.label(dim("…")),
            None => ui.label(mono_c("no match", RED)),
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

/// Fuzzy-ish finder over everything addressable: functions, exports, segments,
/// resources and imported symbols.
fn palette(app: &SithApp, ctx: &Context, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let mut text = app.palette_text.clone();

    let needle = text.to_ascii_lowercase();
    let mut hits: Vec<(String, String, Action)> = Vec::new();
    let matches = |s: &str| needle.is_empty() || s.to_ascii_lowercase().contains(&needle);

    for f in &doc.program.functions {
        let label = f.label();
        if matches(&label) {
            hits.push((
                label,
                format!("function  {}", f.addr),
                Action::Goto(f.addr),
            ));
        }
        if hits.len() > 400 {
            break;
        }
    }
    for s in &doc.ne.segments {
        let label = format!("Segment {}", s.index);
        if matches(&label) {
            hits.push((
                label,
                format!("{}  {} bytes", s.kind().as_str(), s.length),
                Action::Go(Nav::Segment(s.index)),
            ));
        }
    }
    for (i, r) in doc.ne.resources.iter().enumerate() {
        let label = format!("{} {}", r.type_name(), r.res_id);
        if matches(&label) {
            hits.push((
                label,
                format!("resource  {} bytes", r.length),
                Action::Go(Nav::Resource(i)),
            ));
        }
    }
    for (target, sites) in &doc.program.xrefs {
        if target.contains('.') && matches(target) {
            hits.push((
                target.clone(),
                format!("import  {} call sites", sites.len()),
                Action::Go(Nav::Xrefs(target.clone())),
            ));
        }
    }
    // Shorter names first: an exact-ish match is almost always what was meant.
    hits.sort_by_key(|(l, _, _)| (l.len(), l.to_ascii_lowercase()));
    hits.truncate(200);

    let sel = app.palette_sel.min(hits.len().saturating_sub(1));

    egui::Modal::new(egui::Id::new("palette")).show(ctx, |ui| {
        ui.set_width(620.0);
        let r = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(f32::INFINITY)
                .hint_text("find a function, segment, resource or import…")
                .font(egui::TextStyle::Monospace),
        );
        if app.focus_input {
            r.request_focus();
        }
        ui.add_space(4.0);
        ui.label(dim(&format!("{} matches", hits.len())));
        ui.add_space(2.0);

        egui::ScrollArea::vertical()
            .max_height(360.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, (label, detail, _)) in hits.iter().enumerate() {
                    let (_, resp) = crate::widgets::row(
                        ui,
                        ui.id().with(("pal", i)),
                        i == sel,
                        i % 2 == 1,
                        |ui| {
                            ui.label(mono_c(label, TEXT));
                            ui.label(mono_c(detail, FAINT));
                        },
                    );
                    if resp.clicked() {
                        act.push(Action::PaletteChoose(i));
                    }
                }
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
