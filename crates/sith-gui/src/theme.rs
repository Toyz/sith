//! Palette and style.
//!
//! The colours are a dark, high-contrast set chosen so that the three things
//! a reader scans for in a listing -- control flow, resolved symbols, and
//! addresses -- are separable at a glance without the page turning into
//! confetti.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x0D, 0x11, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x13, 0x18, 0x21);
pub const RAISED: Color32 = Color32::from_rgb(0x1B, 0x22, 0x2D);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x33, 0x40);
// Premultiplied: the colour channels must already be scaled by the alpha, or
// the tint renders as a near-white wash instead of a faint band.
pub const STRIPE: Color32 = Color32::from_rgba_premultiplied(0x07, 0x07, 0x07, 0x0A);
pub const HOVER: Color32 = Color32::from_rgba_premultiplied(0x0A, 0x13, 0x1D, 0x1D);
pub const SELECTED: Color32 = Color32::from_rgba_premultiplied(0x18, 0x2D, 0x45, 0x45);

pub const TEXT: Color32 = Color32::from_rgb(0xC9, 0xD1, 0xD9);
pub const DIM: Color32 = Color32::from_rgb(0x7D, 0x8A, 0x99);
pub const FAINT: Color32 = Color32::from_rgb(0x56, 0x62, 0x70);

pub const ACCENT: Color32 = Color32::from_rgb(0x58, 0xA6, 0xFF);
pub const GREEN: Color32 = Color32::from_rgb(0x7E, 0xE7, 0x87);
pub const ORANGE: Color32 = Color32::from_rgb(0xFF, 0xA6, 0x57);
pub const PURPLE: Color32 = Color32::from_rgb(0xD2, 0xA8, 0xFF);
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x7B, 0x72);
pub const CYAN: Color32 = Color32::from_rgb(0x79, 0xC0, 0xFF);
pub const YELLOW: Color32 = Color32::from_rgb(0xE3, 0xB3, 0x41);

/// Semantic aliases, so a view says what it means rather than naming a colour.
pub const ADDR: Color32 = FAINT;
pub const BYTES: Color32 = Color32::from_rgb(0x63, 0x70, 0x7E);
pub const MNEMONIC: Color32 = TEXT;
pub const COMMENT: Color32 = Color32::from_rgb(0x6E, 0x9E, 0xB0);
pub const SYMBOL: Color32 = YELLOW;
pub const CODE_SEG: Color32 = GREEN;
pub const DATA_SEG: Color32 = CYAN;

pub fn flow_color(flow: ne_disasm::Flow) -> Color32 {
    use ne_disasm::Flow as F;
    match flow {
        F::Call | F::CallFar | F::CallIndirect => GREEN,
        F::Jump | F::JumpFar | F::JumpIndirect | F::CondJump => CYAN,
        F::Return => PURPLE,
        F::Interrupt => ORANGE,
        F::Invalid => RED,
        F::Next => TEXT,
    }
}

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();

    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    let v = &mut style.visuals;
    v.dark_mode = true;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = STRIPE;
    v.override_text_color = Some(TEXT);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = SELECTED;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x25, 0x2E, 0x3B);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x25, 0x2E, 0x3B);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x2E, 0x39, 0x49);

    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.scroll.bar_width = 10.0;

    // The viewer's theme can change under us, so both slots get the same
    // style: this tool is dark-only by design, like every other disassembler.
    let style = std::sync::Arc::new(style);
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
    ctx.set_theme(egui::Theme::Dark);
}

pub fn mono_c(s: impl Into<String>, c: Color32) -> RichText {
    RichText::new(s).monospace().color(c)
}

pub fn dim(s: impl Into<String>) -> RichText {
    RichText::new(s).color(DIM)
}
