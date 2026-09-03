//! The new-project wizard.
//!
//! Starting a project on a Windows 3.x title means the same three decisions
//! every time: which folder the game lives in, which of the modules in it are
//! worth annotating, and what to call the result. The wizard asks exactly
//! those, in that order, and does the tedious part -- finding the NE binaries
//! among the data files -- itself.

use crate::icons::{self, Icon};
use crate::state::Action;
use crate::theme::{col, dim, mono_c};
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

impl Wizard {
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
        ui.set_width(820.0);
        header(w, ui);
        crate::ui::sep(ui);

        egui::ScrollArea::vertical()
            .max_height(440.0)
            .auto_shrink([false, false])
            .show(ui, |ui| match w.step {
                Step::Source => source_step(w, ui, act),
                Step::Modules => modules_step(w, ui, act),
                Step::Details => details_step(w, ui, act),
            });

        crate::ui::sep(ui);
        footer(w, ui, act);
    });
}

fn header(w: &Wizard, ui: &mut Ui) {
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Overview, col::accent());
        ui.label(egui::RichText::new("New project").size(16.0).strong());
    });
    ui.add_space(8.0);
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
                ui.spacing_mut().item_spacing.x = 6.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter()
                    .circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.2, color));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if done { "✓".to_string() } else { (i + 1).to_string() },
                    egui::FontId::monospace(11.0),
                    color,
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(*title).size(12.5).color(if active {
                        col::text()
                    } else {
                        col::dim()
                    }));
                    ui.label(egui::RichText::new(*subtitle).size(10.0).color(col::faint()));
                });
            });
            if i < Step::ALL.len() - 1 {
                ui.add_space(6.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 2.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(rect.center(), egui::vec2(40.0, 1.5)),
                    egui::CornerRadius::ZERO,
                    if done { col::green() } else { col::border() },
                );
                ui.add_space(6.0);
            }
        }
    });
}

fn source_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.label(dim(
        "Point at the folder the program was installed to. Everything under it \
         is searched, and the data files are skipped.",
    ));
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("Choose a folder…").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                act.push(Action::WizardScan(p));
            }
        }
        if ui.button("Choose a single binary…").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("16-bit executables", &["exe", "dll", "drv", "EXE", "DLL", "DRV"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                act.push(Action::WizardScan(p));
            }
        }
    });

    ui.add_space(10.0);
    ui.label(dim("…or paste a path:"));
    ui.horizontal(|ui| {
        let mut typed = w.typed.clone();
        let r = ui.add(
            egui::TextEdit::singleline(&mut typed)
                .desired_width(520.0)
                .hint_text("/path/to/the/game")
                .font(egui::TextStyle::Monospace),
        );
        if typed != w.typed {
            act.push(Action::WizardTyped(typed.clone()));
        }
        let path = PathBuf::from(typed.trim());
        let usable = !typed.trim().is_empty() && path.exists();
        let submitted = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_enabled_ui(usable, |ui| {
            if ui.button("Scan").clicked() || (usable && submitted) {
                act.push(Action::WizardScan(path.clone()));
            }
        });
        if !typed.trim().is_empty() && !usable {
            ui.label(mono_c("no such path", col::red()));
        }
    });

    if let Some(src) = &w.source {
        ui.add_space(12.0);
        widgets::section(ui, "SCANNED");
        ui.label(mono_c(src.display().to_string(), col::text()));
        if let Some(msg) = &w.message {
            ui.label(
                egui::RichText::new(msg)
                    .color(if w.found.is_empty() {
                        col::orange()
                    } else {
                        col::green()
                    })
                    .size(12.0),
            );
        }
        if !w.found.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for m in w.found.iter().take(24) {
                    widgets::chip(
                        ui,
                        &m.module,
                        if m.is_library { col::purple() } else { col::green() },
                    );
                }
                if w.found.len() > 24 {
                    ui.label(dim(format!("and {} more", w.found.len() - 24)));
                }
            });
        }
    } else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(dim("nothing scanned yet"));
        });
    }
}

