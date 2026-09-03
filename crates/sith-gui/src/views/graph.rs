//! Call-graph explorer.
//!
//! A layered graph around one function: callees to the right, callers to the
//! left, imported symbols as leaves. The layout is deliberately simple --
//! breadth-first levels, sorted within a level, straight edges -- because a
//! graph you can predict is easier to read than a prettier one that moves
//! every time the root changes.

use crate::icons::{self, Icon};
use crate::state::{Action, GraphDir, GraphState, Nav, SithApp};
use crate::widgets;
use crate::theme::col;

use eframe::egui::{self, Pos2, Rect, Stroke, Ui, Vec2};
use ne_analysis::{Addr, Function};
use std::collections::HashMap;

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
    /// Stable identity, used to remember where the user dragged it to.
    key: String,
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

    // With no explicit root, start from the most connected function: it is the
    // most informative place to land.
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

    let nodes = layout(
        app,
        doc,
        root_fn,
        g.depth,
        g.dir,
        g.show_imports,
        &g.moved,
        &g.expanded,
    );
    let edges = edges(app, doc, &nodes);

    header(app, ui, act, root_fn, &nodes, &edges);
    controls(app, ui, act, g);
    canvas(app, ui, act, g, &nodes, &edges);
}

/// Title, root, and the actions that apply to the whole view.
fn header(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    root: &Function,
    nodes: &[Node],
    edges: &[(usize, usize)],
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        icons::inline(ui, Icon::Graph, col::accent());
        ui.label(egui::RichText::new("Call graph").size(15.0).strong());
        ui.label(
            egui::RichText::new("rooted at")
                .size(11.0)
                .color(col::faint()),
        );
        // The root is the one thing this view is about, so it is a control:
        // click it to open the code it names.
        if widgets::link(ui, app.label(root), col::symbol())
            .on_hover_text("open this function in the listing")
            .clicked()
        {
            act.push(Action::Goto(root.addr));
        }
        widgets::chip(ui, &root.addr.to_string(), col::dim());

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Search, "Pick a different root (Ctrl+P)").clicked() {
                act.push(Action::ShowPalette);
            }
            ui.add_space(4.0);
            if icons::button(ui, Icon::Target, "Fit the graph to the view").clicked() {
                act.push(Action::GraphFit);
            }
            if icons::button(ui, Icon::Plus, "Zoom in").clicked() {
                act.push(Action::GraphZoom(1.25));
            }
            if icons::button(ui, Icon::Minus, "Zoom out").clicked() {
                act.push(Action::GraphZoom(0.8));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("{} nodes · {} edges", nodes.len(), edges.len()))
                    .size(11.0)
                    .color(col::faint()),
            );
        });
    });
    ui.add_space(6.0);
}

/// What the graph is showing: direction, how far, and whether imports count.
fn controls(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, g: &GraphState) {
    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // A fixed strip height, then centre everything against it. egui's
            // ordinary row centring uses the height known when each item is
            // added, so a taller control added later leaves the earlier ones
            // sitting a couple of pixels high.
            ui.set_height(widgets::CONTROL_H + 4.0);
            // Top-aligned, with every item a fixed-height box: nothing here
            // depends on how a centring offset rounds.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                widgets::strip_item(ui, |ui| {
                    ui.label(egui::RichText::new("show").size(11.0).color(col::faint()));
                });
                let options: Vec<(GraphDir, &str)> = GraphDir::ALL
                    .iter()
                    .map(|(d, name)| (*d, *name))
                    .collect();
                let picked = widgets::strip_item(ui, |ui| widgets::segmented(ui, g.dir, &options));
                if let Some(d) = picked {
                    act.push(Action::SetGraphDir(d));
                }

                widgets::strip_item(ui, |ui| {
                    ui.label(egui::RichText::new("levels").size(11.0).color(col::faint()));
                });
                let depth = widgets::strip_item(ui, |ui| widgets::stepper(ui, g.depth, 1, 4));
                if let Some(depth) = depth {
                    act.push(Action::SetGraphDepth(depth));
                }

                let toggled = widgets::strip_item(ui, |ui| {
                    widgets::toggle_chip(ui, g.show_imports, "imports", col::comment())
                });
                if toggled {
                    act.push(Action::ToggleGraphImports);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", g.zoom * 100.0))
                            .monospace()
                            .size(11.0)
                            .color(col::faint()),
                    );
                    ui.add_space(8.0);
                    // A key to the colours, since the shapes are all alike.
                    // Reversed, because the layout runs right to left and the
                    // key should still read root, function, import.
                    for (color, label) in [
                        (col::comment(), "import"),
                        (col::text(), "function"),
                        (col::accent(), "root"),
                    ] {
                        ui.label(egui::RichText::new(label).size(10.5).color(col::faint()));
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 4.0, color);
                    }
                });
            });
        });
    let _ = app;
    ui.add_space(8.0);
}

