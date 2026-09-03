//! Application state.
//!
//! Views render from an immutable borrow and queue [`Action`]s; the app
//! applies them after the frame. Nothing mutates the document while it is
//! being drawn, which keeps every view free of borrow gymnastics and makes
//! navigation from anywhere -- a listing, the navigator, a dialog -- go
//! through one path.

use eframe::egui;
use ne_analysis::{Addr, Program};
use ne_core::project::Project;
use ne_core::{ExportIndex, NeFile};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// What a tab is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    Overview,
    Segment(u16),
    Resource(usize),
    Imports,
    Exports,
    Entries,
    Strings,
    /// The call-graph explorer.
    Graph,
    /// Cross-references, pre-filtered to a symbol when non-empty.
    Xrefs(String),
}

impl Nav {
    pub fn title(&self, doc: Option<&Doc>) -> String {
        match self {
            Nav::Overview => "Overview".into(),
            Nav::Segment(n) => format!("Segment {n}"),
            Nav::Resource(i) => doc
                .and_then(|d| d.ne.resources.get(*i))
                .map(|r| format!("{} {}", r.type_name(), r.res_id))
                .unwrap_or_else(|| format!("Resource {i}")),
            Nav::Imports => "Imports".into(),
            Nav::Exports => "Exports".into(),
            Nav::Entries => "Entry table".into(),
            Nav::Strings => "Strings".into(),
            Nav::Graph => "Call graph".into(),
            Nav::Xrefs(s) if s.is_empty() => "Xrefs".into(),
            Nav::Xrefs(s) => format!("Xrefs: {s}"),
        }
    }

    /// The icon for this view. A resource takes the icon of its type, which
    /// needs the document to look up.
    pub fn icon_for(&self, doc: Option<&Doc>) -> crate::icons::Icon {
        if let Nav::Resource(i) = self {
            let type_id = doc
                .and_then(|d| d.ne.resources.get(*i))
                .and_then(|r| r.type_id.as_id());
            return crate::icons::for_resource(type_id);
        }
        self.icon()
    }

    pub fn icon(&self) -> crate::icons::Icon {
        use crate::icons::Icon;
        match self {
            Nav::Overview => Icon::Overview,
            Nav::Segment(_) => Icon::Segment,
            Nav::Resource(_) => Icon::Resource,
            Nav::Imports => Icon::Import,
            Nav::Exports => Icon::Export,
            Nav::Entries => Icon::Entries,
            Nav::Strings => Icon::Strings,
            Nav::Graph => Icon::Graph,
            Nav::Xrefs(_) => Icon::Xref,
        }
    }
}

/// Which aspect of a segment a tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegTab {
    Disasm,
    Hex,
    Fixups,
    Strings,
}

impl SegTab {
    pub const ALL: [(SegTab, &'static str); 4] = [
        (SegTab::Disasm, "Disassembly"),
        (SegTab::Hex, "Hex"),
        (SegTab::Fixups, "Fixups"),
        (SegTab::Strings, "Strings"),
    ];
}

/// A loaded binary and everything derived from it.
pub struct Doc {
    pub path: PathBuf,
    pub ne: NeFile,
    pub program: Program,
    pub bits32: BTreeSet<u16>,
    /// Which code loads which resource, both ways round.
    pub res_links: ne_analysis::resrefs::ResourceLinks,
}

impl Doc {
    pub fn open(path: &Path, index: &ExportIndex, bits32: BTreeSet<u16>) -> Result<Doc, String> {
        let mut ne = NeFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // The workspace index names the ordinal exports of sibling modules,
        // which is what makes intermodule calls readable.
        if !index.is_empty() {
            ne.set_export_index(index.clone());
        }
        let program = Program::analyze(&ne, &bits32);
        let res_links =
            ne_analysis::resrefs::analyze(&ne, &program, ne_core::ApiDb::embedded());
        Ok(Doc {
            path: path.to_path_buf(),
            ne,
            program,
            bits32,
            res_links,
        })
    }

    /// Number of code bytes the analysis actually decoded.
    pub fn decoded_bytes(&self) -> usize {
        self.program
            .code
            .values()
            .flat_map(|c| c.insns.iter())
            .map(|i| i.len as usize)
            .sum()
    }
}

/// One open view, with its own history and selection.
pub struct Tab {
    /// Index into `SithApp::docs`. Tabs can show different files at once,
    /// which is what makes following a call into a sibling DLL useful.
    pub doc: usize,
    pub nav: Nav,
    pub seg_tab: SegTab,
    /// Selected offset in the current listing.
    pub sel: Option<u32>,
    /// Offset the listing should scroll to; consumed after one frame.
    pub scroll_to: Option<u32>,
    pub history: Vec<Nav>,
    pub forward: Vec<Nav>,
    /// Filter local to this view, distinct from the navigator's filter.
    pub filter: String,
    /// Call-graph explorer state.
    pub graph: GraphState,
}

/// Which direction the call-graph explorer expands from its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDir {
    Callees,
    Callers,
    Both,
}

