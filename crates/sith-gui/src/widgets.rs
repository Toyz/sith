//! Small building blocks shared by every listing.

use crate::theme::{col, *};
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Ui};

/// One row of a listing, laid out at its natural height.
pub fn row<R>(
    ui: &mut Ui,
    id: egui::Id,
    selected: bool,
    striped: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    row_sized(ui, id, h, selected, striped, content)
}

/// One row of a listing at an exact height, with a full-width background,
/// hover and selection states, and a click response.
///
/// The height must be exact for a virtualised listing: `ScrollArea::show_rows`
/// reserves a fixed height per row and only draws the rows it believes are
/// visible, so content that lays out even a few pixels shorter leaves a
/// growing gap at the bottom of the view.
pub fn row_sized<R>(
    ui: &mut Ui,
    id: egui::Id,
    height: f32,
    selected: bool,
    striped: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let bg = ui.painter().add(egui::Shape::Noop);
    let width = ui.available_width();
    let inner = ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(height);
            ui.spacing_mut().item_spacing.x = 10.0;
            content(ui)
        },
    );

    let mut rect = inner.response.rect;
    rect.min.x = ui.max_rect().min.x;
    rect.max.x = ui.max_rect().max.x;
    rect.max.y = rect.min.y + height;

    let resp = ui.interact(rect, id, Sense::click());
    let fill = if selected {
        col::selected()
    } else if resp.hovered() {
        col::hover()
    } else if striped {
        col::stripe()
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().set(
            bg,
            egui::epaint::RectShape::filled(rect, CornerRadius::ZERO, fill),
        );
    }
    (inner.inner, resp)
}

/// A clickable monospace reference.
pub fn link(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let r = ui
        .add(egui::Label::new(mono_c(text, color)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() {
        let mut underline = r.rect;
        underline.min.y = underline.max.y - 1.0;
        ui.painter().rect_filled(underline, CornerRadius::ZERO, color);
    }
    r
}

/// A small pill used for counts, flags and kinds.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::monospace(11.0),
        color,
    );
    let pad = egui::vec2(5.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(3),
        color.gamma_multiply(0.18),
    );
    ui.painter().galley(rect.min + pad, galley, color);
}

/// Section heading inside a scrolling view.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text).color(col::dim()).size(11.0).strong());
    ui.add_space(2.0);
}

/// A framed group with a small-caps heading.
///
/// Panels made of bare label rows read as one undifferentiated wall; a frame
/// per topic is what lets the eye jump to the one it wants.
pub fn card<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    if !title.is_empty() {
        ui.label(
            egui::RichText::new(title)
                .size(10.0)
                .strong()
                .color(col::faint()),
        );
        ui.add_space(3.0);
    }
    let r = egui::Frame::new()
        .fill(col::raised())
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui)
        });
    ui.add_space(10.0);
    r.inner
}

/// A label/value line with a fixed label column, so a card reads as a table.
pub fn kv(ui: &mut Ui, key: &str, value: impl Into<String>) {
    kv_colored(ui, key, value, col::text());
}

pub fn kv_colored(ui: &mut Ui, key: &str, value: impl Into<String>, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            egui::RichText::new(key)
                .size(11.0)
                .color(col::faint())
                .monospace(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(mono_c(value.into(), color));
        });
    });
}
