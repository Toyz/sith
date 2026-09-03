//! A C-shaped rendering of a function.
//!
//! This is not a decompiler. There is no data flow here, no type inference
//! and no control-flow structuring: every line still corresponds to one
//! instruction, and registers stay registers. What it does is spend the
//! knowledge the rest of the crate already has - the stack frame, the call
//! sites, the imported signatures - on making the listing read as statements
//! instead of opcodes.
//!
//! That turns out to be most of the value. `mov ax,[bp+6]` becomes
//! `ax = arg0;`, a run of pushes and a far call becomes
//! `ax = KERNEL.LoadLibrary(ds:13DEh);`, and `cmp ax,20h` / `ja short 002Ch`
//! becomes `if (ax > 0x20) goto L_002C;`. Nothing is invented: where a value
//! was computed rather than pushed as a literal it is left as the register
//! that holds it, and an instruction with no C shape is emitted verbatim as
//! an assembly statement rather than being guessed at.

use crate::callargs;
use crate::{Addr, Function, Program};
use ne_core::Target;
use iced_x86::Mnemonic;
use ne_core::api::ApiDb;
use ne_disasm::{Flow, Insn, SegmentCode};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What a line is, so the view can color it without re-parsing the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A note from the lifter about what it could and could not establish.
    Comment,
    /// The reconstructed signature.
    Signature,
    /// A brace on its own line.
    Punct,
    /// A local variable declaration.
    Decl,
    /// A branch target.
    Label,
    /// An assignment or other plain statement.
    Statement,
    /// A call, to an import or to another function here.
    Call,
    /// A branch or a loop.
    Control,
    /// A return.
    Return,
    /// An instruction with no C shape, left as it was.
    Asm,
    /// A preprocessor line.
    Include,
}

#[derive(Debug, Clone)]
pub struct Line {
    /// The instruction this came from, where it came from one.
    pub addr: Option<u32>,
    pub indent: u8,
    pub kind: Kind,
    pub text: String,
}

impl Line {
    fn new(kind: Kind, indent: u8, text: impl Into<String>) -> Line {
        Line {
            addr: None,
            indent,
            kind,
            text: text.into(),
        }
    }

    fn at(addr: u32, kind: Kind, text: impl Into<String>) -> Line {
        Line {
            addr: Some(addr),
            indent: 1,
            kind,
            text: text.into(),
        }
    }
}

/// The includes and declarations this function would need to compile.
///
/// The tool already knows which modules the code imports from and what each
/// import's signature is, so it can write the part of a header file that
/// this function actually uses: the SDK include for each module, a prototype
/// for every import called, forward declarations for the functions in this
/// module it calls, and the module data it touches.
///
/// The widths are real, taken from the import table and the call sites. The
/// types are not: a `WORD` here may have been an `HWND` or a `BOOL`, and only
/// the header this stands in for could say which.
pub fn preamble(program: &Program, db: &ApiDb, f: &Function, label: &dyn Fn(&Function) -> String) -> Vec<Line> {
    let Some(code) = program.code.get(&f.addr.segment) else {
        return Vec::new();
    };
    let body: Vec<(usize, &Insn)> = code
        .insns
        .iter()
        .enumerate()
        .filter(|(_, i)| i.offset >= f.addr.offset && i.offset < f.end)
        .collect();

    let mut imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut internal: BTreeSet<String> = BTreeSet::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();

    for (idx, insn) in &body {
        if !matches!(insn.flow, Flow::Call | Flow::CallFar) {
            continue;
        }
        if let Some(call) = callargs::reconstruct(code, *idx, db) {
            imports
                .entry(call.module.clone())
                .or_default()
                .insert(call.signature.prototype());
            continue;
        }
        match insn.fixup.as_ref().map(|x| &x.target) {
            Some(Target::Internal {
                segment,
                offset: Some(off),
            }) => {
                if let Some(callee) = program.function_at(Addr {
                    segment: *segment,
                    offset: *off as u32,
                }) {
                    internal.insert(callee.frame.signature(&label(callee)));
                }
            }
            // An import with no signature in the database still has a name,
            // and a name with no prototype is worth saying out loud.
            Some(t) => {
                unknown.insert(t.to_string());
            }
            None => {
                if let Some(callee) = insn.near_target.and_then(|t| {
                    program.function_at(Addr {
                        segment: f.addr.segment,
                        offset: t,
                    })
                }) {
                    internal.insert(callee.frame.signature(&label(callee)));
                }
            }
        }
    }

    let globals = global_reads(&body);

    let mut out = Vec::new();
    let mut headers: BTreeSet<&'static str> = BTreeSet::new();
    let mut headerless: BTreeSet<String> = BTreeSet::new();
    for module in imports.keys() {
        match ne_core::api::header_for(module) {
            Some(h) => {
                headers.insert(h);
            }
            None => {
                headerless.insert(module.clone());
            }
        }
    }
    for h in &headers {
        out.push(Line::new(Kind::Include, 0, format!("#include <{h}>")));
    }
    for m in &headerless {
        // A DLL belonging to the program has no standard header, so the
        // prototypes below are all there is.
        out.push(Line::new(
            Kind::Comment,
            0,
            format!("// {m}: no standard header; declared below"),
        ));
    }
    if !out.is_empty() {
        out.push(Line::new(Kind::Punct, 0, ""));
    }

    for (module, protos) in &imports {
        out.push(Line::new(Kind::Comment, 0, format!("// from {module}")));
        for p in protos {
            out.push(Line::new(Kind::Decl, 0, format!("extern {p}")));
        }
        out.push(Line::new(Kind::Punct, 0, ""));
    }

    if !unknown.is_empty() {
        out.push(Line::new(
            Kind::Comment,
            0,
            "// imported, with no signature on record",
        ));
        for name in &unknown {
            out.push(Line::new(Kind::Comment, 0, format!("//   {name}")));
        }
        out.push(Line::new(Kind::Punct, 0, ""));
    }

    if !internal.is_empty() {
        out.push(Line::new(Kind::Comment, 0, "// in this module"));
        for sig in &internal {
            out.push(Line::new(Kind::Decl, 0, format!("static WORD {sig};")));
        }
        out.push(Line::new(Kind::Punct, 0, ""));
    }

    if !globals.is_empty() {
        out.push(Line::new(
            Kind::Comment,
            0,
            "// module data, named by where it sits",
        ));
        for g in &globals {
            out.push(Line::new(Kind::Decl, 0, format!("static WORD {g};")));
        }
        out.push(Line::new(Kind::Punct, 0, ""));
    }
    out
}

