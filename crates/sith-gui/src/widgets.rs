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
    // Never shorter than what was drawn. Clamping to the assumed height lets a
    // row with a chip in it overflow its own hit rect, so the row below -- laid
    // out later, and therefore on top -- steals the hover and the tooltip
    // flickers out.
    rect.max.y = rect.min.y + height.max(inner.response.rect.height());

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

/// Height shared by the small toolbar controls, so a row of them lines up.
pub const CONTROL_H: f32 = 22.0;

/// A segmented control: several exclusive choices in one strip.
///
/// egui's selectable labels look like text that happens to highlight; a
/// segmented control reads as one control with a state, which is what these
/// choices are.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, current: T, options: &[(T, &str)]) -> Option<T> {
    let mut picked = None;
    egui::Frame::new()
        .fill(col::bg())
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(CONTROL_H);
                ui.spacing_mut().item_spacing.x = 2.0;
                for (value, label) in options {
                    let active = *value == current;
                    let text = egui::RichText::new(*label)
                        .size(11.5)
                        .color(if active { col::text() } else { col::dim() });
                    let resp = ui.add(
                        egui::Button::new(text)
                            .fill(if active {
                                col::accent().gamma_multiply(0.28)
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(CornerRadius::same(4)),
                    );
                    if resp.clicked() {
                        picked = Some(*value);
                    }
                }
            });
        });
    picked
}

/// A minus / value / plus control for a small bounded number.
pub fn stepper(ui: &mut Ui, label: &str, value: usize, min: usize, max: usize) -> Option<usize> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(egui::RichText::new(label).size(11.0).color(col::faint()));
        egui::Frame::new()
            .fill(col::bg())
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::same(2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // The same inner height as the segmented control, so the
                    // two sit level in a toolbar.
                    ui.set_min_height(CONTROL_H);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.add_enabled_ui(value > min, |ui| {
                        if ui.add(small_glyph("\u{2212}")).clicked() {
                            picked = Some(value - 1);
                        }
                    });
                    ui.label(
                        egui::RichText::new(value.to_string())
                            .monospace()
                            .size(11.5)
                            .color(col::text()),
                    );
                    ui.add_enabled_ui(value < max, |ui| {
                        if ui.add(small_glyph("+")).clicked() {
                            picked = Some(value + 1);
                        }
                    });
                });
            });
    });
    picked
}

fn small_glyph(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.to_owned())
            .monospace()
            .size(12.0)
            .color(col::dim()),
    )
    .fill(Color32::TRANSPARENT)
    .stroke(egui::Stroke::NONE)
    .corner_radius(CornerRadius::same(4))
}

/// A chip that is also a switch.
pub fn toggle_chip(ui: &mut Ui, on: bool, label: &str, color: Color32) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.0),
        if on { color } else { col::faint() },
    );
    let pad = egui::vec2(7.0, 3.0);
    let size = egui::vec2(galley.size().x + pad.x * 2.0, CONTROL_H + 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if on {
        color.gamma_multiply(0.22)
    } else if resp.hovered() {
        col::raised()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        egui::Stroke::new(1.0, if on { color } else { col::border() }),
        egui::StrokeKind::Inside,
    );
    let color = if on { color } else { col::faint() };
    let text_at = rect.min + egui::vec2(pad.x, (rect.height() - galley.size().y) / 2.0);
    ui.painter().galley(text_at, galley, color);
    resp.clicked()
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

/// A styled tooltip: a title line, then facts.
///
/// egui's default is a raw block of text, which is fine for a sentence and
/// poor for the six facts a function or a module actually has to offer.
pub fn hover_card<R>(
    resp: Response,
    icon: Option<(crate::icons::Icon, Color32)>,
    title: &str,
    subtitle: &str,
    body: impl FnOnce(&mut Ui) -> R,
) -> Response {
    resp.on_hover_ui(|ui| {
        ui.set_max_width(340.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            if let Some((icon, color)) = icon {
                crate::icons::inline(ui, icon, color);
            }
            ui.label(
                egui::RichText::new(title)
                    .monospace()
                    .size(13.0)
                    .strong()
                    .color(col::text()),
            );
        });
        if !subtitle.is_empty() {
            ui.label(
                egui::RichText::new(subtitle)
                    .monospace()
                    .size(11.0)
                    .color(col::faint()),
            );
        }
        ui.add_space(5.0);
        body(ui);
    })
}

/// A fact line inside a tooltip: a dim label and its value.
pub fn hover_row(ui: &mut Ui, key: &str, value: impl Into<String>, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            egui::RichText::new(format!("{key:<12}"))
                .monospace()
                .size(11.0)
                .color(col::faint()),
        );
        ui.label(egui::RichText::new(value).monospace().size(11.0).color(color));
    });
}

/// A wrapped note at the foot of a tooltip.
pub fn hover_note(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).size(11.0).color(col::dim()));
}

/// Width to keep clear on the right of a scrolling panel.
///
/// Values in this tool are right-aligned, and `available_width` inside a
/// scroll area includes the strip the scrollbar sits in, so without this the
/// last few characters of every value end up underneath it.
pub const SCROLLBAR_GUTTER: f32 = 14.0;

/// A collapsible group styled like [`card`], for context that is worth having
/// but not worth the space by default.
pub fn collapsing_card<R>(
    ui: &mut Ui,
    id: &str,
    title: &str,
    open: bool,
    content: impl FnOnce(&mut Ui) -> R,
) {
    egui::CollapsingHeader::new(
        egui::RichText::new(title)
            .size(10.0)
            .strong()
            .color(col::faint()),
    )
    .id_salt(id)
    .icon(|ui, openness, resp| {
        // The same wedge the navigator uses, rather than egui's triangle.
        let c = if resp.hovered() { col::text() } else { col::faint() };
        let r = resp.rect.shrink(3.0);
        let rot = openness * std::f32::consts::FRAC_PI_2;
        let piv = r.center();
        let rt = |q: egui::Pos2| {
            let v = q - piv;
            piv + egui::vec2(
                v.x * rot.cos() - v.y * rot.sin(),
                v.x * rot.sin() + v.y * rot.cos(),
            )
        };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                rt(piv + egui::vec2(-2.0, -4.0)),
                rt(piv + egui::vec2(-2.0, 4.0)),
                rt(piv + egui::vec2(3.5, 0.0)),
            ],
            c,
            egui::Stroke::NONE,
        ));
    })
    .default_open(open)
    .show_unindented(ui, |ui| {
        ui.add_space(3.0);
        egui::Frame::new()
            .fill(col::raised())
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                content(ui)
            });
        ui.add_space(10.0);
    });
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
