//! Call-graph explorer.
//!
//! A layered graph around one function: callees to the right, callers to the
//! left, imported symbols as leaves. The layout is deliberately simple --
//! breadth-first levels, sorted within a level, straight edges -- because a
//! graph you can predict is easier to read than a prettier one that moves
//! every time the root changes.

use crate::state::{Action, GraphDir, Nav, SithApp};
use crate::theme::{col, *};

use eframe::egui::{self, Pos2, Rect, Stroke, Ui, Vec2};
use ne_analysis::{Addr, Function};
use std::collections::{HashMap, HashSet};

/// How many nodes a single column may hold before the rest are summarised.
///
/// A well-connected function can call sixty things. Drawing all of them makes
/// a column taller than any useful zoom level, and the graph stops being a
/// picture of anything.
const MAX_PER_LEVEL: usize = 14;

const NODE_W: f32 = 190.0;
const NODE_H: f32 = 34.0;
const COL_GAP: f32 = 90.0;
const ROW_GAP: f32 = 14.0;

#[derive(Clone)]
struct Node {
    /// `None` for an imported symbol, which has no address in this module.
    addr: Option<Addr>,
    label: String,
    sub: String,
    /// Horizontal level: negative for callers, positive for callees.
    level: i32,
    is_root: bool,
    is_import: bool,
    /// A stand-in for the entries a level did not have room for.
    is_overflow: bool,
    rect: Rect,
}

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>) {
    let Some(doc) = app.doc() else { return };
    let Some(tab) = app.tab() else { return };
    let g = &tab.graph;

    // With no explicit root, start from the most connected function: it is
    // the most informative place to land.
    let root = g.root.or_else(|| {
        doc.program
            .functions
            .iter()
            .max_by_key(|f| f.calls.len())
            .map(|f| f.addr)
    });
    let Some(root) = root else {
        crate::ui::empty(ui, "no functions to graph");
        return;
    };
    let Some(root_fn) = doc.program.function_at(root) else {
        crate::ui::empty(ui, "the selected address is not a function start");
        return;
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Call graph").size(15.0).strong());
        ui.label(mono_c(app.label(root_fn), col::symbol()));
        ui.separator();
        for (dir, name) in GraphDir::ALL {
            if ui.selectable_label(g.dir == dir, name).clicked() {
                act.push(Action::SetGraphDir(dir));
            }
        }
        ui.separator();
        ui.label(dim("depth"));
        for d in 1..=4usize {
            if ui.selectable_label(g.depth == d, d.to_string()).clicked() {
                act.push(Action::SetGraphDepth(d));
            }
        }
        ui.separator();
        let mut imports = g.show_imports;
        if ui.checkbox(&mut imports, "imports").changed() {
            act.push(Action::ToggleGraphImports);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Open in listing").clicked() {
                act.push(Action::Goto(root));
            }
            if ui.button("Reset view").clicked() {
                act.push(Action::SetGraphRoot(root));
            }
        });
    });
    crate::ui::sep(ui);

    let nodes = layout(app, doc, root_fn, g.depth, g.dir, g.show_imports);
    let edges = edges(app, doc, &nodes, g.dir, g.show_imports);

    ui.label(dim(format!(
        "{} nodes, {} edges — drag to pan, scroll to zoom, click a node to re-centre, \
         double-click to open it",
        nodes.len(),
        edges.len()
    )));
    ui.add_space(4.0);

    // The transform is applied by hand rather than by egui::Scene. Scene puts
    // a scale on the whole layer, which scales the glyph *meshes* that were
    // rasterised at their original size, so text turns to mush as you zoom in.
    // Computing screen positions here means every label is rasterised at the
    // size it is actually drawn at, and it makes level-of-detail possible.
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

    let content = bounds(&nodes);
    let mut zoom = g.zoom;
    let mut pan = g.pan;
    if !g.framed {
        // Frame the whole graph, with a little air around it.
        let sx = rect.width() / (content.width() + 80.0).max(1.0);
        let sy = rect.height() / (content.height() + 80.0).max(1.0);
        zoom = sx.min(sy).clamp(0.15, 1.0);
        pan = content.center().to_vec2();
    }

    if resp.dragged() {
        pan -= resp.drag_delta() / zoom;
    }
    // Zoom about the pointer, so the thing under the cursor stays put.
    if let Some(hover) = resp.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 60.0);
        if scroll.abs() > 0.01 {
            let before = (hover - rect.center()) / zoom + pan;
            zoom = (zoom * (1.0 + scroll * 0.0015)).clamp(0.12, 4.0);
            let after = (hover - rect.center()) / zoom + pan;
            pan += before - after;
        }
    }

    let to_screen = |w: Pos2| rect.center() + (w.to_vec2() - pan) * zoom;
    let painter = ui.painter_at(rect);

    for (from, to) in &edges {
        let a = nodes[*from].rect;
        let b = nodes[*to].rect;
        let p0 = to_screen(Pos2::new(a.max.x, a.center().y));
        let p1 = to_screen(Pos2::new(b.min.x, b.center().y));
        let mid = (p0.x + p1.x) / 2.0;
        let stroke = Stroke::new((1.2 * zoom).clamp(0.6, 2.0), col::border());
        painter.line_segment([p0, Pos2::new(mid, p0.y)], stroke);
        painter.line_segment([Pos2::new(mid, p0.y), Pos2::new(mid, p1.y)], stroke);
        painter.line_segment([Pos2::new(mid, p1.y), p1], stroke);
        let head = (6.0 * zoom).clamp(3.0, 10.0);
        painter.line_segment([p1, p1 + Vec2::new(-head, -head * 0.55)], stroke);
        painter.line_segment([p1, p1 + Vec2::new(-head, head * 0.55)], stroke);
    }

    let pointer = resp.hover_pos();
    let mut hovered: Option<usize> = None;
    for (i, n) in nodes.iter().enumerate() {
        let r = Rect::from_min_max(to_screen(n.rect.min), to_screen(n.rect.max));
        if !rect.intersects(r) {
            continue;
        }
        let over = pointer.is_some_and(|p| r.contains(p));
        if over {
            hovered = Some(i);
        }
        let (fill, border, text_col) = if n.is_overflow {
            (col::bg(), col::border(), col::faint())
        } else if n.is_root {
            (col::accent().gamma_multiply(0.22), col::accent(), col::text())
        } else if n.is_import {
            (col::raised(), col::border(), col::comment())
        } else if over {
            (col::raised().gamma_multiply(1.3), col::accent(), col::text())
        } else {
            (col::raised(), col::border(), col::text())
        };
        let radius = egui::CornerRadius::same((5.0 * zoom).clamp(1.0, 8.0) as u8);
        painter.rect_filled(r, radius, fill);
        painter.rect_stroke(
            r,
            radius,
            Stroke::new((1.0 * zoom).clamp(0.6, 2.0), border),
            egui::StrokeKind::Inside,
        );

        // Level of detail: below a few pixels a label is a smear, so it is
        // left out rather than drawn illegibly.
        let title_px = 12.0 * zoom;
        if title_px >= 5.0 {
            painter.text(
                r.min + Vec2::new(9.0 * zoom, 6.0 * zoom),
                egui::Align2::LEFT_TOP,
                elide(&n.label, ((n.rect.width() - 16.0) / 7.2) as usize),
                egui::FontId::monospace(title_px),
                text_col,
            );
        }
        let sub_px = 10.0 * zoom;
        if sub_px >= 6.5 {
            painter.text(
                r.min + Vec2::new(9.0 * zoom, 19.0 * zoom),
                egui::Align2::LEFT_TOP,
                &n.sub,
                egui::FontId::monospace(sub_px),
                col::faint(),
            );
        }
    }

    if let Some(i) = hovered {
        let n = &nodes[i];
        if !n.is_overflow {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && !n.is_overflow {
            match n.addr {
                Some(addr) => act.push(Action::SetGraphRoot(addr)),
                None => act.push(Action::Go(Nav::Xrefs(n.label.clone()))),
            }
        }
        if resp.double_clicked() {
            if let Some(addr) = n.addr {
                act.push(Action::Goto(addr));
            }
        }
        resp.show_tooltip_ui(|ui| {
                ui.set_max_width(320.0);
                ui.label(
                    egui::RichText::new(&n.label)
                        .monospace()
                        .strong()
                        .color(if n.is_import { col::comment() } else { col::symbol() }),
                );
                ui.label(
                    egui::RichText::new(&n.sub)
                        .monospace()
                        .size(11.0)
                        .color(col::faint()),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(if n.is_overflow {
                        "too many to draw: pick one of these as the root, or filter first"
                    } else if n.is_import {
                        "click for its call sites"
                    } else {
                        "click to re-centre, double-click to open in the listing"
                    })
                    .size(11.0)
                    .color(col::dim()),
                );
        });
    }

    if zoom != g.zoom || pan != g.pan || !g.framed {
        act.push(Action::SetGraphView { pan, zoom });
    }
}

