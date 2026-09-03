//! Small building blocks shared by every listing.

use crate::theme::*;
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Ui};

/// One row of a listing: full-width background, hover and selection states,
/// and a click response.
///
/// `show_rows` gives fixed-height virtualised rows, but nothing that fills the
/// row behind the content, which is what makes a dense listing readable. The
/// background shape is reserved before the content is laid out and filled once
/// the row's rect is known.
pub fn row<R>(
    ui: &mut Ui,
    id: egui::Id,
    selected: bool,
    striped: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let bg = ui.painter().add(egui::Shape::Noop);
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        content(ui)
    });

    let mut rect = inner.response.rect;
    rect.min.x = ui.max_rect().min.x;
    rect.max.x = ui.max_rect().max.x;
    rect = rect.expand2(egui::vec2(0.0, 1.0));

    let resp = ui.interact(rect, id, Sense::click());
    let fill = if selected {
        SELECTED
    } else if resp.hovered() {
        HOVER
    } else if striped {
        STRIPE
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
    ui.label(egui::RichText::new(text).color(DIM).size(11.0).strong());
    ui.add_space(2.0);
}
