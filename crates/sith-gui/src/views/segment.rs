//! Segment view: a tab bar over the disassembly, hex, fixup and string modes.

use crate::state::{Action, SegTab, SithApp};
use crate::theme::{col, *};
use crate::views::{disasm, hex, strings};
use crate::icons::{self, Icon};
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_core::Target;

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else {
        crate::ui::empty(ui, &format!("segment {segno} does not exist"));
        return;
    };
    let Some(tab) = app.tab() else { return };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Segment {segno}")).size(15.0).strong());
        widgets::chip(
            ui,
            seg.kind().as_str(),
            if seg.is_code() { col::code_seg() } else { col::data_seg() },
        );
        ui.separator();
        for (mode, name) in SegTab::ALL {
            if mode.needs_code() && !seg.is_code() {
                continue;
            }
            if ui.selectable_label(tab.seg_tab == mode, name).clicked() {
                act.push(Action::SegTab(mode));
            }
        }
        ui.separator();
        let mut is32 = doc.bits32.contains(&segno);
        if ui
            .checkbox(&mut is32, "32-bit")
            .on_hover_text(
                "Decode as 32-bit code. Needed for segments that promote \
                 themselves through DPMI and run 32-bit instructions inside a \
                 16-bit selector.",
            )
            .changed()
        {
            act.push(Action::ToggleBits32(segno));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Save, "Save this listing to a file").clicked() {
                act.push(Action::SaveListing);
            }
        });
    });
    crate::ui::sep(ui);

    let mode = if !seg.is_code() && tab.seg_tab.needs_code() {
        SegTab::Hex
    } else {
        tab.seg_tab
    };
    match mode {
        SegTab::Disasm => disasm::show(app, ui, act, segno),
        SegTab::Pseudo => super::pseudo::show(app, ui, act, segno),
        SegTab::Hex => hex::show(app, ui, act, segno),
        SegTab::Fixups => fixups(app, ui, act, segno),
        SegTab::Strings => strings::segment_strings(app, ui, act, segno),
    }
}

fn fixups(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else { return };
    let sel = app.tab().and_then(|t| t.sel);
    let all = doc.ne.fixups(seg);

    let filter = crate::views::filter_box(app, ui, act, "filter targets…").to_ascii_lowercase();
    let rows: Vec<_> = all
        .iter()
        .filter(|f| filter.is_empty() || f.target.to_string().to_ascii_lowercase().contains(&filter))
        .collect();
    ui.label(dim(&format!(
        "{} relocation records → {} patch sites, {} shown",
        seg.relocs.len(),
        all.len(),
        rows.len()
    )));
    ui.add_space(4.0);

    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    // `show_rows` adds the ui's vertical item spacing to every row when it
    // computes the visible range and the total height; zeroing it here makes
    // the drawn rows exactly `row_h` apart, so the listing fills the view
    // instead of stopping short.
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_h, rows.len(), |ui, range| {
            for i in range {
                let f = rows[i];
                let (_, resp) = widgets::row_sized(
                    ui,
                    ui.id().with(("fx", f.site)),
                    row_h,
                    sel == Some(f.site as u32),
                    i % 2 == 1,
                    |ui| {
                        ui.label(mono_c(format!("{:04X}", f.site), col::addr()));
                        ui.label(mono_c(format!("{:<8}", f.addr_type.as_str()), col::dim()));
                        ui.label(mono_c(f.target.to_string(), target_color(&f.target)));
                        if f.additive {
                            widgets::chip(ui, "additive", col::orange());
                        }
                    },
                );
                if resp.clicked() {
                    act.push(Action::Select(f.site as u32));
                }
                if resp.double_clicked() {
                    act.push(disasm::target_action(&f.target));
                }
            }
        });
}

fn target_color(t: &Target) -> egui::Color32 {
    match t {
        Target::Internal { .. } | Target::Entry { .. } => col::accent(),
        Target::ImportOrdinal { .. } | Target::ImportName { .. } => col::comment(),
        Target::OsFixup { .. } => col::orange(),
    }
}
