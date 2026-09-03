//! Application state.
//!
//! Views render from an immutable borrow and queue [`Action`]s; the app
//! applies them after the frame. Nothing mutates the document while it is
//! being drawn, which keeps every view free of borrow gymnastics and makes
//! navigation from anywhere -- a listing, the navigator, a dialog -- go
//! through one path.

use eframe::egui;
use ne_analysis::{Addr, Program};
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
        Ok(Doc {
            path: path.to_path_buf(),
            ne,
            program,
            bits32,
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
    /// Pan and zoom rectangle owned by the scene widget.
    pub scene_rect: egui::Rect,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState {
            root: None,
            depth: 2,
            dir: GraphDir::Callees,
            show_imports: true,
            scene_rect: egui::Rect::ZERO,
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
    SetNavFilter(String),
    SetGotoText(String),
    SetPaletteText(String),
    PaletteMove(i32),
    PaletteChoose(usize),
    SetViewFilter(String),
    SetGraphRoot(Addr),
    SetGraphDepth(usize),
    SetGraphRect(egui::Rect),
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

    pub recent: Vec<PathBuf>,
    pub goto_open: bool,
    pub goto_text: String,
    pub palette_open: bool,
    pub palette_text: String,
    pub palette_sel: usize,
    /// Set while a dialog wants keyboard focus on its text field.
    pub focus_input: bool,
}

impl SithApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> SithApp {
        crate::theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut app = SithApp {
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
            recent: Vec::new(),
            goto_open: false,
            goto_text: String::new(),
            palette_open: false,
            palette_text: String::new(),
            palette_sel: 0,
            focus_input: false,
        };
        if let Some(p) = initial {
            app.open(&p);
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
        let bits32 = self.doc().map(|d| d.bits32.clone()).unwrap_or_default();
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
        self.tabs.push(Tab::new(doc, nav));
        self.active = self.tabs.len() - 1;
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
                self.tabs.push(Tab::new(i, Nav::Overview));
                self.active = self.tabs.len() - 1;
                self.error = None;
                self.textures.borrow_mut().clear();
                self.zoom_index.set(None);
                let p = self.docs[i].path.clone();
                self.recent.retain(|x| *x != p);
                self.recent.insert(0, p);
                self.recent.truncate(8);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn reanalyze(&mut self) {
        let Some(i) = self.tab().map(|t| t.doc) else { return };
        let Some(d) = self.docs.get_mut(i) else { return };
        d.program = Program::analyze(&d.ne, &d.bits32);
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
            .find(|f| f.label().eq_ignore_ascii_case(t))
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
            Action::SetNavFilter(f) => self.nav_filter = f,
            Action::SetGotoText(t) => self.goto_text = t,
            Action::SetPaletteText(t) => {
                self.palette_text = t;
                self.palette_sel = 0;
            }
            Action::PaletteMove(d) => {
                self.palette_sel = (self.palette_sel as i32 + d).max(0) as usize;
            }
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
                    // A new root needs a fresh view rectangle, otherwise the
                    // scene stays parked wherever the previous graph was.
                    t.graph.scene_rect = egui::Rect::ZERO;
                }
            }
            Action::SetGraphRect(r) => {
                if let Some(t) = self.tab_mut() {
                    t.graph.scene_rect = r;
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
                self.focus_input = true;
            }
            Action::Dismiss => {
                self.goto_open = false;
                self.palette_open = false;
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
        let doc = self.doc()?;
        let needle = self.palette_text.to_ascii_lowercase();
        let matches = |s: &str| needle.is_empty() || s.to_ascii_lowercase().contains(&needle);
        let mut hits: Vec<(String, Action)> = Vec::new();
        for f in &doc.program.functions {
            let label = f.label();
            if matches(&label) {
                hits.push((label, Action::Goto(f.addr)));
            }
            if hits.len() > 400 {
                break;
            }
        }
        for s in &doc.ne.segments {
            let label = format!("Segment {}", s.index);
            if matches(&label) {
                hits.push((label, Action::Go(Nav::Segment(s.index))));
            }
        }
        for (i, r) in doc.ne.resources.iter().enumerate() {
            let label = format!("{} {}", r.type_name(), r.res_id);
            if matches(&label) {
                hits.push((label, Action::Go(Nav::Resource(i))));
            }
        }
        for target in doc.program.xrefs.keys() {
            if target.contains('.') && matches(target) {
                hits.push((target.clone(), Action::Go(Nav::Xrefs(target.clone()))));
            }
        }
        hits.sort_by_key(|(l, _)| (l.len(), l.to_ascii_lowercase()));
        hits.truncate(200);
        if hits.is_empty() {
            return None;
        }
        Some(hits.swap_remove(index.min(hits.len() - 1)).1)
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
            .map(|f| (f.addr.offset, f.label()))
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
