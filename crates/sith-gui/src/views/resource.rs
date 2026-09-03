//! Resource preview: images, decoded text, and a hex fallback.

use crate::state::{Action, SithApp};
use crate::theme::{col, *};
use crate::icons::{self};
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_core::render;

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, index: usize) {
    let Some(doc) = app.doc() else { return };
    let Some(r) = doc.ne.resources.get(index) else {
        crate::ui::empty(ui, "no such resource");
        return;
    };

    toolbar(ui, act, r, index);
    crate::ui::sep(ui);

    // The picture gets the pane. Everything about it -- its header, its
    // palette, the code that loads it -- is context for a selection, and
    // context for a selection lives in the inspector.
    if let Some(img) = doc.ne.resource_image(r) {
        image_pane(app, ui, index, &img);
        // A font decodes to a glyph sheet, and its header is worth far more
        // than the sheet is, so it keeps its text underneath.
        if r.type_id.as_id() != Some(ne_core::resource::rt::FONT) {
            return;
        }
    }

    if let Some(text) = render::resource_text(&doc.ne, r) {
        egui::ScrollArea::both()
            .id_salt("restext")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(super::highlight::job(&text, egui::FontId::monospace(13.0)));
            });
        return;
    }

    let data = doc.ne.resource_data(r).to_vec();
    super::hex::dump(ui, act, &data, None, None);
}

fn toolbar(ui: &mut Ui, act: &mut Vec<Action>, r: &ne_core::resource::Resource, index: usize) {
    ui.horizontal(|ui| {
        widgets::strip_item(ui, |ui| {
            icons::inline_sized(ui, icons::for_resource(r.type_id.as_id()), col::orange(), 16.0);
            ui.label(
                egui::RichText::new(format!("{} {}", r.type_name(), r.res_id))
                    .size(14.0)
                    .strong()
                    .color(col::text()),
            );
            for f in r.flag_names() {
                widgets::chip(ui, f, col::dim());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::strip_item(ui, |ui| {
                if widgets::toggle_chip(ui, false, "Export raw", col::dim()) {
                    act.push(Action::SaveResource { index, raw: true });
                }
                if widgets::toggle_chip(ui, false, "Export", col::accent()) {
                    act.push(Action::SaveResource { index, raw: false });
                }
            });
        });
    });
}

/// The picture, with the controls for looking at it.
fn image_pane(app: &SithApp, ui: &mut Ui, index: usize, img: &ne_core::dib::Image) {
    if app.zoom_index.get() != Some(index) {
        app.zoom_index.set(Some(index));
        // Aim for roughly a 260px preview: a 32x32 icon comes up at 8x, a
        // full sprite sheet stays at 1:1.
        let longest = img.width.max(img.height) as f32;
        app.image_zoom.set((260.0 / longest).clamp(1.0, 8.0).round());
    }

    let key = (app.doc_index(), index);
    let mut cache = app.textures.borrow_mut();
    let tex = cache.entry(key).or_insert_with(|| {
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
        // Nearest-neighbour keeps 16-color pixel art legible when zoomed.
        ui.ctx().load_texture(
            format!("res{}-{index}", key.0),
            color,
            egui::TextureOptions::NEAREST,
        )
    });
    let size = tex.size_vec2();
    let id = tex.id();
    drop(cache);

    let zoom = app.image_zoom.get();
    ui.horizontal(|ui| {
        widgets::strip_item(ui, |ui| {
            ui.label(mono_c(format!("{} x {}", img.width, img.height), col::dim()));
        });
        ui.add_space(10.0);
        // Fixed steps rather than a slider: at this size the useful zooms are
        // powers of two, and a slider that can land on 7x is a slider that
        // makes pixel art blurry for no reason.
        let steps: [(u32, &str); 6] = [
            (1, "1x"),
            (2, "2x"),
            (4, "4x"),
            (8, "8x"),
            (12, "12x"),
            (16, "16x"),
        ];
        if let Some(z) = widgets::segmented(ui, zoom as u32, &steps) {
            app.image_zoom.set(z as f32);
        }
    });
    ui.add_space(6.0);

    egui::Frame::new()
        .fill(col::bg())
        .corner_radius(egui::CornerRadius::same(5))
        .stroke(egui::Stroke::new(1.0, col::border()))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("respreview")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // A checkerboard behind the image so transparent icon
                    // pixels read as transparent rather than as black.
                    let draw = size * zoom;
                    let (rect, _) = ui.allocate_exact_size(draw, egui::Sense::hover());
                    checkerboard(ui, rect);
                    ui.painter().image(
                        id,
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                });
        });
}

fn checkerboard(ui: &Ui, rect: egui::Rect) {
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::from_gray(30));
    let s = 8.0;
    let cols = (rect.width() / s).ceil() as i32;
    let rows = (rect.height() / s).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            if (x + y) % 2 == 0 {
                continue;
            }
            let cell = egui::Rect::from_min_size(
                rect.min + egui::vec2(x as f32 * s, y as f32 * s),
                egui::vec2(s, s),
            )
            .intersect(rect);
            p.rect_filled(cell, egui::CornerRadius::ZERO, egui::Color32::from_gray(38));
        }
    }
}
