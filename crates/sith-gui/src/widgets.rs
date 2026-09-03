//! Small building blocks shared by every listing.

use crate::theme::{col, *};
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Ui};

/// One row of a listing, laid out at its natural height.
/// Space between a row's highlight and its content, at each end.
pub const ROW_PAD: f32 = 6.0;

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
    // The highlight runs the full width of the list, but the content must not:
    // text that starts and ends flush with the highlight looks clipped by it.
    let width = ui.available_width();
    ui.add_space(ROW_PAD);
    let inner = ui.allocate_ui_with_layout(
        egui::vec2(width - ROW_PAD * 2.0, height),
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
    let hovered = resp.hovered();
    let fill = match (selected, hovered) {
        (true, true) => col::selected_hover(),
        (true, false) => col::selected(),
        (false, true) => col::hover(),
        (false, false) if striped => col::stripe(),
        _ => Color32::TRANSPARENT,
    };
    if fill != Color32::TRANSPARENT {
        // Inset and rounded, so a run of rows reads as a list with one of them
        // picked out rather than a column with a band painted across it. The
        // selected row is outlined as well as filled: the outline is what
        // makes it read as selected, which lets the fill stay light enough to
        // leave every value on the row at full contrast.
        let mut body = rect;
        body.min.x += 2.0;
        body.max.x -= 2.0;
        ui.painter().set(
            bg,
            egui::epaint::RectShape::new(
                body,
                CornerRadius::same(4),
                fill,
                if selected {
                    egui::Stroke::new(1.0, col::selected_outline())
                } else {
                    egui::Stroke::NONE
                },
                egui::StrokeKind::Inside,
            ),
        );
    }
    (inner.inner, resp)
}

/// A byte count in the shortest form that stays honest.
pub fn human(n: impl Into<u64>) -> String {
    let n: u64 = n.into();
    if n >= 1024 * 1024 {
        format!("{:.1}M", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1}K", n as f64 / 1024.0)
    } else {
        format!("{n}")
    }
}

/// The end of a long string, elided at the front.
///
/// Paths are elided from the front because the end is the part that
/// identifies the file; cutting the other way leaves every row reading
/// "/home/someone/work/pro..." and telling you nothing.
pub fn tail(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text
        .chars()
        .rev()
        .take(max.saturating_sub(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("\u{2026}{kept}")
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

/// One item in a control strip.
///
/// A fixed-height box with its content centred inside it. Every item in a
/// strip is the same height, so the row can be laid out top-aligned and
/// nothing depends on how egui rounds a centring offset -- which is where the
/// stray pixel between two controls comes from.
pub fn strip_item<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_height(CONTROL_H + 4.0);
        content(ui)
    })
    .inner
}

/// A segmented control: several exclusive choices in one strip.
///
/// Drawn by hand rather than out of `egui::Button`. A Button re-measures
/// itself per interaction state, so hovering one segment made the strip two
/// pixels narrower and shunted everything after it sideways; laying the text
/// out once and allocating from that keeps the size fixed whatever the
/// pointer is doing, and leaves hover free to be a color change.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, current: T, options: &[(T, &str)]) -> Option<T> {
    let mut picked = None;
    egui::Frame::new()
        .fill(col::bg())
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(CONTROL_H);
                ui.spacing_mut().item_spacing.x = 2.0;
                for (value, label) in options {
                    let active = *value == current;
                    // Laid out in one fixed color: a galley's size must not
                    // depend on the state it is drawn in.
                    let galley = ui.painter().layout_no_wrap(
                        (*label).to_owned(),
                        egui::FontId::proportional(11.5),
                        col::text(),
                    );
                    let size = egui::vec2(galley.size().x + 16.0, CONTROL_H);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let fill = if active {
                        col::accent().gamma_multiply(0.28)
                    } else if resp.hovered() {
                        col::raised()
                    } else {
                        Color32::TRANSPARENT
                    };
                    if fill != Color32::TRANSPARENT {
                        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
                    }
                    let color = if active {
                        col::text()
                    } else if resp.hovered() {
                        col::text()
                    } else {
                        col::dim()
                    };
                    let at = rect.min
                        + egui::vec2(8.0, (rect.height() - galley.size().y) / 2.0);
                    ui.painter().galley(at, galley, color);
                    if resp.clicked() {
                        picked = Some(*value);
                    }
                }
            });
        });
    picked
}

/// A minus / value / plus control for a small bounded number.
///
/// Emits exactly one frame, built the same way [`segmented`] builds its own,
/// and takes no label: any caption is the caller's to place. Two controls that
/// are meant to line up have to be made the same way, or they land a pixel
/// apart for reasons that are tedious to chase.
pub fn stepper(ui: &mut Ui, value: usize, min: usize, max: usize) -> Option<usize> {
    let mut picked = None;
    egui::Frame::new()
        .fill(col::bg())
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Exactly the inner height of the segmented control, so the
                // two sit level. A minimum is not enough: a button a pixel
                // taller than the nominal height pushes the frame down.
                ui.set_height(CONTROL_H);
                ui.spacing_mut().item_spacing.x = 2.0;
                if small_glyph(ui, "\u{2212}", value > min) {
                    picked = Some(value - 1);
                }
                ui.label(
                    egui::RichText::new(value.to_string())
                        .monospace()
                        .size(11.5)
                        .color(col::text()),
                );
                if small_glyph(ui, "+", value < max) {
                    picked = Some(value + 1);
                }
            });
        });
    picked
}

/// A small glyph button, drawn rather than built from `egui::Button`, for the
/// same reason the segments are: a fixed size whatever the pointer is doing.
fn small_glyph(ui: &mut Ui, text: &str, enabled: bool) -> bool {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::monospace(12.0),
        col::dim(),
    );
    let size = egui::vec2(galley.size().x + 12.0, CONTROL_H);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled { Sense::click() } else { Sense::hover() },
    );
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), col::raised());
    }
    let color = if !enabled {
        col::faint().gamma_multiply(0.5)
    } else if resp.hovered() {
        col::text()
    } else {
        col::dim()
    };
    let at = rect.min + egui::vec2(6.0, (rect.height() - galley.size().y) / 2.0);
    ui.painter().galley(at, galley, color);
    enabled && resp.clicked()
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