/// The data-segment globals a body reads or writes.
fn global_reads(body: &[(usize, &Insn)]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, insn) in body {
        let Some((_, ops)) = insn.text.split_once(' ') else {
            continue;
        };
        for op in split_operands(ops) {
            let named = operand(&op, &HashMap::new(), &[]);
            if named.starts_with("g_") {
                out.insert(named);
            }
        }
    }
    out
}

/// Render one function.
pub fn function(program: &Program, db: &ApiDb, f: &Function, name: &str) -> Vec<Line> {
    let Some(code) = program.code.get(&f.addr.segment) else {
        return vec![Line::new(Kind::Comment, 0, "// no code for this segment")];
    };
    let body: Vec<(usize, &Insn)> = code
        .insns
        .iter()
        .enumerate()
        .filter(|(_, i)| i.offset >= f.addr.offset && i.offset < f.end)
        .collect();
    if body.is_empty() {
        return vec![Line::new(Kind::Comment, 0, "// nothing decoded here")];
    }

    let labels = branch_targets(&body);
    let plan = plan(&body, &labels);
    let locals = local_slots(&body);
    let args = argument_names(f);
    // Frame setup and teardown, which the signature and the declarations
    // above already say, and comparisons whose branch says them better.
    let (hidden, saved) = frame_scaffolding(&body);
    let mut out = Vec::new();

    header(&mut out, program, f, name, &saved);
    // The return type is the convention, not a fact, which is what the
    // header comment above just said.
    out.push(Line::new(
        Kind::Signature,
        0,
        format!("WORD {}", f.frame.signature(name)),
    ));
    out.push(Line::new(Kind::Punct, 0, "{"));

    for (offset, size) in &locals {
        out.push(Line::new(
            Kind::Decl,
            1,
            format!("{} {};", size, local_name(*offset)),
        ));
    }
    if !locals.is_empty() {
        out.push(Line::new(Kind::Punct, 1, ""));
    }

    // The instruction that last set the flags, so a branch can be written as
    // the comparison it actually tests rather than as a jump on a flag.
    let mut flags: Option<&Insn> = None;
    // Offsets a peephole above has already accounted for.
    let mut folded: BTreeSet<u32> = BTreeSet::new();
    // Pushes already consumed as the arguments of a call below them.
    let consumed = consumed_pushes(code, &body, db);

    // Blocks currently open, innermost last, each with the offset it ends at.
    let mut depth: Vec<u32> = Vec::new();
    let indent = |depth: &Vec<u32>| (depth.len() as u8) + 1;

    for (i, (idx, insn)) in body.iter().enumerate() {
        while depth.last() == Some(&insn.offset) {
            depth.pop();
            out.push(Line::new(Kind::Punct, indent(&depth), "}"));
        }
        if plan.labels.contains(&insn.offset) {
            out.push(Line::new(Kind::Label, 0, format!("{}:", label_name(insn.offset))));
        }
        if plan.loops.contains(&insn.offset) {
            out.push(Line::new(Kind::Control, indent(&depth), "do {"));
            depth.push(u32::MAX);
        }
        // Flags are recorded before anything is skipped: a comparison hidden
        // because the branch below it says it better is exactly the
        // comparison that branch needs to read.
        let sets_flags = is_flag_test(insn) || writes_flags(insn);
        // `sbb r,r` / `inc r` is not arithmetic, it is how a 16-bit compiler
        // writes a comparison into a variable. Left as two instructions it is
        // unreadable; folded, it says what the code is actually computing.
        // A branch that became a block writes the block instead of itself.
        match plan.shape.get(&insn.offset) {
            Some(Shape::If { end }) => {
                let (_, ops) = insn.text.split_once(' ').unwrap_or((insn.text.as_str(), ""));
                let mnem = insn.text.split(' ').next().unwrap_or("");
                let _ = ops;
                let cond = condition(invert(mnem), flags, &args, &locals);
                let mut line =
                    Line::at(insn.offset, Kind::Control, format!("if ({cond}) {{"));
                line.indent = indent(&depth);
                out.push(line);
                depth.push(*end);
                flags = None;
                continue;
            }
            Some(Shape::Else { end }) => {
                depth.pop();
                out.push(Line::new(Kind::Control, indent(&depth), "} else {"));
                depth.push(*end);
                flags = None;
                continue;
            }
            Some(Shape::JumpOver { target }) => {
                let mnem = insn.text.split(' ').next().unwrap_or("");
                let cond = condition(invert(mnem), flags, &args, &locals);
                let mut line = Line::at(
                    insn.offset,
                    Kind::Control,
                    format!("if ({cond}) goto {};", label_name(*target)),
                );
                line.indent = indent(&depth);
                out.push(line);
                flags = None;
                continue;
            }
            Some(Shape::Folded) => {
                flags = None;
                continue;
            }
            Some(Shape::While) => {
                let mnem = insn.text.split(' ').next().unwrap_or("");
                let cond = condition(mnem, flags, &args, &locals);
                depth.pop();
                let mut line = Line::at(
                    insn.offset,
                    Kind::Control,
                    format!("}} while ({cond});"),
                );
                line.indent = indent(&depth);
                out.push(line);
                flags = None;
                continue;
            }
            None => {}
        }

        if let Some((text, eat)) = boolean_idiom(&body, i, flags, &args, &locals) {
            let mut line = Line::at(insn.offset, Kind::Statement, text);
            line.indent = indent(&depth);
            out.push(line);
            folded.insert(eat);
            flags = None;
            continue;
        }
        if !(consumed.contains(&insn.offset) || hidden.contains(&insn.offset)
            || folded.contains(&insn.offset))
        {
            let mut line = statement(program, db, code, *idx, insn, flags, &args, &locals, &labels, f)
                .unwrap_or_else(|| {
                    Line::at(
                        insn.offset,
                        Kind::Asm,
                        format!("__asm {{ {} }}", squash(&insn.text)),
                    )
                });
            line.indent = indent(&depth);
            out.push(line);
        }
        flags = if sets_flags {
            Some(insn)
        } else if insn.flow == Flow::CondJump {
            flags
        } else {
            None
        };
    }

    // Anything still open ran to the end of the function.
    while !depth.is_empty() {
        depth.pop();
        out.push(Line::new(Kind::Punct, indent(&depth), "}"));
    }
    out.push(Line::new(Kind::Punct, 0, "}"));
    out
}

