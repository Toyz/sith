//! The disassembly listing.
//!
//! Three things make a listing readable rather than merely correct: a gutter
//! that draws the local branches so loops and early-outs are visible without
//! reading addresses, a header that says which function you are inside while
//! you scroll, and highlighting that ties every mention of the selected
//! symbol together.

use crate::icons::{self, Icon};
use crate::state::{Action, Nav, SithApp};
use crate::theme::{col, *};
use crate::widgets;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Ui};
use ne_analysis::{callargs, Addr, Function};
use ne_core::{ApiDb, Target};
use ne_disasm::{Flow, Insn};
use std::collections::BTreeMap;

/// What clicking a fixup target should do.
pub fn target_action(target: &Target) -> Action {
    match target {
        Target::Internal {
            segment,
            offset: Some(offset),
        } => Action::Goto(Addr {
            segment: *segment,
            offset: *offset as u32,
        }),
        Target::Entry {
            segment, offset, ..
        } => Action::Goto(Addr {
            segment: *segment,
            offset: *offset as u32,
        }),
        // An imported symbol lives in another file: open it when the
        // workspace has it, and fall back to its call sites when it does not.
        Target::ImportOrdinal {
            module, ordinal, ..
        } => Action::OpenModule {
            module: module.clone(),
            ordinal: Some(*ordinal),
            name: None,
        },
        Target::ImportName { module, name } => Action::OpenModule {
            module: module.clone(),
            ordinal: None,
            name: Some(name.clone()),
        },
        other => Action::Go(Nav::Xrefs(other.to_string())),
    }
}

enum Row {
    /// Function banner, carrying the index of the function it introduces.
    Header(usize),
    Insn(usize),
}

/// A near branch drawn in the gutter.
struct Branch {
    from: usize,
    to: usize,
    lane: usize,
    conditional: bool,
}

const LANE_W: f32 = 7.0;
const GUTTER_PAD: f32 = 6.0;

