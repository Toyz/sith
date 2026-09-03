//! Palettes and style.
//!
//! Colour is load-bearing in a disassembly listing: the three things a reader
//! scans for -- control flow, resolved symbols, and addresses -- have to be
//! separable at a glance without the page turning into confetti. Every theme
//! here fills the same set of roles, so switching one changes the look without
//! changing what the colours mean.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle};
use std::sync::RwLock;

/// The roles a theme must fill. Views name roles, never raw colours.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub name: &'static str,
    pub dark: bool,
    /// Deepest surface: the listing background.
    pub bg: Color32,
    /// Panels either side of the listing.
    pub panel: Color32,
    /// Raised chrome: toolbar, tabs, cards.
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub green: Color32,
    pub orange: Color32,
    pub purple: Color32,
    pub red: Color32,
    pub cyan: Color32,
    pub yellow: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

pub const THEMES: &[Palette] = &[
    Palette {
        name: "Catppuccin Mocha",
        dark: true,
        bg: rgb(0x1E1E2E),
        panel: rgb(0x181825),
        raised: rgb(0x313244),
        border: rgb(0x45475A),
        text: rgb(0xCDD6F4),
        dim: rgb(0xA6ADC8),
        faint: rgb(0x7F849C),
        accent: rgb(0x89B4FA),
        green: rgb(0xA6E3A1),
        orange: rgb(0xFAB387),
        purple: rgb(0xCBA6F7),
        red: rgb(0xF38BA8),
        cyan: rgb(0x89DCEB),
        yellow: rgb(0xF9E2AF),
    },
    Palette {
        name: "Catppuccin Macchiato",
        dark: true,
        bg: rgb(0x24273A),
        panel: rgb(0x1E2030),
        raised: rgb(0x363A4F),
        border: rgb(0x494D64),
        text: rgb(0xCAD3F5),
        dim: rgb(0xA5ADCB),
        faint: rgb(0x8087A2),
        accent: rgb(0x8AADF4),
        green: rgb(0xA6DA95),
        orange: rgb(0xF5A97F),
        purple: rgb(0xC6A0F6),
        red: rgb(0xED8796),
        cyan: rgb(0x91D7E3),
        yellow: rgb(0xEED49F),
    },
    Palette {
        name: "Catppuccin Latte",
        dark: false,
        bg: rgb(0xEFF1F5),
        panel: rgb(0xE6E9EF),
        raised: rgb(0xDCE0E8),
        border: rgb(0xBCC0CC),
        text: rgb(0x4C4F69),
        dim: rgb(0x6C6F85),
        faint: rgb(0x8C8FA1),
        accent: rgb(0x1E66F5),
        green: rgb(0x40A02B),
        orange: rgb(0xFE640B),
        purple: rgb(0x8839EF),
        red: rgb(0xD20F39),
        cyan: rgb(0x179299),
        yellow: rgb(0xDF8E1D),
    },
    Palette {
        name: "Nord",
        dark: true,
        bg: rgb(0x2E3440),
        panel: rgb(0x272B35),
        raised: rgb(0x3B4252),
        border: rgb(0x4C566A),
        text: rgb(0xECEFF4),
        dim: rgb(0xD8DEE9),
        faint: rgb(0x7B879D),
        accent: rgb(0x88C0D0),
        green: rgb(0xA3BE8C),
        orange: rgb(0xD08770),
        purple: rgb(0xB48EAD),
        red: rgb(0xBF616A),
        cyan: rgb(0x8FBCBB),
        yellow: rgb(0xEBCB8B),
    },
    Palette {
        name: "Tokyo Night",
        dark: true,
        bg: rgb(0x1A1B26),
        panel: rgb(0x16161E),
        raised: rgb(0x292E42),
        border: rgb(0x3B4261),
        text: rgb(0xC0CAF5),
        dim: rgb(0x9AA5CE),
        faint: rgb(0x565F89),
        accent: rgb(0x7AA2F7),
        green: rgb(0x9ECE6A),
        orange: rgb(0xFF9E64),
        purple: rgb(0xBB9AF7),
        red: rgb(0xF7768E),
        cyan: rgb(0x7DCFFF),
        yellow: rgb(0xE0AF68),
    },
    Palette {
        name: "Gruvbox Dark",
        dark: true,
        bg: rgb(0x282828),
        panel: rgb(0x1D2021),
        raised: rgb(0x3C3836),
        border: rgb(0x504945),
        text: rgb(0xEBDBB2),
        dim: rgb(0xBDAE93),
        faint: rgb(0x928374),
        accent: rgb(0x83A598),
        green: rgb(0xB8BB26),
        orange: rgb(0xFE8019),
        purple: rgb(0xD3869B),
        red: rgb(0xFB4934),
        cyan: rgb(0x8EC07C),
        yellow: rgb(0xFABD2F),
    },
    Palette {
        name: "Midnight",
        dark: true,
        bg: rgb(0x0D1117),
        panel: rgb(0x131821),
        raised: rgb(0x1B222D),
        border: rgb(0x2A3340),
        text: rgb(0xC9D1D9),
        dim: rgb(0x7D8A99),
        faint: rgb(0x566270),
        accent: rgb(0x58A6FF),
        green: rgb(0x7EE787),
        orange: rgb(0xFFA657),
        purple: rgb(0xD2A8FF),
        red: rgb(0xFF7B72),
        cyan: rgb(0x79C0FF),
        yellow: rgb(0xE3B341),
    },
];