/// The viewport.
fn canvas(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    g: &GraphState,
    nodes: &[Node],
    edges: &[(usize, usize)],
) {
    let _ = app;
    egui::Frame::new()
        .fill(col::bg())
        .corner_radius(egui::CornerRadius::same(6))
        .stroke(Stroke::new(1.0, col::border()))
        .inner_margin(egui::Margin::same(1))
        .show(ui, |ui| {
            // The transform is applied by hand rather than by egui::Scene.
            // Scene puts a scale on the whole layer, which scales the glyph
            // meshes that were rasterised at their original size, so text turns
            // to mush as you zoom in. Computing screen positions here means
            // every label is rasterised at the size it is actually drawn at,
            // and it makes level of detail possible.
            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            let content = bounds(nodes);
            let mut zoom = g.zoom;
            let mut pan = g.pan;
            if !g.framed {
                let sx = rect.width() / (content.width() + 80.0).max(1.0);
                let sy = rect.height() / (content.height() + 80.0).max(1.0);
                zoom = sx.min(sy).clamp(0.15, 1.0);
                pan = content.center().to_vec2();
            }
            if g.zoom_nudge != 1.0 {
                zoom = (zoom * g.zoom_nudge).clamp(0.12, 4.0);
            }

            // Node rectangles are needed before the drag is resolved, so the
            // press can tell a node drag from a pan. They are projected with
            // the transform as it stands now, which is what the pointer was
            // pressed against.
            let project = |w: Pos2, pan: Vec2, zoom: f32| {
                rect.center() + (w.to_vec2() - pan) * zoom
            };
            // Captured by value: the transform is about to change, and the
            // press was made against this one.
            let (press_pan, press_zoom) = (pan, zoom);
            let at_pointer = move |p: Pos2| {
                nodes.iter().position(|n| {
                    Rect::from_min_max(
                        project(n.rect.min, press_pan, press_zoom),
                        project(n.rect.max, press_pan, press_zoom),
                    )
                    .contains(p)
                })
            };

            if resp.drag_started() {
                if let Some(i) = resp.interact_pointer_pos().and_then(at_pointer) {
                    act.push(Action::GraphDragStart(nodes[i].key.clone()));
                }
            }
            if resp.drag_stopped() {
                act.push(Action::GraphDragEnd);
            }
            if resp.dragged() {
                if g.dragging.is_some() {
                    // Moving one node, in world units so it tracks the pointer
                    // at any zoom.
                    act.push(Action::GraphDragBy(resp.drag_delta() / zoom));
                } else {
                    pan -= resp.drag_delta() / zoom;
                }
            }
            // Zoom about the pointer, so the thing under the cursor stays put.
            if let Some(hover) = resp.hover_pos() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() > 0.01 {
                    let before = (hover - rect.center()) / zoom + pan;
                    zoom = (zoom * (1.0 + scroll * 0.0015)).clamp(0.12, 4.0);
                    let after = (hover - rect.center()) / zoom + pan;
                    pan += before - after;
                }
            }

            let to_screen = |w: Pos2| project(w, pan, zoom);
            let painter = ui.painter_at(rect);

            for (from, to) in edges {
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

            let pointer = resp.hover_pos().filter(|p| rect.contains(*p));
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
                // A colour the user assigned outranks everything but the
                // root marker: it is the whole point of assigning one.
                let tint = n
                    .addr
                    .and_then(|a| app.user_color(a.segment, a.offset));
                let (fill, border, text_col) = if n.is_overflow {
                    (
                        egui::Color32::TRANSPARENT,
                        col::border(),
                        if over { col::accent() } else { col::faint() },
                    )
                } else if let Some(c) = tint {
                    (
                        c.gamma_multiply(if over { 0.34 } else { 0.22 }),
                        c,
                        col::text(),
                    )
                } else if n.is_root {
                    (
                        col::accent().gamma_multiply(0.22),
                        col::accent(),
                        col::text(),
                    )
                } else if n.is_import {
                    (col::raised(), col::border(), col::comment())
                } else if over {
                    (col::raised().gamma_multiply(1.3), col::accent(), col::text())
                } else {
                    (col::raised(), col::border(), col::text())
                };
                if n.addr.is_some() && n.addr == g.selected {
                    painter.rect_stroke(
                        r.expand(3.0),
                        radius_of(zoom),
                        Stroke::new((1.6 * zoom).clamp(1.0, 3.0), col::yellow()),
                        egui::StrokeKind::Outside,
                    );
                }
                if n.is_root && tint.is_some() {
                    // The root still has to be findable once it is coloured.
                    painter.rect_stroke(
                        r.expand(2.0),
                        radius_of(zoom),
                        Stroke::new((1.0 * zoom).clamp(0.6, 2.0), col::accent()),
                        egui::StrokeKind::Outside,
                    );
                }
                let radius = radius_of(zoom);
                painter.rect_filled(r, radius, fill);
                painter.rect_stroke(
                    r,
                    radius,
                    Stroke::new((1.0 * zoom).clamp(0.6, 2.0), border),
                    egui::StrokeKind::Inside,
                );

                // Level of detail: below a few pixels a label is a smear, so it
                // is left out rather than drawn illegibly.
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
                        // Same clipping as the title: a sub-line that runs
                        // past its box reads as a rendering fault.
                        elide(&n.sub, ((n.rect.width() - 16.0) / 6.0) as usize),
                        egui::FontId::monospace(sub_px),
                        col::faint(),
                    );
                }
            }

            // Right-click opens the menu for whichever node it landed on, and
            // the choice is remembered: the pointer moves into the menu, so
            // "what is hovered now" is the wrong question by then.
            if resp.secondary_clicked() {
                let key = resp.interact_pointer_pos().and_then(at_pointer).map(|i| nodes[i].key.clone());
                act.push(Action::GraphMenuFor(key));
            }
            let menu_node = g
                .menu_for
                .as_ref()
                .and_then(|k| nodes.iter().find(|n| n.key == *k));
            if let Some(n) = menu_node {
                let addr = n.addr;
                let label = n.label.clone();
                resp.context_menu(|ui| {
                    ui.label(
                        egui::RichText::new(&label)
                            .monospace()
                            .size(11.5)
                            .color(col::faint()),
                    );
                    ui.separator();
                    if let Some(addr) = addr {
                        if ui.button("Open in listing").clicked() {
                            act.push(Action::Goto(addr));
                            ui.close();
                        }
                        if ui.button("Centre the graph here").clicked() {
                            act.push(Action::SetGraphRoot(addr));
                            ui.close();
                        }
                        if ui.button("Name this function…").clicked() {
                            act.push(Action::ShowRename {
                                segment: addr.segment,
                                offset: addr.offset,
                            });
                            ui.close();
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new("colour")
                                .size(10.5)
                                .color(col::faint()),
                        );
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for name in crate::theme::USER_COLORS {
                                let Some(c) = crate::theme::named_color(name) else {
                                    continue;
                                };
                                let (sw, swr) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    sw,
                                    egui::CornerRadius::same(4),
                                    c.gamma_multiply(0.7),
                                );
                                if swr.hovered() {
                                    ui.painter().rect_stroke(
                                        sw,
                                        egui::CornerRadius::same(4),
                                        Stroke::new(1.5, c),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if swr.on_hover_text(*name).clicked() {
                                    act.push(Action::SetColor {
                                        segment: addr.segment,
                                        offset: addr.offset,
                                        color: Some(name),
                                    });
                                    ui.close();
                                }
                            }
                        });
                        if ui.button("Clear colour").clicked() {
                            act.push(Action::SetColor {
                                segment: addr.segment,
                                offset: addr.offset,
                                color: None,
                            });
                            ui.close();
                        }
                    } else if ui.button("Show its call sites").clicked() {
                        act.push(Action::Go(Nav::Xrefs(label.clone())));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset the layout").clicked() {
                        act.push(Action::GraphResetLayout);
                        ui.close();
                    }
                });
            }

            if let Some(i) = hovered {
                let n = &nodes[i];
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                // A click reads the node; it does not move the graph or leave
                // the view. Re-centring is the deliberate act, so it takes a
                // double click, and opening the code is on the menu.
                if resp.clicked() {
                    if n.is_overflow {
                        act.push(Action::GraphExpandLevel(n.level));
                    } else {
                        act.push(Action::GraphSelect(n.addr));
                    }
                }
                if resp.double_clicked() && !n.is_overflow {
                    match n.addr {
                        Some(addr) => act.push(Action::SetGraphRoot(addr)),
                        None => act.push(Action::Go(Nav::Xrefs(n.label.clone()))),
                    }
                }
                // Anchored to the pointer, not to the response: the response
                // is the whole canvas, so its own corner is nowhere near the
                // node being described.
                egui::Tooltip::for_widget(&resp)
                    .at_pointer()
                    .show(|ui| {
                    ui.set_max_width(320.0);
                    ui.label(
                        egui::RichText::new(&n.label)
                            .monospace()
                            .strong()
                            .color(if n.is_import {
                                col::comment()
                            } else {
                                col::symbol()
                            }),
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
                            "the column was trimmed to stay readable; click to show the rest"
                        } else if n.is_import {
                            "double-click for its call sites"
                        } else {
                            "click to read it, double-click to centre the graph here, \
                             right-click for more"
                        })
                            .size(11.0)
                            .color(col::dim()),
                        );
                    });
            }

            if zoom != g.zoom || pan != g.pan || !g.framed || g.zoom_nudge != 1.0 {
                act.push(Action::SetGraphView { pan, zoom });
            }
        });
}