pub fn show(app: &SithApp, ui: &mut Ui, act: &mut Vec<Action>, segno: u16) {
    let Some(doc) = app.doc() else { return };
    let Some(code) = doc.program.code.get(&segno) else {
        crate::ui::empty(ui, "this segment holds no decoded code");
        return;
    };
    let Some(tab) = app.tab() else { return };

    let funcs: Vec<&Function> = doc
        .program
        .functions
        .iter()
        .filter(|f| f.addr.segment == segno)
        .collect();
    let func_at: BTreeMap<u32, usize> = funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.addr.offset, i))
        .collect();
    let labels: BTreeMap<u32, String> = funcs
        .iter()
        .map(|f| (f.addr.offset, app.label(f)))
        .collect();

    // Row list: a banner before each function, then its instructions.
    let mut rows: Vec<Row> = Vec::with_capacity(code.insns.len() + funcs.len());
    let mut row_of_offset: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, insn) in code.insns.iter().enumerate() {
        if let Some(fi) = func_at.get(&insn.offset) {
            rows.push(Row::Header(*fi));
        }
        row_of_offset.insert(insn.offset, rows.len());
        rows.push(Row::Insn(i));
    }

    let branches = branches(code, &row_of_offset);
    let lanes = branches.iter().map(|b| b.lane + 1).max().unwrap_or(0);
    let gutter_w = if lanes == 0 {
        GUTTER_PAD
    } else {
        lanes as f32 * LANE_W + GUTTER_PAD
    };

    // Anything matching the selected row's symbol gets highlighted.
    let selected = tab.sel;
    let highlight = selected
        .and_then(|s| code.insns.iter().find(|i| i.offset == s))
        .and_then(|i| i.fixup.as_ref().map(|f| f.target.to_string()));

    let byte_w = ne_disasm::byte_column_width(&code.insns).max(8);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;

    sticky_header(app, ui, act, &rows, &funcs, row_h);

    // `show_rows` adds the ui's vertical item spacing to every row when it
    // computes the visible range and the total height; zeroing it here makes
    // the drawn rows exactly `row_h` apart, so the listing fills the view
    // instead of stopping short.
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if let Some(target) = tab.scroll_to {
        let idx = row_of_offset
            .range(target..)
            .next()
            .map(|(_, r)| *r)
            .unwrap_or(0);
        // Land the target a few rows down rather than flush against the top.
        area = area.vertical_scroll_offset(idx.saturating_sub(5) as f32 * row_h);
    }

    let out = area.show_rows(ui, row_h, rows.len(), |ui, range| {
        let first = range.start;
        let top = ui.cursor().top();
        let left = ui.max_rect().left();
        let gutter = Rect::from_min_max(
            Pos2::new(left, ui.clip_rect().top()),
            Pos2::new(left + gutter_w, ui.clip_rect().bottom()),
        );
        let pointer = ui
            .ctx()
            .pointer_latest_pos()
            .filter(|p| gutter.contains(*p));

        let hovered = draw_branches(
            ui, &branches, first, range.end, top, left, row_h, selected, &rows, code, pointer,
        );

        // Only claim the pointer when an arc is actually under it, so the
        // gutter does not swallow clicks meant for the rows behind it.
        if let Some(h) = hovered {
            let b = &branches[h];
            if let (Some(from), Some(to)) = (
                row_offset(&rows, code, b.from),
                row_offset(&rows, code, b.to),
            ) {
                let resp = ui
                    .interact(gutter, ui.id().with("branch-gutter"), egui::Sense::click())
                    .on_hover_text(format!(
                        "{} {from:04X} \u{2192} {to:04X}\nclick to follow",
                        if b.conditional {
                            "conditional jump"
                        } else {
                            "jump"
                        }
                    ));
                if resp.clicked() {
                    act.push(Action::Goto(Addr {
                        segment: segno,
                        offset: to,
                    }));
                }
            }
        }

        for r in range {
            match &rows[r] {
                Row::Header(fi) => {
                    function_banner(app, ui, act, funcs[*fi], gutter_w, row_h)
                }
                Row::Insn(i) => {
                    // Reconstructing the call is cheap and only done for rows
                    // actually on screen.
                    let call = callargs::reconstruct(code, *i, ApiDb::embedded());
                    let loads = doc.res_links.resource_at(Addr {
                        segment: segno,
                        offset: code.insns[*i].offset,
                    });
                    insn_row(
                        app,
                        ui,
                        act,
                        segno,
                        &code.insns[*i],
                        byte_w,
                        gutter_w,
                        &labels,
                        highlight.as_deref(),
                        call.as_ref(),
                        loads,
                        row_h,
                    );
                }
            }
        }
    });

    // The sticky header above is drawn before the scroll area exists, so it
    // reads the offset this frame stores. One frame of lag is invisible.
    ui.ctx().memory_mut(|m| {
        m.data
            .insert_temp(ui.id().with("disasm_scroll"), out.state.offset.y)
    });
}

/// The segment offset a listing row sits at, if it is an instruction.
fn row_offset(rows: &[Row], code: &ne_disasm::SegmentCode, row: usize) -> Option<u32> {
    match rows.get(row)? {
        Row::Insn(i) => Some(code.insns[*i].offset),
        Row::Header(_) => None,
    }
}

/// Near jumps inside the segment, packed into non-overlapping lanes.
fn branches(code: &ne_disasm::SegmentCode, row_of: &BTreeMap<u32, usize>) -> Vec<Branch> {
    let mut edges: Vec<Branch> = Vec::new();
    for insn in &code.insns {
        if !matches!(insn.flow, Flow::Jump | Flow::CondJump) {
            continue;
        }
        let (Some(from), Some(to)) = (
            row_of.get(&insn.offset),
            insn.near_target.and_then(|t| row_of.get(&t)),
        ) else {
            continue;
        };
        edges.push(Branch {
            from: *from,
            to: *to,
            lane: 0,
            conditional: insn.flow == Flow::CondJump,
        });
    }
    // Shortest spans get the innermost lane, which puts tight loops closest to
    // the code and long jumps out at the edge.
    edges.sort_by_key(|b| b.from.abs_diff(b.to));
    let mut lane_ends: Vec<Vec<(usize, usize)>> = Vec::new();
    for e in &mut edges {
        let (lo, hi) = (e.from.min(e.to), e.from.max(e.to));
        let mut placed = false;
        for (lane, spans) in lane_ends.iter_mut().enumerate() {
            if spans.iter().all(|(a, b)| hi < *a || lo > *b) {
                spans.push((lo, hi));
                e.lane = lane;
                placed = true;
                break;
            }
        }
        if !placed {
            // Past a handful of lanes the gutter stops helping and starts
            // eating the listing, so deeper nesting shares the outer lane.
            if lane_ends.len() >= 6 {
                e.lane = 5;
            } else {
                e.lane = lane_ends.len();
                lane_ends.push(vec![(lo, hi)]);
            }
        }
    }
    edges
}

