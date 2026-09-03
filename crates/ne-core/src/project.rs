//! Project files: the annotations a person adds to a binary.
//!
//! Everything a tool derives from an NE image can be recomputed at any time.
//! What cannot is the part that came out of someone's head -- that this
//! function is the sprite blitter, that this segment really holds 32-bit code,
//! that this address is worth coming back to. A project file holds exactly
//! that, keyed by address, so it survives re-analysis and can be shared or
//! kept in version control beside the binary.
//!
//! The format is JSON on purpose: it is readable, diffable, and mergeable by
//! hand when two people annotate the same binary.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bumped when the on-disk shape changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

/// An address inside a module, rendered as `SS:OOOO` so the file reads well
/// and sorts sensibly.
pub fn addr_key(segment: u16, offset: u32) -> String {
    format!("{segment:02}:{offset:04X}")
}

/// Parse a key written by [`addr_key`].
pub fn parse_addr_key(key: &str) -> Option<(u16, u32)> {
    let (seg, off) = key.split_once(':')?;
    Some((
        seg.trim().parse().ok()?,
        u32::from_str_radix(off.trim(), 16).ok()?,
    ))
}

/// Annotations for one binary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryNotes {
    /// Path as written in the project file. Relative paths are resolved
    /// against the project's own directory, so a project stays valid when the
    /// whole folder is moved or checked out somewhere else.
    pub path: PathBuf,
    /// Module name, recorded so a moved or renamed file can still be matched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub module: String,
    /// Segments the user has marked as holding 32-bit code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bits32: Vec<u16>,
    /// `SS:OOOO` -> the name the user gave that address.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// `SS:OOOO` -> a note attached to that address.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub comments: BTreeMap<String, String>,
    /// Addresses worth coming back to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<String>,
}

impl BinaryNotes {
    pub fn name_at(&self, segment: u16, offset: u32) -> Option<&str> {
        self.names.get(&addr_key(segment, offset)).map(String::as_str)
    }

    pub fn comment_at(&self, segment: u16, offset: u32) -> Option<&str> {
        self.comments
            .get(&addr_key(segment, offset))
            .map(String::as_str)
    }

    /// Set or clear a name. An empty name removes the entry rather than
    /// storing a blank, so the file does not accumulate noise.
    pub fn set_name(&mut self, segment: u16, offset: u32, name: &str) {
        let key = addr_key(segment, offset);
        if name.trim().is_empty() {
            self.names.remove(&key);
        } else {
            self.names.insert(key, name.trim().to_string());
        }
    }

    pub fn set_comment(&mut self, segment: u16, offset: u32, text: &str) {
        let key = addr_key(segment, offset);
        if text.trim().is_empty() {
            self.comments.remove(&key);
        } else {
            self.comments.insert(key, text.trim().to_string());
        }
    }

    pub fn toggle_bookmark(&mut self, segment: u16, offset: u32) -> bool {
        let key = addr_key(segment, offset);
        match self.bookmarks.iter().position(|b| *b == key) {
            Some(i) => {
                self.bookmarks.remove(i);
                false
            }
            None => {
                self.bookmarks.push(key);
                self.bookmarks.sort();
                true
            }
        }
    }

    pub fn is_bookmarked(&self, segment: u16, offset: u32) -> bool {
        self.bookmarks.contains(&addr_key(segment, offset))
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
            && self.comments.is_empty()
            && self.bookmarks.is_empty()
            && self.bits32.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub binaries: Vec<BinaryNotes>,
    /// Where the project was loaded from. Not part of the file.
    #[serde(skip)]
    pub path: Option<PathBuf>,
    /// Set when there are unsaved changes. Not part of the file.
    #[serde(skip)]
    pub dirty: bool,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            format_version: FORMAT_VERSION,
            name: String::new(),
            notes: String::new(),
            binaries: Vec::new(),
            path: None,
            dirty: false,
        }
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Project {
        Project {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn load(path: &Path) -> std::io::Result<Project> {
        let text = std::fs::read_to_string(path)?;
        let mut p: Project = serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if p.format_version > FORMAT_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "project format version {} is newer than this build understands ({FORMAT_VERSION})",
                    p.format_version
                ),
            ));
        }
        p.path = Some(path.to_path_buf());
        p.dirty = false;
        Ok(p)
    }

    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text + "\n")?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// Resolve a stored path against the project's own directory.
    pub fn resolve(&self, stored: &Path) -> PathBuf {
        if stored.is_absolute() {
            return stored.to_path_buf();
        }
        match self.path.as_ref().and_then(|p| p.parent()) {
            Some(dir) => dir.join(stored),
            None => stored.to_path_buf(),
        }
    }

    /// Store a path relative to the project where that is possible, so the
    /// project folder can be moved or shared as a unit.
    pub fn relativize(&self, absolute: &Path) -> PathBuf {
        let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) else {
            return absolute.to_path_buf();
        };
        match absolute.strip_prefix(dir) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => absolute.to_path_buf(),
        }
    }

    /// Notes for a binary, matched by path first and by module name second so
    /// a file that moved beside the project is still recognised.
    pub fn notes_for(&self, path: &Path, module: &str) -> Option<&BinaryNotes> {
        self.binaries
            .iter()
            .find(|b| self.resolve(&b.path) == path)
            .or_else(|| {
                self.binaries
                    .iter()
                    .find(|b| !b.module.is_empty() && b.module.eq_ignore_ascii_case(module))
            })
    }

    /// Notes for a binary, adding an entry if there is none yet.
    pub fn notes_mut(&mut self, path: &Path, module: &str) -> &mut BinaryNotes {
        let existing = self
            .binaries
            .iter()
            .position(|b| self.resolve(&b.path) == path)
            .or_else(|| {
                self.binaries
                    .iter()
                    .position(|b| !b.module.is_empty() && b.module.eq_ignore_ascii_case(module))
            });
        let index = match existing {
            Some(i) => i,
            None => {
                let stored = self.relativize(path);
                self.binaries.push(BinaryNotes {
                    path: stored,
                    module: module.to_string(),
                    ..Default::default()
                });
                self.binaries.len() - 1
            }
        };
        self.dirty = true;
        &mut self.binaries[index]
    }

    /// Total annotations across every binary, for status display.
    pub fn annotation_count(&self) -> usize {
        self.binaries
            .iter()
            .map(|b| b.names.len() + b.comments.len() + b.bookmarks.len())
            .sum()
    }

    // There is deliberately no "drop the empty entries" helper. An entry with
    // no annotations still records that the binary belongs to the project,
    // which is information in its own right: pruning them turns a project you
    // have not annotated yet into one that lists nothing at all.
}
