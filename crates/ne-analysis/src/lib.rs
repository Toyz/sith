//! Function discovery, call graphs and cross-references.
//!
//! Everything here works from decoded instructions rather than disassembler
//! text, so a call is recognised by its mnemonic and its resolved fixup
//! instead of by a regular expression over a formatted line.

pub mod callargs;
pub mod resrefs;

use ne_core::{NeFile, Target};
use ne_disasm::{disassemble, Flow, Insn, Options, SegmentCode};

/// What a function's stack frame says about its arguments and locals.
///
/// Win16 code gives this away without any symbols, from two independent
/// signals:
///
/// - **`ret imm16` / `retf imm16`.** The pascal convention makes the *callee*
///   pop its arguments, so the immediate on the return is the argument area in
///   bytes, stated by the compiler. A bare `ret` means either cdecl -- where
///   the caller cleans up -- or no arguments at all.
/// - **`[bp+n]` accesses.** After `push bp; mov bp,sp` the frame is fixed:
///   below `bp` are locals, above it are the saved frame pointer, the return
///   address, and then the caller's arguments. The first argument therefore
///   sits at `[bp+4]` for a near function and `[bp+6]` for a far one, and the
///   highest offset touched shows how far the argument area reaches.
///
/// The two agree in the ordinary case and disagree in the interesting ones: a
/// function that pops eight bytes but only ever reads the first four is
/// ignoring an argument, and one that reads past what it pops is reading its
/// caller's frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// True when the function sets up a `bp` frame.
    pub has_frame: bool,
    /// True when it returns with `retf`, so it is called far.
    pub far: bool,
    /// Bytes of arguments the callee pops, from the return instruction.
    pub popped_bytes: Option<u16>,
    /// Bytes of locals, from the `sub sp, n` in the prologue.
    pub local_bytes: Option<u16>,
    /// Distinct positive `[bp+n]` offsets the body reads, sorted.
    pub argument_offsets: Vec<i32>,
}

impl Frame {
    /// Where the first argument sits, given how the function returns.
    pub fn first_argument_offset(&self) -> i32 {
        if self.far {
            6
        } else {
            4
        }
    }

    /// Arguments in bytes, preferring what the compiler stated.
    pub fn argument_bytes(&self) -> Option<u16> {
        if let Some(n) = self.popped_bytes {
            return Some(n);
        }
        // Nothing was popped, so fall back to how far the reads reached. This
        // is a floor, not a count: an argument that is never read leaves no
        // trace at all.
        let highest = *self.argument_offsets.last()?;
        let span = highest - self.first_argument_offset();
        (span >= 0).then_some(span as u16 + 2)
    }

    /// Does it take arguments at all?
    pub fn takes_arguments(&self) -> bool {
        self.popped_bytes.is_some_and(|n| n > 0) || !self.argument_offsets.is_empty()
    }

    /// A short description for a listing or a tooltip.
    pub fn describe(&self) -> String {
        if !self.has_frame && self.popped_bytes.is_none() {
            return "no stack frame".into();
        }
        let mut parts = Vec::new();
        match self.argument_bytes() {
            Some(0) | None if self.argument_offsets.is_empty() => {
                parts.push("no arguments".to_string())
            }
            Some(n) => {
                let counted = if self.popped_bytes.is_some() {
                    format!("{n} bytes of arguments")
                } else {
                    format!("at least {n} bytes of arguments")
                };
                parts.push(counted);
            }
            None => parts.push("arguments of unknown size".to_string()),
        }
        if let Some(l) = self.local_bytes.filter(|l| *l > 0) {
            parts.push(format!("{l} bytes of locals"));
        }
        parts.join(", ")
    }
}

