//! The new-project wizard.
//!
//! Starting a project on a Windows 3.x title means the same three decisions
//! every time: which folder the game lives in, which of the modules in it are
//! worth annotating, and what to call the result. The wizard asks exactly
//! those, in that order, and does the tedious part -- finding the NE binaries
//! among the data files -- itself.

use crate::icons::{self, Icon};
use crate::state::Action;
use crate::theme::{col, mono_c};
use crate::widgets;
use eframe::egui::{self, Ui};
use ne_core::ModuleSummary;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Source,
    Modules,
    Details,
}

impl Step {
    pub const ALL: [(Step, &'static str, &'static str); 3] = [
        (Step::Source, "Source", "where the binaries are"),
        (Step::Modules, "Modules", "which ones to include"),
        (Step::Details, "Details", "name and location"),
    ];

    fn index(self) -> usize {
        match self {
            Step::Source => 0,
            Step::Modules => 1,
            Step::Details => 2,
        }
    }
}

pub struct Wizard {
    pub step: Step,
    /// Folder or file the scan started from.
    pub source: Option<PathBuf>,
    /// Everything the scan found, in path order.
    pub found: Vec<ModuleSummary>,
    /// Parallel to `found`: whether each module goes into the project.
    pub picked: Vec<bool>,
    pub filter: String,
    pub name: String,
    pub save_to: Option<PathBuf>,
    /// Set while a scan has been requested but not yet run.
    pub scanning: bool,
    pub message: Option<String>,
    /// Path typed or pasted into the source box.
    pub typed: String,
}

impl Default for Wizard {
    fn default() -> Self {
        Wizard {
            step: Step::Source,
            source: None,
            found: Vec::new(),
            picked: Vec::new(),
            filter: String::new(),
            name: String::new(),
            save_to: None,
            scanning: false,
            message: None,
            typed: String::new(),
        }
    }
}

/// What the set of scanned modules says about itself.
///
/// A folder of Win16 binaries has a shape: one or two applications, a handful
/// of libraries they pull in, and usually something that is not referenced at
/// all. Saying so up front is what turns a file list into a decision.
pub struct Digest {
    pub apps: usize,
    pub libs: usize,
    pub total_bytes: u64,
    pub selected_bytes: u64,
    /// Module name -> how many *selected* modules import it.
    pub used_by: std::collections::HashMap<String, usize>,
    /// Modules with the same name in different files.
    pub duplicates: Vec<String>,
    /// Imported modules that are not among the files found: the system DLLs,
    /// and anything genuinely missing.
    pub external: Vec<String>,
}

impl Wizard {
    /// Cross-reference the scan: who imports whom, and what is missing.
    pub fn digest(&self) -> Digest {
        use std::collections::{HashMap, HashSet};
        let present: HashSet<String> = self
            .found
            .iter()
            .map(|m| m.module.to_ascii_uppercase())
            .collect();

        let mut used_by: HashMap<String, usize> = HashMap::new();
        let mut external: HashSet<String> = HashSet::new();
        for (m, on) in self.found.iter().zip(&self.picked) {
            for imp in &m.imports {
                let key = imp.to_ascii_uppercase();
                if present.contains(&key) {
                    if *on {
                        *used_by.entry(key).or_insert(0) += 1;
                    }
                } else {
                    external.insert(key);
                }
            }
        }

        let mut seen: HashMap<String, usize> = HashMap::new();
        for m in &self.found {
            *seen.entry(m.module.to_ascii_uppercase()).or_insert(0) += 1;
        }
        let mut duplicates: Vec<String> = seen
            .into_iter()
            .filter(|(_, n)| *n > 1)
            .map(|(k, _)| k)
            .collect();
        duplicates.sort();

        let mut external: Vec<String> = external.into_iter().collect();
        external.sort();

        Digest {
            apps: self.found.iter().filter(|m| !m.is_library).count(),
            libs: self.found.iter().filter(|m| m.is_library).count(),
            total_bytes: self.found.iter().map(|m| m.file_size).sum(),
            selected_bytes: self.selected().map(|m| m.file_size).sum(),
            used_by,
            duplicates,
            external,
        }
    }