pub const DEFAULT_THEME: &str = "Catppuccin Mocha";

static CURRENT: RwLock<Palette> = RwLock::new(THEMES[0]);

pub fn current() -> Palette {
    *CURRENT.read().unwrap()
}

/// Select a theme by name. Unknown names leave the current one in place, so a
/// settings file written by a newer build cannot leave the window unreadable.
pub fn set_theme(name: &str) -> bool {
    match THEMES.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
        Some(p) => {
            *CURRENT.write().unwrap() = *p;
            true
        }
        None => false,
    }
}

/// Colour roles, resolved against the current theme.
///
/// These are functions rather than constants because the theme changes at
/// runtime; naming the role rather than the colour is what keeps every view
/// theme-agnostic.
pub mod col {
    use super::{current, Color32};

    pub fn bg() -> Color32 {
        current().bg
    }
    pub fn panel() -> Color32 {
        current().panel
    }
    pub fn raised() -> Color32 {
        current().raised
    }
    pub fn border() -> Color32 {
        current().border
    }
    pub fn text() -> Color32 {
        current().text
    }
    pub fn dim() -> Color32 {
        current().dim
    }
    pub fn faint() -> Color32 {
        current().faint
    }
    pub fn accent() -> Color32 {
        current().accent
    }
    pub fn green() -> Color32 {
        current().green
    }
    pub fn orange() -> Color32 {
        current().orange
    }
    pub fn purple() -> Color32 {
        current().purple
    }
    pub fn red() -> Color32 {
        current().red
    }
    pub fn cyan() -> Color32 {
        current().cyan
    }
    pub fn yellow() -> Color32 {
        current().yellow
    }

    /// Alternating row tint. Premultiplied: the channels must already be
    /// scaled by the alpha or the tint renders as a wash rather than a band.
    pub fn stripe() -> Color32 {
        let p = current();
        if p.dark {
            Color32::from_rgba_premultiplied(0x07, 0x07, 0x07, 0x0A)
        } else {
            Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0A)
        }
    }

    pub fn hover() -> Color32 {
        tint(current().accent, 0.12)
    }

    pub fn selected() -> Color32 {
        tint(current().accent, 0.28)
    }

    fn tint(c: Color32, alpha: f32) -> Color32 {
        let a = (alpha * 255.0) as u8;
        Color32::from_rgba_premultiplied(
            (c.r() as f32 * alpha) as u8,
            (c.g() as f32 * alpha) as u8,
            (c.b() as f32 * alpha) as u8,
            a,
        )
    }

    // Semantic aliases, so a view says what it means rather than naming a hue.
    pub fn addr() -> Color32 {
        faint()
    }
    pub fn bytes() -> Color32 {
        current().faint.gamma_multiply(0.85)
    }
    pub fn mnemonic() -> Color32 {
        text()
    }
    pub fn comment() -> Color32 {
        current().cyan.gamma_multiply(0.85)
    }
    pub fn symbol() -> Color32 {
        yellow()
    }
    pub fn code_seg() -> Color32 {
        green()
    }
    pub fn data_seg() -> Color32 {
        cyan()
    }
}

pub fn flow_color(flow: ne_disasm::Flow) -> Color32 {
    use ne_disasm::Flow as F;
    match flow {
        F::Call | F::CallFar | F::CallIndirect => col::green(),
        F::Jump | F::JumpFar | F::JumpIndirect | F::CondJump => col::cyan(),
        F::Return => col::purple(),
        F::Interrupt => col::orange(),
        F::Invalid => col::red(),
        F::Next => col::text(),
    }
}

/// Apply the current palette to egui's style.
pub fn install(ctx: &egui::Context) {
    let p = current();
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
    v.dark_mode = p.dark;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = col::stripe();
    v.override_text_color = Some(p.text);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = col::selected();
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.weak_bg_fill = p.raised;
    v.widgets.inactive.bg_fill = p.raised;
    let lift = if p.dark { 1.18 } else { 0.94 };
    v.widgets.hovered.weak_bg_fill = p.raised.gamma_multiply(lift);
    v.widgets.hovered.bg_fill = p.raised.gamma_multiply(lift);
    v.widgets.active.weak_bg_fill = p.raised.gamma_multiply(lift * lift);

    // Labels are not documents. egui makes every label selectable text by
    // default, which puts an I-beam over chips, headings and table cells and
    // makes them look editable. Copying is offered explicitly instead, through
    // context menus and Copy buttons.
    style.interaction.selectable_labels = false;

    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    // Tooltips are cards in this tool, not sentences, so they need room and
    // the same frame as everything else.
    style.spacing.tooltip_width = 340.0;
    style.spacing.scroll.bar_width = 10.0;

    // The tool is themed by its own palette, so both of egui's slots get the
    // same style: a viewer switching their desktop theme should not repaint
    // half the window.
    let style = std::sync::Arc::new(style);
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
    ctx.set_theme(if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
}

pub fn mono_c(s: impl Into<String>, c: Color32) -> RichText {
    RichText::new(s).monospace().color(c)
}

pub fn dim(s: impl Into<String>) -> RichText {
    RichText::new(s).color(col::dim())
}
