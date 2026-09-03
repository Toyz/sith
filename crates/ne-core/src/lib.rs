//! Reader for 16-bit Windows NE (New Executable) binaries.
//!
//! Covers the whole container: header, segments and their relocation chains,
//! the entry table, imports and exports, and the resource tree including
//! decoding of bitmaps, icons, cursors, menus, dialogs, accelerators, string
//! tables and version info.
//!
//! ```no_run
//! let ne = ne_core::NeFile::open("SETUP.EXE")?;
//! for seg in &ne.segments {
//!     println!("{} {:?} {} bytes", seg.index, seg.kind(), seg.length);
//! }
//! # Ok::<(), ne_core::Error>(())
//! ```

pub mod api;
pub mod dib;
pub mod entry;
pub mod header;
pub mod index;
pub mod ordinals;
pub mod project;
mod read;
pub mod reloc;
pub mod render;
pub mod resource;
pub mod rsrc;
pub mod strings;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use api::{ApiDb, ArgKind, CallConv, Signature};
pub use entry::Entry;
pub use header::{NeHeader, TargetOs};
pub use index::ExportIndex;
pub use ordinals::OrdinalDb;
pub use project::{BinaryNotes, Project};
pub use reloc::{AddrType, Fixup, RelKind, Reloc, Target};
pub use resource::{GroupDir, ResId, Resource};
pub use segment::{SegKind, Segment};

pub mod segment;

