//! Syntax highlighting for decoded resources.
//!
//! Menus, dialogs, version blocks and font headers all decode to a small
//! resource-script-like language. Rendered as one flat color it reads as a
//! wall; picking out the keyword, the string and the number is most of what
//! makes a listing scannable, and it is the same three things every time.

use crate::theme::col;
use eframe::egui::{self, text::LayoutJob, Color32, TextFormat};

/// Words that begin a statement in the text the decoders produce.
const KEYWORDS: &[&str] = &[
    "MENU", "POPUP", "MENUITEM", "SEPARATOR", "DIALOG", "CAPTION", "FONT", "CLASS", "STYLE",
    "FILEVERSION", "PRODUCTVERSION", "FILEFLAGS", "FILEOS", "FACE", "DEVICE", "SIZE", "PITCH",
    "CHARSET", "RESOLUTION", "GLYPHS", "COPYRIGHT", "ACCELERATORS", "STRINGTABLE",
];

/// Predefined dialog control classes, which read as types rather than values.
const CLASSES: &[&str] = &[
    "BUTTON", "EDIT", "STATIC", "LISTBOX", "SCROLLBAR", "COMBOBOX",
];

pub fn job(text: &str, font: egui::FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for line in text.lines() {
        highlight_line(&mut job, line, &font);
        push(&mut job, "\n", col::text(), &font);
    }
    job
}

fn push(job: &mut LayoutJob, text: &str, color: Color32, font: &egui::FontId) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        },
    );
}

fn highlight_line(job: &mut LayoutJob, line: &str, font: &egui::FontId) {
    // A comment runs to the end of the line and outranks everything in it.
    if let Some(pos) = line.find(';') {
        let (head, comment) = line.split_at(pos);
        highlight_line(job, head, font);
        push(job, comment, col::faint(), font);
        return;
    }

    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let mut first_word = true;

    while i < bytes.len() {
        let c = bytes[i];

        // Whitespace and punctuation pass through untouched.
        if c.is_whitespace() {
            let start = i;
            while i < bytes.len() && bytes[i].is_whitespace() {
                i += 1;
            }
            push(job, &line_slice(&bytes, start, i), col::text(), font);
            continue;
        }

        // A quoted string, kept whole so an embedded space stays green.
        if c == '"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != '"' {
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            push(job, &line_slice(&bytes, start, i), col::green(), font);
            first_word = false;
            continue;
        }

        if c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '#' || c == '.' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == '_'
                    || bytes[i] == '@'
                    || bytes[i] == '#'
                    || bytes[i] == '.')
            {
                i += 1;
            }
            let word = line_slice(&bytes, start, i);
            push(job, &word, word_color(&word, first_word), font);
            first_word = false;
            continue;
        }

        push(job, &c.to_string(), col::faint(), font);
        i += 1;
    }
}

fn line_slice(chars: &[char], from: usize, to: usize) -> String {
    chars[from..to.min(chars.len())].iter().collect()
}

fn word_color(word: &str, first_word: bool) -> Color32 {
    if KEYWORDS.contains(&word) || (first_word && is_shouty(word)) {
        return col::purple();
    }
    if CLASSES.contains(&word) {
        return col::cyan();
    }
    // A number, in any of the shapes the decoders emit.
    let numeric = word.starts_with("0x")
        || word.starts_with("0X")
        || word.chars().all(|c| c.is_ascii_digit())
        || (word.len() >= 4 && word.chars().all(|c| c.is_ascii_hexdigit()));
    if numeric {
        return col::orange();
    }
    // A named constant: SHOUTY_SNAKE with at least one underscore or a known
    // Win16 prefix. Plain capitalised words are left alone so face names and
    // captions do not light up.
    if is_shouty(word) && (word.contains('_') || word.len() <= 4) {
        return col::yellow();
    }
    col::text()
}

fn is_shouty(word: &str) -> bool {
    word.len() > 1
        && word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && word.chars().any(|c| c.is_ascii_uppercase())
}