/// What the lifter wants the reader to know before they trust any of it.
fn header(out: &mut Vec<Line>, program: &Program, f: &Function, name: &str, saved: &[String]) {
    out.push(Line::new(
        Kind::Comment,
        0,
        format!("// {name}  at {}", f.addr),
    ));
    out.push(Line::new(
        Kind::Comment,
        0,
        format!(
            "// {} bytes, {} instructions, {} calls",
            f.size(),
            f.insn_count,
            f.calls.len()
        ),
    ));
    let callers = program.callers_of(f.addr).len();
    out.push(Line::new(
        Kind::Comment,
        0,
        format!(
            "// called from {callers} place{}",
            if callers == 1 { "" } else { "s" }
        ),
    ));
    out.push(Line::new(
        Kind::Comment,
        0,
        "// Reconstructed from the stack frame and the call sites. Registers",
    ));
    out.push(Line::new(
        Kind::Comment,
        0,
        "// are machine registers, not variables, and the return value is ax",
    ));
    out.push(Line::new(
        Kind::Comment,
        0,
        "// by convention rather than by proof.",
    ));
    if !saved.is_empty() {
        out.push(Line::new(
            Kind::Comment,
            0,
            format!("// saves and restores {}", saved.join(", ")),
        ));
    }
}

/// What structuring made of a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A forward conditional jump that skips a block: `if (!cond) { ... }`.
    If { end: u32 },
    /// The unconditional jump at the end of an if body, which is its else.
    Else { end: u32 },
    /// A backward conditional jump closing a loop: `} while (cond);`.
    While,
    /// A conditional jump over a single unconditional one, which is the long
    /// way of writing the opposite branch straight to where it goes.
    JumpOver { target: u32 },
    /// The unconditional jump that a `JumpOver` above swallowed.
    Folded,
}

/// Where blocks open and close, and which branches were spent doing it.
#[derive(Debug, Default)]
struct Plan {
    /// Branch offset -> what it became.
    shape: HashMap<u32, Shape>,
    /// Offsets that a `do {` opens before.
    loops: BTreeSet<u32>,
    /// Offsets where a block ends, innermost last.
    closes: HashMap<u32, usize>,
    /// Labels still reached by a goto that survived structuring.
    labels: BTreeSet<u32>,
}

/// Turn the branches into blocks, where the branches allow it.
///
/// Conservative on purpose. A region only becomes a block when nothing jumps
/// into the middle of it from outside, because a block with two entrances is
/// not a block -- writing one as if it were would be the first place this
/// view lied. Everything else keeps its `goto`, which is ugly and correct.
fn plan(body: &[(usize, &Insn)], labels: &BTreeSet<u32>) -> Plan {
    let mut plan = Plan::default();
    // target -> the offsets that branch to it.
    let mut incoming: HashMap<u32, Vec<u32>> = HashMap::new();
    for (_, insn) in body {
        if matches!(insn.flow, Flow::Jump | Flow::CondJump) {
            if let Some(t) = insn.near_target.filter(|t| labels.contains(t)) {
                incoming.entry(t).or_default().push(insn.offset);
            }
        }
    }

    // Nothing outside `[lo, hi)` may branch into it past its first
    // instruction. Entering at `lo` is how you get there; entering anywhere
    // else means the region has a second entrance.
    let sealed = |lo: u32, hi: u32| {
        incoming.iter().all(|(to, froms)| {
            !(lo < *to && *to < hi) || froms.iter().all(|f| *f >= lo && *f < hi)
        })
    };

    let offsets: Vec<u32> = body.iter().map(|(_, i)| i.offset).collect();
    let last_before = |t: u32| offsets.iter().rev().find(|o| **o < t).copied();

    // Loops first: a back edge claims its whole span, so an `if` inside it
    // nests rather than the other way round.
    let mut ends: Vec<u32> = Vec::new();
    for (_, insn) in body {
        if insn.flow != Flow::CondJump {
            continue;
        }
        let Some(t) = insn.near_target.filter(|t| labels.contains(t)) else {
            continue;
        };
        if t <= insn.offset && sealed(t, insn.offset) {
            plan.shape.insert(insn.offset, Shape::While);
            plan.loops.insert(t);
            ends.push(insn.offset);
        }
    }

    // Then forward conditionals, outermost first so nesting is checked
    // against blocks that are already open.
    let mut open: Vec<u32> = Vec::new();
    for (_, insn) in body {
        while open.last().is_some_and(|e| *e <= insn.offset) {
            open.pop();
        }
        if insn.flow != Flow::CondJump || plan.shape.contains_key(&insn.offset) {
            continue;
        }
        let Some(t) = insn.near_target.filter(|t| labels.contains(t)) else {
            continue;
        };
        if t <= insn.offset {
            continue;
        }
        // A block has to close inside whatever already surrounds it.
        if open.last().is_some_and(|e| t > *e) || !sealed(insn.offset, t) {
            continue;
        }
        // `if (c) goto next; goto L;` is the long way of writing
        // `if (!c) goto L;`. Fold it rather than wrapping it in a block,
        // which would be a third way of saying the same thing.
        let inner: Vec<&Insn> = body
            .iter()
            .map(|(_, i)| *i)
            .filter(|i| i.offset > insn.offset && i.offset < t)
            .collect();
        if inner.len() == 1 && inner[0].flow == Flow::Jump {
            if let Some(e) = inner[0].near_target {
                plan.shape.insert(insn.offset, Shape::JumpOver { target: e });
                plan.shape.insert(inner[0].offset, Shape::Folded);
                continue;
            }
        }
        if inner.is_empty() {
            continue;
        }

        // An unconditional jump as the body's last instruction is its else.
        let mut end = t;
        let tail = last_before(t).and_then(|o| body.iter().find(|(_, i)| i.offset == o));
        if let Some((_, j)) = tail {
            if j.flow == Flow::Jump && !plan.shape.contains_key(&j.offset) {
                if let Some(e) = j.near_target.filter(|e| *e > t && labels.contains(e)) {
                    if sealed(t, e) && !open.last().is_some_and(|o| e > *o) {
                        plan.shape.insert(j.offset, Shape::Else { end: e });
                        end = e;
                    }
                }
            }
        }
        plan.shape.insert(insn.offset, Shape::If { end: t });
        *plan.closes.entry(end).or_default() += 1;
        open.push(end);
    }

    // A label is only worth printing if something still jumps to it, and
    // what each branch jumps to is decided by the shape it became.
    for (_, insn) in body {
        if !matches!(insn.flow, Flow::Jump | Flow::CondJump) {
            continue;
        }
        match plan.shape.get(&insn.offset) {
            // These say where they go with a block, not with a label.
            Some(Shape::If { .. }) | Some(Shape::Else { .. }) | Some(Shape::While) => {}
            // Folded into the branch above it, which now carries the target.
            Some(Shape::Folded) => {}
            Some(Shape::JumpOver { target }) => {
                plan.labels.insert(*target);
            }
            None => {
                if let Some(t) = insn.near_target.filter(|t| labels.contains(t)) {
                    plan.labels.insert(t);
                }
            }
        }
    }
    plan
}

