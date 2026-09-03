//! Call-graph explorer.
//!
//! A layered graph around one function: callees to the right, callers to the
//! left, imported symbols as leaves. The layout is deliberately simple --
//! breadth-first levels, sorted within a level, straight edges -- because a
//! graph you can predict is easier to read than a prettier one that moves
//! every time the root changes.

use crate::state::{Action, GraphDir, Nav, SithApp};
use crate::theme::{col, *};

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};
use ne_analysis::{Addr, Function};
use std::collections::{HashMap, HashSet};

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

    ui.label(dim(&format!(
        "{} nodes, {} edges — drag to pan, scroll to zoom, click a node to re-centre",
        nodes.len(),
        edges.len()
    )));
    ui.add_space(4.0);

    let mut scene_rect = g.scene_rect;
    if scene_rect == Rect::ZERO {
        // First frame for this root: frame the whole graph.
        scene_rect = bounds(&nodes).expand(40.0);
    }
    let before = scene_rect;

    egui::Scene::new()
        .zoom_range(0.15..=3.0)
        .show(ui, &mut scene_rect, |ui| {
            let painter = ui.painter();
            for (from, to) in &edges {
                let a = nodes[*from].rect;
                let b = nodes[*to].rect;
                let (p0, p1) = (
                    Pos2::new(a.max.x, a.center().y),
                    Pos2::new(b.min.x, b.center().y),
                );
                let mid = (p0.x + p1.x) / 2.0;
                let stroke = Stroke::new(1.2, Color32::from_rgb(0x35, 0x42, 0x52));
                painter.line_segment([p0, Pos2::new(mid, p0.y)], stroke);
                painter.line_segment([Pos2::new(mid, p0.y), Pos2::new(mid, p1.y)], stroke);
                painter.line_segment([Pos2::new(mid, p1.y), p1], stroke);
                // Arrow head.
                painter.line_segment([p1, p1 + Vec2::new(-6.0, -3.5)], stroke);
                painter.line_segment([p1, p1 + Vec2::new(-6.0, 3.5)], stroke);
            }

            for (i, n) in nodes.iter().enumerate() {
                let resp = ui.interact(
                    n.rect,
                    ui.id().with(("node", i)),
                    egui::Sense::click(),
                );
                let (fill, border, text_col) = if n.is_root {
                    (col::accent().gamma_multiply(0.22), col::accent(), col::text())
                } else if n.is_import {
                    (col::raised(), Color32::from_rgb(0x3A, 0x46, 0x54), col::comment())
                } else if resp.hovered() {
                    (Color32::from_rgb(0x22, 0x2B, 0x36), col::accent(), col::text())
                } else {
                    (col::raised(), Color32::from_rgb(0x30, 0x3B, 0x49), col::text())
                };
                let p = ui.painter();
                p.rect_filled(n.rect, CornerRadius::same(5), fill);
                p.rect_stroke(
                    n.rect,
                    CornerRadius::same(5),
                    Stroke::new(1.0, border),
                    egui::StrokeKind::Inside,
                );
                p.text(
                    n.rect.min + Vec2::new(9.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    elide(&n.label, 24),
                    egui::FontId::monospace(12.0),
                    text_col,
                );
                p.text(
                    n.rect.min + Vec2::new(9.0, 19.0),
                    egui::Align2::LEFT_TOP,
                    &n.sub,
                    egui::FontId::monospace(10.0),
                    col::faint(),
                );

                if let Some(addr) = n.addr {
                    if resp.clicked() {
                        act.push(Action::SetGraphRoot(addr));
                    }
                    if resp.double_clicked() {
                        act.push(Action::Goto(addr));
                    }
                } else if resp.clicked() {
                    act.push(Action::Go(Nav::Xrefs(n.label.clone())));
                }
            }
        });

    if scene_rect != before {
        act.push(Action::SetGraphRect(scene_rect));
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

    // Place each level in its own column, centred vertically.
    let mut by_level: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        by_level.entry(n.level).or_default().push(i);
    }
    for (level, idxs) in by_level {
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