/// Draw the branch gutter, and report the arc under the pointer.
///
/// Hovering an arc is how you read a jump without leaving your place: the
/// whole path lights up, both of its endpoints highlight, and a click follows
/// it. Hit testing is done against the drawn polyline rather than a bounding
/// box, so arcs stacked in adjacent lanes stay individually selectable.
#[allow(clippy::too_many_arguments)]
fn draw_branches(
    ui: &Ui,
    branches: &[Branch],
    first: usize,
    last: usize,
    top: f32,
    left: f32,
    row_h: f32,
    selected: Option<u32>,
    rows: &[Row],
    code: &ne_disasm::SegmentCode,
    pointer: Option<Pos2>,
) -> Option<usize> {
    let sel_row = selected.and_then(|s| {
        rows.iter().position(|r| match r {
            Row::Insn(i) => code.insns[*i].offset == s,
            _ => false,
        })
    });
    let y = |row: usize| top + (row as f32 - first as f32 + 0.5) * row_h;
    let lane_x = |lane: usize| left + 2.0 + lane as f32 * LANE_W;
    let edge_x = |lane: usize| left + 2.0 + (lane as f32 + 0.85) * LANE_W;

    // Which arc the pointer is on, chosen by distance so overlapping lanes
    // resolve to the nearest one rather than to whichever drew last.
    let mut hovered: Option<(usize, f32)> = None;
    if let Some(p) = pointer {
        for (i, b) in branches.iter().enumerate() {
            let (lo, hi) = (b.from.min(b.to), b.from.max(b.to));
            if hi < first || lo > last {
                continue;
            }
            let (x, edge) = (lane_x(b.lane), edge_x(b.lane));
            let (y0, y1) = (y(b.from), y(b.to));
            let d = dist_to_segment(p, Pos2::new(x, y0), Pos2::new(x, y1))
                .min(dist_to_segment(p, Pos2::new(x, y0), Pos2::new(edge, y0)))
                .min(dist_to_segment(p, Pos2::new(x, y1), Pos2::new(edge, y1)));
            if d <= 4.0 && hovered.is_none_or(|(_, best)| d < best) {
                hovered = Some((i, d));
            }
        }
    }
    let hovered = hovered.map(|(i, _)| i);

    let painter = ui.painter();
    // The hovered arc is drawn last so it sits above its neighbours.
    let order = branches
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != hovered)
        .chain(hovered.map(|i| (i, &branches[i])));

    for (i, b) in order {
        let (lo, hi) = (b.from.min(b.to), b.from.max(b.to));
        if hi < first || lo > last {
            continue;
        }
        let is_hovered = Some(i) == hovered;
        let touches_selection = sel_row.is_some_and(|s| s == b.from || s == b.to);
        let color = if is_hovered {
            col::orange()
        } else if touches_selection {
            col::accent()
        } else if b.conditional {
            Color32::from_rgb(0x3E, 0x54, 0x6B)
        } else {
            Color32::from_rgb(0x4B, 0x5F, 0x45)
        };
        let width = if is_hovered {
            2.0
        } else if touches_selection {
            1.6
        } else {
            1.0
        };
        let stroke = Stroke::new(width, color);
        let (x, edge) = (lane_x(b.lane), edge_x(b.lane));
        let (y0, y1) = (y(b.from), y(b.to));

        painter.line_segment([Pos2::new(x, y0), Pos2::new(edge, y0)], stroke);
        painter.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], stroke);
        painter.line_segment([Pos2::new(x, y1), Pos2::new(edge, y1)], stroke);
        painter.line_segment(
            [Pos2::new(edge, y1), Pos2::new(edge - 3.5, y1 - 3.0)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(edge, y1), Pos2::new(edge - 3.5, y1 + 3.0)],
            stroke,
        );

        if is_hovered {
            // Mark both ends, so a jump off the top or bottom of the view is
            // still readable as "from here to somewhere above".
            for (row, filled) in [(b.from, false), (b.to, true)] {
                if row < first || row > last {
                    continue;
                }
                let c = Pos2::new(edge + 3.0, y(row));
                if filled {
                    painter.circle_filled(c, 2.5, col::orange());
                } else {
                    painter.circle_stroke(c, 2.5, stroke);
                }
            }
        }
    }
    hovered
}