/// The opposite branch, for writing `if (cond) skip` as `if (!cond) do`.
fn invert(mnem: &str) -> &str {
    match mnem {
        "je" | "jz" => "jne",
        "jne" | "jnz" => "je",
        "jl" | "jnge" => "jge",
        "jge" | "jnl" => "jl",
        "jle" | "jng" => "jg",
        "jg" | "jnle" => "jle",
        "jb" | "jnae" | "jc" => "jae",
        "jae" | "jnb" | "jnc" => "jb",
        "jbe" | "jna" => "ja",
        "ja" | "jnbe" => "jbe",
        "js" => "jns",
        "jns" => "js",
        other => other,
    }
}

/// Whether an instruction is there to set flags for the branch below it.
///
/// `cmp` and `test` obviously are. So is `or ax,ax`, which is how a 16-bit
/// compiler asks whether a register is zero -- the result is the value it
/// already held, so the only thing the instruction produces is flags.
fn is_flag_test(insn: &Insn) -> bool {
    match insn.mnemonic {
        Mnemonic::Cmp | Mnemonic::Test => true,
        Mnemonic::Or | Mnemonic::And => {
            let ops = insn
                .text
                .split_once(' ')
                .map(|(_, o)| split_operands(o))
                .unwrap_or_default();
            ops.len() == 2 && ops[0] == ops[1]
        }
        _ => false,
    }
}

/// Whether an instruction leaves flags a branch below it could be reading.
///
/// Arithmetic sets flags as a side effect, and a 16-bit compiler leans on
/// that: a switch comes out as a chain of `sub ax,n` / `je`, each testing
/// whether what is left is zero rather than comparing against anything.
fn writes_flags(insn: &Insn) -> bool {
    matches!(
        insn.mnemonic,
        Mnemonic::Sub
            | Mnemonic::Add
            | Mnemonic::Adc
            | Mnemonic::Sbb
            | Mnemonic::And
            | Mnemonic::Or
            | Mnemonic::Xor
            | Mnemonic::Inc
            | Mnemonic::Dec
            | Mnemonic::Neg
            | Mnemonic::Shl
            | Mnemonic::Sal
            | Mnemonic::Shr
            | Mnemonic::Sar
    )
}

/// Whether this instruction begins the `sbb r,r` comparison-to-value idiom.
fn starts_boolean_idiom(insn: &Insn) -> bool {
    if insn.mnemonic != Mnemonic::Sbb {
        return false;
    }
    insn.text
        .split_once(' ')
        .map(|(_, o)| split_operands(o))
        .is_some_and(|ops| ops.len() == 2 && ops[0] == ops[1])
}

/// Offsets that something inside the function branches to.
fn branch_targets(body: &[(usize, &Insn)]) -> BTreeSet<u32> {
    let lo = body.first().map(|(_, i)| i.offset).unwrap_or(0);
    let hi = body.last().map(|(_, i)| i.offset).unwrap_or(0);
    body.iter()
        .filter(|(_, i)| matches!(i.flow, Flow::Jump | Flow::CondJump))
        .filter_map(|(_, i)| i.near_target)
        .filter(|t| *t >= lo && *t <= hi)
        .collect()
}

/// Negative frame displacements the body touches, with a width for each.
///
/// The width comes from the gap to the next slot. Two words a byte apart are
/// two words; a slot with sixteen bytes below it before the next one is
/// something larger, and calling it an array is closer than calling it a word.
fn local_slots(body: &[(usize, &Insn)]) -> Vec<(i32, String)> {
    let mut offsets: Vec<i32> = body
        .iter()
        .filter_map(|(_, i)| i.bp_displacement)
        .filter(|d| *d < 0)
        .collect();
    offsets.sort_unstable();
    offsets.dedup();

    let mut out = Vec::new();
    for (i, off) in offsets.iter().enumerate() {
        let next = offsets.get(i + 1).copied().unwrap_or(0);
        let span = next - off;
        let ty = match span {
            1 => "char".to_owned(),
            2 => "int".to_owned(),
            4 => "long".to_owned(),
            n if n > 4 => format!("char /*[{n}]*/"),
            _ => "int".to_owned(),
        };
        out.push((*off, ty));
    }
    out
}

fn local_name(offset: i32) -> String {
    format!("local_{:X}", -offset)
}

fn label_name(offset: u32) -> String {
    format!("L_{offset:04X}")
}

/// Frame offset -> argument name, for rewriting `[bp+n]`.
fn argument_names(f: &Function) -> HashMap<i32, String> {
    f.frame
        .parameters()
        .into_iter()
        .map(|p| (p.offset, p.name()))
        .collect()
}

/// The pushes that belong to a call's argument list.
///
/// They are dropped from the output because the call renders them itself.
/// Anything left over stays as a push, which is the honest thing to do: a
/// push that is not part of a reconstructed call is doing something else.
fn consumed_pushes(
    code: &SegmentCode,
    body: &[(usize, &Insn)],
    db: &ApiDb,
) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (idx, insn) in body {
        if insn.flow != Flow::CallFar && insn.flow != Flow::Call {
            continue;
        }
        let Some(call) = callargs::reconstruct(code, *idx, db) else {
            continue;
        };
        // Exactly the pushes the reconstruction walked over. Whether their
        // values were recovered is a separate question from whether the call
        // has already said them.
        let mut back = *idx;
        let mut left = call.pushes;
        while left > 0 && back > 0 {
            back -= 1;
            let prev = &code.insns[back];
            if !callargs::iced_mnemonic::is_push(prev) {
                break;
            }
            out.insert(prev.offset);
            left -= 1;
        }
    }
    out
}

