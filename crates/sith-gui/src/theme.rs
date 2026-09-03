//! Palettes and style.
//!
//! Color is load-bearing in a disassembly listing: the three things a reader
//! scans for -- control flow, resolved symbols, and addresses -- have to be
//! separable at a glance without the page turning into confetti. Every theme
//! fills the same set of roles, so switching one changes the look without
//! changing what the colors mean.
//!
//! A theme is data. The built-in ones are the same shape as anything a user
//! writes, so the editor can start from one, and a saved theme is a small JSON
//! file in the config directory that can be copied between machines or shared.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle};
use std::path::PathBuf;
use std::sync::RwLock;

/// The color roles. Views name roles, never a hue.
///
/// `Copy` on purpose: every one of these is read many times a frame, so the
/// lookup has to be a read of a small value and not a clone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
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

/// A named palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub colors: Colors,
    /// Built-in themes cannot be deleted or overwritten in place; editing one
    /// makes a copy.
    pub builtin: bool,
}

/// The editable roles, grouped the way the editor presents them.
///
/// Grouping is not decoration: the surfaces have to work against each other,
/// the text shades against the surfaces, and the accents against everything.
/// Editing them in those three groups is how the decisions actually get made.
pub const ROLE_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "SURFACES",
        &[
            ("bg", "the listing itself"),
            ("panel", "navigator and inspector"),
            ("raised", "toolbars, tabs, cards"),
            ("border", "outlines and separators"),
        ],
    ),
    (
        "TEXT",
        &[
            ("text", "instructions, values, names"),
            ("dim", "secondary text and labels"),
            ("faint", "captions, addresses, bytes"),
        ],
    ),
    (
        "MEANING",
        &[
            ("accent", "selection, links, the graph root"),
            ("green", "calls, code segments, exports"),
            ("cyan", "jumps, data segments, your names"),
            ("purple", "returns, libraries"),
            ("orange", "interrupts, warnings, bookmarks"),
            ("red", "invalid instructions, errors"),
            ("yellow", "symbols and your notes"),
        ],
    ),
];

/// Every role, flattened.
pub const ROLES: &[(&str, &str)] = &[
    ("bg", "the listing itself"),
    ("panel", "navigator and inspector"),
    ("raised", "toolbars, tabs, cards"),
    ("border", "outlines and separators"),
    ("text", "instructions, values, names"),
    ("dim", "secondary text and labels"),
    ("faint", "captions, addresses, bytes"),
    ("accent", "selection, links, the graph root"),
    ("green", "calls, code segments, exports"),
    ("cyan", "jumps, data segments, your names"),
    ("purple", "returns, libraries"),
    ("orange", "interrupts, warnings, bookmarks"),
    ("red", "invalid instructions, errors"),
    ("yellow", "symbols and your notes"),
];

/// WCAG relative luminance.
fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// Contrast ratio between two colors, 1.0 (identical) to 21.0 (black on white).
///
/// Worth checking in a tool whose whole job is reading dense text: a palette
/// that looks pleasant as a row of swatches can be unreadable as a listing.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

impl Colors {
    pub fn role(&self, name: &str) -> Color32 {
        match name {
            "bg" => self.bg,
            "panel" => self.panel,
            "raised" => self.raised,
            "border" => self.border,
            "text" => self.text,
            "dim" => self.dim,
            "faint" => self.faint,
            "accent" => self.accent,
            "green" => self.green,
            "orange" => self.orange,
            "purple" => self.purple,
            "red" => self.red,
            "cyan" => self.cyan,
            "yellow" => self.yellow,
            _ => self.text,
        }
    }

