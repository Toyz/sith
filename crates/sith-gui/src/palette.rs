//! The command palette: one keystroke to anything in the binary.
//!
//! The candidate list is built once and consulted by both the dialog and the
//! action handler, so what the dialog shows and what Enter does can never drift
//! apart -- the earlier version rebuilt the list twice and indices had to
//! agree by luck.
//!
//! Matching is subsequence-based with a small ranking model: a prefix beats a
//! word-boundary hit, which beats a scattered match, and shorter names win
//! ties. That is what makes `mwp` find `MAINWNDPROC` and `gsb` find
//! `GetStockObject`'s call sites.

use crate::icons::Icon;
use crate::state::{Action, Doc, Nav, SithApp};
use eframe::egui::Color32;

/// What kind of thing a candidate is, which decides its icon and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Function,
    Export,
    Segment,
    Resource,
    Import,
    String,
    Module,
}

impl Kind {
    pub fn icon(self) -> Icon {
        match self {
            Kind::Command => Icon::Target,
            Kind::Function | Kind::Export => Icon::Code,
            Kind::Segment => Icon::Segment,
            Kind::Resource => Icon::Resource,
            Kind::Import => Icon::Import,
            Kind::String => Icon::Strings,
            Kind::Module => Icon::Module,
        }
    }

    pub fn color(self) -> Color32 {
        use crate::theme::col;
        match self {
            Kind::Command => col::accent(),
            Kind::Function => col::text(),
            Kind::Export => col::symbol(),
            Kind::Segment => col::code_seg(),
            Kind::Resource => col::orange(),
            Kind::Import => col::comment(),
            Kind::String => col::green(),
            Kind::Module => col::purple(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Command => "command",
            Kind::Function => "function",
            Kind::Export => "export",
            Kind::Segment => "segment",
            Kind::Resource => "resource",
            Kind::Import => "import",
            Kind::String => "string",
            Kind::Module => "module",
        }
    }
}

pub struct Candidate {
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    /// Byte positions in `title` that the query matched, for highlighting.
    pub hits: Vec<usize>,
    pub score: i64,
    pub action: Action,
}

/// A leading sigil restricts the search to one kind, the way a real palette
/// does: `>` commands, `@` functions, `#` strings, `:` an address.
fn scope(query: &str) -> (Option<Kind>, &str) {
    let mut chars = query.chars();
    match chars.next() {
        Some('>') => (Some(Kind::Command), chars.as_str()),
        Some('@') => (Some(Kind::Function), chars.as_str()),
        Some('#') => (Some(Kind::String), chars.as_str()),
        Some('$') => (Some(Kind::Resource), chars.as_str()),
        _ => (None, query),
    }
}

/// The one-line hint shown under the input.
pub fn hint() -> &'static str {
    ">  commands     @  functions     #  strings     $  resources     seg02:1A40  address"
}

/// Build the ranked candidate list for `query`.
pub fn candidates(app: &SithApp, query: &str) -> Vec<Candidate> {
    let (only, needle) = scope(query);
    let needle = needle.trim();
    let mut out: Vec<Candidate> = Vec::new();

    // An address typed directly is offered first: it is unambiguous.
    if only.is_none() {
        if let Some(addr) = app.resolve(needle) {
            if needle.contains(':') || needle.chars().all(|c| c.is_ascii_hexdigit()) {
                out.push(Candidate {
                    kind: Kind::Command,
                    title: format!("Go to {addr}"),
                    detail: "address".into(),
                    hits: Vec::new(),
                    score: i64::MAX,
                    action: Action::Goto(addr),
                });
            }
        }
    }

    let push = |out: &mut Vec<Candidate>,
                    kind: Kind,
                    title: String,
                    detail: String,
                    action: Action| {
        if only.is_some_and(|k| k != kind) {
            return;
        }
        match score(&title, needle) {
            Some((score, hits)) => out.push(Candidate {
                kind,
                title,
                detail,
                hits,
                score,
                action,
            }),
            None => {}
        }
    };

    for (title, detail, nav) in commands(app) {
        push(&mut out, Kind::Command, title, detail, Action::Go(nav));
    }
    if app.doc().is_some() {
        for (title, detail, action) in extra_commands() {
            push(&mut out, Kind::Command, title, detail, action);
        }
    }

    let Some(doc) = app.doc() else {
        out.sort_by(|a, b| b.score.cmp(&a.score));
        return out;
    };

    for f in &doc.program.functions {
        let kind = if f.name.is_some() {
            Kind::Export
        } else {
            Kind::Function
        };
        push(
            &mut out,
            kind,
            app.label(f),
            format!("{}  {} bytes", f.addr, f.size()),
            Action::Goto(f.addr),
        );
    }

    for s in &doc.ne.segments {
        push(
            &mut out,
            Kind::Segment,
            format!("Segment {}", s.index),
            format!("{}  {} bytes", s.kind().as_str(), s.length),
            Action::Go(Nav::Segment(s.index)),
        );
    }

    for (i, r) in doc.ne.resources.iter().enumerate() {
        push(
            &mut out,
            Kind::Resource,
            format!("{} {}", r.type_name(), r.res_id),
            format!("{} bytes", r.length),
            Action::Go(Nav::Resource(i)),
        );
    }

    for (target, sites) in &doc.program.xrefs {
        if target.contains('.') {
            push(
                &mut out,
                Kind::Import,
                target.clone(),
                format!("{} call sites", sites.len()),
                Action::Go(Nav::Xrefs(target.clone())),
            );
        }
    }

    // Strings are only searched once there is something to search for: every
    // string in the binary would otherwise swamp the list.
    if !needle.is_empty() {
        strings(doc, app.min_string_len, |seg, off, text, refs| {
            push(
                &mut out,
                Kind::String,
                text,
                format!("seg{seg:02}:{off:04X}  {refs} refs"),
                Action::Goto(ne_analysis::Addr {
                    segment: seg,
                    offset: off,
                }),
            );
        });
    }

    for m in app.index.modules() {
        push(
            &mut out,
            Kind::Module,
            m.module.clone(),
            format!("{} exports  {}", m.exports.len(), m.path.display()),
            Action::OpenModule {
                module: m.module.clone(),
                ordinal: None,
                name: None,
            },
        );
    }

    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.title.len().cmp(&b.title.len()))
            .then(a.title.cmp(&b.title))
    });
    out.truncate(300);
    out
}

