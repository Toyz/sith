//! Hex dumps.

use crate::state::{Action, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Ui};

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(seg) = doc.ne.segment(segno) else { return };
    let tab = app.tab();
    dump(
        ui,
        act,
        &seg.data,
        tab.and_then(|t| t.scroll_to),
        tab.and_then(|t| t.sel),
    );
}

/// A virtualised 16-bytes-per-row dump with a clickable address gutter.
pub fn dump(
    ui: &mut Ui,
    act: &mut Vec<Action>,
    data: &[u8],
    scroll_to: Option<u32>,
    sel: Option<u32>,
) {
    if data.is_empty() {
        crate::ui::empty(ui, "no bytes");
        return;
    }
    let rows = data.len().div_ceil(16);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    let sel_row = sel.map(|s| s as usize / 16);

    // `show_rows` adds the ui's vertical item spacing to every row when it
    // computes the visible range and the total height; zeroing it here makes
    // the drawn rows exactly `row_h` apart, so the listing fills the view
    // instead of stopping short.
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if let Some(t) = scroll_to {
        area = area.vertical_scroll_offset(((t as usize / 16).saturating_sub(4)) as f32 * row_h);
    }
    area.show_rows(ui, row_h, rows, |ui, range| {
        for r in range {
            let start = r * 16;
            let chunk = &data[start..(start + 16).min(data.len())];
            let (_, resp) = widgets::row_sized(
                ui,
                ui.id().with(("hex", start)),
                row_h,
                sel_row == Some(r),
                r % 2 == 1,
                |ui| {
                    ui.label(mono_c(format!("{start:08X}"), col::addr()));
                    let mut hex = String::with_capacity(50);
                    for i in 0..16 {
                        match chunk.get(i) {
                            Some(b) => hex.push_str(&format!("{b:02X} ")),
                            None => hex.push_str("   "),
                        }
                        if i == 7 {
                            hex.push(' ');
                        }
                    }
                    ui.label(mono_c(hex, col::bytes()));
                    let ascii: String = chunk
                        .iter()
                        .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '·' })
                        .collect();
                    ui.label(mono_c(ascii, col::text()));
                },
            );
            if resp.clicked() {
                act.push(Action::Select(start as u32));
            }
        }
    });
}