/// Distance from a point to a line segment, for hit-testing the arcs.
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = (((p - a).dot(ab)) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// A banner introducing a function, with its size and reference count.
fn function_banner(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    f: &Function,
    gutter_w: f32,
    row_h: f32,
) {
    let (_, resp) = widgets::row_sized(
        ui,
        ui.id().with(("fnhdr", f.addr.offset)),
        row_h,
        false,
        false,
        |ui| {
            ui.add_space(gutter_w);
            icons::inline(ui, Icon::Code, col::symbol());
            let named = app.user_name(f.addr.segment, f.addr.offset).is_some();
            ui.label(mono_c(app.label(f), if named { col::cyan() } else { col::symbol() }).strong());
            if named {
                widgets::chip(ui, "named", col::cyan());
            }
            widgets::chip(ui, f.kind.as_str(), col::dim());
            ui.label(mono_c(
                format!(
                    "{} bytes  {} insns  {} calls",
                    f.size(),
                    f.insn_count,
                    f.calls.len()
                ),
                col::faint(),
            ));
        },
    );
    if resp.clicked() {
        act.push(Action::SetGraphRoot(f.addr));
    }
    if resp.double_clicked() {
        act.push(Action::ShowRename {
            segment: f.addr.segment,
            offset: f.addr.offset,
        });
    }
}

/// A bar above the listing naming the function currently at the top.
fn sticky_header(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    rows: &[Row],
    funcs: &[&Function],
    row_h: f32,
) {
    let Some(tab) = app.tab() else { return };
    // The visible top row is derived from the scroll offset that egui stored
    // for this area on the previous frame.
    let offset = ui
        .ctx()
        .memory(|m| m.data.get_temp::<f32>(ui.id().with("disasm_scroll")))
        .unwrap_or(0.0);
    let top_row = (offset / row_h) as usize;
    let current = rows
        .iter()
        .take(top_row.min(rows.len()))
        .rev()
        .find_map(|r| match r {
            Row::Header(i) => Some(*i),
            _ => None,
        });
    let Some(fi) = current else { return };
    let f = funcs[fi];

    egui::Frame::new()
        .fill(col::raised())
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::inline(ui, Icon::Code, col::symbol());
                ui.label(mono_c(f.label(), col::symbol()).strong());
                ui.label(mono_c(f.addr.to_string(), col::faint()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("call graph").clicked() {
                        act.push(Action::SetGraphRoot(f.addr));
                    }
                });
            });
        });
    let _ = tab;
}