impl GraphDir {
    pub const ALL: [(GraphDir, &'static str); 3] = [
        (GraphDir::Callees, "Calls"),
        (GraphDir::Callers, "Callers"),
        (GraphDir::Both, "Both"),
    ];
}

#[derive(Debug, Clone)]
pub struct GraphState {
    /// Function at the centre of the graph, or `None` to pick automatically.
    pub root: Option<Addr>,
    pub depth: usize,
    pub dir: GraphDir,
    /// Draw calls to imported symbols as leaf nodes.
    pub show_imports: bool,
    /// World point shown at the centre of the view.
    pub pan: egui::Vec2,
    pub zoom: f32,
    /// Cleared when the graph should re-frame itself, after a new root.
    pub framed: bool,
    /// A pending zoom step from the toolbar, applied where the clamping lives.
    pub zoom_nudge: f32,
    /// Where the user has dragged individual nodes, relative to where the
    /// layout put them. Keyed by node, so it survives a re-layout.
    pub moved: std::collections::HashMap<String, egui::Vec2>,
    /// The node currently being dragged.
    pub dragging: Option<String>,
    /// The node whose context menu is open.
    pub menu_for: Option<String>,
    /// The node the user last clicked. Selecting reads it in the inspector
    /// without moving the graph, which is what a click should do.
    pub selected: Option<Addr>,
    /// Columns the user has asked to see in full, overriding the cap.
    pub expanded: std::collections::HashSet<i32>,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState {
            root: None,
            // One level out is legible at a glance; a function with sixty
            // callees already fills the view at depth 1.
            depth: 1,
            dir: GraphDir::Callees,
            show_imports: true,
            pan: egui::Vec2::ZERO,
            zoom: 1.0,
            framed: false,
            zoom_nudge: 1.0,
            moved: Default::default(),
            dragging: None,
            menu_for: None,
            selected: None,
            expanded: Default::default(),
        }
    }
}

impl Tab {
    pub fn new(doc: usize, nav: Nav) -> Tab {
        Tab {
            doc,
            nav,
            seg_tab: SegTab::Disasm,
            sel: None,
            scroll_to: None,
            history: Vec::new(),
            forward: Vec::new(),
            filter: String::new(),
            graph: GraphState::default(),
        }
    }
}