/// Build the graph around `root`.
///
/// One node per function, not one per path to it. A breadth-first walk records
/// each function at its shortest distance from the root and never adds it
/// again, so a helper reachable four ways appears once with four edges into
/// it, rather than four times in four columns.
fn layout(
    app: &SithApp,
    doc: &crate::state::Doc,
    root: &Function,
    depth: usize,
    dir: GraphDir,
    show_imports: bool,
    moved: &HashMap<String, Vec2>,
    expanded: &std::collections::HashSet<i32>,
) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut level_of: HashMap<String, i32> = HashMap::new();

    let add = |nodes: &mut Vec<Node>,
                   level_of: &mut HashMap<String, i32>,
                   key: String,
                   label: String,
                   sub: String,
                   addr: Option<Addr>,
                   level: i32,
                   is_root: bool,
                   is_import: bool|
     -> bool {
        if level_of.contains_key(&key) {
            return false;
        }
        level_of.insert(key.clone(), level);
        nodes.push(Node {
            key,
            addr,
            label,
            sub,
            level,
            is_root,
            is_import,
            is_overflow: false,
            rect: Rect::ZERO,
        });
        true
    };

    add(
        &mut nodes,
        &mut level_of,
        fn_key(root.addr),
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
                    if add(
                        &mut nodes,
                        &mut level_of,
                        fn_key(g.addr),
                        app.label(g),
                        node_sub(g),
                        Some(g.addr),
                        level,
                        false,
                        false,
                    ) {
                        next.push(g.addr);
                    }
                }
                if show_imports {
                    for name in doc.program.external_calls_of(f) {
                        add(
                            &mut nodes,
                            &mut level_of,
                            import_key(&name),
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
                    if add(
                        &mut nodes,
                        &mut level_of,
                        fn_key(f.addr),
                        app.label(f),
                        node_sub(f),
                        Some(f.addr),
                        -level,
                        false,
                        false,
                    ) {
                        next.push(f.addr);
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
    }

    trim_columns(&mut nodes, expanded);
    order_columns(&mut nodes, doc, app);
    place(&mut nodes, moved);
    nodes
}

/// The line under a node's name: where it is, how big, and what it takes.
fn node_sub(f: &Function) -> String {
    match f.frame.argument_bytes() {
        Some(n) if n > 0 => format!("{}  {} B  ({n} args)", f.addr, f.size()),
        _ => format!("{}  {} B", f.addr, f.size()),
    }
}

fn fn_key(addr: Addr) -> String {
    format!("f{:02}:{:04X}", addr.segment, addr.offset)
}

fn import_key(name: &str) -> String {
    format!("i{name}")
}

/// Keep each column to something readable, the module's own functions first.
fn trim_columns(nodes: &mut Vec<Node>, expanded: &std::collections::HashSet<i32>) {
    let mut by_level: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        by_level.entry(n.level).or_default().push(i);
    }
    let mut keep = vec![true; nodes.len()];
    let mut hidden: Vec<(i32, usize)> = Vec::new();
    for (level, mut idxs) in by_level {
        if idxs.len() <= MAX_PER_LEVEL || level == 0 || expanded.contains(&level) {
            continue;
        }
        idxs.sort_by_key(|i| (nodes[*i].is_import, nodes[*i].label.clone()));
        for i in idxs.iter().skip(MAX_PER_LEVEL) {
            keep[*i] = false;
        }
        hidden.push((level, idxs.len() - MAX_PER_LEVEL));
    }
    if hidden.is_empty() {
        return;
    }
    let mut kept = Vec::with_capacity(nodes.len());
    for (n, k) in std::mem::take(nodes).into_iter().zip(&keep) {
        if *k {
            kept.push(n);
        }
    }
    *nodes = kept;
    for (level, n) in hidden {
        nodes.push(Node {
            key: format!("o{level}"),
            addr: None,
            label: format!("+{n} more"),
            // The cap is per column, so raising the levels adds columns rather
            // than entries here. Showing them is a click.
            sub: "click to show them".into(),
            level,
            is_root: false,
            is_import: false,
            is_overflow: true,
            rect: Rect::ZERO,
        });
    }
}

/// Order each column so edges cross as little as possible.
///
/// The classic barycentre heuristic: put a node opposite the average position
/// of the nodes it connects to in the column before it. Two passes is enough
/// to stop the picture looking like a cat's cradle, and it is stable, so the
/// layout does not reshuffle itself when nothing changed.
fn order_columns(nodes: &mut [Node], doc: &crate::state::Doc, app: &SithApp) {
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.clone(), i))
        .collect();
    let links = adjacency(nodes, &index, doc, app);

    let mut levels: Vec<i32> = nodes.iter().map(|n| n.level).collect();
    levels.sort_unstable();
    levels.dedup();

    // Position within a column, seeded by the order nodes were discovered.
    let mut pos: HashMap<usize, f32> = HashMap::new();
    for level in &levels {
        let mut col: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.level == *level)
            .map(|(i, _)| i)
            .collect();
        col.sort_by_key(|i| (nodes[*i].is_overflow, nodes[*i].is_import));
        for (k, i) in col.iter().enumerate() {
            pos.insert(*i, k as f32);
        }
    }

    for _ in 0..2 {
        for level in &levels {
            if *level == 0 {
                continue;
            }
            // Look towards the root: that is the column already settled.
            let previous = if *level > 0 { level - 1 } else { level + 1 };
            let mut col: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.level == *level)
                .map(|(i, _)| i)
                .collect();
            let bary = |i: usize| -> f32 {
                let mut sum = 0.0;
                let mut count = 0.0;
                for (a, b) in &links {
                    let other = if *a == i {
                        *b
                    } else if *b == i {
                        *a
                    } else {
                        continue;
                    };
                    if nodes[other].level == previous {
                        sum += pos.get(&other).copied().unwrap_or(0.0);
                        count += 1.0;
                    }
                }
                if count == 0.0 {
                    // Nothing to line up with: keep where it was.
                    pos.get(&i).copied().unwrap_or(0.0)
                } else {
                    sum / count
                }
            };
            let mut keyed: Vec<(usize, f32)> = col.iter().map(|i| (*i, bary(*i))).collect();
            keyed.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    // Overflow markers stay at the foot of their column.
                    .then(nodes[a.0].is_overflow.cmp(&nodes[b.0].is_overflow))
            });
            col = keyed.into_iter().map(|(i, _)| i).collect();
            for (k, i) in col.iter().enumerate() {
                pos.insert(*i, k as f32);
            }
        }
    }

    for (i, n) in nodes.iter_mut().enumerate() {
        // Stash the row in the rect's y for `place` to turn into a position.
        n.rect = Rect::from_min_size(
            Pos2::new(0.0, pos.get(&i).copied().unwrap_or(0.0)),
            Vec2::ZERO,
        );
    }
}