#[allow(clippy::too_many_arguments)]
fn insn_row(
    app: &SithApp,
    ui: &mut Ui,
    act: &mut Vec<Action>,
    segno: u16,
    insn: &Insn,
    byte_w: usize,
    gutter_w: f32,
    labels: &BTreeMap<u32, String>,
    highlight: Option<&str>,
    call: Option<&callargs::CallArgs>,
    loads: Option<usize>,
    row_h: f32,
) {
    let selected = app.tab().and_then(|t| t.sel) == Some(insn.offset);
    let matches_highlight = highlight.is_some_and(|h| {
        insn.fixup
            .as_ref()
            .is_some_and(|f| f.target.to_string() == h)
    }) && !selected;

    // A link inside the row handles its own click. The row's click area is
    // created after the content and sits on top of it, so without this the
    // row would also select itself and undo the jump the link just made.
    let (link_clicked, resp) = widgets::row_sized(
        ui,
        ui.id().with(("insn", segno, insn.offset)),
        row_h,
        selected,
        matches_highlight,
        |ui| {
            let mut consumed = false;
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add_space(gutter_w);
            if app.is_bookmarked(segno, insn.offset) {
                ui.label(mono_c("\u{25C6}", col::orange()));
            } else {
                ui.label(mono_c(" ", col::addr()));
            }
            ui.label(mono_c(format!("{:04X}", insn.offset), col::addr()));
            if app.show_bytes {
                ui.label(mono_c(format!("{:<byte_w$}", insn.hex()), col::bytes()));
            }
            // Splitting the mnemonic from its operands keeps a column of verbs
            // down the left, which is what makes a listing skimmable.
            let (mnem, ops) = insn.text.split_once(' ').unwrap_or((insn.text.as_str(), ""));
            ui.label(mono_c(format!("{mnem:<7}"), flow_color(insn.flow)));
            if !ops.is_empty() {
                ui.label(mono_c(ops, col::mnemonic()));
            }
            if let Some(f) = &insn.fixup {
                // A reconstructed call reads better than the bare symbol, so
                // it replaces the comment where one is available.
                let text = match call {
                    Some(c) => format!("; {}.{}", c.module, c.render()),
                    None => format!("; {}", f.target),
                };
                let mut r = widgets::link(ui, text, col::comment());
                if let Some(c) = call {
                    r = r.on_hover_text(format!(
                        "{}.{}\n\n{}{}",
                        c.module,
                        c.signature.render(),
                        c.render(),
                        if c.complete {
                            ""
                        } else {
                            "\n\nsome arguments were not literal pushes"
                        }
                    ));
                }
                if r.clicked() {
                    act.push(target_action(&f.target));
                    consumed = true;
                }
                if f.additive {
                    widgets::chip(ui, "additive", col::orange());
                }
            } else if let Some(t) = insn.near_target {
                if let Some(name) = labels.get(&t) {
                    if widgets::link(ui, format!("; {name}"), col::comment().gamma_multiply(0.75)).clicked()
                    {
                        act.push(Action::Goto(Addr {
                            segment: segno,
                            offset: t,
                        }));
                        consumed = true;
                    }
                }
            }
            // A call that loads a resource gets a link straight to it: the
            // artwork is usually what you actually wanted to look at.
            if let Some(res) = loads {
                if let Some(r) = app.doc().and_then(|d| d.ne.resources.get(res)) {
                    ui.add_space(2.0);
                    icons::inline(ui, icons::for_resource(r.type_id.as_id()), col::orange());
                    if widgets::link(
                        ui,
                        format!("{} {}", r.type_name(), r.res_id),
                        col::orange(),
                    )
                    .on_hover_text("open this resource")
                    .clicked()
                    {
                        act.push(Action::Go(Nav::Resource(res)));
                        consumed = true;
                    }
                }
            }
            // A note the user wrote outranks anything generated, so it sits
            // at the end of the line where the eye finishes.
            if let Some(note) = app.user_comment(segno, insn.offset) {
                ui.label(mono_c(format!("; {note}"), col::yellow()));
            }
            consumed
        },
    );
    if resp.clicked() && !link_clicked {
        act.push(Action::Select(insn.offset));
    }
    if resp.double_clicked() {
        act.push(Action::Select(insn.offset));
        act.push(Action::FollowSelection);
    }
    resp.context_menu(|ui| {
        if ui.button("Name this address…").clicked() {
            act.push(Action::ShowRename {
                segment: segno,
                offset: insn.offset,
            });
            ui.close();
        }
        let marked = app.is_bookmarked(segno, insn.offset);
        if ui
            .button(if marked { "Remove bookmark" } else { "Bookmark" })
            .clicked()
        {
            act.push(Action::ToggleBookmark {
                segment: segno,
                offset: insn.offset,
            });
            ui.close();
        }
        if ui.button("Copy address").clicked() {
            ui.ctx()
                .copy_text(format!("seg{segno:02}:{:04X}", insn.offset));
            ui.close();
        }
    });
}