use read::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not an MZ executable")]
    NotMz,
    #[error("no NE signature at {offset:#x} (this is not a 16-bit New Executable)")]
    NotNe { offset: u32 },
    #[error("truncated while reading {what} at {offset:#x}")]
    Truncated { what: &'static str, offset: u64 },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A parsed NE image, holding the whole file in memory.
///
/// NE binaries are at most a few megabytes, and every view the tools present
/// (hex, disassembly, resource preview) wants random access to the raw bytes,
/// so there is no streaming mode.
pub struct NeFile {
    pub path: PathBuf,
    pub buf: Vec<u8>,
    pub header: NeHeader,
    pub segments: Vec<Segment>,
    /// Offsets into the imported-names table, one per module reference.
    pub module_refs: Vec<u16>,
    /// Ordinal -> entry, with unused ordinal runs simply absent.
    pub entries: BTreeMap<u16, Entry>,
    /// Resident name table. The first pair is the module's own name.
    pub resident_names: Vec<(String, u16)>,
    /// Non-resident name table. The first pair is the module description.
    pub nonresident_names: Vec<(String, u16)>,
    pub resources: Vec<Resource>,
    /// Consulted for import-by-ordinal names, ahead of the built-in Win16 table.
    pub export_index: Option<ExportIndex>,
    ordinals: OrdinalDb,
}

impl NeFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<NeFile> {
        let path = path.as_ref().to_path_buf();
        let buf = std::fs::read(&path)?;
        NeFile::from_bytes(path, buf)
    }

    pub fn from_bytes(path: PathBuf, buf: Vec<u8>) -> Result<NeFile> {
        let header = NeHeader::parse(&buf)?;
        let mut ne = NeFile {
            path,
            buf,
            header,
            segments: Vec::new(),
            module_refs: Vec::new(),
            entries: BTreeMap::new(),
            resident_names: Vec::new(),
            nonresident_names: Vec::new(),
            resources: Vec::new(),
            export_index: None,
            ordinals: OrdinalDb::embedded().clone(),
        };
        ne.parse_segments()?;
        ne.parse_module_refs()?;
        ne.parse_entries()?;
        ne.parse_names()?;
        ne.parse_resources()?;
        Ok(ne)
    }

    /// Add or override ordinal names, for a project-specific table.
    pub fn merge_ordinals(&mut self, db: &OrdinalDb) {
        self.ordinals.merge(db);
    }

    pub fn ordinal_db(&self) -> &OrdinalDb {
        &self.ordinals
    }

    /// Resolve ordinal imports of sibling modules through the exports of every
    /// NE file in `index`.
    pub fn set_export_index(&mut self, index: ExportIndex) {
        self.export_index = Some(index);
    }

    // ------------------------------------------------------------ parsing

    fn ne(&self, rel: u16) -> usize {
        self.header.ne_offset as usize + rel as usize
    }

    fn parse_segments(&mut self) -> Result<()> {
        let shift = self.header.align_shift_or_default();
        let base = self.ne(self.header.segment_table);
        let mut segments = Vec::with_capacity(self.header.segment_count as usize);
        for i in 0..self.header.segment_count {
            let o = base + i as usize * 8;
            let sector = u16(&self.buf, o)?;
            let stored_len = u16(&self.buf, o + 2)?;
            let flags = u16(&self.buf, o + 4)?;
            let min_alloc = u16(&self.buf, o + 6)?;

            // A zero length field means 64K, both for the file image and for
            // the allocation size -- 0x10000 does not fit in the word.
            let length = if stored_len == 0 && sector != 0 {
                0x1_0000u32
            } else {
                stored_len as u32
            };
            let min_alloc = if min_alloc == 0 { 0x1_0000 } else { min_alloc as u32 };
            let file_offset = (sector as u64) << shift;

            let mut seg = Segment {
                index: i + 1,
                file_offset,
                length,
                min_alloc,
                flags,
                data: Vec::new(),
                relocs: Vec::new(),
            };
            if sector != 0 {
                let start = file_offset as usize;
                let end = (start + length as usize).min(self.buf.len());
                seg.data = self.buf.get(start..end).unwrap_or(&[]).to_vec();
                if seg.has_relocs() {
                    seg.relocs = parse_relocs(&self.buf, start + length as usize)?;
                }
            }
            segments.push(seg);
        }
        self.segments = segments;
        Ok(())
    }

    fn parse_module_refs(&mut self) -> Result<()> {
        let base = self.ne(self.header.module_ref_table);
        self.module_refs = (0..self.header.module_ref_count)
            .map(|i| u16(&self.buf, base + i as usize * 2))
            .collect::<Result<_>>()?;
        Ok(())
    }

    fn parse_entries(&mut self) -> Result<()> {
        let start = self.ne(self.header.entry_table);
        let end = start + self.header.entry_table_len as usize;
        let mut p = start;
        let mut ordinal: u16 = 1;
        while p < end {
            let count = match u8(&self.buf, p) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let bundle_kind = u8(&self.buf, p + 1)?;
            p += 2;
            for _ in 0..count {
                match bundle_kind {
                    // A null bundle is a run of unused ordinals.
                    0x00 => {}
                    // 0xFF marks a movable entry: flags, an int 3Fh thunk,
                    // then the real segment and offset.
                    0xFF => {
                        let flags = u8(&self.buf, p)?;
                        let segment = u8(&self.buf, p + 3)? as u16;
                        let offset = u16(&self.buf, p + 4)?;
                        self.entries.insert(
                            ordinal,
                            Entry {
                                ordinal,
                                segment,
                                offset,
                                flags,
                                moveable: true,
                                name: None,
                                resident: false,
                            },
                        );
                        p += 6;
                    }
                    // Anything else is a fixed entry, and the bundle type
                    // byte is itself the segment number.
                    seg => {
                        let flags = u8(&self.buf, p)?;
                        let offset = u16(&self.buf, p + 1)?;
                        self.entries.insert(
                            ordinal,
                            Entry {
                                ordinal,
                                segment: seg as u16,
                                offset,
                                flags,
                                moveable: false,
                                name: None,
                                resident: false,
                            },
                        );
                        p += 3;
                    }
                }
                ordinal = ordinal.wrapping_add(1);
            }
        }
        Ok(())
    }

    fn parse_names(&mut self) -> Result<()> {
        self.resident_names = read_name_table(&self.buf, self.ne(self.header.resident_names_table), None);
        if self.header.nonresident_names_table != 0 && self.header.nonresident_names_len != 0 {
            let start = self.header.nonresident_names_table as usize;
            let limit = start + self.header.nonresident_names_len as usize;
            self.nonresident_names = read_name_table(&self.buf, start, Some(limit));
        }
        // The first pair of each table names the module itself, not an entry.
        for (name, ord) in self.resident_names.iter().skip(1) {
            if let Some(e) = self.entries.get_mut(ord) {
                e.name = Some(name.clone());
                e.resident = true;
            }
        }
        for (name, ord) in self.nonresident_names.iter().skip(1) {
            if let Some(e) = self.entries.get_mut(ord) {
                if e.name.is_none() {
                    e.name = Some(name.clone());
                }
            }
        }
        Ok(())
    }

    fn parse_resources(&mut self) -> Result<()> {
        // A module with no resources sets the resource table offset equal to
        // the resident name table offset rather than to zero.
        if self.header.resource_table == 0
            || self.header.resource_table == self.header.resident_names_table
        {
            return Ok(());
        }
        let base = self.ne(self.header.resource_table);
        let shift = u16(&self.buf, base)?;
        let mut p = base + 2;
        loop {
            let type_id = u16(&self.buf, p)?;
            if type_id == 0 {
                break;
            }
            let count = u16(&self.buf, p + 2)?;
            p += 8;
            let type_id = decode_res_id(&self.buf, base, type_id);
            for _ in 0..count {
                let offset = u16(&self.buf, p)? as u64;
                let length = u16(&self.buf, p + 2)? as u32;
                let flags = u16(&self.buf, p + 4)?;
                let raw_id = u16(&self.buf, p + 6)?;
                p += 12;
                self.resources.push(Resource {
                    type_id: type_id.clone(),
                    res_id: decode_res_id(&self.buf, base, raw_id),
                    offset: offset << shift,
                    length: length << shift,
                    flags,
                });
            }
        }
        Ok(())
    }

    // ----------------------------------------------------------- accessors

    /// The module's own name, from the head of the resident name table.
    pub fn module_name(&self) -> &str {
        self.resident_names
            .first()
            .map(|(n, _)| n.as_str())
            .unwrap_or("?")
    }

    /// The description string, from the head of the non-resident name table.
    pub fn description(&self) -> &str {
        self.nonresident_names
            .first()
            .map(|(n, _)| n.as_str())
            .unwrap_or("")
    }

    /// A length-prefixed name from the imported names table.
    pub fn imported_name(&self, offset: u16) -> String {
        pascal_string(&self.buf, self.ne(self.header.imported_names_table) + offset as usize)
            .map(|(s, _)| s)
            .unwrap_or_else(|_| format!("?{offset:#x}"))
    }

    /// Name of the `index`-th (1-based) referenced module.
    pub fn module_ref_name(&self, index: u16) -> String {
        match self.module_refs.get(index.wrapping_sub(1) as usize) {
            Some(&off) => self.imported_name(off),
            None => format!("MOD{index}?"),
        }
    }

    pub fn module_ref_names(&self) -> Vec<String> {
        (1..=self.module_refs.len() as u16)
            .map(|i| self.module_ref_name(i))
            .collect()
    }

    /// Best-known name for an ordinal import: the Win16 system table first,
    /// then the exports of sibling modules if an index has been attached.
    pub fn import_ordinal_name(&self, module_index: u16, ordinal: u16) -> Option<String> {
        let module = self.module_ref_name(module_index);
        if let Some(n) = self.ordinals.lookup(&module, ordinal) {
            return Some(n.to_string());
        }
        self.export_index
            .as_ref()
            .and_then(|ix| ix.lookup(&module, ordinal))
            .map(str::to_string)
    }

    pub fn segment(&self, index: u16) -> Option<&Segment> {
        self.segments.get(index.checked_sub(1)? as usize)
    }

    pub fn exports(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.values().filter(|e| e.is_exported()).collect();
        v.sort_by_key(|e| e.ordinal);
        v
    }

    pub fn resource_data(&self, r: &Resource) -> &[u8] {
        let start = r.offset as usize;
        let end = (start + r.length as usize).min(self.buf.len());
        self.buf.get(start..end).unwrap_or(&[])
    }

    /// The `RT_ICON` / `RT_CURSOR` body with the given ordinal.
    pub fn resource_by_type_ordinal(&self, type_id: u16, ordinal: u16) -> Option<&Resource> {
        self.resources
            .iter()
            .find(|r| r.type_id == ResId::Id(type_id) && r.res_id == ResId::Id(ordinal))
    }

    // ------------------------------------------------------------- fixups

    /// Resolve one relocation record to a named target.
    ///
    /// `site` is the offset the fixup writes to, and it matters: an
    /// intersegment call is an `ADDR_SEGMENT` fixup that relocates only the
    /// segment word of the far pointer, so the record carries `target2 == 0`
    /// and the real destination offset is left sitting in the code two bytes
    /// below the site. Reading the record alone reports every intersegment
    /// call as `segNN:0000`.
    pub fn fixup_target(&self, seg: &Segment, r: &Reloc, site: Option<u16>) -> Target {
        match r.kind {
            RelKind::Internal => {
                if r.target1 == 0xFF {
                    let e = self.entries.get(&r.target2);
                    return Target::Entry {
                        ordinal: r.target2,
                        name: e.and_then(|e| e.name.clone()),
                        segment: e.map(|e| e.segment).unwrap_or(0),
                        offset: e.map(|e| e.offset).unwrap_or(0),
                    };
                }
                let offset = if r.target2 != 0 {
                    Some(r.target2)
                } else {
                    // Recovery from the code is a heuristic: it only holds
                    // when the fixup really is the segment half of a far
                    // pointer. Validating against the target segment's size
                    // rejects the reads that land on unrelated bytes, such as
                    // the ones before a bare `mov ax, seg X` immediate.
                    site.and_then(|s| recover_offset(&seg.data, r.addr_type, s))
                        .filter(|off| self.offset_is_plausible(r.target1, *off))
                };
                Target::Internal {
                    segment: r.target1,
                    offset,
                }
            }
            RelKind::ImportOrdinal => Target::ImportOrdinal {
                module: self.module_ref_name(r.target1),
                ordinal: r.target2,
                name: self.import_ordinal_name(r.target1, r.target2),
            },
            RelKind::ImportName => Target::ImportName {
                module: self.module_ref_name(r.target1),
                name: self.imported_name(r.target2),
            },
            RelKind::OsFixup => Target::OsFixup {
                target1: r.target1,
                target2: r.target2,
            },
        }
    }

    /// Is `offset` inside the named segment? Used to reject offsets recovered
    /// from code bytes that turn out not to be a far pointer.
    fn offset_is_plausible(&self, segment: u16, offset: u16) -> bool {
        match self.segment(segment) {
            Some(s) => (offset as u32) < s.length.max(s.min_alloc),
            None => false,
        }
    }

    /// Every patch site in a segment, expanded from its relocation chains and
    /// sorted by offset.
    pub fn fixups(&self, seg: &Segment) -> Vec<Fixup> {
        let mut out = Vec::new();
        for r in &seg.relocs {
            for site in r.sites(&seg.data) {
                out.push(Fixup {
                    site,
                    addr_type: r.addr_type,
                    additive: r.additive,
                    target: self.fixup_target(seg, r, Some(site)),
                });
            }
        }
        out.sort_by_key(|f| f.site);
        out
    }

    /// `offset -> fixup` covering every byte a fixup writes, so a
    /// disassembler can annotate an instruction by scanning its byte range.
    pub fn fixup_map(&self, seg: &Segment) -> BTreeMap<u16, Fixup> {
        let mut m = BTreeMap::new();
        for f in self.fixups(seg) {
            m.insert(f.site, f);
        }
        m
    }
}