fn modules_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(dim(format!(
            "{} of {} selected",
            w.selected_count(),
            w.found.len()
        )));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("None").clicked() {
                act.push(Action::WizardSelectAll(false));
            }
            if ui.small_button("All").clicked() {
                act.push(Action::WizardSelectAll(true));
            }
            let mut filter = w.filter.clone();
            ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("filter…")
                    .desired_width(180.0),
            );
            if filter != w.filter {
                act.push(Action::WizardFilter(filter));
            }
        });
    });
    ui.add_space(6.0);

    let needle = w.filter.to_ascii_lowercase();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.add_space(26.0);
        ui.label(mono_c(format!("{:<14}", "module"), col::faint()));
        ui.label(mono_c(format!("{:<5}", "kind"), col::faint()));
        ui.label(mono_c(format!("{:>5}", "segs"), col::faint()));
        ui.label(mono_c(format!("{:>5}", "exp"), col::faint()));
        ui.label(mono_c(format!("{:>5}", "res"), col::faint()));
        ui.label(mono_c("file", col::faint()));
    });

    for (i, m) in w.found.iter().enumerate() {
        if !needle.is_empty()
            && !m.module.to_ascii_lowercase().contains(&needle)
            && !m.path.to_string_lossy().to_ascii_lowercase().contains(&needle)
        {
            continue;
        }
        let on = w.picked.get(i).copied().unwrap_or(false);
        let (_, resp) = widgets::row(ui, ui.id().with(("wiz", i)), on, i % 2 == 1, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add_space(4.0);
            let mut checked = on;
            if ui.checkbox(&mut checked, "").changed() {
                act.push(Action::WizardToggle(i));
            }
            ui.label(mono_c(
                format!("{:<14}", m.module),
                if on { col::text() } else { col::faint() },
            ));
            widgets::chip(
                ui,
                if m.is_library { "DLL" } else { "EXE" },
                if m.is_library { col::purple() } else { col::green() },
            );
            ui.label(mono_c(format!("{:>4}", m.segments), col::dim()));
            ui.label(mono_c(format!("{:>5}", m.exports), col::dim()));
            ui.label(mono_c(format!("{:>5}", m.resources), col::dim()));
            ui.label(mono_c(
                m.path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                col::faint(),
            ));
        });
        let resp = resp.on_hover_text(format!(
            "{}\n{}\n{} bytes\nimports: {}",
            m.path.display(),
            if m.description.is_empty() {
                "(no description)"
            } else {
                &m.description
            },
            m.file_size,
            m.imports.join(", ")
        ));
        if resp.clicked() {
            act.push(Action::WizardToggle(i));
        }
    }
}

fn details_step(w: &Wizard, ui: &mut Ui, act: &mut Vec<Action>) {
    ui.add_space(6.0);
    widgets::section(ui, "NAME");
    let mut name = w.name.clone();
    ui.add(
        egui::TextEdit::singleline(&mut name)
            .desired_width(360.0)
            .hint_text("project name"),
    );
    if name != w.name {
        act.push(Action::WizardName(name));
    }

    widgets::section(ui, "SAVE TO");
    ui.horizontal(|ui| {
        ui.label(mono_c(
            w.save_to
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(choose a location)".into()),
            col::text(),
        ));
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
    ui.label(dim(
        "Binary paths are stored relative to this file where possible, so the \
         folder can be moved or checked in as a unit.",
    ));

    widgets::section(ui, &format!("INCLUDES ({})", w.selected_count()));
    for m in w.selected() {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            icons::inline(
                ui,
                Icon::Module,
                if m.is_library { col::purple() } else { col::green() },
            );
            ui.label(mono_c(format!("{:<14}", m.module), col::text()));
            ui.label(mono_c(
                m.path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                col::faint(),
            ));
        });
    }
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
                    .button(if last { "Create project" } else { "Next" })
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
                ui.label(dim(match w.step {
                    Step::Source => "scan a folder to continue",
                    Step::Modules => "select at least one module",
                    Step::Details => "a name and a location are needed",
                }));
            }
        });
    });
}