/// Something the user asked for. Queued during rendering, applied after.
pub enum Action {
    Open(PathBuf),
    Reload,
    Go(Nav),
    GoNewTab(Nav),
    Goto(Addr),
    GotoNewTab(Addr),
    SegTab(SegTab),
    Select(u32),
    /// Move the selection by a number of rows, for keyboard navigation.
    MoveSelection(i32),
    /// Follow whatever the selected row points at.
    FollowSelection,
    ToggleBits32(u16),
    NewTab(Nav),
    CloseTab(usize),
    SelectTab(usize),
    Back,
    Forward,
    Status(String),
    SaveResource { index: usize, raw: bool },
    SaveListing,
    OpenModule {
        module: String,
        ordinal: Option<u16>,
        name: Option<String>,
    },
    ShowWizard,
    /// Drop a path from the recent list.
    ForgetRecent(PathBuf),
    /// Remove binaries the project refers to that are no longer on disk.
    DropMissingBinaries,
    /// Pick a binary and add it to the open project.
    AddBinaryToProject,
    DismissMissing,
    WizardScan(PathBuf),
    WizardNext,
    WizardBack,
    WizardCancel,
    WizardToggle(usize),
    WizardSelectAll(bool),
    WizardFilter(String),
    WizardTyped(String),
    WizardName(String),
    WizardSaveTo(PathBuf),
    WizardCreate,
    OpenProject,
    OpenProjectAt(PathBuf),
    SaveProject,
    SaveProjectAs,
    /// Open the rename box for an address.
    ShowRename { segment: u16, offset: u32 },
    SetName { segment: u16, offset: u32, name: String },
    SetComment { segment: u16, offset: u32, text: String },
    SetColor { segment: u16, offset: u32, color: Option<&'static str> },
    ToggleBookmark { segment: u16, offset: u32 },
    SetRenameText(String),
    SetTheme(&'static str),
    SetNavFilter(String),
    SetGotoText(String),
    SetPaletteText(String),
    PaletteMove(i32),
    PaletteScrolled,
    PaletteChoose(usize),
    SetViewFilter(String),
    SetGraphRoot(Addr),
    SetGraphDepth(usize),
    SetGraphView { pan: egui::Vec2, zoom: f32 },
    GraphFit,
    GraphZoom(f32),
    GraphDragStart(String),
    GraphDragBy(egui::Vec2),
    GraphDragEnd,
    GraphResetLayout,
    GraphMenuFor(Option<String>),
    GraphSelect(Option<Addr>),
    GraphExpandLevel(i32),
    ConsumeScroll,
    SetGraphDir(GraphDir),
    ToggleGraphImports,
    ToggleNavigator,
    ToggleInspector,
    ToggleBytes,
    ShowGoto,
    ShowPalette,
    Dismiss,
}

pub struct SithApp {
    /// Annotations the user has added: names, comments and bookmarks. Always
    /// present, so renaming works before a project has been saved anywhere.
    pub project: Project,
    /// Every file opened this session. Tabs index into this.
    pub docs: Vec<Doc>,
    /// Every NE file found beside the ones opened, so an import can be
    /// followed into the module that exports it.
    pub index: ExportIndex,
    /// Directories already scanned into `index`.
    scanned_dirs: Vec<PathBuf>,
    pub tabs: Vec<Tab>,
    pub active: usize,

    pub error: Option<String>,
    pub status: String,

    pub nav_filter: String,
    pub show_inspector: bool,
    pub show_navigator: bool,
    pub show_bytes: bool,
    pub min_string_len: usize,

    /// Texture cache for resource previews, keyed by resource index.
    pub textures: RefCell<HashMap<usize, egui::TextureHandle>>,
    pub image_zoom: Cell<f32>,
    pub zoom_index: Cell<Option<usize>>,

    /// Recently opened binaries and projects, newest first, persisted between
    /// runs so the start screen is useful on a cold launch.
    pub recent: Vec<RecentEntry>,
    pub goto_open: bool,
    pub goto_text: String,
    pub palette_open: bool,
    pub palette_text: String,
    pub palette_sel: usize,
    /// Set when the highlighted palette row must be scrolled back into view.
    pub palette_scroll: bool,
    /// A path the user could reasonably want dropped from the recent list,
    /// offered beside the error that revealed it.
    pub forget_candidate: Option<PathBuf>,
    /// Binaries the open project refers to that are not on disk.
    pub missing: Vec<PathBuf>,
    /// The new-project wizard, while it is open.
    pub wizard: Option<crate::wizard::Wizard>,
    /// Address the rename box is editing, if it is open.
    pub rename_at: Option<(u16, u32)>,
    pub rename_text: String,
    /// Set while a dialog wants keyboard focus on its text field.
    pub focus_input: bool,
    /// Name of the active theme, persisted between runs.
    pub theme: String,
    /// Set when the egui style must be rebuilt, after a theme change.
    pub restyle: bool,
}

impl SithApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> SithApp {
        let settings = load_settings();
        if !crate::theme::set_theme(&settings.theme) {
            crate::theme::set_theme(crate::theme::DEFAULT_THEME);
        }
        let theme_name = crate::theme::current().name.to_string();
        crate::theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut app = SithApp {
            project: Project::new("untitled"),
            docs: Vec::new(),
            index: ExportIndex::new(),
            scanned_dirs: Vec::new(),
            tabs: Vec::new(),
            active: 0,
            error: None,
            status: "no file loaded".into(),
            nav_filter: String::new(),
            show_inspector: true,
            show_navigator: true,
            show_bytes: true,
            min_string_len: 4,
            textures: Default::default(),
            image_zoom: Cell::new(1.0),
            zoom_index: Cell::new(None),
            recent: load_recent(),
            goto_open: false,
            goto_text: String::new(),
            palette_open: false,
            palette_text: String::new(),
            palette_sel: 0,
            palette_scroll: false,
            forget_candidate: None,
            missing: Vec::new(),
            wizard: None,
            rename_at: None,
            rename_text: String::new(),
            focus_input: false,
            theme: theme_name,
            restyle: false,
        };
        if let Some(p) = initial {
            // A project and a binary are both reasonable things to name on the
            // command line, so the extension decides which was meant.
            let is_project = p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("sith"));
            if is_project {
                app.open_project(&p);
            } else {
                app.open(&p);
            }
        }
        app
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active.min(self.tabs.len().saturating_sub(1)))
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        let i = self.active.min(self.tabs.len().saturating_sub(1));
        self.tabs.get_mut(i)
    }

    /// The document the active tab is showing.
    pub fn doc(&self) -> Option<&Doc> {
        self.docs.get(self.tab()?.doc)
    }

    /// What the active tab is showing, or `Nav::Overview` when nothing is open.
    pub fn nav(&self) -> Nav {
        self.tab().map(|t| t.nav.clone()).unwrap_or(Nav::Overview)
    }

    /// Scan a directory into the workspace index, once.
    fn index_dir(&mut self, dir: &Path) {
        if self.scanned_dirs.iter().any(|d| d == dir) {
            return;
        }
        self.scanned_dirs.push(dir.to_path_buf());
        let _ = self.index.scan(dir);
    }

    /// Load a file, reusing it if already open, and return its document index.
    fn load(&mut self, path: &Path) -> Result<usize, String> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.docs.iter().position(|d| d.path == path) {
            return Ok(i);
        }
        if let Some(dir) = path.parent() {
            self.index_dir(dir);
        }
        // A 32-bit segment marking is a judgement the user made, so it is
        // restored from the project rather than inherited from whatever file
        // happened to be open.
        let bits32 = self
            .project
            .binaries
            .iter()
            .find(|b| self.project.resolve(&b.path) == path)
            .map(|b| b.bits32.iter().copied().collect())
            .unwrap_or_default();
        let doc = Doc::open(&path, &self.index, bits32)?;
        self.docs.push(doc);
        Ok(self.docs.len() - 1)
    }

    /// Follow an import into the module that exports it.
    ///
    /// System modules are not present as files, so this reports what it
    /// could not find rather than failing silently.
    fn open_module(&mut self, module: &str, ordinal: Option<u16>, name: Option<String>) {
        let Some(path) = self.index.path_of(module).map(Path::to_path_buf) else {
            self.error = Some(format!(
                "{module} is not among the {} modules found beside this file",
                self.index.module_count()
            ));
            return;
        };
        let target = match (ordinal, name.as_deref()) {
            (Some(o), _) => self.index.export(module, o).cloned(),
            (None, Some(n)) => self.index.export_by_name(module, n).cloned(),
            _ => None,
        };
        let doc = match self.load(&path) {
            Ok(i) => i,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let nav = match &target {
            Some(e) => Nav::Segment(e.segment),
            None => Nav::Overview,
        };
        // Already open? Go to it rather than stacking another copy.
        if !self.focus_tab(doc, Some(&nav)) {
            self.tabs.push(Tab::new(doc, nav));
            self.active = self.tabs.len() - 1;
        }
        if let Some(e) = target {
            let off = e.offset as u32;
            if let Some(t) = self.tab_mut() {
                t.seg_tab = SegTab::Disasm;
                t.sel = Some(off);
                t.scroll_to = Some(off);
            }
            self.status = format!(
                "{module}.{} at seg{:02}:{:04X}",
                e.name.unwrap_or_else(|| format!("@{}", e.ordinal)),
                e.segment,
                e.offset
            );
        } else {
            self.status = format!("opened {module}");
        }
    }

    pub fn open(&mut self, path: &Path) {
        match self.load(path) {
            Ok(i) => {
                let d = &self.docs[i];
                self.status = format!(
                    "{}  —  {} segments, {} functions, {} resources",
                    d.ne.module_name(),
                    d.ne.segments.len(),
                    d.program.functions.len(),
                    d.ne.resources.len()
                );
                if !self.focus_tab(i, None) {
                    self.tabs.push(Tab::new(i, Nav::Overview));
                    self.active = self.tabs.len() - 1;
                }
                self.error = None;
                self.textures.borrow_mut().clear();
                self.zoom_index.set(None);
                let p = self.docs[i].path.clone();
                self.remember_recent(&p, false);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn reanalyze(&mut self) {
        let Some(i) = self.tab().map(|t| t.doc) else { return };
        let Some(d) = self.docs.get_mut(i) else { return };
        d.program = Program::analyze(&d.ne, &d.bits32);
        d.res_links = ne_analysis::resrefs::analyze(&d.ne, &d.program, ne_core::ApiDb::embedded());
    }

    /// The name to show for a function: the user's, if they gave one.
    pub fn label(&self, f: &ne_analysis::Function) -> String {
        self.user_name(f.addr.segment, f.addr.offset)
            .map(str::to_string)
            .unwrap_or_else(|| f.label())
    }

    /// A user-assigned name at an address, if there is one.
    pub fn user_name(&self, segment: u16, offset: u32) -> Option<&str> {
        let doc = self.doc()?;
        self.project
            .notes_for(&doc.path, doc.ne.module_name())?
            .name_at(segment, offset)
    }

    /// A user-written note at an address, if there is one.
    pub fn user_comment(&self, segment: u16, offset: u32) -> Option<&str> {
        let doc = self.doc()?;
        self.project
            .notes_for(&doc.path, doc.ne.module_name())?
            .comment_at(segment, offset)
    }

    /// The colour the user gave an address, resolved against the theme.
    ///
    /// Stored by name, so a project keeps its meaning when the theme changes.
    pub fn user_color(&self, segment: u16, offset: u32) -> Option<egui::Color32> {
        let doc = self.doc()?;
        let name = self
            .project
            .notes_for(&doc.path, doc.ne.module_name())?
            .color_at(segment, offset)?;
        crate::theme::named_color(name)
    }

    pub fn user_color_name(&self, segment: u16, offset: u32) -> Option<&str> {
        let doc = self.doc()?;
        self.project
            .notes_for(&doc.path, doc.ne.module_name())?
            .color_at(segment, offset)
    }

    pub fn is_bookmarked(&self, segment: u16, offset: u32) -> bool {
        let Some(doc) = self.doc() else { return false };
        self.project
            .notes_for(&doc.path, doc.ne.module_name())
            .is_some_and(|n| n.is_bookmarked(segment, offset))
    }

    /// Mutable notes for the active document, creating the entry on demand.
    fn notes_mut(&mut self) -> Option<&mut ne_core::BinaryNotes> {
        let (path, module) = {
            let doc = self.doc()?;
            (doc.path.clone(), doc.ne.module_name().to_string())
        };
        Some(self.project.notes_mut(&path, &module))
    }

    /// Resolve `seg:offset`, a bare offset, or a symbol name.
    pub fn resolve(&self, text: &str) -> Option<Addr> {
        let doc = self.doc()?;
        let t = text.trim();
        let parse = |s: &str| -> Option<u32> {
            let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
            u32::from_str_radix(s, 16).ok()
        };
        if let Some((seg, off)) = t.split_once(':') {
            let seg = seg.trim().trim_start_matches("seg");
            let seg: u16 = seg.parse().ok().or_else(|| u16::from_str_radix(seg, 16).ok())?;
            return Some(Addr {
                segment: seg,
                offset: parse(off)?,
            });
        }
        // A bare number is an offset in the segment currently shown.
        if let Nav::Segment(seg) = self.nav() {
            if let Some(off) = parse(t) {
                return Some(Addr {
                    segment: seg,
                    offset: off,
                });
            }
        }
        doc.program
            .functions
            .iter()
            .find(|f| self.label(f).eq_ignore_ascii_case(t) || f.label().eq_ignore_ascii_case(t))
            .map(|f| f.addr)
    }

    pub fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            self.apply_one(a);
        }
    }

    fn apply_one(&mut self, action: Action) {
        match action {
            Action::Open(p) => self.open(&p),
            Action::Reload => {
                if let Some(p) = self.doc().map(|d| d.path.clone()) {
                    // Reloading replaces the document in place so open tabs
                    // keep pointing at it.
                    let i = self.tab().map(|t| t.doc).unwrap_or(0);
                    let bits32 = self.docs[i].bits32.clone();
                    match Doc::open(&p, &self.index, bits32) {
                        Ok(d) => {
                            self.docs[i] = d;
                            self.textures.borrow_mut().clear();
                            self.status = format!("reloaded {}", p.display());
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Action::Go(nav) => self.go(nav),
            Action::GoNewTab(nav) => self.new_tab(nav),
            Action::NewTab(nav) => self.new_tab(nav),
            Action::Goto(addr) => {
                self.go(Nav::Segment(addr.segment));
                if let Some(t) = self.tab_mut() {
                    t.seg_tab = SegTab::Disasm;
                    t.scroll_to = Some(addr.offset);
                    t.sel = Some(addr.offset);
                }
            }
            Action::GotoNewTab(addr) => {
                self.new_tab(Nav::Segment(addr.segment));
                if let Some(t) = self.tab_mut() {
                    t.seg_tab = SegTab::Disasm;
                    t.scroll_to = Some(addr.offset);
                    t.sel = Some(addr.offset);
                }
            }
            Action::OpenModule {
                module,
                ordinal,
                name,
            } => self.open_module(&module, ordinal, name),
            Action::SegTab(tab) => {
                if let Some(t) = self.tab_mut() {
                    t.seg_tab = tab;
                    t.sel = None;
                }
            }
            Action::Select(off) => {
                if let Some(t) = self.tab_mut() {
                    t.sel = Some(off);
                }
            }
            Action::MoveSelection(delta) => self.move_selection(delta),
            Action::FollowSelection => self.follow_selection(),
            Action::ToggleBits32(seg) => {
                if let Some(i) = self.tab().map(|t| t.doc) {
                    if let Some(d) = self.docs.get_mut(i) {
                        if !d.bits32.remove(&seg) {
                            d.bits32.insert(seg);
                        }
                    }
                }
                self.reanalyze();
                let bits: Vec<u16> = self
                    .doc()
                    .map(|d| d.bits32.iter().copied().collect())
                    .unwrap_or_default();
                if let Some(n) = self.notes_mut() {
                    n.bits32 = bits;
                }
                self.autosave();
            }
            Action::CloseTab(i) => {
                if i < self.tabs.len() {
                    let doc = self.tabs[i].doc;
                    self.tabs.remove(i);
                    if i < self.active || self.active >= self.tabs.len() {
                        self.active = self.active.saturating_sub(1);
                    }
                    // Closing the last tab of a loaded file leaves nothing to
                    // look at, so a fresh overview takes its place.
                    if self.tabs.is_empty() && self.docs.get(doc).is_some() {
                        self.tabs.push(Tab::new(doc, Nav::Overview));
                        self.active = 0;
                    }
                }
            }
            Action::SelectTab(i) => self.active = i.min(self.tabs.len() - 1),
            Action::Back => {
                if let Some(t) = self.tab_mut() {
                    if let Some(prev) = t.history.pop() {
                        let cur = std::mem::replace(&mut t.nav, prev);
                        t.forward.push(cur);
                        t.sel = None;
                    }
                }
            }
            Action::Forward => {
                if let Some(t) = self.tab_mut() {
                    if let Some(next) = t.forward.pop() {
                        let cur = std::mem::replace(&mut t.nav, next);
                        t.history.push(cur);
                        t.sel = None;
                    }
                }
            }
            Action::Status(s) => self.status = s,
            Action::SaveResource { index, raw } => self.save_resource(index, raw),
            Action::SaveListing => self.save_listing(),
            Action::ShowWizard => self.wizard = Some(crate::wizard::Wizard::default()),
            Action::ForgetRecent(p) => {
                if self.forget_candidate.as_deref() == Some(p.as_path()) {
                    self.forget_candidate = None;
                    self.error = None;
                }
                self.recent.retain(|r| r.path != p);
                save_recent(&self.recent);
                self.status = format!("forgot {}", p.display());
            }
            Action::DropMissingBinaries => {
                // Annotations go with the entry, so this is only ever done on
                // an explicit choice, never as a side effect of opening.
                let missing: Vec<PathBuf> = self.missing.drain(..).collect();
                let removed = missing.len();
                let keep: Vec<bool> = self
                    .project
                    .binaries
                    .iter()
                    .map(|b| !missing.contains(&self.project.resolve(&b.path)))
                    .collect();
                let mut it = keep.into_iter();
                self.project.binaries.retain(|_| it.next().unwrap_or(true));
                self.project.dirty = true;
                self.autosave();
                self.status = format!(
                    "removed {removed} missing binar{} from the project",
                    if removed == 1 { "y" } else { "ies" }
                );
            }
            Action::DismissMissing => self.missing.clear(),
            Action::AddBinaryToProject => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "16-bit executables",
                        &["exe", "dll", "drv", "fon", "EXE", "DLL", "DRV", "FON"],
                    )
                    .add_filter("All files", &["*"])
                    .pick_file()
                else {
                    return;
                };
                self.open(&path);
                if let Some((p, m)) = self
                    .doc()
                    .map(|d| (d.path.clone(), d.ne.module_name().to_string()))
                {
                    let _ = self.project.notes_mut(&p, &m);
                    self.autosave();
                    self.status = format!("added {m} to {}", self.project.name);
                }
            }
            Action::WizardScan(root) => {
                if let Some(w) = &mut self.wizard {
                    w.scan(root);
                    // Landing straight on the module list is what the user
                    // came for; an empty scan stays put so the message is read.
                    if !w.found.is_empty() {
                        w.step = crate::wizard::Step::Modules;
                    }
                }
            }
            Action::WizardNext => {
                if let Some(w) = &mut self.wizard {
                    w.step = match w.step {
                        crate::wizard::Step::Source => crate::wizard::Step::Modules,
                        _ => crate::wizard::Step::Details,
                    };
                }
            }
            Action::WizardBack => {
                if let Some(w) = &mut self.wizard {
                    w.step = match w.step {
                        crate::wizard::Step::Details => crate::wizard::Step::Modules,
                        _ => crate::wizard::Step::Source,
                    };
                }
            }
            Action::WizardCancel => self.wizard = None,
            Action::WizardToggle(i) => {
                if let Some(w) = &mut self.wizard {
                    if let Some(p) = w.picked.get_mut(i) {
                        *p = !*p;
                    }
                }
            }
            Action::WizardSelectAll(on) => {
                if let Some(w) = &mut self.wizard {
                    w.picked.iter_mut().for_each(|p| *p = on);
                }
            }
            Action::WizardFilter(f) => {
                if let Some(w) = &mut self.wizard {
                    w.filter = f;
                }
            }
            Action::WizardTyped(t) => {
                if let Some(w) = &mut self.wizard {
                    w.typed = t;
                }
            }
            Action::WizardName(n) => {
                if let Some(w) = &mut self.wizard {
                    // The suggested filename tracks the name until the user
                    // picks a location of their own.
                    if let Some(path) = &w.save_to {
                        let dir = path.parent().map(PathBuf::from).unwrap_or_default();
                        w.save_to = Some(dir.join(format!("{}.sith", n.trim())));
                    }
                    w.name = n;
                }
            }
            Action::WizardSaveTo(p) => {
                if let Some(w) = &mut self.wizard {
                    w.save_to = Some(p);
                }
            }
            Action::WizardCreate => self.create_from_wizard(),
            Action::OpenProject => self.open_project_dialog(),
            Action::OpenProjectAt(p) => self.open_project(&p),
            Action::SaveProject => self.save_project(false),
            Action::SaveProjectAs => self.save_project(true),
            Action::ShowRename { segment, offset } => {
                self.rename_at = Some((segment, offset));
                self.rename_text = self
                    .user_name(segment, offset)
                    .map(str::to_string)
                    .or_else(|| {
                        self.doc()
                            .and_then(|d| {
                                d.program.function_at(ne_analysis::Addr { segment, offset })
                            })
                            .map(|f| f.label())
                    })
                    .unwrap_or_default();
                self.focus_input = true;
            }
            Action::SetName {
                segment,
                offset,
                name,
            } => {
                if let Some(n) = self.notes_mut() {
                    n.set_name(segment, offset, &name);
                }
                self.rename_at = None;
                self.autosave();
                self.status = if name.trim().is_empty() {
                    format!("cleared the name at seg{segment:02}:{offset:04X}")
                } else {
                    format!("named seg{segment:02}:{offset:04X} {name}")
                };
            }
            Action::SetComment {
                segment,
                offset,
                text,
            } => {
                if let Some(n) = self.notes_mut() {
                    n.set_comment(segment, offset, &text);
                }
                self.autosave();
            }
            Action::SetColor {
                segment,
                offset,
                color,
            } => {
                if let Some(n) = self.notes_mut() {
                    n.set_color(segment, offset, color);
                }
                self.autosave();
                self.status = match color {
                    Some(c) => format!("coloured seg{segment:02}:{offset:04X} {c}"),
                    None => format!("cleared the colour on seg{segment:02}:{offset:04X}"),
                };
            }
            Action::ToggleBookmark { segment, offset } => {
                let on = self
                    .notes_mut()
                    .map(|n| n.toggle_bookmark(segment, offset))
                    .unwrap_or(false);
                self.autosave();
                self.status = format!(
                    "{} seg{segment:02}:{offset:04X}",
                    if on { "bookmarked" } else { "un-bookmarked" }
                );
            }
            Action::SetRenameText(t) => self.rename_text = t,
            Action::SetTheme(name) => {
                if crate::theme::set_theme(name) {
                    self.theme = name.to_string();
                    self.restyle = true;
                    save_settings(&Settings {
                        theme: self.theme.clone(),
                    });
                    self.status = format!("theme: {name}");
                }
            }
            Action::SetNavFilter(f) => self.nav_filter = f,
            Action::SetGotoText(t) => self.goto_text = t,
            Action::SetPaletteText(t) => {
                self.palette_text = t;
                self.palette_sel = 0;
                self.palette_scroll = true;
            }
            Action::PaletteMove(d) => {
                self.palette_sel = (self.palette_sel as i32 + d).max(0) as usize;
                self.palette_scroll = true;
            }
            Action::PaletteScrolled => self.palette_scroll = false,
            Action::PaletteChoose(i) => self.palette_choose(i),
            Action::SetViewFilter(f) => {
                if let Some(t) = self.tab_mut() {
                    t.filter = f;
                }
            }
            Action::SetGraphRoot(a) => {
                if let Some(t) = self.tab_mut() {
                    t.nav = Nav::Graph;
                    t.graph.root = Some(a);
                    // Positions belong to the graph that was drawn, not to the
                    // next one, so a new root starts from the clean layout.
                    t.graph.moved.clear();
                    t.graph.dragging = None;
                    t.graph.expanded.clear();
                    t.graph.selected = Some(a);
                    // A new root needs re-framing, otherwise the view stays
                    // parked wherever the previous graph happened to be.
                    t.graph.framed = false;
                }
            }
            Action::SetGraphView { pan, zoom } => {
                if let Some(t) = self.tab_mut() {
                    t.graph.pan = pan;
                    t.graph.zoom = zoom;
                    t.graph.framed = true;
                    t.graph.zoom_nudge = 1.0;
                }
            }
            Action::GraphFit => {
                if let Some(t) = self.tab_mut() {
                    t.graph.framed = false;
                }
            }
            // Applied by the canvas on the next frame, where the viewport size
            // is known and the clamping lives.
            Action::GraphZoom(factor) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.zoom_nudge = factor;
                }
            }
            Action::GraphDragStart(key) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.dragging = Some(key);
                }
            }
            Action::GraphDragBy(delta) => {
                if let Some(t) = self.tab_mut() {
                    if let Some(key) = t.graph.dragging.clone() {
                        *t.graph.moved.entry(key).or_default() += delta;
                    }
                }
            }
            Action::GraphDragEnd => {
                if let Some(t) = self.tab_mut() {
                    t.graph.dragging = None;
                }
            }
            Action::GraphSelect(addr) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.selected = addr;
                }
            }
            Action::GraphExpandLevel(level) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.expanded.insert(level);
                    // The column is about to get much taller, so re-frame.
                    t.graph.framed = false;
                }
            }
            Action::GraphMenuFor(key) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.menu_for = key;
                }
            }
            Action::GraphResetLayout => {
                if let Some(t) = self.tab_mut() {
                    t.graph.moved.clear();
                    t.graph.dragging = None;
                    t.graph.expanded.clear();
                    t.graph.framed = false;
                }
            }
            Action::ConsumeScroll => {
                if let Some(t) = self.tab_mut() {
                    t.scroll_to = None;
                }
            }
            Action::SetGraphDepth(d) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.depth = d.clamp(1, 4);
                }
            }
            Action::SetGraphDir(d) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.dir = d;
                }
            }
            Action::ToggleGraphImports => {
                if let Some(t) = self.tab_mut() {
                    t.graph.show_imports = !t.graph.show_imports;
                }
            }
            Action::ToggleNavigator => self.show_navigator = !self.show_navigator,
            Action::ToggleInspector => self.show_inspector = !self.show_inspector,
            Action::ToggleBytes => self.show_bytes = !self.show_bytes,
            Action::ShowGoto => {
                self.goto_open = true;
                self.palette_open = false;
                self.focus_input = true;
            }
            Action::ShowPalette => {
                self.palette_open = true;
                self.goto_open = false;
                self.palette_sel = 0;
                self.palette_scroll = true;
                self.focus_input = true;
            }
            Action::Dismiss => {
                self.forget_candidate = None;
                self.goto_open = false;
                self.palette_open = false;
                self.rename_at = None;
                self.wizard = None;
                self.error = None;
            }
        }
    }

    /// Act on the palette entry at `index`, recomputing the same candidate
    /// list the dialog showed so the indices line up.
    fn palette_choose(&mut self, index: usize) {
        let Some(action) = self.palette_hit(index) else {
            return;
        };
        self.palette_open = false;
        self.apply_one(action);
    }

    fn palette_hit(&self, index: usize) -> Option<Action> {
        let mut hits = crate::palette::candidates(self, &self.palette_text);
        if hits.is_empty() {
            return None;
        }
        Some(hits.swap_remove(index.min(hits.len() - 1)).action)
    }

    pub fn go(&mut self, nav: Nav) {
        let Some(t) = self.tab_mut() else { return };
        if t.nav == nav {
            return;
        }
        let cur = std::mem::replace(&mut t.nav, nav);
        t.history.push(cur);
        t.forward.clear();
        t.sel = None;
        t.filter.clear();
    }

    fn new_tab(&mut self, nav: Nav) {
        let Some(doc) = self.tab().map(|t| t.doc) else { return };
        self.tabs.push(Tab::new(doc, nav));
        self.active = self.tabs.len() - 1;
    }

    /// Move the listing selection by whole rows, for arrow-key navigation.
    fn move_selection(&mut self, delta: i32) {
        let Some(doc) = self.doc() else { return };
        let Some(tab) = self.tab() else { return };
        let Nav::Segment(segno) = tab.nav else { return };
        let offsets: Vec<u32> = match tab.seg_tab {
            SegTab::Disasm => match doc.program.code.get(&segno) {
                Some(c) => c.insns.iter().map(|i| i.offset).collect(),
                None => return,
            },
            SegTab::Fixups => match doc.ne.segment(segno) {
                Some(s) => doc.ne.fixups(s).iter().map(|f| f.site as u32).collect(),
                None => return,
            },
            _ => return,
        };
        if offsets.is_empty() {
            return;
        }
        let cur = tab
            .sel
            .and_then(|s| offsets.binary_search(&s).ok())
            .unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, offsets.len() as i32 - 1) as usize;
        let off = offsets[next];
        if let Some(t) = self.tab_mut() {
            t.sel = Some(off);
            t.scroll_to = Some(off);
        }
    }

    /// Follow the reference on the selected row, if it has one.
    fn follow_selection(&mut self) {
        let Some(doc) = self.doc() else { return };
        let Some(tab) = self.tab() else { return };
        let (Nav::Segment(segno), Some(sel)) = (tab.nav.clone(), tab.sel) else {
            return;
        };
        let Some(code) = doc.program.code.get(&segno) else {
            return;
        };
        let Some(insn) = code.insns.iter().find(|i| i.offset == sel) else {
            return;
        };
        let action = match (&insn.fixup, insn.near_target) {
            (Some(f), _) => crate::views::disasm::target_action(&f.target),
            (None, Some(t)) => Action::Goto(Addr {
                segment: segno,
                offset: t,
            }),
            _ => return,
        };
        self.apply_one(action);
    }

    /// Record a path in the recent list and write it back to disk.
    fn remember_recent(&mut self, path: &Path, is_project: bool) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|r| r.path != path);
        self.recent.insert(
            0,
            RecentEntry {
                path,
                is_project,
                label: if is_project {
                    self.project.name.clone()
                } else {
                    self.docs
                        .iter()
                        .find(|d| d.path == *self.recent.first().map(|r| &r.path).unwrap_or(&PathBuf::new()))
                        .map(|d| d.ne.module_name().to_string())
                        .unwrap_or_default()
                },
            },
        );
        self.recent.truncate(12);
        save_recent(&self.recent);
    }

    /// Turn the wizard's choices into a project on disk, then open it.
    fn create_from_wizard(&mut self) {
        let Some(w) = self.wizard.take() else { return };
        let Some(path) = w.save_to.clone() else {
            self.wizard = Some(w);
            return;
        };
        let chosen: Vec<(PathBuf, String)> = w
            .selected()
            .map(|m| (m.path.clone(), m.module.clone()))
            .collect();
        if chosen.is_empty() {
            self.wizard = Some(w);
            return;
        }

        let mut project = Project::new(w.name.trim());
        // The path has to be set before any entry is added, or every binary is
        // stored absolute and the project stops being portable.
        project.path = Some(path.clone());
        for (p, m) in &chosen {
            let _ = project.notes_mut(p, m);
        }
        if let Err(e) = project.save(&path) {
            self.error = Some(format!("{}: {e}", path.display()));
            self.wizard = Some(w);
            return;
        }

        self.project = project;
        self.docs.clear();
        self.tabs.clear();
        self.textures.borrow_mut().clear();
        for (p, _) in &chosen {
            self.open(p);
        }
        // The first binary is the interesting one; leave the user there rather
        // than on whichever happened to be scanned last.
        self.active = 0;
        self.remember_recent(&path, true);
        self.status = format!(
            "created {} with {} binaries",
            path.display(),
            chosen.len()
        );
    }

    /// Write the project back to its file once it has one. Annotations are
    /// cheap to write and losing them to a crash is not acceptable, so this
    /// happens on every change rather than on demand.
    fn autosave(&mut self) {
        let Some(path) = self.project.path.clone() else {
            return;
        };
        if let Err(e) = self.project.save(&path) {
            self.error = Some(format!("{}: {e}", path.display()));
        }
    }

    fn open_project_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("sith project", &["sith", "json"])
            .pick_file()
        else {
            return;
        };
        self.open_project(&path);
    }

    /// Activate an existing tab showing `doc`, preferring one already on
    /// `nav`. Returns false when there is none.
    ///
    /// Opening something already open should take you to it. Only an explicit
    /// new-tab gesture -- middle-click, or the + button -- adds one.
    fn focus_tab(&mut self, doc: usize, nav: Option<&Nav>) -> bool {
        let exact = nav.and_then(|n| {
            self.tabs
                .iter()
                .position(|t| t.doc == doc && t.nav == *n)
        });
        let any = self.tabs.iter().position(|t| t.doc == doc);
        match exact.or(any) {
            Some(i) => {
                self.active = i;
                if let (Some(n), Some(t)) = (nav, self.tabs.get_mut(i)) {
                    if t.nav != *n {
                        let cur = std::mem::replace(&mut t.nav, n.clone());
                        t.history.push(cur);
                        t.forward.clear();
                        t.sel = None;
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Load a project and open every binary it refers to.
    pub fn open_project(&mut self, path: &Path) {
        match Project::load(path) {
            Ok(p) => {
                let binaries: Vec<PathBuf> =
                    p.binaries.iter().map(|b| p.resolve(&b.path)).collect();
                self.project = p;
                self.docs.clear();
                self.tabs.clear();
                self.textures.borrow_mut().clear();
                let empty = binaries.is_empty();
                let mut opened = 0;
                self.missing.clear();
                for b in &binaries {
                    if b.exists() {
                        self.open(b);
                        opened += 1;
                    } else {
                        // Not an error: a project is often opened on a machine
                        // that has only some of the binaries. It does need
                        // saying, though, and the annotations must not vanish.
                        self.missing.push(b.clone());
                    }
                }
                // Land on the first binary listed, not on whichever happened
                // to be opened last.
                self.active = 0;
                self.remember_recent(path, true);
                self.status = if empty {
                    // Loading a project that lists nothing leaves the window
                    // looking untouched, so it has to say so rather than
                    // dropping the user back on the start screen in silence.
                    format!("{} lists no binaries", self.project.name)
                } else {
                    format!(
                        "{} — {} annotations across {} binaries{}",
                        self.project.name,
                        self.project.annotation_count(),
                        self.project.binaries.len(),
                        if opened < binaries.len() {
                            format!(", {} missing", binaries.len() - opened)
                        } else {
                            String::new()
                        }
                    )
                };
            }
            Err(e) => {
                let missing = !path.exists();
                self.error = Some(if missing {
                    format!("{} is no longer there", path.display())
                } else {
                    format!("{}: {e}", path.display())
                });
                // A project that has been moved or deleted is the common case
                // here, so offer to stop listing it rather than just failing.
                if missing {
                    self.forget_candidate = Some(path.to_path_buf());
                }
            }
        }
    }

    fn save_project(&mut self, force_dialog: bool) {
        let path = if force_dialog || self.project.path.is_none() {
            let suggested = if self.project.name.is_empty() {
                "project.sith".to_string()
            } else {
                format!("{}.sith", self.project.name)
            };
            match rfd::FileDialog::new()
                .add_filter("sith project", &["sith"])
                .set_file_name(&suggested)
                .save_file()
            {
                Some(p) => p,
                None => return,
            }
        } else {
            self.project.path.clone().unwrap()
        };

        // Record every open binary so reopening the project restores them.
        let open: Vec<(PathBuf, String)> = self
            .docs
            .iter()
            .map(|d| (d.path.clone(), d.ne.module_name().to_string()))
            .collect();
        // The path must be set before relativising, or every entry is stored
        // absolute and the project stops being portable.
        self.project.path = Some(path.clone());
        for (p, m) in open {
            let _ = self.project.notes_mut(&p, &m);
        }
        if self.project.name.is_empty() {
            self.project.name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".into());
        }
        match self.project.save(&path) {
            Ok(()) => {
                self.remember_recent(&path, true);
                self.status = format!("saved {}", path.display());
            }
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }

    fn save_resource(&mut self, index: usize, raw: bool) {
        let Some(doc) = self.doc() else { return };
        let Some(r) = doc.ne.resources.get(index) else {
            return;
        };
        let name = if raw {
            format!("{}.bin", r.label())
        } else {
            format!("{}.{}", r.label(), r.extension())
        };
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
            return;
        };
        let bytes = if raw {
            doc.ne.resource_data(r).to_vec()
        } else {
            doc.ne.resource_file_bytes(r)
        };
        match std::fs::write(&path, &bytes) {
            Ok(()) => self.status = format!("wrote {} bytes to {}", bytes.len(), path.display()),
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }

    /// Write the current segment's listing to a text file.
    fn save_listing(&mut self) {
        let Some(doc) = self.doc() else { return };
        let Nav::Segment(segno) = self.nav() else {
            return;
        };
        let Some(code) = doc.program.code.get(&segno) else {
            return;
        };
        let labels: std::collections::BTreeMap<u32, String> = doc
            .program
            .functions
            .iter()
            .filter(|f| f.addr.segment == segno)
            .map(|f| (f.addr.offset, self.label(f)))
            .collect();
        let width = ne_disasm::byte_column_width(&code.insns);
        let mut out = format!(
            "; {} segment {} ({}-bit)\n",
            doc.ne.module_name(),
            segno,
            code.bits
        );
        for insn in &code.insns {
            if let Some(name) = labels.get(&insn.offset) {
                out.push_str(&format!("\n{name}:\n"));
            }
            out.push_str(&insn.render(width));
            out.push('\n');
        }
        let name = format!("{}_seg{segno:02}.asm", doc.ne.module_name());
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
            return;
        };
        match std::fs::write(&path, out) {
            Ok(()) => self.status = format!("wrote listing to {}", path.display()),
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }
}


/// One entry in the persisted recent list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    #[serde(default)]
    pub is_project: bool,
    /// Module or project name, so the start screen does not have to open every
    /// file just to label the list.
    #[serde(default)]
    pub label: String,
}

impl RecentEntry {
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

/// Where the recent list is kept. Follows the XDG layout on Linux and falls
/// back to the home directory elsewhere; a missing or unreadable file simply
/// means an empty list.
fn recent_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    Some(dir.join("sith").join("recent.json"))
}

fn load_recent() -> Vec<RecentEntry> {
    let Some(path) = recent_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_recent(entries: &[RecentEntry]) {
    let Some(path) = recent_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(path, text);
    }
}


/// Persisted preferences. Kept beside the recent list, and deliberately small:
/// anything that can be recomputed does not belong here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: crate::theme::DEFAULT_THEME.to_string(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    recent_path().map(|p| p.with_file_name("settings.json"))
}

fn load_settings() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_settings(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path, text);
    }
}