/// Breadth-first levels out from the root, laid out in columns.
fn layout(
    app: &SithApp,
    doc: &crate::state::Doc,
    root: &Function,
    depth: usize,
    dir: GraphDir,
    show_imports: bool,
) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let push = |nodes: &mut Vec<Node>,
                    seen: &mut HashSet<String>,
                    label: String,
                    sub: String,
                    addr: Option<Addr>,
                    level: i32,
                    is_root: bool,
                    is_import: bool| {
        let key = format!("{level}|{label}");
        if !seen.insert(key) {
            return;
        }
        nodes.push(Node {
            addr,
            label,
            sub,
            level,
            is_root,
            is_import,
            is_overflow: false,
            rect: Rect::ZERO,
        });
    };

    push(
        &mut nodes,
        &mut seen,
        app.label(root),
        root.addr.to_string(),
        Some(root.addr),
        0,
        true,
        false,
    );

    if matches!(dir, GraphDir::Callees | GraphDir::Both) {
        let mut frontier = vec![root.addr];
        for level in 1..=depth as i32 {
            let mut next = Vec::new();
            for a in &frontier {
                let Some(f) = doc.program.function_at(*a) else {
                    continue;
                };
                for g in doc.program.callees_of(f) {
                    push(
                        &mut nodes,
                        &mut seen,
                        app.label(g),
                        format!("{}  {} B", g.addr, g.size()),
                        Some(g.addr),
                        level,
                        false,
                        false,
                    );
                    next.push(g.addr);
                }
                if show_imports {
                    for name in doc.program.external_calls_of(f) {
                        push(
                            &mut nodes,
                            &mut seen,
                            name,
                            "import".into(),
                            None,
                            level,
                            false,
                            true,
                        );
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
    }

    if matches!(dir, GraphDir::Callers | GraphDir::Both) {
        let mut frontier = vec![root.addr];
        for level in 1..=depth as i32 {
            let mut next = Vec::new();
            for a in &frontier {
                for f in doc.program.callers_of(*a) {
                    push(
                        &mut nodes,
                        &mut seen,
                        app.label(f),
                        format!("{}  {} B", f.addr, f.size()),
                        Some(f.addr),
                        -level,
                        false,
                        false,
                    );
                    next.push(f.addr);
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
    }

    // Trim each column to something that can be read, keeping the module's own
    // functions ahead of imported symbols: the structure is the interesting
    // part, and a list of KERNEL calls is available elsewhere.
    let mut by_level_all: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        by_level_all.entry(n.level).or_default().push(i);
    }
    let mut keep: Vec<bool> = vec![true; nodes.len()];
    let mut overflow: Vec<(i32, usize)> = Vec::new();
    for (level, mut idxs) in by_level_all {
        if idxs.len() <= MAX_PER_LEVEL || level == 0 {
            continue;
        }
        idxs.sort_by_key(|i| (nodes[*i].is_import, nodes[*i].label.clone()));
        for i in idxs.iter().skip(MAX_PER_LEVEL) {
            keep[*i] = false;
        }
        overflow.push((level, idxs.len() - MAX_PER_LEVEL));
    }
    if keep.iter().any(|k| !k) {
        let mut kept = Vec::with_capacity(nodes.len());
        for (n, k) in nodes.into_iter().zip(&keep) {
            if *k {
                kept.push(n);
            }
        }
        nodes = kept;
        for (level, hidden) in overflow {
            nodes.push(Node {
                addr: None,
                label: format!("+{hidden} more"),
                sub: "raise the depth or pick a smaller root".into(),
                level,
                is_root: false,
                is_import: false,
                is_overflow: true,
                rect: Rect::ZERO,
            });
        }
    }

    // Place each level in its own column, centred vertically.
    let mut by_level: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        by_level.entry(n.level).or_default().push(i);
    }
    for (level, mut idxs) in by_level {
        // Overflow markers sit at the foot of their column.
        idxs.sort_by_key(|i| (nodes[*i].is_overflow, nodes[*i].is_import));
        let x = level as f32 * (NODE_W + COL_GAP);
        let total = idxs.len() as f32 * (NODE_H + ROW_GAP) - ROW_GAP;
        let mut y = -total / 2.0;
        for i in idxs {
            nodes[i].rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(NODE_W, NODE_H));
            y += NODE_H + ROW_GAP;
        }
    }
    nodes
}

fn edges(
    app: &SithApp,
    doc: &crate::state::Doc,
    nodes: &[Node],
    dir: GraphDir,
    show_imports: bool,
) -> Vec<(usize, usize)> {
    let index: HashMap<(i32, &str), usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| ((n.level, n.label.as_str()), i))
        .collect();
    let mut out = Vec::new();

    for (i, n) in nodes.iter().enumerate() {
        let Some(addr) = n.addr else { continue };
        let Some(f) = doc.program.function_at(addr) else {
            continue;
        };
        if matches!(dir, GraphDir::Callees | GraphDir::Both) && n.level >= 0 {
            for g in doc.program.callees_of(f) {
                if let Some(&j) = index.get(&(n.level + 1, app.label(g).as_str())) {
                    out.push((i, j));
                }
            }
            if show_imports {
                for name in doc.program.external_calls_of(f) {
                    if let Some(&j) = index.get(&(n.level + 1, name.as_str())) {
                        out.push((i, j));
                    }
                }
            }
        }
        // Caller columns sit to the left, so the edge runs from them inwards.
        if matches!(dir, GraphDir::Callers | GraphDir::Both) && n.level <= 0 {
            for g in doc.program.callers_of(addr) {
                if let Some(&j) = index.get(&(n.level - 1, app.label(g).as_str())) {
                    out.push((j, i));
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn bounds(nodes: &[Node]) -> Rect {
    nodes
        .iter()
        .map(|n| n.rect)
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)))
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{head}…")
}