/// Where the target offset hides when the relocation record does not carry it.
fn recover_offset(data: &[u8], addr_type: AddrType, site: u16) -> Option<u16> {
    let i = site as usize;
    match addr_type {
        // The chain runs through the segment word; the offset word precedes it.
        AddrType::Segment if i >= 2 => {
            Some(u16::from_le_bytes([*data.get(i - 2)?, *data.get(i - 1)?]))
        }
        // The chain runs through the offset word of the far pointer itself.
        AddrType::Far => Some(u16::from_le_bytes([*data.get(i)?, *data.get(i + 1)?])),
        _ => None,
    }
}

fn parse_relocs(buf: &[u8], at: usize) -> Result<Vec<Reloc>> {
    let count = match u16(buf, at) {
        Ok(n) => n as usize,
        // Relocation data past the end of a truncated image is not fatal;
        // the segment bytes are still worth reading.
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = at + 2 + i * 8;
        match buf.get(o..o + 8) {
            Some(rec) => out.push(Reloc::parse(rec)),
            None => break,
        }
    }
    Ok(out)
}

fn read_name_table(buf: &[u8], mut p: usize, limit: Option<usize>) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    loop {
        if let Some(l) = limit {
            if p >= l {
                break;
            }
        }
        let Ok((name, used)) = pascal_string(buf, p) else {
            break;
        };
        if name.is_empty() {
            break;
        }
        let Ok(ord) = u16(buf, p + used) else { break };
        out.push((name, ord));
        p += used + 2;
    }
    out
}