/// `sbb r,r` followed by `inc r`, which is a comparison being stored.
///
/// `sbb r,r` leaves -1 when the carry is set and 0 when it is not, so the
/// `inc` turns it into 1 when the carry is clear. After a `cmp a,b` the carry
/// is clear exactly when `a >= b`, which is the whole statement. Returns the
/// text and the offset of the instruction it swallowed.
fn boolean_idiom(
    body: &[(usize, &Insn)],
    i: usize,
    flags: Option<&Insn>,
    args: &HashMap<i32, String>,
    locals: &[(i32, String)],
) -> Option<(String, u32)> {
    let (_, insn) = body.get(i)?;
    if insn.mnemonic != Mnemonic::Sbb {
        return None;
    }
    let (_, ops) = insn.text.split_once(' ')?;
    let parts = split_operands(ops);
    let reg = parts.first()?;
    if parts.get(1) != Some(reg) {
        return None;
    }
    let (_, next) = body.get(i + 1)?;
    let (_, next_ops) = next.text.split_once(' ')?;
    let inverted = match next.mnemonic {
        Mnemonic::Inc => false,
        Mnemonic::Neg => true,
        _ => return None,
    };
    if split_operands(next_ops).first() != Some(reg) {
        return None;
    }

    // `neg` keeps the carry's own sense; `inc` flips it.
    let cond = condition(if inverted { "jb" } else { "jae" }, flags, args, locals);
    Some((format!("{reg} = ({cond});"), next.offset))
}

/// Instructions that say nothing the reader has not already been told.
///
/// The prologue and epilogue describe the frame that the signature and the
/// local declarations already describe. They are matched as runs from each
/// end rather than by pattern anywhere in the body, because the same
/// instruction means something quite different in the middle of a function:
/// `mov ds,ax` on the way in is the Win16 data-segment reload, and `mov ds,ax`
/// two hundred bytes later is the program doing something.
///
/// Also returns the registers the prologue saved, which is worth stating once
/// rather than as four lines of pushes and four of pops.
fn frame_scaffolding(body: &[(usize, &Insn)]) -> (BTreeSet<u32>, Vec<String>) {
    use iced_x86::Register;
    let mut out = BTreeSet::new();
    let mut saved: Vec<String> = Vec::new();

    let reg_of = |insn: &Insn, n: usize| -> Option<String> {
        insn.text
            .split_once(' ')
            .map(|(_, o)| split_operands(o))
            .and_then(|ops| ops.get(n).cloned())
    };
    let is_reg = |name: &str| {
        matches!(
            name,
            "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" | "sp" | "ds" | "es" | "ss" | "cs"
        )
    };

    for (_, insn) in body {
        let keep = match insn.mnemonic {
            Mnemonic::Nop => false,
            // The far-frame marker a Win16 compiler sets so a debugger can
            // tell a far frame from a near one.
            Mnemonic::Inc | Mnemonic::Dec if insn.op0_register == Some(Register::BP) => false,
            Mnemonic::Push => {
                let r = reg_of(insn, 0).unwrap_or_default();
                if r == "bp" || r == "ds" {
                    false
                } else if is_reg(&r) {
                    saved.push(r);
                    false
                } else {
                    true
                }
            }
            Mnemonic::Mov => {
                let dst = reg_of(insn, 0).unwrap_or_default();
                let src = reg_of(insn, 1).unwrap_or_default();
                !((dst == "bp" || dst == "ds" || dst == "sp") && is_reg(&src)
                    || dst == "ax" && src == "ds")
            }
            Mnemonic::Sub | Mnemonic::Add if insn.op0_register == Some(Register::SP) => false,
            _ => true,
        };
        if keep {
            break;
        }
        out.insert(insn.offset);
    }
    // `mov ax,ds` only belongs to the prologue if `mov ds,ax` follows it.
    // Otherwise it is a real read, and dropping it would hide one.
    if !body
        .iter()
        .any(|(_, i)| out.contains(&i.offset) && i.op0_register == Some(Register::DS))
    {
        if let Some((_, first)) = body.first() {
            if first.op0_register == Some(Register::AX) {
                out.remove(&first.offset);
            }
        }
    }
    let _ = Register::AX;

    for (_, insn) in body.iter().rev() {
        let keep = match insn.mnemonic {
            Mnemonic::Ret | Mnemonic::Retf | Mnemonic::Nop | Mnemonic::Leave => false,
            Mnemonic::Inc | Mnemonic::Dec if insn.op0_register == Some(Register::BP) => false,
            Mnemonic::Pop => !is_reg(&reg_of(insn, 0).unwrap_or_default()),
            // `lea sp,[bp-n]` and `mov sp,bp` both put the stack back.
            Mnemonic::Lea | Mnemonic::Mov if insn.op0_register == Some(Register::SP) => false,
            Mnemonic::Add | Mnemonic::Sub if insn.op0_register == Some(Register::SP) => false,
            _ => true,
        };
        if keep {
            break;
        }
        // The return itself is the one part of the epilogue worth keeping:
        // it is where the function ends.
        if !matches!(insn.mnemonic, Mnemonic::Ret | Mnemonic::Retf) {
            out.insert(insn.offset);
        }
    }

    // A nop is padding wherever it sits, not only in the prologue.
    for (_, insn) in body {
        if insn.mnemonic == Mnemonic::Nop {
            out.insert(insn.offset);
        }
    }

    // Then, anywhere in the body, a flag test whose result is written out by
    // what follows -- a conditional jump, or the sbb idiom -- is said better
    // there than on a line of its own.
    for (i, (_, insn)) in body.iter().enumerate() {
        let next = body.get(i + 1).map(|(_, n)| *n);
        if is_flag_test(insn)
            && next.is_some_and(|n| n.flow == Flow::CondJump || starts_boolean_idiom(n))
        {
            out.insert(insn.offset);
        }
    }

    saved.sort();
    saved.dedup();
    (out, saved)
}

