//! The C view: a segment's functions written as statements.
//!
//! What the lifter can establish is in `ne_analysis::pseudo`. This draws it,
//! and keeps the two views tied together: every line still knows the address
//! it came from, so a click goes straight back to the instruction.

use crate::icons::{self, Icon};
use crate::state::{Action, SegTab, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Color32, Ui};
use ne_analysis::pseudo::{Kind, Line};

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let funcs: Vec<&ne_analysis::Function> = doc
        .program
        .functions
        .iter()
        .filter(|f| f.addr.segment == segno)
        .collect();
    if funcs.is_empty() {
        crate::ui::empty(ui, "no functions found in this segment");
        return;
    }

    // One function at a time. A segment is tens of thousands of statements,
    // and a wall of them is not something anyone reads.
    let selected = app
        .tab()
        .and_then(|t| t.sel)
        .and_then(|off| doc.program.function_containing(ne_analysis::Addr {
            segment: segno,
            offset: off,
        }))
        .map(|f| f.addr.offset)
        .unwrap_or(funcs[0].addr.offset);
    let Some(f) = funcs.iter().find(|f| f.addr.offset == selected).or(funcs.first()) else {
        return;
    };

    picker(app, ui, act, &funcs, f.addr.offset);
    crate::ui::sep(ui);

    let lines = ne_analysis::pseudo::function(&doc.program, ne_core::ApiDb::embedded(), f, &app.label(f));
    body(app, ui, act, segno, &lines);
}

/// Which function is being read, and a way to the next one.
fn picker(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    funcs: &[&ne_analysis::Function],
    current: u32,
) {
    let at = funcs.iter().position(|f| f.addr.offset == current).unwrap_or(0);
    ui.horizontal(|ui| {
        widgets::strip_item(ui, |ui| {
            icons::inline(ui, Icon::Code, col::symbol());
            ui.label(mono_c(app.label(funcs[at]), col::symbol()).strong());
            ui.label(mono_c(funcs[at].addr.to_string(), col::faint()));
        });
        ui.add_space(8.0);
        // Counted from one, because the label beside it says which function
        // this is and the number should say where it sits in the segment.
        if let Some(next) = widgets::stepper(ui, at + 1, 1, funcs.len()) {
            act.push(Action::Goto(funcs[next - 1].addr));
        }
        ui.label(
            egui::RichText::new(format!("of {}", funcs.len()))
                .size(11.0)
                .color(col::faint()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::strip_item(ui, |ui| {
                if widgets::toggle_chip(ui, false, "Disassembly", col::accent()) {
                    act.push(Action::SegTabAt(SegTab::Disasm, funcs[at].addr));
                }
                if widgets::toggle_chip(ui, false, "Call graph", col::dim()) {
                    act.push(Action::SetGraphRoot(funcs[at].addr));
                }
            });
        });
    });
}

fn body(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16, lines: &[Line]) {
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    ui.spacing_mut().item_spacing.y = 0.0;

    egui::ScrollArea::both()
        .id_salt("pseudo")
        .auto_shrink([false, false])
        .show_rows(ui, row_h, lines.len(), |ui, range| {
            for i in range {
                let line = &lines[i];
                let selected = line.addr.is_some()
                    && app.tab().and_then(|t| t.sel) == line.addr;
                let (_, resp) = widgets::row_sized(
                    ui,
                    ui.id().with(("cline", i)),
                    row_h,
                    selected,
                    false,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // The address column is what ties this back to the
                        // listing, and blank where a line has no instruction
                        // behind it -- a brace is not something you can go to.
                        match line.addr {
                            Some(a) => ui.label(
                                egui::RichText::new(format!("{a:04X}  "))
                                    .monospace()
                                    .size(12.0)
                                    .color(col::addr()),
                            ),
                            None => ui.label(
                                egui::RichText::new("      ").monospace().size(12.0),
                            ),
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "{}{}",
                                "    ".repeat(line.indent as usize),
                                line.text
                            ))
                            .monospace()
                            .size(13.0)
                            .color(color_of(line.kind)),
                        );
                    },
                );
                if let Some(a) = line.addr {
                    if resp.clicked() {
                        act.push(Action::Goto(ne_analysis::Addr {
                            segment: segno,
                            offset: a,
                        }));
                    }
                    if resp.double_clicked() {
                        act.push(Action::SegTabAt(
                            SegTab::Disasm,
                            ne_analysis::Addr {
                                segment: segno,
                                offset: a,
                            },
                        ));
                    }
                }
            }
        });

}

fn color_of(kind: Kind) -> Color32 {
    match kind {
        Kind::Comment => col::comment(),
        Kind::Signature => col::symbol(),
        Kind::Punct => col::dim(),
        Kind::Decl => col::cyan(),
        Kind::Label => col::yellow(),
        Kind::Statement => col::text(),
        Kind::Call => col::green(),
        Kind::Control => col::cyan(),
        Kind::Return => col::purple(),
        // An instruction with no C shape is left as it was, and marked, so
        // nothing here is mistaken for a complete translation.
        Kind::Asm => col::orange(),
    }
}