    /// Run the scan and pre-select everything, which is right far more often
    /// than not: a project usually wants the whole game, and unticking a few
    /// is less work than ticking twenty.
    pub fn scan(&mut self, root: PathBuf) {
        self.found = ne_core::scan_dir(&root);
        self.picked = vec![true; self.found.len()];
        self.message = Some(match self.found.len() {
            0 => format!("no NE binaries under {}", root.display()),
            1 => "1 NE binary found".to_string(),
            n => format!("{n} NE binaries found"),
        });
        if self.name.is_empty() {
            // A folder name is almost always the game's name; a single file
            // falls back to its own stem.
            let stem = if root.is_dir() {
                root.file_name()
            } else {
                root.file_stem()
            };
            self.name = stem
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "project".into());
        }
        let dir = if root.is_dir() {
            root.clone()
        } else {
            root.parent().map(PathBuf::from).unwrap_or_default()
        };
        self.save_to = Some(dir.join(format!("{}.sith", self.name)));
        self.source = Some(root);
        self.scanning = false;
    }

    pub fn selected(&self) -> impl Iterator<Item = &ModuleSummary> {
        self.found
            .iter()
            .zip(&self.picked)
            .filter(|(_, on)| **on)
            .map(|(m, _)| m)
    }

    pub fn selected_count(&self) -> usize {
        self.picked.iter().filter(|p| **p).count()
    }

    fn can_advance(&self) -> bool {
        match self.step {
            Step::Source => !self.found.is_empty(),
            Step::Modules => self.selected_count() > 0,
            Step::Details => !self.name.trim().is_empty() && self.save_to.is_some(),
        }
    }
}

pub fn show(w: &Wizard, ctx: &egui::Context, act: &mut Vec<Action>) {
    egui::Modal::new(egui::Id::new("wizard")).show(ctx, |ui| {
        ui.set_width(860.0);
        header(w, ui);

        // A fixed body height keeps the footer still between steps; a dialog
        // whose buttons move as you advance feels unfinished.
        egui::Frame::new()
            .fill(col::bg())
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_min_height(400.0);
                ui.set_max_height(400.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match w.step {
                            Step::Source => source_step(w, ui, act),
                            Step::Modules => modules_step(w, ui, act),
                            Step::Details => details_step(w, ui, act),
                        }
                    });
            });

        ui.add_space(10.0);
        footer(w, ui, act);
    });
}

fn header(w: &Wizard, ui: &mut Ui) {
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Plus, col::accent());
        ui.label(egui::RichText::new("New project").size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("step {} of 3", w.step.index() + 1))
                    .size(11.0)
                    .color(col::faint()),
            );
        });
    });
    ui.add_space(10.0);

    // A step strip rather than a bare title: it says how much is left.
    ui.horizontal(|ui| {
        for (i, (step, title, subtitle)) in Step::ALL.iter().enumerate() {
            let done = step.index() < w.step.index();
            let active = *step == w.step;
            let color = if active {
                col::accent()
            } else if done {
                col::green()
            } else {
                col::faint()
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                if active {
                    ui.painter()
                        .circle_filled(rect.center(), 9.0, color.gamma_multiply(0.22));
                }
                ui.painter()
                    .circle_stroke(rect.center(), 8.5, egui::Stroke::new(1.2, color));
                if done {
                    icons::draw(ui.painter(), rect.shrink(4.0), Icon::Back, color);
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        (i + 1).to_string(),
                        egui::FontId::monospace(11.0),
                        color,
                    );
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(egui::RichText::new(*title).size(12.5).strong().color(
                        if active {
                            col::text()
                        } else {
                            col::dim()
                        },
                    ));
                    ui.label(egui::RichText::new(*subtitle).size(10.0).color(col::faint()));
                });
            });
            if i < Step::ALL.len() - 1 {
                ui.add_space(10.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(46.0, 2.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(rect.center(), egui::vec2(46.0, 1.5)),
                    egui::CornerRadius::ZERO,
                    if done { col::green() } else { col::border() },
                );
                ui.add_space(10.0);
            }
        }
    });
    ui.add_space(12.0);
}

/// Explanatory prose, one voice throughout the dialog.
fn lede(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(col::dim()));
    ui.add_space(10.0);
}

/// A framed group with a small-caps heading, matching the inspector panel.
fn card<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.label(
        egui::RichText::new(title)
            .size(10.5)
            .strong()
            .color(col::faint()),
    );
    ui.add_space(3.0);
    let r = egui::Frame::new()
        .fill(col::panel())
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui)
        });
    ui.add_space(12.0);
    r.inner
}

