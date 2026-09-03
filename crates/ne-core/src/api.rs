//! Win16 API signatures and named constants.
//!
//! Knowing that a call goes to `KERNEL.GlobalAlloc` is useful; knowing that it
//! was handed `GMEM_MOVEABLE|GMEM_ZEROINIT` is what tells you what the code is
//! doing. The signature table is generated from Wine's 16-bit `.spec` files,
//! which record each entry point's calling convention and argument widths. The
//! constant table is curated: it binds a named flag or enumeration set to a
//! specific parameter of a specific function, which is knowledge no machine
//! -readable source carries.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const API_JSON: &str = include_str!("../data/win16_api.json");
const CONST_JSON: &str = include_str!("../data/win16_constants.json");

/// How arguments reach the callee, and therefore what order they are pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallConv {
    /// Left-to-right pushes, callee cleans up. The Win16 default.
    Pascal,
    /// Right-to-left pushes, caller cleans up.
    Cdecl,
    /// A register entry point, a stub, or something else with no stack shape.
    Other,
}

impl CallConv {
    fn parse(s: &str) -> CallConv {
        if s.starts_with("pascal") {
            CallConv::Pascal
        } else if s.starts_with("cdecl") || s.starts_with("varargs") {
            CallConv::Cdecl
        } else {
            CallConv::Other
        }
    }
}

/// One argument's width and shape, as Wine records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// 16-bit value.
    Word,
    /// 16-bit signed value.
    SWord,
    /// 32-bit value.
    Long,
    /// A far pointer to a string.
    Str,
    /// A far pointer.
    Ptr,
    /// A 16:16 pointer passed as a 32-bit value.
    SegPtr,
    Other,
}

impl ArgKind {
    fn parse(s: &str) -> ArgKind {
        match s {
            "word" => ArgKind::Word,
            "s_word" => ArgKind::SWord,
            "long" | "s_long" | "double" => ArgKind::Long,
            "str" | "wstr" => ArgKind::Str,
            "ptr" => ArgKind::Ptr,
            "segptr" | "segstr" => ArgKind::SegPtr,
            _ => ArgKind::Other,
        }
    }

