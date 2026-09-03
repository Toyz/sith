//! Linking resources to the code that loads them.
//!
//! A resource is inert until something asks for it, and the ask is always the
//! same shape: a `LoadBitmap`, `DialogBox` or `LoadString` call with the
//! resource's id or name as an argument. Recovering that link is what lets a
//! bitmap say which function draws it, and a call say which artwork it pulls.
//!
//! Two idioms have to be handled, because both are common:
//!
//! - **By id.** The id is pushed as a literal, so the reconstructed call
//!   carries it directly.
//! - **By name.** A far pointer to a string is pushed instead. The string
//!   lives in a data segment, so the name is matched against the segment's
//!   text and the code that loads that offset is found through the constant
//!   references the disassembly already collects.
//!
//! Both are heuristics and are reported as such: an id match is strong, a name
//! match is only as good as the string search behind it.

use crate::{callargs, Addr, Program};
use ne_core::api::ApiDb;
use ne_core::resource::rt;
use ne_core::{NeFile, ResId};
use std::collections::HashMap;

/// How a reference was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The resource id was a literal argument to a loading call.
    Id,
    /// The resource's name appears as a string that this code loads.
    Name,
}

#[derive(Debug, Clone)]
pub struct ResourceUse {
    pub addr: Addr,
    /// The API that does the loading, e.g. `LoadBitmap`.
    pub api: String,
    pub confidence: Confidence,
    /// Which string was asked for, when the loader was `LoadString`.
    ///
    /// A string resource is a block of sixteen, so knowing the block is only
    /// a sixteenth of the answer. The id the call actually passed is what
    /// ties a line of code to a line of text.
    pub string_id: Option<u16>,
}

#[derive(Debug, Default, Clone)]
pub struct ResourceLinks {
    /// Resource index -> the code that loads it.
    pub by_resource: HashMap<usize, Vec<ResourceUse>>,
    /// Code address -> the resource it loads, for the reverse jump.
    pub by_addr: HashMap<Addr, usize>,
}

impl ResourceLinks {
    pub fn uses(&self, resource: usize) -> &[ResourceUse] {
        self.by_resource
            .get(&resource)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn resource_at(&self, addr: Addr) -> Option<usize> {
        self.by_addr.get(&addr).copied()
    }

    pub fn len(&self) -> usize {
        self.by_addr.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }
}

/// The loading APIs, and which parameter names the resource.
///
/// `LoadString` is the odd one: its argument is a string id, and the resource
/// that holds it is the block of sixteen that id falls into.
const LOADERS: &[(&str, usize, u16)] = &[
    ("LoadBitmap", 1, rt::BITMAP),
    ("LoadIcon", 1, rt::GROUP_ICON),
    ("LoadCursor", 1, rt::GROUP_CURSOR),
    ("LoadMenu", 1, rt::MENU),
    ("LoadMenuIndirect", 0, rt::MENU),
    ("LoadAccelerators", 1, rt::ACCELERATOR),
    ("LoadString", 1, rt::STRING),
    ("DialogBox", 1, rt::DIALOG),
    ("DialogBoxParam", 1, rt::DIALOG),
    ("CreateDialog", 1, rt::DIALOG),
    ("CreateDialogParam", 1, rt::DIALOG),
];

fn loader(name: &str) -> Option<(usize, u16)> {
    LOADERS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, i, t)| (*i, *t))
}

/// Find every link between the module's resources and its code.
pub fn analyze(ne: &NeFile, program: &Program, db: &ApiDb) -> ResourceLinks {
    let mut links = ResourceLinks::default();

    // (type, integer id) -> resource index, for the by-id case.
    let mut by_id: HashMap<(u16, u16), usize> = HashMap::new();
    for (i, r) in ne.resources.iter().enumerate() {
        if let (Some(t), Some(id)) = (r.type_id.as_id(), r.res_id.as_id()) {
            by_id.insert((t, id), i);
            // An icon or cursor is usually asked for by its group's id, but a
            // module with no group directory is asked for the image directly.
            if t == rt::ICON {
                by_id.entry((rt::GROUP_ICON, id)).or_insert(i);
            }
            if t == rt::CURSOR {
                by_id.entry((rt::GROUP_CURSOR, id)).or_insert(i);
            }
        }
    }

    for (segno, code) in &program.code {
        for i in 0..code.insns.len() {
            let Some(call) = callargs::reconstruct(code, i, db) else {
                continue;
            };
            let Some((arg_index, res_type)) = loader(&call.function) else {
                continue;
            };
            let Some(arg) = call.args.get(arg_index) else {
                continue;
            };
            let Some(value) = arg.value else { continue };

            // A string id names the block of sixteen that holds it.
            let string_id = (res_type == rt::STRING).then_some(value as u16);
            let id = if res_type == rt::STRING {
                (value / 16 + 1) as u16
            } else {
                value as u16
            };
            let Some(&res) = by_id.get(&(res_type, id)) else {
                continue;
            };

            let addr = Addr {
                segment: *segno,
                offset: code.insns[i].offset,
            };
            links.by_resource.entry(res).or_default().push(ResourceUse {
                addr,
                api: call.function.clone(),
                confidence: Confidence::Id,
                string_id,
            });
            links.by_addr.insert(addr, res);
        }
    }

    link_by_name(ne, program, &mut links);

    for v in links.by_resource.values_mut() {
        v.sort_by_key(|u| u.addr);
        v.dedup_by_key(|u| u.addr);
    }
    links
}

/// Match named resources against the strings the code loads.
fn link_by_name(ne: &NeFile, program: &Program, links: &mut ResourceLinks) {
    // Named resources, upper-cased: resource names are case-insensitive.
    let named: Vec<(usize, String)> = ne
        .resources
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match &r.res_id {
            ResId::Name(n) => Some((i, n.to_ascii_uppercase())),
            ResId::Id(_) => None,
        })
        .collect();
    if named.is_empty() {
        return;
    }

    for seg in &ne.segments {
        if seg.data.is_empty() {
            continue;
        }
        for found in ne_core::strings::scan(&seg.data, 2) {
            let text = found.text.to_ascii_uppercase();
            let Some((res, _)) = named.iter().find(|(_, n)| *n == text) else {
                continue;
            };
            for addr in program.data_refs(found.offset) {
                links
                    .by_resource
                    .entry(*res)
                    .or_default()
                    .push(ResourceUse {
                        addr: *addr,
                        api: "by name".into(),
                        confidence: Confidence::Name,
                        string_id: None,
                    });
                links.by_addr.entry(*addr).or_insert(*res);
            }
        }
    }
}