/// Read the frame of the function covering `range` in `code`.
pub fn analyze_frame(code: &SegmentCode, start: u32, end: u32) -> Frame {
    use iced_x86::{Mnemonic, Register};
    let mut frame = Frame::default();
    let body: Vec<&Insn> = code
        .insns
        .iter()
        .filter(|i| i.offset >= start && i.offset < end)
        .collect();

    for (i, insn) in body.iter().enumerate() {
        match insn.mnemonic {
            // `mov bp, sp` right after a `push bp` is the frame being set up.
            Mnemonic::Mov if insn.op0_register == Some(Register::BP) => {
                if i > 0 && body[i - 1].mnemonic == Mnemonic::Push {
                    frame.has_frame = true;
                }
            }
            Mnemonic::Enter => {
                frame.has_frame = true;
                frame.local_bytes = insn.immediate.map(|v| v as u16);
            }
            // The prologue's `sub sp, n` reserves the locals, and only counts
            // while the frame is being built.
            Mnemonic::Sub if insn.op0_register == Some(Register::SP) => {
                if frame.local_bytes.is_none() && frame.has_frame {
                    frame.local_bytes = insn.immediate.map(|v| v as u16);
                }
            }
            Mnemonic::Ret => {
                frame.popped_bytes = Some(insn.immediate.unwrap_or(0) as u16);
            }
            Mnemonic::Retf => {
                frame.far = true;
                frame.popped_bytes = Some(insn.immediate.unwrap_or(0) as u16);
            }
            _ => {}
        }
        if let Some(d) = insn.bp_displacement {
            if d > 0 {
                frame.argument_offsets.push(d);
            }
        }
    }
    frame.argument_offsets.sort_unstable();
    frame.argument_offsets.dedup();
    // Anything at or below the saved frame pointer and return address is not an
    // argument, whatever the displacement says.
    let first = frame.first_argument_offset();
    frame.argument_offsets.retain(|d| *d >= first);
    frame
}

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Address of a code location within the module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr {
    pub segment: u16,
    pub offset: u32,
}

impl std::fmt::Display for Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "seg{:02}:{:04X}", self.segment, self.offset)
    }
}

/// Why a function start was believed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FuncKind {
    /// Named in the entry table, so the address is certain.
    Export,
    /// An unnamed entry-table slot.
    Entry,
    /// The module's `CS:IP`.
    EntryPoint,
    /// Referenced by an internal relocation from some segment.
    Relocated,
    /// The target of a near call inside this segment.
    Called,
    /// Only a recognised stack-frame prologue; the weakest evidence.
    Prologue,
}

impl FuncKind {
    /// A sentence explaining why this address is believed to start a function.
    pub fn describe(self) -> &'static str {
        match self {
            FuncKind::Export => "named in the entry table, so the address is certain",
            FuncKind::Entry => "an unnamed entry-table slot",
            FuncKind::EntryPoint => "the module's own entry point",
            FuncKind::Relocated => "referenced by a relocation from somewhere in the module",
            FuncKind::Called => "the target of a near call in this segment",
            FuncKind::Prologue => "only a recognised stack-frame prologue: the weakest evidence",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FuncKind::Export => "export",
            FuncKind::Entry => "entry",
            FuncKind::EntryPoint => "entrypoint",
            FuncKind::Relocated => "reloc",
            FuncKind::Called => "called",
            FuncKind::Prologue => "prologue",
        }
    }
}

#[derive(Debug, Clone)]
pub enum CallTarget {
    /// A near call, staying inside the segment.
    Near(Addr),
    /// A far call or intersegment reference, named by its fixup.
    Fixup(Target),
    /// A call through a register or memory operand; the destination needs
    /// data-flow to recover and is not resolved here.
    Indirect,
}

impl std::fmt::Display for CallTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallTarget::Near(a) => write!(f, "{a}"),
            CallTarget::Fixup(t) => write!(f, "{t}"),
            CallTarget::Indirect => f.write_str("(indirect)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallSite {
    pub from: Addr,
    pub target: CallTarget,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub addr: Addr,
    /// Exclusive end offset, taken as the next known start.
    pub end: u32,
    pub name: Option<String>,
    pub ordinal: Option<u16>,
    pub kind: FuncKind,
    pub calls: Vec<CallSite>,
    /// Instruction count, useful as a rough size signal in the GUI.
    pub insn_count: usize,
    /// What the stack frame says about arguments and locals.
    pub frame: Frame,
}

impl Function {
    pub fn label(&self) -> String {
        match (&self.name, self.ordinal) {
            (Some(n), _) => n.clone(),
            (None, Some(o)) => format!("ord_{o}"),
            (None, None) => format!("sub_{:02}_{:04X}", self.addr.segment, self.addr.offset),
        }
    }

    pub fn size(&self) -> u32 {
        self.end.saturating_sub(self.addr.offset)
    }
}

/// Whole-module analysis: decoded code for every code segment, the functions
/// found in them, and the cross-reference index.
pub struct Program {
    pub code: BTreeMap<u16, SegmentCode>,
    pub functions: Vec<Function>,
    /// Target label -> call sites referencing it.
    pub xrefs: HashMap<String, Vec<Addr>>,
    /// Constant value -> code sites that mention it.
    ///
    /// Used to tie data offsets, and therefore strings, back to the code that
    /// loads them. See [`Program::data_refs`].
    pub value_refs: HashMap<u32, Vec<Addr>>,

