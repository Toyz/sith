//! Names for imports referenced by ordinal.
//!
//! Almost every Win16 binary imports KERNEL/USER/GDI purely by ordinal, so
//! without a lookup table the disassembly reads `USER.@57` instead of
//! `USER.RegisterClass`. The shipped table is generated from Wine's 16-bit
//! `.spec` files (`tools/fetch_ordinals.py`), which are authoritative --
//! hand-written tables in circulation have GDI.27 as BitBlt when it is
//! Rectangle, and every annotation made against one of those is wrong.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../data/win16_ordinals.json");

/// `MODULE` -> ordinal -> export name.
#[derive(Debug, Default, Clone)]
pub struct OrdinalDb {
    modules: HashMap<String, HashMap<u16, String>>,
}

impl OrdinalDb {
    /// The table compiled into the binary.
    pub fn embedded() -> &'static OrdinalDb {
        static DB: OnceLock<OrdinalDb> = OnceLock::new();
        DB.get_or_init(|| OrdinalDb::from_json_str(EMBEDDED).unwrap_or_default())
    }

    /// Parse a `{"MODULE.ordinal": "Name"}` map.
    pub fn from_json_str(s: &str) -> Result<OrdinalDb, serde_json::Error> {
        let flat: HashMap<String, String> = serde_json::from_str(s)?;
        let mut db = OrdinalDb::default();
        for (key, name) in flat {
            let Some((module, ord)) = key.rsplit_once('.') else {
                continue;
            };
            let Ok(ord) = ord.parse::<u16>() else { continue };
            db.insert(module, ord, name);
        }
        Ok(db)
    }

    pub fn from_json_file(path: &Path) -> std::io::Result<OrdinalDb> {
        let text = std::fs::read_to_string(path)?;
        OrdinalDb::from_json_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn insert(&mut self, module: &str, ordinal: u16, name: String) {
        self.modules
            .entry(module.to_ascii_uppercase())
            .or_default()
            .insert(ordinal, name);
    }

    pub fn lookup(&self, module: &str, ordinal: u16) -> Option<&str> {
        self.modules
            .get(&module.to_ascii_uppercase())?
            .get(&ordinal)
            .map(String::as_str)
    }

    /// Entries from `other` win, so a project-specific table can correct the
    /// shipped one.
    pub fn merge(&mut self, other: &OrdinalDb) {
        for (module, table) in &other.modules {
            let dst = self.modules.entry(module.clone()).or_default();
            for (ord, name) in table {
                dst.insert(*ord, name.clone());
            }
        }
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn len(&self) -> usize {
        self.modules.values().map(HashMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn modules(&self) -> impl Iterator<Item = &str> {
        self.modules.keys().map(String::as_str)
    }
}
