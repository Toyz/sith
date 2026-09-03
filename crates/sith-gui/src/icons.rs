//! Vector icons.
//!
//! Drawn with the painter rather than loaded from files: the set is small, it
//! stays crisp at any zoom or DPI, it takes its color from the theme, and it
//! costs no dependency. (egui can render SVG through `egui_extras`' `svg`
//! feature if a richer set is ever wanted; this avoids pulling in a renderer
//! for sixteen glyphs.)
//!
//! Every icon is drawn on a nominal 16x16 grid and scaled into the rect it is
//! given, so the shapes stay consistent with each other.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Back,
    Forward,
    Target,
    Search,
    Open,
    Reload,
    Save,
    Copy,
    Close,
    Plus,
    Minus,
    Overview,
    Segment,
    Code,
    Data,
    Resource,
    Import,
    Export,
    Entries,
    Strings,
    Graph,
    Xref,
    Module,
    Dialog,
    Menu,
    Font,
    Cursor,
    IconBadge,
    Accelerator,
    Version,
}

/// The icon for a resource type.
///
/// A picture frame on every resource says only "resource"; the whole point of
/// the tree is to tell a dialog from a menu from a font at a glance.
pub fn for_resource(type_id: Option<u16>) -> Icon {
    use ne_core::resource::rt;
    match type_id {
        Some(rt::CURSOR) | Some(rt::GROUP_CURSOR) => Icon::Cursor,
        Some(rt::BITMAP) => Icon::Resource,
        Some(rt::ICON) | Some(rt::GROUP_ICON) => Icon::IconBadge,
        Some(rt::MENU) => Icon::Menu,
        Some(rt::DIALOG) => Icon::Dialog,
        Some(rt::STRING) | Some(rt::MESSAGETABLE) | Some(rt::NAMETABLE) => Icon::Strings,
        Some(rt::FONT) | Some(rt::FONTDIR) => Icon::Font,
        Some(rt::ACCELERATOR) => Icon::Accelerator,
        Some(rt::VERSION) => Icon::Version,
        Some(rt::RCDATA) => Icon::Data,
        _ => Icon::Resource,
    }
}