    // Indices built once, so the views that ask these questions on every
    // frame -- the call graph rebuilds its whole layout each time it is drawn
    // -- are not each a scan over every function and every call it makes.
    /// Function start -> its index in `functions`.
    by_addr: HashMap<Addr, usize>,
    /// Function index -> the functions it calls.
    callees: Vec<Vec<usize>>,
    /// Function index -> the functions that call it.
    callers: Vec<Vec<usize>>,
    /// Function index -> the imported symbols it calls, in call order.
    externals: Vec<Vec<String>>,
    /// Segment -> its functions, ordered by address, for containment lookups.
    by_segment: HashMap<u16, Vec<usize>>,
}

/// The address a call goes to, when it is a place in this module.
fn target_address(target: &CallTarget) -> Option<Addr> {
    match target {
        CallTarget::Near(a) => Some(*a),
        CallTarget::Fixup(Target::Internal {
            segment,
            offset: Some(off),
        }) => Some(Addr {
            segment: *segment,
            offset: *off as u32,
        }),
        CallTarget::Fixup(Target::Entry {
            segment, offset, ..
        }) => Some(Addr {
            segment: *segment,
            offset: *offset as u32,
        }),
        _ => None,
    }
}

impl Program {
    /// Analyse every code segment. `bits32` names segments that hold 32-bit
    /// code; these exist in Win16 titles that promote themselves through DPMI
    /// and would otherwise decode as nonsense.
    pub fn analyze(ne: &NeFile, bits32: &BTreeSet<u16>) -> Program {
        let mut code = BTreeMap::new();
        for seg in &ne.segments {
            if !seg.is_code() || seg.data.is_empty() {
                continue;
            }
            let opts = Options {
                bits: if bits32.contains(&seg.index) { 32 } else { 16 },
                ..Default::default()
            };
            code.insert(seg.index, disassemble(ne, seg, &opts));
        }

        let seeds = collect_seeds(ne, &code);
        let mut functions = Vec::new();
        for (segno, sc) in &code {
            functions.extend(find_functions(ne, *segno, sc, seeds.get(segno)));
        }

        let mut xrefs: HashMap<String, Vec<Addr>> = HashMap::new();
        for f in &functions {
            for c in &f.calls {
                xrefs.entry(c.target.to_string()).or_default().push(c.from);
            }
        }
        for v in xrefs.values_mut() {
            v.sort();
            v.dedup();
        }

        let mut value_refs: HashMap<u32, Vec<Addr>> = HashMap::new();
        for (segno, sc) in &code {
            for insn in &sc.insns {
                // A fixup already names the target; a constant that is also a
                // relocation site is not a data offset.
                if insn.fixup.is_some() {
                    continue;
                }
                for v in &insn.operand_values {
                    value_refs.entry(*v).or_default().push(Addr {
                        segment: *segno,
                        offset: insn.offset,
                    });
                }
            }
        }
        for v in value_refs.values_mut() {
            v.sort();
            v.dedup();
        }

        let by_addr: HashMap<Addr, usize> = functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.addr, i))
            .collect();

        let mut callees: Vec<Vec<usize>> = vec![Vec::new(); functions.len()];
        let mut callers: Vec<Vec<usize>> = vec![Vec::new(); functions.len()];
        let mut externals: Vec<Vec<String>> = vec![Vec::new(); functions.len()];
        for (i, f) in functions.iter().enumerate() {
            for c in &f.calls {
                if let Some(addr) = target_address(&c.target) {
                    if let Some(&j) = by_addr.get(&addr) {
                        if !callees[i].contains(&j) {
                            callees[i].push(j);
                        }
                        if !callers[j].contains(&i) {
                            callers[j].push(i);
                        }
                    }
                    continue;
                }
                if let CallTarget::Fixup(
                    t @ (Target::ImportOrdinal { .. } | Target::ImportName { .. }),
                ) = &c.target
                {
                    let name = t.to_string();
                    if !externals[i].contains(&name) {
                        externals[i].push(name);
                    }
                }
            }
        }

        let mut by_segment: HashMap<u16, Vec<usize>> = HashMap::new();
        for (i, f) in functions.iter().enumerate() {
            by_segment.entry(f.addr.segment).or_default().push(i);
        }
        for v in by_segment.values_mut() {
            v.sort_by_key(|i| functions[*i].addr.offset);
        }

        Program {
            code,
            functions,
            xrefs,
            value_refs,
            by_addr,
            callees,
            callers,
            externals,
            by_segment,
        }
    }

    /// Code sites that mention `value` as a constant.
    ///
    /// This is a heuristic: 16-bit code loads a data-segment pointer as a
    /// plain immediate, so a match is evidence rather than proof, and small
    /// values match a great deal of arithmetic. It is accurate enough to find
    /// the code behind a string and is presented as such.
    pub fn data_refs(&self, value: u32) -> &[Addr] {
        self.value_refs.get(&value).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Functions that call `addr`, following near calls and entry thunks.
    pub fn callers_of(&self, addr: Addr) -> Vec<&Function> {
        let Some(&i) = self.by_addr.get(&addr) else {
            return Vec::new();
        };
        self.callers[i].iter().map(|j| &self.functions[*j]).collect()
    }

    /// Functions `f` calls that are themselves known functions of this module.
    pub fn callees_of(&self, f: &Function) -> Vec<&Function> {
        let Some(&i) = self.by_addr.get(&f.addr) else {
            return Vec::new();
        };
        self.callees[i].iter().map(|j| &self.functions[*j]).collect()
    }

    /// External symbols `f` calls, in call order without repeats.
    pub fn external_calls_of(&self, f: &Function) -> Vec<String> {
        self.by_addr
            .get(&f.addr)
            .map(|i| self.externals[*i].clone())
            .unwrap_or_default()
    }

    pub fn function_at(&self, addr: Addr) -> Option<&Function> {
        self.by_addr.get(&addr).map(|i| &self.functions[*i])
    }

    /// The function containing `addr`, if any.
    pub fn function_containing(&self, addr: Addr) -> Option<&Function> {
        let col = self.by_segment.get(&addr.segment)?;
        // The column is ordered by address, so the candidate is the last start
        // at or before this one.
        let pos = col.partition_point(|i| self.functions[*i].addr.offset <= addr.offset);
        let f = &self.functions[*col.get(pos.checked_sub(1)?)?];
        (addr.offset < f.end).then_some(f)
    }

    /// Call sites whose target label matches `needle`, case-insensitively.
    pub fn find_xrefs(&self, needle: &str) -> Vec<(&str, &[Addr])> {
        let needle = needle.to_ascii_uppercase();
        let mut out: Vec<(&str, &[Addr])> = self
            .xrefs
            .iter()
            .filter(|(k, _)| k.to_ascii_uppercase().contains(&needle))
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        out.sort_by_key(|(k, _)| *k);
        out
    }
}