/// One instruction as a C statement, or `None` when it has no C shape.
fn statement(
    program: &Program,
    db: &ApiDb,
    code: &SegmentCode,
    idx: usize,
    insn: &Insn,
    flags: Option<&Insn>,
    args: &HashMap<i32, String>,
    locals: &[(i32, String)],
    labels: &BTreeSet<u32>,
    f: &Function,
) -> Option<Line> {
    let jump_to = |target: Option<u32>| match target {
        Some(t) if labels.contains(&t) => label_name(t),
        // Outside the function. Naming it as a label would send the reader
        // looking for one that is not here.
        Some(t) => format!(
            "/* {} */",
            Addr {
                segment: f.addr.segment,
                offset: t
            }
        ),
        None => "/* indirect */".to_owned(),
    };
    let (mnem, ops) = insn.text.split_once(' ').unwrap_or((insn.text.as_str(), ""));
    let ops: Vec<String> = split_operands(ops)
        .into_iter()
        .map(|o| operand(&o, args, locals))
        .collect();
    let a = ops.first().cloned().unwrap_or_default();
    let b = ops.get(1).cloned().unwrap_or_default();

    // Calls first: they are the reason this view is worth having.
    if matches!(insn.flow, Flow::Call | Flow::CallFar) {
        return Some(Line::at(
            insn.offset,
            Kind::Call,
            call_text(program, db, code, idx, insn, args, locals, f.addr.segment),
        ));
    }
    if insn.flow == Flow::CallIndirect {
        return Some(Line::at(insn.offset, Kind::Call, format!("ax = (*{a})();")));
    }

    let text = match insn.mnemonic {
        Mnemonic::Mov => format!("{a} = {b};"),
        Mnemonic::Lea => format!("{a} = &{b};"),
        Mnemonic::Movzx => format!("{a} = (unsigned){b};"),
        Mnemonic::Movsx => format!("{a} = (int){b};"),
        Mnemonic::Xchg => format!("swap({a}, {b});"),
        // `les bx,[bp+6]` loads a far pointer: the segment half lands in es.
        Mnemonic::Les => format!("es:{a} = (void far *){b};"),
        Mnemonic::Lds => format!("ds:{a} = (void far *){b};"),
        Mnemonic::Add => format!("{a} += {b};"),
        Mnemonic::Sub => format!("{a} -= {b};"),
        Mnemonic::Adc => format!("{a} += {b} + carry;"),
        Mnemonic::Sbb => format!("{a} -= {b} + carry;"),
        Mnemonic::And => format!("{a} &= {b};"),
        Mnemonic::Or => format!("{a} |= {b};"),
        // The idiom, not the instruction. `xor ax,ax` is how a compiler
        // writes zero, and rendering it as an exclusive or hides that.
        Mnemonic::Xor if a == b => format!("{a} = 0;"),
        Mnemonic::Xor => format!("{a} ^= {b};"),
        Mnemonic::Shl | Mnemonic::Sal => format!("{a} <<= {b};"),
        Mnemonic::Shr | Mnemonic::Sar => format!("{a} >>= {b};"),
        Mnemonic::Neg => format!("{a} = -{a};"),
        Mnemonic::Not => format!("{a} = ~{a};"),
        Mnemonic::Inc => format!("{a}++;"),
        Mnemonic::Dec => format!("{a}--;"),
        Mnemonic::Imul | Mnemonic::Mul if ops.len() == 1 => format!("dx:ax = ax * {a};"),
        Mnemonic::Imul | Mnemonic::Mul => format!("{a} *= {b};"),
        Mnemonic::Idiv | Mnemonic::Div => format!("ax = dx:ax / {a};  dx = dx:ax % {a};"),
        Mnemonic::Push => format!("push({a});"),
        Mnemonic::Pop => format!("{a} = pop();"),
        Mnemonic::Int => format!("interrupt({a});"),
        // A comparison has no effect of its own; it is the branch below it
        // that says what was being asked.
        // A comparison that reached here is one no branch below it used,
        // so it is worth showing: something else is reading those flags.
        Mnemonic::Cmp => format!("compare({a}, {b});"),
        Mnemonic::Test => format!("test({a}, {b});"),
        Mnemonic::Ret | Mnemonic::Retf => {
            return Some(Line::at(insn.offset, Kind::Return, "return ax;"))
        }
        _ => match insn.flow {
            Flow::Jump => format!("goto {};", jump_to(insn.near_target)),
            Flow::CondJump => format!(
                "if ({}) goto {};",
                condition(mnem, flags, args, locals),
                jump_to(insn.near_target)
            ),
            _ => return None,
        },
    };

    let kind = match insn.flow {
        Flow::Jump | Flow::CondJump => Kind::Control,
        Flow::Return => Kind::Return,
        _ => Kind::Statement,
    };
    Some(Line::at(insn.offset, kind, text))
}

/// The call, with whatever the reconstruction could establish about it.
#[allow(clippy::too_many_arguments)]
fn call_text(
    program: &Program,
    db: &ApiDb,
    code: &SegmentCode,
    idx: usize,
    insn: &Insn,
    args: &HashMap<i32, String>,
    locals: &[(i32, String)],
    f_segment: u16,
) -> String {
    if let Some(call) = callargs::reconstruct(code, idx, db) {
        let args: Vec<String> = call
            .args
            .iter()
            .map(|a| {
                // The reconstruction hands back assembly operands. Naming
                // them here keeps one vocabulary across the whole view.
                let raw = a.render();
                if a.name.is_some() || a.value.is_some() {
                    raw
                } else {
                    raw.split(':')
                        .map(|part| operand(part, args, locals))
                        .collect::<Vec<_>>()
                        .join(":")
                }
            })
            .collect();
        // The note belongs on a list with a hole in it, not on one where a
        // value simply came from a register rather than a literal. An
        // argument the reconstruction could say nothing at all about renders
        // as a question mark, and that is the case worth flagging.
        let missing = args.iter().any(|a| a.contains('?'));
        let mut text = format!("{}.{}({})", call.module, call.function, args.join(", "));
        if missing {
            text.push_str("  /* some arguments not recovered */");
        }
        // A call whose signature says it returns nothing is a statement.
        // One with no stated return type might return something, so the
        // assignment stays and the reader can ignore it.
        let void = call
            .signature
            .ret
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case("void"));
        return if void {
            format!("{text};")
        } else {
            format!("ax = {text};")
        };
    }

    // No signature, so the name is all we have -- but a call inside this
    // module has one, and the fixup only knows where it points.
    let named = match insn.fixup.as_ref().map(|x| &x.target) {
        Some(Target::Internal {
            segment,
            offset: Some(off),
        }) => program
            .function_at(Addr {
                segment: *segment,
                offset: *off as u32,
            })
            .map(|f| f.label()),
        Some(t) => Some(t.to_string()),
        None => insn
            .near_target
            .and_then(|t| {
                program.function_at(Addr {
                    segment: f_segment,
                    offset: t,
                })
            })
            .map(|f| f.label()),
    };
    match named {
        Some(name) => format!("ax = {name}();"),
        None => format!("ax = {}();", squash(&insn.text)),
    }
}