/// Draw `icon` inside `rect`, scaled to fit.
pub fn draw(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - Vec2::splat(8.0 * s);
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let stroke = Stroke::new((1.4 * s).max(1.0), color);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let path = |pts: Vec<Pos2>| {
        painter.add(egui::Shape::line(pts, stroke));
    };

    match icon {
        Icon::Back => path(vec![p(10.0, 3.0), p(5.0, 8.0), p(10.0, 13.0)]),
        Icon::Forward => path(vec![p(6.0, 3.0), p(11.0, 8.0), p(6.0, 13.0)]),
        Icon::Target => {
            painter.circle_stroke(p(8.0, 8.0), 4.5 * s, stroke);
            painter.circle_filled(p(8.0, 8.0), 1.3 * s, color);
            line(p(8.0, 1.0), p(8.0, 3.0));
            line(p(8.0, 13.0), p(8.0, 15.0));
            line(p(1.0, 8.0), p(3.0, 8.0));
            line(p(13.0, 8.0), p(15.0, 8.0));
        }
        Icon::Search => {
            painter.circle_stroke(p(7.0, 7.0), 4.0 * s, stroke);
            line(p(10.0, 10.0), p(14.0, 14.0));
        }
        Icon::Open => {
            path(vec![
                p(2.0, 12.5),
                p(2.0, 4.0),
                p(6.0, 4.0),
                p(7.5, 6.0),
                p(12.0, 6.0),
            ]);
            path(vec![p(2.0, 12.5), p(5.0, 7.5), p(15.0, 7.5), p(12.0, 12.5), p(2.0, 12.5)]);
        }
        Icon::Reload => {
            painter.circle_stroke(p(8.0, 8.0), 5.0 * s, stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![p(11.0, 1.5), p(14.5, 4.0), p(10.5, 5.5)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Save => {
            path(vec![p(8.0, 2.0), p(8.0, 10.0)]);
            path(vec![p(5.0, 7.0), p(8.0, 10.5), p(11.0, 7.0)]);
            path(vec![p(3.0, 13.0), p(13.0, 13.0)]);
        }
        Icon::Copy => {
            painter.rect_stroke(
                Rect::from_min_max(p(5.5, 2.5), p(13.5, 11.0)),
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            path(vec![p(10.0, 13.5), p(2.5, 13.5), p(2.5, 5.0)]);
            line(p(2.5, 13.5), p(2.5, 5.0));
            line(p(2.5, 5.0), p(5.5, 5.0));
        }
        Icon::Close => {
            line(p(4.0, 4.0), p(12.0, 12.0));
            line(p(12.0, 4.0), p(4.0, 12.0));
        }
        Icon::Plus => {
            line(p(8.0, 3.5), p(8.0, 12.5));
            line(p(3.5, 8.0), p(12.5, 8.0));
        }
        Icon::Minus => {
            line(p(3.5, 8.0), p(12.5, 8.0));
        }
        Icon::Overview => {
            path(vec![
                p(3.0, 2.0),
                p(10.0, 2.0),
                p(13.0, 5.0),
                p(13.0, 14.0),
                p(3.0, 14.0),
                p(3.0, 2.0),
            ]);
            line(p(5.5, 7.0), p(10.5, 7.0));
            line(p(5.5, 10.0), p(10.5, 10.0));
        }
        Icon::Segment => {
            for i in 0..3 {
                let y = 3.5 + i as f32 * 4.0;
                painter.rect_stroke(
                    Rect::from_min_max(p(2.5, y), p(13.5, y + 2.6)),
                    egui::CornerRadius::same(1),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Code => {
            path(vec![p(6.0, 4.0), p(2.5, 8.0), p(6.0, 12.0)]);
            path(vec![p(10.0, 4.0), p(13.5, 8.0), p(10.0, 12.0)]);
        }
        Icon::Data => {
            painter.circle_stroke(p(8.0, 4.5), 5.0 * s, stroke);
            path(vec![p(3.0, 4.5), p(3.0, 11.5)]);
            path(vec![p(13.0, 4.5), p(13.0, 11.5)]);
            path(vec![p(3.0, 11.5), p(5.0, 13.5), p(11.0, 13.5), p(13.0, 11.5)]);
        }
        Icon::Resource => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.5, 3.0), p(13.5, 13.0)),
                egui::CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(p(6.0, 6.5), 1.2 * s, color);
            path(vec![p(3.5, 12.0), p(7.0, 8.5), p(10.0, 11.0), p(12.0, 9.0)]);
        }
        Icon::Import => {
            line(p(8.0, 2.0), p(8.0, 9.5));
            path(vec![p(5.0, 6.5), p(8.0, 10.0), p(11.0, 6.5)]);
            path(vec![p(3.0, 13.5), p(13.0, 13.5)]);
        }
        Icon::Export => {
            line(p(8.0, 14.0), p(8.0, 6.5));
            path(vec![p(5.0, 9.5), p(8.0, 6.0), p(11.0, 9.5)]);
            path(vec![p(3.0, 2.5), p(13.0, 2.5)]);
        }
        Icon::Entries => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.5, 3.0), p(13.5, 13.0)),
                egui::CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            line(p(2.5, 6.5), p(13.5, 6.5));
            line(p(6.5, 6.5), p(6.5, 13.0));
        }
        Icon::Strings => {
            path(vec![p(5.0, 3.0), p(3.0, 5.5), p(5.0, 8.0)]);
            path(vec![p(9.5, 3.0), p(7.5, 5.5), p(9.5, 8.0)]);
            line(p(3.0, 12.5), p(13.0, 12.5));
        }
        Icon::Graph => {
            painter.circle_stroke(p(3.5, 4.0), 2.0 * s, stroke);
            painter.circle_stroke(p(3.5, 12.0), 2.0 * s, stroke);
            painter.circle_stroke(p(12.5, 8.0), 2.0 * s, stroke);
            line(p(5.4, 4.8), p(10.6, 7.4));
            line(p(5.4, 11.2), p(10.6, 8.6));
        }
        Icon::Xref => {
            path(vec![p(3.0, 5.5), p(13.0, 5.5)]);
            path(vec![p(10.5, 3.0), p(13.5, 5.5), p(10.5, 8.0)]);
            path(vec![p(13.0, 10.5), p(3.0, 10.5)]);
            path(vec![p(5.5, 8.0), p(2.5, 10.5), p(5.5, 13.0)]);
        }
        Icon::Dialog => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.0, 3.0), p(14.0, 13.0)),
                egui::CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            line(p(2.0, 6.0), p(14.0, 6.0));
            painter.circle_filled(p(12.2, 4.5), 0.9 * s, color);
            painter.rect_stroke(
                Rect::from_min_max(p(8.5, 9.0), p(12.5, 11.5)),
                egui::CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            line(p(4.0, 9.0), p(7.0, 9.0));
            line(p(4.0, 11.0), p(7.0, 11.0));
        }
        Icon::Menu => {
            line(p(2.0, 3.5), p(14.0, 3.5));
            painter.rect_stroke(
                Rect::from_min_max(p(5.0, 5.5), p(13.0, 13.5)),
                egui::CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            line(p(6.5, 8.0), p(11.5, 8.0));
            line(p(6.5, 10.5), p(11.5, 10.5));
        }
        Icon::Font => {
            // A letterform in a box reads as "font" at any size.
            path(vec![p(4.0, 13.0), p(8.0, 3.0), p(12.0, 13.0)]);
            line(p(5.6, 9.5), p(10.4, 9.5));
        }
        Icon::Cursor => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(4.0, 2.5), p(4.0, 12.5), p(6.8, 9.8), p(8.6, 13.5), p(10.4, 12.6), p(8.6, 9.2), p(12.0, 8.8)],
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::IconBadge => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.5, 2.5), p(13.5, 13.5)),
                egui::CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![p(8.0, 4.8), p(9.4, 7.4), p(12.0, 7.8), p(10.0, 9.6), p(10.6, 12.2), p(8.0, 10.9), p(5.4, 12.2), p(6.0, 9.6), p(4.0, 7.8), p(6.6, 7.4)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Accelerator => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.0, 4.0), p(14.0, 12.0)),
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            for x in [4.5f32, 7.0, 9.5] {
                line(p(x, 6.5), p(x + 1.2, 6.5));
            }
            line(p(11.8, 6.5), p(12.4, 6.5));
            line(p(5.0, 9.5), p(11.0, 9.5));
        }
        Icon::Version => {
            // A luggage tag: a label attached to something.
            path(vec![
                p(2.5, 7.5),
                p(8.0, 2.0),
                p(14.0, 2.0),
                p(14.0, 8.0),
                p(8.5, 13.5),
                p(2.5, 7.5),
            ]);
            painter.circle_stroke(p(11.3, 4.7), 1.2 * s, stroke);
        }
        Icon::Module => {
            path(vec![
                p(8.0, 2.0),
                p(14.0, 5.0),
                p(14.0, 11.0),
                p(8.0, 14.0),
                p(2.0, 11.0),
                p(2.0, 5.0),
                p(8.0, 2.0),
            ]);
            path(vec![p(2.0, 5.0), p(8.0, 8.0), p(14.0, 5.0)]);
            line(p(8.0, 8.0), p(8.0, 14.0));
        }
    }
}

/// A toolbar button carrying an icon.
pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let size = Vec2::splat(22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let visuals = ui.style().interact(&resp);
    if resp.hovered() || resp.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), visuals.weak_bg_fill);
    }
    let color = if ui.is_enabled() {
        crate::theme::col::text()
    } else {
        crate::theme::col::faint()
    };
    draw(ui.painter(), rect.shrink(4.0), icon, color);
    resp.on_hover_text(tooltip)
}

/// An icon rendered inline, sized to the current text.
pub fn inline(ui: &mut Ui, icon: Icon, color: Color32) {
    let h = ui.text_style_height(&egui::TextStyle::Body);
    inline_sized(ui, icon, color, h);
}

/// An icon rendered inline at a chosen size, for pairing with a heading.
pub fn inline_sized(ui: &mut Ui, icon: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw(ui.painter(), rect.shrink(size * 0.08), icon, color);
}