/// A big figure with a caption, matching the overview's stat tiles.
fn stat(ui: &mut Ui, value: &str, label: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(col::raised())
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(14, 7))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(egui::RichText::new(value).size(17.0).strong().color(color));
                ui.label(egui::RichText::new(label).size(10.0).color(col::dim()));
            });
        });
}

/// The end of a path, which is the part that identifies it.
fn tail(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text
        .chars()
        .rev()
        .take(max - 1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("\u{2026}{kept}")
}

fn human(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.0} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

// ------------------------------------------------------------------ source

fn source_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    lede(
        ui,
        "Point at the folder the program was installed to. Everything under it \
         is searched and the data files are skipped.",
    );

    card(ui, "FOLDER", |ui| {
        // The path box mirrors the navigator's search field: an icon, a
        // frameless edit, and the action on the right.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            icons::inline(ui, Icon::Open, col::faint());
            let mut typed = w.typed.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut typed)
                    .desired_width(ui.available_width() - 150.0)
                    .hint_text("paste a path, or browse")
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace),
            );
            if typed != w.typed {
                act.push(Action::WizardTyped(typed.clone()));
            }
            let path = PathBuf::from(typed.trim());
            let usable = !typed.trim().is_empty() && path.exists();
            let submitted = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Browse…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        act.push(Action::WizardScan(p));
                    }
                }
                ui.add_enabled_ui(usable, |ui| {
                    if ui.button("Scan").clicked() || (usable && submitted) {
                        act.push(Action::WizardScan(path.clone()));
                    }
                });
            });
        });
        if !w.typed.trim().is_empty() && !PathBuf::from(w.typed.trim()).exists() {
            ui.add_space(4.0);
            ui.label(mono_c("no such path", col::red()));
        }
    });

    let Some(src) = &w.source else {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Nothing scanned yet")
                    .size(13.0)
                    .color(col::faint()),
            );
            ui.label(
                egui::RichText::new("choose a folder above to see what is in it")
                    .size(11.0)
                    .color(col::faint()),
            );
        });
        return;
    };

    if w.found.is_empty() {
        card(ui, "RESULT", |ui| {
            ui.label(mono_c(src.display().to_string(), col::dim()));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Nothing here is a 16-bit NE binary. A Windows 3.x program has \
                     .EXE, .DLL or .DRV files with an NE header, so a folder of \
                     32-bit or DOS executables comes up empty.",
                )
                .size(12.0)
                .color(col::orange()),
            );
        });
        return;
    }

    let d = w.digest();
    card(ui, "FOUND", |ui| {
        ui.label(mono_c(src.display().to_string(), col::dim()));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            stat(ui, &w.found.len().to_string(), "binaries", col::accent());
            stat(ui, &d.apps.to_string(), "applications", col::green());
            stat(ui, &d.libs.to_string(), "libraries", col::purple());
            stat(ui, &human(d.total_bytes), "on disk", col::text());
        });
    });

    // The applications are where reading starts, so name them.
    let apps: Vec<&ModuleSummary> = w.found.iter().filter(|m| !m.is_library).collect();
    if !apps.is_empty() {
        card(ui, "STARTS HERE", |ui| {
            for m in apps {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    icons::inline(ui, Icon::Module, col::green());
                    ui.label(mono_c(format!("{:<13}", m.module), col::text()));
                    ui.label(mono_c(format!("{:<16}", file_name(m)), col::faint()));
                    if !m.description.is_empty() {
                        ui.label(
                            egui::RichText::new(&m.description)
                                .size(11.5)
                                .color(col::dim()),
                        );
                    }
                });
            }
        });
    }

    if !d.external.is_empty() {
        card(ui, "IMPORTED FROM OUTSIDE THIS FOLDER", |ui| {
            ui.horizontal_wrapped(|ui| {
                for name in &d.external {
                    widgets::chip(ui, name, col::faint());
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "System modules, unless one of these is a component that lives \
                     somewhere else.",
                )
                .size(11.0)
                .color(col::faint()),
            );
        });
    }

    if !d.duplicates.is_empty() {
        card(ui, "SAME MODULE NAME, DIFFERENT FILES", |ui| {
            ui.horizontal_wrapped(|ui| {
                for name in &d.duplicates {
                    widgets::chip(ui, name, col::orange());
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Annotations are matched by path first, so these stay separate.")
                    .size(11.0)
                    .color(col::faint()),
            );
        });
    }
}

// ----------------------------------------------------------------- modules

fn modules_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    lede(
        ui,
        "Everything is included by default. Leave out what you will not be \
         reading; a module can always be opened later without being in the project.",
    );

    let d = w.digest();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        icons::inline(ui, Icon::Search, col::faint());
        let mut filter = w.filter.clone();
        ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("filter modules…")
                .desired_width(220.0)
                .frame(egui::Frame::NONE),
        );
        if filter != w.filter {
            act.push(Action::WizardFilter(filter));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("None").clicked() {
                act.push(Action::WizardSelectAll(false));
            }
            if ui.small_button("All").clicked() {
                act.push(Action::WizardSelectAll(true));
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} of {} · {}",
                    w.selected_count(),
                    w.found.len(),
                    human(d.selected_bytes)
                ))
                .size(11.5)
                .color(col::dim()),
            );
        });
    });
    ui.add_space(8.0);

    let needle = w.filter.to_ascii_lowercase();
    // Applications first: they are the way into everything else.
    let mut order: Vec<usize> = (0..w.found.len()).collect();
    order.sort_by_key(|i| {
        let m = &w.found[*i];
        (m.is_library, m.module.to_ascii_uppercase())
    });

    let mut heading: Option<bool> = None;
    for i in order {
        let m = &w.found[i];
        if !needle.is_empty()
            && !m.module.to_ascii_lowercase().contains(&needle)
            && !m.path.to_string_lossy().to_ascii_lowercase().contains(&needle)
        {
            continue;
        }
        if heading != Some(m.is_library) {
            heading = Some(m.is_library);
            ui.add_space(if m.is_library { 10.0 } else { 0.0 });
            ui.label(
                egui::RichText::new(if m.is_library {
                    "LIBRARIES"
                } else {
                    "APPLICATIONS"
                })
                .size(10.5)
                .strong()
                .color(col::faint()),
            );
            ui.add_space(3.0);
        }

        let on = w.picked.get(i).copied().unwrap_or(false);
        let uses = d
            .used_by
            .get(&m.module.to_ascii_uppercase())
            .copied()
            .unwrap_or(0);
        // Inclusion is shown by the tick and the text weight, not by tinting
        // the row: with everything included by default a tinted list reads as
        // one solid block.
        let (_, resp) = widgets::row(ui, ui.id().with(("wiz", i)), false, false, |ui| {
            ui.spacing_mut().item_spacing.x = 9.0;
            ui.add_space(2.0);
            // A drawn tick rather than egui's checkbox, so the row matches
            // every other list in the tool.
            let (box_rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_stroke(
                box_rect.shrink(1.0),
                egui::CornerRadius::same(3),
                egui::Stroke::new(1.0, if on { col::accent() } else { col::border() }),
                egui::StrokeKind::Inside,
            );
            if on {
                p.rect_filled(
                    box_rect.shrink(4.0),
                    egui::CornerRadius::same(1),
                    col::accent(),
                );
            }
            ui.label(mono_c(
                format!("{:<13}", m.module),
                if on { col::text() } else { col::faint() },
            ));
            widgets::chip(
                ui,
                if m.is_library { "DLL" } else { "EXE" },
                if m.is_library { col::purple() } else { col::green() },
            );
            ui.label(mono_c(format!("{:>8}", human(m.file_size)), col::dim()));
            ui.label(mono_c(
                format!("{:>3} seg  {:>3} exp  {:>3} res", m.segments, m.exports, m.resources),
                col::faint(),
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Who needs this module is the fact that decides whether to
                // keep it, so it goes on the row rather than in a tooltip.
                if m.is_library {
                    if uses > 0 {
                        widgets::chip(ui, &format!("used by {uses}"), col::accent());
                    } else {
                        widgets::chip(ui, "unreferenced", col::faint());
                    }
                }
                ui.label(mono_c(file_name(m), col::faint()));
            });
        });
        let resp = widgets::hover_card(
            resp,
            Some((
                Icon::Module,
                if m.is_library { col::purple() } else { col::green() },
            )),
            &m.module,
            if m.is_library { "library" } else { "application" },
            |ui| {
                widgets::hover_row(ui, "file", file_name(m), col::text());
                widgets::hover_row(ui, "size", human(m.file_size), col::text());
                widgets::hover_row(ui, "segments", m.segments.to_string(), col::text());
                widgets::hover_row(ui, "exports", m.exports.to_string(), col::text());
                widgets::hover_row(ui, "resources", m.resources.to_string(), col::text());
                if m.is_library {
                    widgets::hover_row(
                        ui,
                        "used by",
                        if uses > 0 {
                            format!("{uses} selected module(s)")
                        } else {
                            "nothing selected".to_string()
                        },
                        if uses > 0 { col::accent() } else { col::faint() },
                    );
                }
                if !m.imports.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("imports")
                            .size(10.0)
                            .color(col::faint()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for name in &m.imports {
                            widgets::chip(ui, name, col::comment());
                        }
                    });
                }
                if !m.description.is_empty() {
                    widgets::hover_note(ui, &m.description);
                }
            },
        );
        if resp.clicked() {
            act.push(Action::WizardToggle(i));
        }
    }
}