/// A conditional jump written as the test it performs.
fn condition(
    mnem: &str,
    flags: Option<&Insn>,
    args: &HashMap<i32, String>,
    locals: &[(i32, String)],
) -> String {
    // Arithmetic leaves its own result in the destination, so the branch
    // below is asking about that value against zero.
    if let Some(f) = flags {
        if !is_flag_test(f) && writes_flags(f) {
            let dest = f
                .text
                .split_once(' ')
                .map(|(_, o)| split_operands(o))
                .and_then(|o| o.first().cloned())
                .map(|o| operand(&o, args, locals))
                .unwrap_or_else(|| "?".into());
            // The carry flag after an add or a subtract is a carry out,
            // not a sign. Reading `jb` as "less than zero" is wrong in the
            // one case the compiler most often means it: an overflow check.
            let op = match mnem {
                "je" | "jz" => "==",
                "jne" | "jnz" => "!=",
                "js" | "jl" | "jnge" => "<",
                "jns" | "jge" | "jnl" => ">=",
                "jle" | "jng" => "<=",
                "jg" | "jnle" => ">",
                "jb" | "jc" | "jnae" => return "carry".to_owned(),
                "jnb" | "jnc" | "jae" => return "!carry".to_owned(),
                "ja" | "jnbe" => return "!carry && !zero".to_owned(),
                "jbe" | "jna" => return "carry || zero".to_owned(),
                _ => return format!("{mnem}({dest})"),
            };
            return format!("{dest} {op} 0");
        }
    }

    let (lhs, rhs) = match flags {
        Some(f) => {
            let (_, ops) = f.text.split_once(' ').unwrap_or((f.text.as_str(), ""));
            let parts = split_operands(ops);
            (
                parts
                    .first()
                    .map(|o| operand(o, args, locals))
                    .unwrap_or_else(|| "?".into()),
                parts
                    .get(1)
                    .map(|o| operand(o, args, locals))
                    .unwrap_or_else(|| "?".into()),
            )
        }
        None => return format!("{mnem} /* no comparison found */"),
    };

    // `test x,x` asks whether x is zero, which is not what "and" means.
    let is_test = flags.is_some_and(|f| {
        matches!(f.mnemonic, Mnemonic::Test | Mnemonic::Or | Mnemonic::And)
    });
    if is_test && lhs == rhs {
        return match mnem {
            "je" | "jz" => format!("{lhs} == 0"),
            "jne" | "jnz" => format!("{lhs} != 0"),
            "js" => format!("{lhs} < 0"),
            "jns" => format!("{lhs} >= 0"),
            _ => format!("{mnem}({lhs})"),
        };
    }
    let op = match mnem {
        "je" | "jz" => "==",
        "jne" | "jnz" => "!=",
        "jl" | "jnge" | "jb" | "jnae" | "jc" => "<",
        "jle" | "jng" | "jbe" | "jna" => "<=",
        "jg" | "jnle" | "ja" | "jnbe" => ">",
        "jge" | "jnl" | "jae" | "jnb" | "jnc" => ">=",
        _ => return format!("{mnem}({lhs}, {rhs})"),
    };
    if is_test {
        return format!("({lhs} & {rhs}) {op} 0");
    }
    format!("{lhs} {op} {rhs}")
}

/// Split an operand list on commas that are not inside brackets.
fn split_operands(ops: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in ops.chars() {
        match c {
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|s| s.trim().to_owned()).collect()
}

/// Rewrite one operand into something with a name.
fn operand(op: &str, args: &HashMap<i32, String>, locals: &[(i32, String)]) -> String {
    let op = op.trim();
    // Size prefixes say nothing a C reader needs.
    let bare = op
        .trim_start_matches("word ")
        .trim_start_matches("byte ")
        .trim_start_matches("dword ")
        .trim_start_matches("short ")
        .trim();

    if let Some(d) = frame_displacement(bare) {
        if d > 0 {
            if let Some(name) = args.get(&d) {
                return name.clone();
            }
            return format!("arg_at_{d:X}");
        }
        if locals.iter().any(|(o, _)| *o == d) {
            return local_name(d);
        }
        return local_name(d);
    }

    // A bare memory operand is a data-segment global. Naming it by its
    // address is not a name, but it is stable, and it reads as one variable
    // rather than as an addressing mode.
    if let Some(inner) = bare.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some(v) = parse_hex(inner) {
            return format!("g_{v:04X}");
        }
        return format!("*({inner})");
    }

    // Hex literals read better in C form.
    if let Some(v) = parse_hex(bare) {
        return if v < 10 {
            v.to_string()
        } else {
            format!("{v:#X}")
        };
    }
    bare.to_owned()
}

/// The signed displacement of a `[bp+n]` or `[bp-n]` operand.
fn frame_displacement(op: &str) -> Option<i32> {
    let inner = op.strip_prefix('[')?.strip_suffix(']')?;
    let rest = inner.strip_prefix("bp")?;
    if rest.is_empty() {
        return Some(0);
    }
    let (sign, digits) = rest.split_at(1);
    let v = parse_hex(digits)? as i32;
    match sign {
        "+" => Some(v),
        "-" => Some(-v),
        _ => None,
    }
}

/// Parse a NASM-style number: `1234h`, `0FFFFh`, or plain decimal.
fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(body) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
        return u32::from_str_radix(body, 16).ok();
    }
    s.parse().ok()
}

