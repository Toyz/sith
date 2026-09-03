//! Cross-module export index.
//!
//! Win16 code imports sibling DLLs by ordinal just as it does the system
//! modules, so an intermodule call reads as `MYENGINE.@107` unless the exports
//! of `MYENGINE.DLL` are known. Scanning the directory the binary ships in
//! recovers those names -- and, because the index keeps the file each module
//! came from and the address of each export, it is also what lets a tool
//! follow a call across file boundaries.

use crate::NeFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One export of an indexed module.
#[derive(Debug, Clone)]
pub struct ExportRef {
    pub ordinal: u16,
    pub name: Option<String>,
    pub segment: u16,
    pub offset: u16,
}

/// One indexed file.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Module name from the resident name table, upper-cased.
    pub module: String,
    pub path: PathBuf,
    pub is_library: bool,
    pub description: String,
    pub exports: HashMap<u16, ExportRef>,
}

impl ModuleInfo {
    pub fn by_name(&self, name: &str) -> Option<&ExportRef> {
        self.exports
            .values()
            .find(|e| e.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))
    }
}

/// What a directory scan reports about one NE file.
///
/// Enough to decide whether a module belongs in a project without opening it,
/// which is what both the `scan` command and the new-project wizard need.
#[derive(Debug, Clone)]
pub struct ModuleSummary {
    pub path: PathBuf,
    pub module: String,
    pub description: String,
    pub is_library: bool,
    pub file_size: u64,
    pub segments: usize,
    pub exports: usize,
    pub resources: usize,
    pub imports: Vec<String>,
}

/// Extensions worth opening. A directory beside a game executable is mostly
/// data files, and trying to parse all of them wastes time to no purpose.
pub fn is_candidate(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        ext.as_str(),
        "EXE" | "DLL" | "DRV" | "FON" | "MOD" | "OCX" | "VBX"
    )
}

/// Every NE binary under `root`, summarised, sorted by path.
///
/// Files that are not NE binaries are skipped silently: none of them failing
/// to parse is an error worth reporting.
pub fn scan_dir(root: &Path) -> Vec<ModuleSummary> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    if root.is_file() {
        stack.clear();
        if let Some(s) = summarise(root) {
            out.push(s);
        }
    }
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if is_candidate(&p) {
                if let Some(s) = summarise(&p) {
                    out.push(s);
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn summarise(path: &Path) -> Option<ModuleSummary> {
    let ne = NeFile::open(path).ok()?;
    Some(ModuleSummary {
        path: path.to_path_buf(),
        module: ne.module_name().to_string(),
        description: ne.description().to_string(),
        is_library: ne.header.is_library(),
        file_size: ne.buf.len() as u64,
        segments: ne.segments.len(),
        exports: ne.exports().len(),
        resources: ne.resources.len(),
        imports: ne.module_ref_names(),
    })
}

#[derive(Debug, Default, Clone)]
pub struct ExportIndex {
    modules: HashMap<String, ModuleInfo>,
    /// Files that parsed as NE, in scan order.
    pub scanned: Vec<PathBuf>,
}

impl ExportIndex {
    pub fn new() -> ExportIndex {
        ExportIndex::default()
    }

    /// Index every NE file under `root`, recursing into subdirectories.
    ///
    /// Files that are not NE binaries are skipped silently: the directory
    /// beside a game executable is full of data files, and none of them
    /// failing to parse is an error worth reporting.
    pub fn scan<P: AsRef<Path>>(&mut self, root: P) -> std::io::Result<&mut Self> {
        let root = root.as_ref();
        if root.is_file() {
            self.add_file(root);
            return Ok(self);
        }
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for p in paths {
                if p.is_dir() {
                    stack.push(p);
                } else {
                    self.add_file(&p);
                }
            }
        }
        Ok(self)
    }

    fn add_file(&mut self, path: &Path) {
        if !is_candidate(path) {
            return;
        }
        let Ok(ne) = NeFile::open(path) else { return };
        let key = ne.module_name().to_ascii_uppercase();
        let info = self.modules.entry(key.clone()).or_insert_with(|| ModuleInfo {
            module: key,
            path: path.to_path_buf(),
            is_library: ne.header.is_library(),
            description: ne.description().to_string(),
            exports: HashMap::new(),
        });
        for e in ne.entries.values() {
            info.exports.entry(e.ordinal).or_insert_with(|| ExportRef {
                ordinal: e.ordinal,
                name: e.name.clone(),
                segment: e.segment,
                offset: e.offset,
            });
        }
        self.scanned.push(path.to_path_buf());
    }

    pub fn lookup(&self, module: &str, ordinal: u16) -> Option<&str> {
        self.export(module, ordinal)?.name.as_deref()
    }

    pub fn export(&self, module: &str, ordinal: u16) -> Option<&ExportRef> {
        self.modules
            .get(&module.to_ascii_uppercase())?
            .exports
            .get(&ordinal)
    }

    /// Find an export by name, for following a by-name import.
    pub fn export_by_name(&self, module: &str, name: &str) -> Option<&ExportRef> {
        self.modules.get(&module.to_ascii_uppercase())?.by_name(name)
    }

    pub fn module(&self, module: &str) -> Option<&ModuleInfo> {
        self.modules.get(&module.to_ascii_uppercase())
    }

    /// The file a module was indexed from, so a caller can open it.
    pub fn path_of(&self, module: &str) -> Option<&Path> {
        Some(self.module(module)?.path.as_path())
    }

    pub fn modules(&self) -> impl Iterator<Item = &ModuleInfo> {
        self.modules.values()
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn len(&self) -> usize {
        self.modules.values().map(|m| m.exports.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