fn file_name(m: &ModuleSummary) -> String {
    m.path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

// ----------------------------------------------------------------- details

fn details_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    lede(
        ui,
        "The project file holds the names, notes and bookmarks you add. It is \
         plain JSON, so it diffs and merges like source.",
    );

    card(ui, "PROJECT", |ui| {
        ui.horizontal(|ui| {
            ui.label(mono_c(format!("{:<13}", "name"), col::faint()));
            let mut name = w.name.clone();
            ui.add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(300.0)
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("project name"),
            );
            if name != w.name {
                act.push(Action::WizardName(name));
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(mono_c(format!("{:<13}", "file"), col::faint()));
            // The path can be longer than the row; the end of it is the part
            // that identifies the file, so the front gives way.
            let full = w
                .save_to
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(choose a location)".into());
            ui.label(mono_c(tail(&full, 62), col::text()))
                .on_hover_text(&full);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Change…").clicked() {
                    let suggested = format!("{}.sith", w.name.trim());
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("sith project", &["sith"])
                        .set_file_name(&suggested)
                        .save_file()
                    {
                        act.push(Action::WizardSaveTo(p));
                    }
                }
            });
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Binary paths are stored relative to this file where possible, so \
                 the folder can be moved or checked in as a unit.",
            )
            .size(11.0)
            .color(col::faint()),
        );
    });

    let d = w.digest();
    card(ui, "SUMMARY", |ui| {
        ui.horizontal_wrapped(|ui| {
            stat(ui, &w.selected_count().to_string(), "modules", col::accent());
            stat(
                ui,
                &w.selected().map(|m| m.segments).sum::<usize>().to_string(),
                "segments",
                col::green(),
            );
            stat(
                ui,
                &w.selected().map(|m| m.exports).sum::<usize>().to_string(),
                "exports",
                col::purple(),
            );
            stat(
                ui,
                &w.selected().map(|m| m.resources).sum::<usize>().to_string(),
                "resources",
                col::orange(),
            );
            stat(ui, &human(d.selected_bytes), "on disk", col::text());
        });
    });

    card(ui, &format!("INCLUDES ({})", w.selected_count()), |ui| {
        for m in w.selected() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 9.0;
                icons::inline(
                    ui,
                    Icon::Module,
                    if m.is_library { col::purple() } else { col::green() },
                );
                ui.label(mono_c(format!("{:<13}", m.module), col::text()));
                widgets::chip(
                    ui,
                    if m.is_library { "DLL" } else { "EXE" },
                    if m.is_library { col::purple() } else { col::green() },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(mono_c(file_name(m), col::faint()));
                });
            });
        }
    });
}

fn footer(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            act.push(Action::WizardCancel);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let last = w.step == Step::Details;
            ui.add_enabled_ui(w.can_advance(), |ui| {
                if ui
                    .button(if last { "  Create project  " } else { "  Next  " })
                    .clicked()
                {
                    act.push(if last {
                        Action::WizardCreate
                    } else {
                        Action::WizardNext
                    });
                }
            });
            ui.add_enabled_ui(w.step != Step::Source, |ui| {
                if ui.button("Back").clicked() {
                    act.push(Action::WizardBack);
                }
            });
            if !w.can_advance() {
                ui.label(
                    egui::RichText::new(match w.step {
                        Step::Source => "scan a folder to continue",
                        Step::Modules => "select at least one module",
                        Step::Details => "a name and a location are needed",
                    })
                    .size(11.0)
                    .color(col::faint()),
                );
            }
        });
    });
}