    /// Stack words the argument occupies, which is what a caller pushes.
    pub fn words(self) -> usize {
        match self {
            ArgKind::Word | ArgKind::SWord => 1,
            _ => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ArgKind::Word => "word",
            ArgKind::SWord => "int",
            ArgKind::Long => "long",
            ArgKind::Str => "str",
            ArgKind::Ptr => "ptr",
            ArgKind::SegPtr => "segptr",
            ArgKind::Other => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    pub conv: CallConv,
    pub args: Vec<ArgKind>,
}

impl Signature {
    /// Total stack words the call consumes.
    pub fn stack_words(&self) -> usize {
        self.args.iter().map(|a| a.words()).sum()
    }

    /// A C-ish rendering, for tooltips and headers.
    pub fn render(&self) -> String {
        format!(
            "{}({})",
            self.name,
            self.args
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// A named set of constants bound to a parameter.
#[derive(Debug, Clone)]
pub struct ConstSet {
    pub name: String,
    /// Bits combine with `|`; an enumeration matches exactly.
    pub flags: bool,
    pub values: Vec<(u32, String)>,
}

impl ConstSet {
    /// Render `value` using this set, or `None` if nothing matches.
    pub fn decode(&self, value: u32) -> Option<String> {
        if !self.flags {
            return self
                .values
                .iter()
                .find(|(v, _)| *v == value)
                .map(|(_, n)| n.clone());
        }
        // Zero is only meaningful when the set names it explicitly, which is
        // how `GMEM_FIXED` and `MB_OK` are expressed.
        if value == 0 {
            return self
                .values
                .iter()
                .find(|(v, _)| *v == 0)
                .map(|(_, n)| n.clone());
        }
        let mut parts = Vec::new();
        let mut rest = value;
        // Widest masks first, so a composite name wins over its own bits.
        let mut sorted: Vec<&(u32, String)> = self.values.iter().filter(|(v, _)| *v != 0).collect();
        sorted.sort_by_key(|(v, _)| std::cmp::Reverse(v.count_ones()));
        for (bits, name) in sorted {
            if *bits != 0 && rest & bits == *bits {
                parts.push(name.clone());
                rest &= !bits;
            }
        }
        if parts.is_empty() {
            return None;
        }
        if rest != 0 {
            parts.push(format!("{rest:#X}"));
        }
        Some(parts.join("|"))
    }
}

/// Signatures and constants, keyed by `MODULE.ordinal` and `MODULE.Name`.
#[derive(Debug, Default, Clone)]
pub struct ApiDb {
    by_ordinal: HashMap<(String, u16), Signature>,
    sets: HashMap<String, ConstSet>,
    /// `MODULE.Name` -> per-parameter constant set names.
    params: HashMap<String, Vec<Option<String>>>,
}

impl ApiDb {
    /// The table compiled into the binary.
    pub fn embedded() -> &'static ApiDb {
        static DB: OnceLock<ApiDb> = OnceLock::new();
        DB.get_or_init(|| {
            let mut db = ApiDb::from_json(API_JSON).unwrap_or_default();
            db.load_constants(CONST_JSON).ok();
            db
        })
    }

    pub fn from_json(s: &str) -> Result<ApiDb, serde_json::Error> {
        #[derive(serde::Deserialize)]
        struct Raw {
            n: String,
            cc: String,
            a: Vec<String>,
        }
        let flat: HashMap<String, Raw> = serde_json::from_str(s)?;
        let mut db = ApiDb::default();
        for (key, raw) in flat {
            let Some((module, ord)) = key.rsplit_once('.') else {
                continue;
            };
            let Ok(ord) = ord.parse::<u16>() else { continue };
            db.by_ordinal.insert(
                (module.to_ascii_uppercase(), ord),
                Signature {
                    name: raw.n,
                    conv: CallConv::parse(&raw.cc),
                    args: raw.a.iter().map(|a| ArgKind::parse(a)).collect(),
                },
            );
        }
        Ok(db)
    }

    pub fn load_constants(&mut self, s: &str) -> Result<(), serde_json::Error> {
        #[derive(serde::Deserialize)]
        struct RawSet {
            kind: String,
            values: HashMap<String, String>,
        }
        #[derive(serde::Deserialize)]
        struct Raw {
            sets: HashMap<String, RawSet>,
            params: HashMap<String, Vec<Option<String>>>,
        }
        let raw: Raw = serde_json::from_str(s)?;
        for (name, set) in raw.sets {
            let mut values: Vec<(u32, String)> = set
                .values
                .iter()
                .filter_map(|(k, v)| parse_u32(k).map(|n| (n, v.clone())))
                .collect();
            values.sort_by_key(|(v, _)| *v);
            self.sets.insert(
                name.clone(),
                ConstSet {
                    name,
                    flags: set.kind == "flags",
                    values,
                },
            );
        }
        self.params = raw.params;
        Ok(())
    }

    pub fn load_file(&mut self, path: &Path) -> std::io::Result<()> {
        let text = std::fs::read_to_string(path)?;
        self.load_constants(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn signature(&self, module: &str, ordinal: u16) -> Option<&Signature> {
        self.by_ordinal
            .get(&(module.to_ascii_uppercase(), ordinal))
    }

    /// Signature by name, for imports referenced by string rather than ordinal.
    pub fn signature_by_name(&self, module: &str, name: &str) -> Option<&Signature> {
        let m = module.to_ascii_uppercase();
        self.by_ordinal
            .iter()
            .find(|((mm, _), sig)| *mm == m && sig.name.eq_ignore_ascii_case(name))
            .map(|(_, sig)| sig)
    }

    /// The constant set bound to parameter `index` of `MODULE.Name`.
    pub fn param_set(&self, module: &str, name: &str, index: usize) -> Option<&ConstSet> {
        let key = format!("{}.{}", module.to_ascii_uppercase(), name);
        let set_name = self.params.get(&key)?.get(index)?.as_ref()?;
        self.sets.get(set_name)
    }

    pub fn set(&self, name: &str) -> Option<&ConstSet> {
        self.sets.get(name)
    }

    pub fn len(&self) -> usize {
        self.by_ordinal.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_ordinal.is_empty()
    }

    pub fn constant_set_count(&self) -> usize {
        self.sets.len()
    }
}

fn parse_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).ok()
    } else {
        t.parse().ok()
    }
}
