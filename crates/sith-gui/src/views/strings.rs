//! The strings browser.
//!
//! A raw list of printable runs is easy to produce and hard to use. What makes
//! a string useful in reverse engineering is knowing which code touches it, so
//! each row carries its references: 16-bit code loads a data-segment pointer
//! as a bare immediate, and matching those constants against string offsets
//! recovers the link. It is a heuristic, and the view says so by showing the
//! count rather than claiming certainty.

use crate::state::{Action, SithApp};
use crate::theme::*;
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_analysis::Addr;
use ne_core::strings::{self, FoundString};

struct Row {
    segment: u16,
    s: FoundString,
    refs: Vec<Addr>,
}

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };

    let filter = crate::views::filter_box(app, ui, act, "filter strings…");
    let needle = filter.to_ascii_lowercase();

    let mut rows: Vec<Row> = Vec::new();
    for seg in &doc.ne.segments {
        if seg.data.is_empty() {
            continue;
        }
        for s in strings::scan(&seg.data, app.min_string_len) {
            if !needle.is_empty() && !s.text.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            let refs = doc.program.data_refs(s.offset).to_vec();
            rows.push(Row {
                segment: seg.index,
                s,
                refs,
            });
        }
    }
    // Referenced strings first: those are the ones tied to behaviour.
    rows.sort_by(|a, b| {
        b.refs
            .len()
            .cmp(&a.refs.len())
            .then(a.segment.cmp(&b.segment))
            .then(a.s.offset.cmp(&b.s.offset))
    });

    let referenced = rows.iter().filter(|r| !r.refs.is_empty()).count();
    ui.horizontal(|ui| {
        ui.label(dim(&format!(
            "{} strings, {referenced} referenced from code",
            rows.len()
        )));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Copy all").clicked() {
                let text: String = rows
                    .iter()
                    .map(|r| format!("seg{:02}:{:04X}\t{}\n", r.segment, r.s.offset, r.s.text))
                    .collect();
                ui.ctx().copy_text(text);
                act.push(Action::Status(format!("copied {} strings", rows.len())));
            }
        });
    });
    ui.add_space(4.0);

    header(ui);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
        ui,
        row_h,
        rows.len(),
        |ui, range| {
            for i in range {
                let r = &rows[i];
                let (_, resp) = widgets::row(
                    ui,
                    ui.id().with(("str", r.segment, r.s.offset)),
                    false,
                    i % 2 == 1,
                    |ui| {
                        ui.label(mono_c(format!("seg{:02}:{:04X}", r.segment, r.s.offset), ADDR));
                        ui.label(mono_c(format!("{:>4}", r.s.text.len()), FAINT));
                        if r.refs.is_empty() {
                            ui.label(mono_c("   -", FAINT));
                        } else {
                            ui.label(mono_c(format!("{:>4}", r.refs.len()), ACCENT));
                        }
                        ui.label(mono_c(
                            if r.s.nul_terminated { "·" } else { " " },
                            GREEN,
                        ));
                        ui.label(mono_c(&r.s.text, TEXT));
                    },
                );
                if resp.clicked() {
                    // Jumping to the first reference is far more useful than
                    // landing on the bytes of the string itself.
                    match r.refs.first() {
                        Some(a) => act.push(Action::Goto(*a)),
                        None => act.push(Action::Goto(Addr {
                            segment: r.segment,
                            offset: r.s.offset,
                        })),
                    }
                }
                resp.on_hover_ui(|ui| {
                    ui.label(mono_c(&r.s.text, TEXT));
                    if r.refs.is_empty() {
                        ui.label(dim("no code references found"));
                        return;
                    }
                    ui.label(dim(&format!("{} references", r.refs.len())));
                    for a in r.refs.iter().take(12) {
                        ui.label(mono_c(a.to_string(), ACCENT));
                    }
                });
            }
        },
    );
}

fn header(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(mono_c("address   ", FAINT));
        ui.label(mono_c(" len", FAINT));
        ui.label(mono_c("refs", FAINT));
        ui.label(mono_c(" ", FAINT));
        ui.label(mono_c("text", FAINT));
    });
    crate::ui::sep(ui);
}

/// The per-segment string list shown inside a segment view.
pub fn segment_strings(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else { return };
    let filter = crate::views::filter_box(app, ui, act, "filter strings…").to_ascii_lowercase();
    let found: Vec<FoundString> = strings::scan(&seg.data, app.min_string_len)
        .into_iter()
        .filter(|s| filter.is_empty() || s.text.to_ascii_lowercase().contains(&filter))
        .collect();
    ui.label(dim(&format!("{} strings", found.len())));
    ui.add_space(4.0);

    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
        ui,
        row_h,
        found.len(),
        |ui, range| {
            for i in range {
                let s = &found[i];
                let refs = doc.program.data_refs(s.offset).len();
                let (_, resp) = widgets::row(
                    ui,
                    ui.id().with(("segstr", segno, s.offset)),
                    app.tab().and_then(|t| t.sel) == Some(s.offset),
                    i % 2 == 1,
                    |ui| {
                        ui.label(mono_c(format!("{:04X}", s.offset), ADDR));
                        if refs > 0 {
                            ui.label(mono_c(format!("{refs:>3}↗"), ACCENT));
                        } else {
                            ui.label(mono_c("   ", FAINT));
                        }
                        ui.label(mono_c(&s.text, TEXT));
                    },
                );
                if resp.clicked() {
                    act.push(Action::Select(s.offset));
                }
            }
        },
    );
}