/// Seed addresses per segment, gathered before any single segment is walked:
/// entry-table slots, the module entry point, and every internal relocation
/// target from anywhere in the module.
fn collect_seeds(
    ne: &NeFile,
    code: &BTreeMap<u16, SegmentCode>,
) -> BTreeMap<u16, BTreeMap<u32, (FuncKind, Option<String>, Option<u16>)>> {
    let mut seeds: BTreeMap<u16, BTreeMap<u32, (FuncKind, Option<String>, Option<u16>)>> =
        BTreeMap::new();

    for e in ne.entries.values() {
        if !code.contains_key(&e.segment) {
            continue;
        }
        let kind = if e.is_exported() {
            FuncKind::Export
        } else {
            FuncKind::Entry
        };
        seeds
            .entry(e.segment)
            .or_default()
            .insert(e.offset as u32, (kind, e.name.clone(), Some(e.ordinal)));
    }

    let cs = (ne.header.cs_ip >> 16) as u16;
    let ip = (ne.header.cs_ip & 0xFFFF) as u32;
    if code.contains_key(&cs) {
        seeds
            .entry(cs)
            .or_default()
            .entry(ip)
            .or_insert((FuncKind::EntryPoint, Some("__entry".into()), None));
    }

    for seg in &ne.segments {
        for f in ne.fixups(seg) {
            if let Target::Internal {
                segment,
                offset: Some(offset),
            } = f.target
            {
                if code.contains_key(&segment) && offset != 0 {
                    seeds
                        .entry(segment)
                        .or_default()
                        .entry(offset as u32)
                        .or_insert((FuncKind::Relocated, None, None));
                }
            }
        }
    }
    seeds
}