fn strings(doc: &Doc, min_len: usize, mut f: impl FnMut(u16, u32, String, usize)) {
    for seg in &doc.ne.segments {
        if seg.data.is_empty() {
            continue;
        }
        for s in ne_core::strings::scan(&seg.data, min_len) {
            let refs = doc.program.data_refs(s.offset).len();
            f(seg.index, s.offset, s.text, refs);
        }
    }
}

fn commands(app: &SithApp) -> Vec<(String, String, Nav)> {
    let doc = app.doc();
    vec![
        ("Overview".into(), "module summary".into(), Nav::Overview),
        (
            "Imports".into(),
            doc.map(|d| format!("{} modules", d.ne.module_ref_names().len()))
                .unwrap_or_default(),
            Nav::Imports,
        ),
        (
            "Exports".into(),
            doc.map(|d| format!("{} entry points", d.ne.exports().len()))
                .unwrap_or_default(),
            Nav::Exports,
        ),
        ("Entry table".into(), "every ordinal slot".into(), Nav::Entries),
        ("Strings".into(), "with code references".into(), Nav::Strings),
        ("Call graph".into(), "callers and callees".into(), Nav::Graph),
        (
            "Cross-references".into(),
            "what calls what".into(),
            Nav::Xrefs(String::new()),
        ),
    ]
}

fn extra_commands() -> Vec<(String, String, Action)> {
    vec![
        (
            "Reload file".into(),
            "re-read from disk".into(),
            Action::Reload,
        ),
        (
            "Save listing".into(),
            "write the current disassembly".into(),
            Action::SaveListing,
        ),
        (
            "Toggle navigator".into(),
            "show or hide the left panel".into(),
            Action::ToggleNavigator,
        ),
        (
            "Toggle inspector".into(),
            "show or hide the right panel".into(),
            Action::ToggleInspector,
        ),
        (
            "Toggle instruction bytes".into(),
            "show or hide the byte column".into(),
            Action::ToggleBytes,
        ),
    ]
}

/// Score `text` against `needle`, returning the matched byte positions.
///
/// An empty needle matches everything at a neutral score, which is what makes
/// the palette useful as a browser before anything is typed.
pub fn score(text: &str, needle: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }
    let hay: Vec<char> = text.chars().collect();
    let hay_lower: Vec<char> = text.to_lowercase().chars().collect();
    let pat: Vec<char> = needle.to_lowercase().chars().collect();
    if pat.len() > hay.len() {
        return None;
    }

    let mut hits = Vec::with_capacity(pat.len());
    let mut score: i64 = 0;
    let mut hi = 0usize;
    let mut last_hit: Option<usize> = None;

    for &p in &pat {
        let found = (hi..hay_lower.len()).find(|&i| hay_lower[i] == p)?;
        // A match at the start, or just after a separator or a lowercase to
        // uppercase transition, is a word boundary and worth far more than a
        // match in the middle of a word.
        let boundary = found == 0
            || matches!(hay[found - 1], '_' | '.' | ' ' | '-' | ':' | '/')
            || (hay[found - 1].is_lowercase() && hay[found].is_uppercase());
        score += if boundary { 16 } else { 4 };
        if last_hit == Some(found.wrapping_sub(1)) {
            score += 12; // consecutive characters
        }
        if found == 0 {
            score += 20; // anchored at the very start
        }
        hits.push(found);
        last_hit = Some(found);
        hi = found + 1;
    }

    // Prefer a tight match in a short name over a scattered one in a long one.
    let span = hits.last().copied().unwrap_or(0) - hits.first().copied().unwrap_or(0) + 1;
    score -= span as i64;
    score -= (hay.len() / 8) as i64;
    if text.to_lowercase() == needle.to_lowercase() {
        score += 200;
    }
    Some((score, hits))
}