/// A resource type or name word: the high bit marks an integer id, otherwise
/// the low 15 bits are an offset to a length-prefixed string.
fn decode_res_id(buf: &[u8], table_base: usize, raw: u16) -> ResId {
    if raw & 0x8000 != 0 {
        ResId::Id(raw & 0x7FFF)
    } else {
        match pascal_string(buf, table_base + raw as usize) {
            Ok((s, _)) => ResId::Name(s),
            Err(_) => ResId::Id(raw),
        }
    }
}

impl NeFile {
    /// The resource as a standalone file: a `.bmp` for `RT_BITMAP`, a `.ico`
    /// or `.cur` assembled from the group directory and its members, and the
    /// raw body for everything else.
    pub fn resource_file_bytes(&self, r: &Resource) -> Vec<u8> {
        let data = self.resource_data(r);
        match r.type_id.as_id() {
            Some(resource::rt::BITMAP) => dib::to_bmp_file(data).unwrap_or_else(|| data.to_vec()),
            Some(t @ (resource::rt::GROUP_ICON | resource::rt::GROUP_CURSOR)) => {
                let is_cursor = t == resource::rt::GROUP_CURSOR;
                let member = if is_cursor {
                    resource::rt::CURSOR
                } else {
                    resource::rt::ICON
                };
                match GroupDir::parse(data, is_cursor) {
                    Some(dir) => {
                        let images: Vec<Vec<u8>> = dir
                            .entries
                            .iter()
                            .map(|e| {
                                self.resource_by_type_ordinal(member, e.res_ordinal)
                                    .map(|r| self.resource_data(r).to_vec())
                                    .unwrap_or_default()
                            })
                            .collect();
                        resource::build_icon_file(&dir, &images)
                    }
                    None => data.to_vec(),
                }
            }
            Some(t @ (resource::rt::ICON | resource::rt::CURSOR)) => {
                resource::build_single_icon_file(data, t == resource::rt::CURSOR)
                    .unwrap_or_else(|| data.to_vec())
            }
            _ => data.to_vec(),
        }
    }

