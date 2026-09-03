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

    /// The type a Windows SDK header would have written.
    pub fn c_type(self) -> &'static str {
        match self {
            ArgKind::Word => "WORD",
            ArgKind::SWord => "int",
            ArgKind::Long => "LONG",
            ArgKind::Str => "LPCSTR",
            ArgKind::Ptr => "LPVOID",
            ArgKind::SegPtr => "DWORD",
            // Nothing is known about it beyond its width, and saying WORD
            // would be a guess dressed as a fact.
            ArgKind::Other => "DWORD",
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
    /// Return type, where the curated overlay supplies one.
    pub ret: Option<String>,
    /// Parameter names, where the curated overlay supplies them. Always the
    /// same length as `args` when present.
    pub params: Vec<String>,
}

impl Signature {
    /// Total stack words the call consumes.
    pub fn stack_words(&self) -> usize {
        self.args.iter().map(|a| a.words()).sum()
    }

    /// Name of parameter `index`, where one is known.
    pub fn param_name(&self, index: usize) -> Option<&str> {
        self.params.get(index).map(String::as_str)
    }

    /// The declaration a Windows SDK header would carry for this function.
    ///
    /// Reconstructed from the signature database, which knows argument widths
    /// and, for the curated entries, parameter names. Widths are not types:
    /// a `WORD` here may have been an `HWND` or a `BOOL`, and the real header
    /// this stands in for would have said which.
    pub fn prototype(&self) -> String {
        let conv = match self.conv {
            CallConv::Pascal => "FAR PASCAL",
            CallConv::Cdecl => "FAR CDECL",
            CallConv::Other => "FAR",
        };
        let args = if self.args.is_empty() {
            "void".to_owned()
        } else {
            self.args
                .iter()
                .enumerate()
                .map(|(i, a)| match self.param_name(i) {
                    Some(n) => format!("{} {n}", a.c_type()),
                    None => a.c_type().to_owned(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let ret = self.ret.as_deref().unwrap_or("WORD");
        format!("{ret} {conv} {}({args});", self.name)
    }

    /// A C-ish rendering, for tooltips and headers.
    pub fn render(&self) -> String {
        let args = self
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| match self.param_name(i) {
                Some(n) => format!("{} {n}", a.as_str()),
                None => a.as_str().to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        match &self.ret {
            Some(r) => format!("{r} {}({args})", self.name),
            None => format!("{}({args})", self.name),
        }
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

/// The Windows SDK header that declared a module's exports.
///
/// Only the modules that had one. A DLL belonging to the program being read
/// has no standard header, and claiming otherwise would send someone looking
/// for a file that was never shipped.
pub fn header_for(module: &str) -> Option<&'static str> {
    Some(match module.to_ascii_uppercase().as_str() {
        "KERNEL" | "USER" | "GDI" | "KEYBOARD" | "SOUND" | "SYSTEM" | "DISPLAY" | "MOUSE"
        | "COMM" => "windows.h",
        "MMSYSTEM" | "MCIAVI" | "MCIANIM" | "MCICDA" | "MCISEQ" | "MCIWAVE" => "mmsystem.h",
        "COMMDLG" => "commdlg.h",
        "TOOLHELP" => "toolhelp.h",
        "SHELL" => "shellapi.h",
        "VER" => "ver.h",
        "LZEXPAND" => "lzexpand.h",
        "DDEML" => "ddeml.h",
        "STRESS" => "stress.h",
        "WING" => "wing.h",
        "DISPDIB" => "dispdib.h",
        "MSVIDEO" | "AVIFILE" => "vfw.h",
        "OLE2" | "OLECLI" | "OLESVR" | "COMPOBJ" | "STORAGE" => "ole.h",
        "WINSPOOL" => "winspool.h",
        "WIN87EM" | "WPROCS" => return None,
        _ => return None,
    })
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
            name: String,
            conv: String,
            #[serde(default)]
            ret: Option<String>,
            #[serde(default)]
            args: Vec<String>,
            #[serde(default)]
            params: Vec<String>,
        }
        // The file carries a `_comment` key; JSON has no comments, so keys
        // beginning with an underscore are documentation and are skipped.
        let flat: HashMap<String, serde_json::Value> = serde_json::from_str(s)?;
        let mut db = ApiDb::default();
        for (key, value) in flat {
            if key.starts_with('_') {
                continue;
            }
            let Some((module, ord)) = key.rsplit_once('.') else {
                continue;
            };
            let Ok(ord) = ord.parse::<u16>() else { continue };
            let raw: Raw = serde_json::from_value(value)?;
            let args: Vec<ArgKind> = raw.args.iter().map(|a| ArgKind::parse(a)).collect();
            // A stale overlay whose names no longer line up is dropped: a
            // wrong parameter name is worse than none.
            let params = if raw.params.len() == args.len() {
                raw.params
            } else {
                Vec::new()
            };
            db.by_ordinal.insert(
                (module.to_ascii_uppercase(), ord),
                Signature {
                    name: raw.name,
                    conv: CallConv::parse(&raw.conv),
                    args,
                    ret: raw.ret,
                    params,
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
            #[serde(default, rename = "_comment")]
            _comment: String,
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

#[cfg(test)]
mod header_tests {
    use super::*;

    #[test]
    fn the_core_modules_share_one_header() {
        for m in ["KERNEL", "USER", "GDI", "kernel", "Gdi"] {
            assert_eq!(header_for(m), Some("windows.h"), "{m}");
        }
    }

    #[test]
    fn a_module_from_the_program_has_no_header() {
        // A DLL shipped with the game was never in the SDK, so there is no
        // file to include and saying otherwise sends someone looking for one.
        assert_eq!(header_for("WEP4UTIL"), None);
        assert_eq!(header_for("MYGAME"), None);
    }

    #[test]
    fn a_prototype_reads_as_a_declaration() {
        let sig = Signature {
            name: "SetErrorMode".into(),
            conv: CallConv::Pascal,
            args: vec![ArgKind::Word],
            ret: Some("WORD".into()),
            params: vec!["mode".into()],
        };
        assert_eq!(
            sig.prototype(),
            "WORD FAR PASCAL SetErrorMode(WORD mode);"
        );
    }

    #[test]
    fn a_function_taking_nothing_says_void() {
        let sig = Signature {
            name: "GetCurrentTask".into(),
            conv: CallConv::Pascal,
            args: vec![],
            ret: None,
            params: vec![],
        };
        // No return type on record falls back to the Win16 convention
        // rather than inventing one.
        assert_eq!(sig.prototype(), "WORD FAR PASCAL GetCurrentTask(void);");
    }
}