/// Collapse runs of whitespace, for text going inside a statement.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> HashMap<i32, String> {
        [(6, "arg0".to_owned()), (8, "arg1".to_owned())]
            .into_iter()
            .collect()
    }

    #[test]
    fn frame_reads_become_argument_names() {
        assert_eq!(operand("word [bp+6]", &args(), &[]), "arg0");
        assert_eq!(operand("[bp+8]", &args(), &[]), "arg1");
    }

    #[test]
    fn frame_writes_below_the_pointer_are_locals() {
        assert_eq!(operand("[bp-2]", &args(), &[]), "local_2");
        assert_eq!(operand("[bp-0Ah]", &args(), &[]), "local_A");
    }

    #[test]
    fn a_bare_memory_operand_is_a_global() {
        assert_eq!(operand("word [13DAh]", &args(), &[]), "g_13DA");
    }

    #[test]
    fn hex_literals_come_out_in_c_form() {
        assert_eq!(operand("8000h", &args(), &[]), "0x8000");
        assert_eq!(operand("2", &args(), &[]), "2");
        assert_eq!(operand("0FFFFh", &args(), &[]), "0xFFFF");
    }

    #[test]
    fn operands_split_outside_brackets_only() {
        assert_eq!(split_operands("ax,ds"), vec!["ax", "ds"]);
        assert_eq!(split_operands("word [bx+si],ax"), vec!["word [bx+si]", "ax"]);
    }

    #[test]
    fn a_branch_reads_as_the_comparison_above_it() {
        let cmp = Insn {
            offset: 0,
            len: 3,
            bytes: vec![],
            text: "cmp ax,20h".into(),
            mnemonic: Mnemonic::Cmp,
            flow: Flow::Next,
            near_target: None,
            fixup: None,
            immediate: Some(0x20),
            bp_displacement: None,
            op0_register: None,
            operand_values: vec![],
        };
        assert_eq!(condition("ja", Some(&cmp), &args(), &[]), "ax > 0x20");
        assert_eq!(condition("je", Some(&cmp), &args(), &[]), "ax == 0x20");
    }

    #[test]
    fn a_self_test_asks_whether_the_value_is_zero() {
        let test = Insn {
            offset: 0,
            len: 2,
            bytes: vec![],
            text: "test ax,ax".into(),
            mnemonic: Mnemonic::Test,
            flow: Flow::Next,
            near_target: None,
            fixup: None,
            immediate: None,
            bp_displacement: None,
            op0_register: None,
            operand_values: vec![],
        };
        assert_eq!(condition("je", Some(&test), &args(), &[]), "ax == 0");
        assert_eq!(condition("jne", Some(&test), &args(), &[]), "ax != 0");
    }

    fn insn(offset: u32, text: &str, m: Mnemonic, flow: Flow, target: Option<u32>) -> Insn {
        Insn {
            offset,
            len: 2,
            bytes: vec![],
            text: text.into(),
            mnemonic: m,
            flow,
            near_target: target,
            fixup: None,
            immediate: None,
            bp_displacement: None,
            op0_register: None,
            operand_values: vec![],
        }
    }

    #[test]
    fn every_branch_has_an_opposite() {
        for m in ["je", "jne", "jl", "jge", "jle", "jg", "jb", "jae", "jbe", "ja", "js", "jns"] {
            assert_eq!(invert(invert(m)), m, "{m}");
            assert_ne!(invert(m), m, "{m}");
        }
        // Anything with no opposite is left alone rather than guessed at.
        assert_eq!(invert("jcxz"), "jcxz");
    }

    #[test]
    fn a_forward_branch_over_a_body_becomes_a_block() {
        let insns = vec![
            insn(0, "je 8h", Mnemonic::Je, Flow::CondJump, Some(8)),
            insn(2, "mov ax,1", Mnemonic::Mov, Flow::Next, None),
            insn(5, "mov bx,2", Mnemonic::Mov, Flow::Next, None),
            insn(8, "retf", Mnemonic::Retf, Flow::Return, None),
        ];
        let body: Vec<(usize, &Insn)> = insns.iter().enumerate().collect();
        let labels = branch_targets(&body);
        let p = plan(&body, &labels);
        assert_eq!(p.shape.get(&0), Some(&Shape::If { end: 8 }));
        // The block says where it goes, so the label is not needed.
        assert!(!p.labels.contains(&8));
    }

    #[test]
    fn a_branch_over_a_single_jump_folds_into_it() {
        let insns = vec![
            insn(0, "je 5h", Mnemonic::Je, Flow::CondJump, Some(5)),
            insn(2, "jmp 20h", Mnemonic::Jmp, Flow::Jump, Some(0x20)),
            insn(5, "mov ax,1", Mnemonic::Mov, Flow::Next, None),
            insn(0x20, "retf", Mnemonic::Retf, Flow::Return, None),
        ];
        let body: Vec<(usize, &Insn)> = insns.iter().enumerate().collect();
        let labels = branch_targets(&body);
        let p = plan(&body, &labels);
        assert_eq!(p.shape.get(&0), Some(&Shape::JumpOver { target: 0x20 }));
        assert_eq!(p.shape.get(&2), Some(&Shape::Folded));
        // The branch now carries the far target, and only that one.
        assert!(p.labels.contains(&0x20));
        assert!(!p.labels.contains(&5));
    }

    #[test]
    fn a_region_something_else_jumps_into_is_not_a_block() {
        let insns = vec![
            insn(0, "jmp 5h", Mnemonic::Jmp, Flow::Jump, Some(5)),
            insn(2, "je 8h", Mnemonic::Je, Flow::CondJump, Some(8)),
            insn(5, "mov ax,1", Mnemonic::Mov, Flow::Next, None),
            insn(8, "retf", Mnemonic::Retf, Flow::Return, None),
        ];
        let body: Vec<(usize, &Insn)> = insns.iter().enumerate().collect();
        let labels = branch_targets(&body);
        let p = plan(&body, &labels);
        // Offset 5 is reached from outside (2, 8), so it has two entrances
        // and writing it as a block would be a lie.
        assert_eq!(p.shape.get(&2), None);
        assert!(p.labels.contains(&8));
    }

    #[test]
    fn a_backward_branch_closes_a_loop() {
        let insns = vec![
            insn(0, "mov ax,1", Mnemonic::Mov, Flow::Next, None),
            insn(3, "dec cx", Mnemonic::Dec, Flow::Next, None),
            insn(5, "jne 0h", Mnemonic::Jne, Flow::CondJump, Some(0)),
            insn(7, "retf", Mnemonic::Retf, Flow::Return, None),
        ];
        let body: Vec<(usize, &Insn)> = insns.iter().enumerate().collect();
        let labels = branch_targets(&body);
        let p = plan(&body, &labels);
        assert_eq!(p.shape.get(&5), Some(&Shape::While));
        assert!(p.loops.contains(&0));
    }

    #[test]
    fn a_branch_with_nothing_above_it_says_so() {
        assert!(condition("je", None, &args(), &[]).contains("no comparison"));
    }
}