/// Every edge between nodes that are both present.
fn adjacency(
    nodes: &[Node],
    index: &HashMap<String, usize>,
    doc: &crate::state::Doc,
    app: &SithApp,
) -> Vec<(usize, usize)> {
    let _ = app;
    let mut out = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        let Some(addr) = n.addr else { continue };
        let Some(f) = doc.program.function_at(addr) else {
            continue;
        };
        for g in doc.program.callees_of(f) {
            if let Some(j) = index.get(&fn_key(g.addr)) {
                out.push((i, *j));
            }
        }
        for name in doc.program.external_calls_of(f) {
            if let Some(j) = index.get(&import_key(&name)) {
                out.push((i, *j));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Turn column and row into a rectangle, then apply anything the user moved.
fn place(nodes: &mut [Node], moved: &HashMap<String, Vec2>) {
    let mut per_level: HashMap<i32, usize> = HashMap::new();
    for n in nodes.iter() {
        let e = per_level.entry(n.level).or_insert(0);
        *e = (*e).max(n.rect.min.y as usize + 1);
    }
    for n in nodes.iter_mut() {
        let rows = per_level.get(&n.level).copied().unwrap_or(1) as f32;
        let total = rows * (NODE_H + ROW_GAP) - ROW_GAP;
        let x = n.level as f32 * (NODE_W + COL_GAP);
        let y = -total / 2.0 + n.rect.min.y * (NODE_H + ROW_GAP);
        let offset = moved.get(&n.key).copied().unwrap_or(Vec2::ZERO);
        n.rect = Rect::from_min_size(Pos2::new(x, y) + offset, Vec2::new(NODE_W, NODE_H));
    }
}

fn edges(app: &SithApp, doc: &crate::state::Doc, nodes: &[Node]) -> Vec<(usize, usize)> {
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.clone(), i))
        .collect();
    adjacency(nodes, &index, doc, app)
}

fn radius_of(zoom: f32) -> egui::CornerRadius {
    egui::CornerRadius::same((5.0 * zoom).clamp(1.0, 8.0) as u8)
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
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}\u{2026}")
}
