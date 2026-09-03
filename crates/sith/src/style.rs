//! Minimal ANSI styling. Color is a real aid when scanning a fixup map or a
//! segment table, but not worth a dependency, and it must switch itself off
//! when the output is a pipe.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn parse(s: &str) -> Option<ColorChoice> {
        match s {
            "auto" => Some(ColorChoice::Auto),
            "always" => Some(ColorChoice::Always),
            "never" => Some(ColorChoice::Never),
            _ => None,
        }
    }
}

pub fn init(choice: ColorChoice) {
    let on = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        // NO_COLOR is honoured by convention; any non-empty value disables.
        ColorChoice::Auto => {
            std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    };
    ENABLED.store(on, Ordering::Relaxed);
}

fn wrap(code: &str, s: &str) -> String {
    if ENABLED.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    wrap("1", s)
}
pub fn dim(s: &str) -> String {
    wrap("2", s)
}
pub fn red(s: &str) -> String {
    wrap("31", s)
}
pub fn green(s: &str) -> String {
    wrap("32", s)
}
pub fn yellow(s: &str) -> String {
    wrap("33", s)
}
pub fn blue(s: &str) -> String {
    wrap("34", s)
}
pub fn magenta(s: &str) -> String {
    wrap("35", s)
}
pub fn cyan(s: &str) -> String {
    wrap("36", s)
}

/// Section heading, printed before each block of a multi-part report.
pub fn heading(s: &str) -> String {
    bold(&format!("== {s}"))
}