    pub fn set_role(&mut self, name: &str, value: Color32) {
        match name {
            "bg" => self.bg = value,
            "panel" => self.panel = value,
            "raised" => self.raised = value,
            "border" => self.border = value,
            "text" => self.text = value,
            "dim" => self.dim = value,
            "faint" => self.faint = value,
            "accent" => self.accent = value,
            "green" => self.green = value,
            "orange" => self.orange = value,
            "purple" => self.purple = value,
            "red" => self.red = value,
            "cyan" => self.cyan = value,
            "yellow" => self.yellow = value,
            _ => {}
        }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

pub fn hex_of(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Parse `#RRGGBB`, with or without the hash.
pub fn color_of(hex: &str) -> Option<Color32> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(rgb(v))
}

pub const DEFAULT_THEME: &str = "Catppuccin Mocha";

/// Colors a user can assign to an address, by name.
///
/// A short list on purpose: enough to separate subsystems at a glance, few
/// enough that each stays distinguishable from the next.
pub const USER_COLORS: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "grey",
];

/// Resolve a stored color name against the active theme.
pub fn named_color(name: &str) -> Option<Color32> {
    let p = current();
    Some(match name {
        "red" => p.red,
        "orange" => p.orange,
        "yellow" => p.yellow,
        "green" => p.green,
        "cyan" => p.cyan,
        "blue" => p.accent,
        "purple" => p.purple,
        "grey" | "gray" => p.dim,
        _ => return None,
    })
}

fn builtin(name: &str, dark: bool, v: [u32; 14]) -> Theme {
    Theme {
        name: name.to_string(),
        builtin: true,
        colors: Colors {
            dark,
            bg: rgb(v[0]),
            panel: rgb(v[1]),
            raised: rgb(v[2]),
            border: rgb(v[3]),
            text: rgb(v[4]),
            dim: rgb(v[5]),
            faint: rgb(v[6]),
            accent: rgb(v[7]),
            green: rgb(v[8]),
            orange: rgb(v[9]),
            purple: rgb(v[10]),
            red: rgb(v[11]),
            cyan: rgb(v[12]),
            yellow: rgb(v[13]),
        },
    }
}

pub fn builtins() -> Vec<Theme> {
    vec![
        builtin("Catppuccin Mocha", true, [
            0x1E1E2E, 0x181825, 0x313244, 0x45475A, 0xCDD6F4, 0xA6ADC8, 0x7F849C,
            0x89B4FA, 0xA6E3A1, 0xFAB387, 0xCBA6F7, 0xF38BA8, 0x89DCEB, 0xF9E2AF,
        ]),
        builtin("Catppuccin Macchiato", true, [
            0x24273A, 0x1E2030, 0x363A4F, 0x494D64, 0xCAD3F5, 0xA5ADCB, 0x8087A2,
            0x8AADF4, 0xA6DA95, 0xF5A97F, 0xC6A0F6, 0xED8796, 0x91D7E3, 0xEED49F,
        ]),
        builtin("Catppuccin Latte", false, [
            0xEFF1F5, 0xE6E9EF, 0xDCE0E8, 0xBCC0CC, 0x4C4F69, 0x6C6F85, 0x8C8FA1,
            0x1E66F5, 0x40A02B, 0xFE640B, 0x8839EF, 0xD20F39, 0x179299, 0xDF8E1D,
        ]),
        builtin("Nord", true, [
            0x2E3440, 0x272B35, 0x3B4252, 0x4C566A, 0xECEFF4, 0xD8DEE9, 0x7B879D,
            0x88C0D0, 0xA3BE8C, 0xD08770, 0xB48EAD, 0xBF616A, 0x8FBCBB, 0xEBCB8B,
        ]),
        builtin("Tokyo Night", true, [
            0x1A1B26, 0x16161E, 0x292E42, 0x3B4261, 0xC0CAF5, 0x9AA5CE, 0x565F89,
            0x7AA2F7, 0x9ECE6A, 0xFF9E64, 0xBB9AF7, 0xF7768E, 0x7DCFFF, 0xE0AF68,
        ]),
        builtin("Gruvbox Dark", true, [
            0x282828, 0x1D2021, 0x3C3836, 0x504945, 0xEBDBB2, 0xBDAE93, 0x928374,
            0x83A598, 0xB8BB26, 0xFE8019, 0xD3869B, 0xFB4934, 0x8EC07C, 0xFABD2F,
        ]),
        builtin("Midnight", true, [
            0x0D1117, 0x131821, 0x1B222D, 0x2A3340, 0xC9D1D9, 0x7D8A99, 0x566270,
            0x58A6FF, 0x7EE787, 0xFFA657, 0xD2A8FF, 0xFF7B72, 0x79C0FF, 0xE3B341,
        ]),
    ]
}

// The active colors are read constantly, so they live apart from the name and
// stay `Copy`.
static CURRENT: RwLock<Colors> = RwLock::new(Colors {
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
});
static CURRENT_NAME: RwLock<String> = RwLock::new(String::new());
static REGISTRY: RwLock<Vec<Theme>> = RwLock::new(Vec::new());

pub fn current() -> Colors {
    *CURRENT.read().unwrap()
}

pub fn current_name() -> String {
    CURRENT_NAME.read().unwrap().clone()
}

pub fn themes() -> Vec<Theme> {
    REGISTRY.read().unwrap().clone()
}

pub fn theme(name: &str) -> Option<Theme> {
    REGISTRY
        .read()
        .unwrap()
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .cloned()
}

/// Load the built-ins plus anything saved in the config directory.
pub fn load_registry() {
    let mut all = builtins();
    for t in load_custom() {
        // A saved theme with a built-in's name replaces it, which is how you
        // adjust one of the shipped palettes without losing the original file.
        if let Some(i) = all.iter().position(|b| b.name == t.name) {
            all[i] = t;
        } else {
            all.push(t);
        }
    }
    *REGISTRY.write().unwrap() = all;
}

/// Select a theme by name. An unknown name leaves the current one alone, so a
/// settings file written by a newer build cannot leave the window unreadable.
pub fn set_theme(name: &str) -> bool {
    match theme(name) {
        Some(t) => {
            *CURRENT.write().unwrap() = t.colors;
            *CURRENT_NAME.write().unwrap() = t.name;
            true
        }
        None => false,
    }
}

/// Apply colors without naming them, for a live preview while editing.
pub fn preview(colors: Colors) {
    *CURRENT.write().unwrap() = colors;
}

pub fn themes_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    Some(dir.join("sith").join("themes"))
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn to_json(t: &Theme) -> String {
    let mut lines = vec![
        format!("  \"name\": {}", serde_json::to_string(&t.name).unwrap_or_default()),
        format!("  \"dark\": {}", t.colors.dark),
    ];
    for (role, _) in ROLES {
        lines.push(format!("  \"{role}\": \"{}\"", hex_of(t.colors.role(role))));
    }
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

fn from_json(text: &str) -> Option<Theme> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let name = v.get("name")?.as_str()?.to_string();
    let mut colors = builtins()[0].colors;
    colors.dark = v.get("dark").and_then(|d| d.as_bool()).unwrap_or(true);
    for (role, _) in ROLES {
        if let Some(c) = v.get(*role).and_then(|s| s.as_str()).and_then(color_of) {
            colors.set_role(role, c);
        }
    }
    Some(Theme {
        name,
        colors,
        builtin: false,
    })
}

fn load_custom() -> Vec<Theme> {
    let Some(dir) = themes_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Theme> = entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("json"))
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| from_json(&t))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Write a theme to the config directory and put it in the registry.
pub fn save_theme(t: &Theme) -> std::io::Result<PathBuf> {
    let Some(dir) = themes_dir() else {
        return Err(std::io::Error::other("no config directory"));
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", slug(&t.name)));
    std::fs::write(&path, to_json(t))?;
    let mut saved = t.clone();
    saved.builtin = false;
    let mut reg = REGISTRY.write().unwrap();
    match reg.iter().position(|x| x.name == saved.name) {
        Some(i) => reg[i] = saved,
        None => reg.push(saved),
    }
    Ok(path)
}

/// Remove a saved theme. Built-ins come back on the next load.
pub fn delete_theme(name: &str) -> std::io::Result<()> {
    if let Some(dir) = themes_dir() {
        let path = dir.join(format!("{}.json", slug(name)));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    load_registry();
    Ok(())
}

/// Color roles, resolved against the current theme.
///
/// These are functions rather than constants because the theme changes at
/// runtime; naming the role rather than the color is what keeps every view
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
        // egui grows a widget by a pixel on hover, which nudges everything
        // beside it. In a toolbar of adjacent controls that reads as the row
        // twitching, so the growth is switched off and hover is shown by
        // color alone.
        w.expansion = 0.0;
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
