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

    ui.horizontal(|ui| {
        icons::inline(ui, icons::for_resource(r.type_id.as_id()), col::orange());
        ui.label(
            egui::RichText::new(format!("{} {}", r.type_name(), r.res_id))
                .size(15.0)
                .strong(),
        );
        for f in r.flag_names() {
            widgets::chip(ui, f, col::dim());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Export…")
                .on_hover_text("Save as a real .bmp / .ico / .cur file")
                .clicked()
            {
                act.push(Action::SaveResource { index, raw: false });
            }
            if ui
                .button("Export raw…")
                .on_hover_text("Save the resource body exactly as stored")
                .clicked()
            {
                act.push(Action::SaveResource { index, raw: true });
            }
        });
    });
    crate::ui::sep(ui);

    if let Some(img) = doc.ne.resource_image(r) {
        if app.zoom_index.get() != Some(index) {
            app.zoom_index.set(Some(index));
            // Aim for roughly a 260px preview: a 32x32 icon comes up at 8x,
            // a full sprite sheet stays at 1:1.
            let longest = img.width.max(img.height) as f32;
            app.image_zoom.set((260.0 / longest).clamp(1.0, 8.0).round());
        }

        let mut cache = app.textures.borrow_mut();
        let tex = cache.entry(index).or_insert_with(|| {
            let color =
                egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
            // Nearest-neighbour keeps 16-color pixel art legible when zoomed.
            ui.ctx()
                .load_texture(format!("res{index}"), color, egui::TextureOptions::NEAREST)
        });
        let size = tex.size_vec2();

        let mut zoom = app.image_zoom.get();
        ui.horizontal(|ui| {
            ui.label(mono_c(format!("{} x {}", img.width, img.height), col::dim()));
            if ui
                .add(egui::Slider::new(&mut zoom, 1.0..=16.0).integer().text("zoom"))
                .changed()
            {
                app.image_zoom.set(zoom);
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::both()
            .id_salt("respreview")
            .max_height(ui.available_height() * 0.62)
            .show(ui, |ui| {
                // A checkerboard behind the image so transparent icon pixels
                // read as transparent rather than as black.
                let draw = size * zoom;
                let (rect, _) = ui.allocate_exact_size(draw, egui::Sense::hover());
                checkerboard(ui, rect);
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            });
        drop(cache);
        crate::ui::sep(ui);
    }

    references(app, ui, act, index);

    if let Some(text) = render::resource_text(&doc.ne, r) {
        egui::ScrollArea::both()
            .id_salt("restext")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(super::highlight::job(
                    &text,
                    egui::FontId::monospace(13.0),
                ));
            });
        return;
    }

    let data = doc.ne.resource_data(r).to_vec();
    super::hex::dump(ui, act, &data, None, None);
}

/// The code that loads this resource.
///
/// This is the half of the link that is hard to get any other way: a bitmap
/// tells you nothing about which function draws it.
fn references(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, index: usize) {
    let Some(doc) = app.doc() else { return };
    let uses = doc.res_links.uses(index);
    if uses.is_empty() {
        return;
    }
    widgets::section(ui, &format!("LOADED BY ({})", uses.len()));
    for u in uses {
        let owner = doc
            .program
            .function_containing(u.addr)
            .map(|f| app.label(f))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add_space(2.0);
            if widgets::link(ui, u.addr.to_string(), col::accent()).clicked() {
                act.push(Action::Goto(u.addr));
            }
            ui.label(mono_c(format!("{:<22}", owner), col::symbol()));
            widgets::chip(
                ui,
                &u.api,
                match u.confidence {
                    ne_analysis::resrefs::Confidence::Id => col::green(),
                    ne_analysis::resrefs::Confidence::Name => col::faint(),
                },
            );
        });
    }
    crate::ui::sep(ui);
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