type Seeds = BTreeMap<u32, (FuncKind, Option<String>, Option<u16>)>;

fn find_functions(ne: &NeFile, segno: u16, sc: &SegmentCode, seeds: Option<&Seeds>) -> Vec<Function> {
    let empty = Seeds::new();
    let seeds = seeds.unwrap_or(&empty);

    // Offsets that actually begin an instruction; a seed pointing into the
    // middle of one is data or a mis-decode and is dropped.
    let boundaries: BTreeSet<u32> = sc.insns.iter().map(|i| i.offset).collect();

    let mut starts: BTreeMap<u32, (FuncKind, Option<String>, Option<u16>)> = BTreeMap::new();
    for (off, meta) in seeds {
        if boundaries.contains(off) {
            starts.insert(*off, meta.clone());
        }
    }

    // Near call targets inside this segment.
    for insn in &sc.insns {
        if insn.flow == Flow::Call {
            if let Some(t) = insn.near_target {
                if boundaries.contains(&t) {
                    starts.entry(t).or_insert((FuncKind::Called, None, None));
                }
            }
        }
    }

    // Prologues, as the last resort for code nothing references directly.
    for (i, insn) in sc.insns.iter().enumerate() {
        if is_prologue(&sc.insns, i) {
            starts
                .entry(insn.offset)
                .or_insert((FuncKind::Prologue, None, None));
        }
    }

    let ordered: Vec<u32> = starts.keys().copied().collect();
    let seg_end = ne
        .segment(segno)
        .map(|s| s.data.len() as u32)
        .unwrap_or_else(|| sc.insns.last().map(|i| i.offset + i.len as u32).unwrap_or(0));

    let mut out = Vec::with_capacity(ordered.len());
    for (i, &start) in ordered.iter().enumerate() {
        let end = ordered.get(i + 1).copied().unwrap_or(seg_end);
        let addr = Addr {
            segment: segno,
            offset: start,
        };
        let (kind, name, ordinal) = starts[&start].clone();
        let body: Vec<&Insn> = sc
            .insns
            .iter()
            .filter(|x| x.offset >= start && x.offset < end)
            .collect();
        let calls = body
            .iter()
            .filter(|x| x.flow.is_call())
            .map(|x| CallSite {
                from: Addr {
                    segment: segno,
                    offset: x.offset,
                },
                target: call_target(segno, x),
            })
            .collect();
        out.push(Function {
            addr,
            end,
            name,
            ordinal,
            kind,
            calls,
            insn_count: body.len(),
            frame: analyze_frame(sc, start, end),
        });
    }
    out
}

fn call_target(segment: u16, insn: &Insn) -> CallTarget {
    if let Some(f) = &insn.fixup {
        return CallTarget::Fixup(f.target.clone());
    }
    match insn.flow {
        Flow::Call => match insn.near_target {
            Some(t) => CallTarget::Near(Addr {
                segment,
                offset: t,
            }),
            None => CallTarget::Indirect,
        },
        _ => CallTarget::Indirect,
    }
}

/// Recognise the stack-frame prologues emitted by the Microsoft and Borland
/// 16-bit compilers.
fn is_prologue(insns: &[Insn], i: usize) -> bool {
    let at = |n: usize| insns.get(i + n).map(|x| x.bytes.as_slice());

    // The Windows far prologue for a function that may be called through a
    // MakeProcInstance thunk: mov ax,ds / nop / inc bp / push bp / mov bp,sp.
    // The loader rewrites the first three bytes when the module is a DLL, so
    // the `push ds` / `pop ax` spelling turns up as well.
    let far_head = matches!(at(0), Some([0x8C, 0xD8]) | Some([0x1E]));
    if far_head {
        for n in 1..5 {
            if at(n) == Some(&[0x55][..]) && at(n + 1) == Some(&[0x8B, 0xEC][..]) {
                return true;
            }
        }
    }

    // The plain near prologue, but not when it is the tail of the far one
    // already matched above.
    if at(0) == Some(&[0x55][..]) && at(1) == Some(&[0x8B, 0xEC][..]) {
        let preceded_by_far = (1..4).any(|n| {
            i >= n
                && matches!(
                    insns.get(i - n).map(|x| x.bytes.as_slice()),
                    Some([0x8C, 0xD8]) | Some([0x1E])
                )
        });
        return !preceded_by_far;
    }

    // `enter imm16, 0`, used by Borland for frames with locals.
    matches!(at(0), Some([0xC8, ..]))
}