    /// Decode a resource to RGBA for preview, where it holds an image.
    ///
    /// Icons and cursors store the AND mask stacked below the colour bits, so
    /// the visible height is half the header height and the mask becomes the
    /// alpha channel.
    pub fn resource_image(&self, r: &Resource) -> Option<dib::Image> {
        let data = self.resource_data(r);
        match r.type_id.as_id()? {
            resource::rt::BITMAP => dib::decode(data, None),
            resource::rt::ICON | resource::rt::CURSOR => {
                let is_cursor = r.type_id.as_id() == Some(resource::rt::CURSOR);
                let body = if is_cursor && data.len() > 4 { &data[4..] } else { data };
                let info = dib::DibInfo::parse(body)?;
                let mut img = dib::decode(body, Some(info.abs_height() / 2))?;
                dib::apply_and_mask(&mut img, body, &info);
                Some(img)
            }
            resource::rt::GROUP_ICON | resource::rt::GROUP_CURSOR => {
                let is_cursor = r.type_id.as_id() == Some(resource::rt::GROUP_CURSOR);
                let dir = GroupDir::parse(data, is_cursor)?;
                // Preview the largest member; a group is usually the same
                // artwork at several sizes and colour depths.
                let best = dir
                    .entries
                    .iter()
                    .max_by_key(|e| (e.width as u32 * e.height as u32, e.bit_count))?;
                let member = self.resource_by_type_ordinal(
                    if is_cursor { resource::rt::CURSOR } else { resource::rt::ICON },
                    best.res_ordinal,
                )?;
                self.resource_image(member)
            }
            _ => None,
        }
    }
}
