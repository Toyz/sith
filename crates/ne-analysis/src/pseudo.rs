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
use iced_x86::Mnemonic;
use ne_core::api::ApiDb;
use ne_disasm::{Flow, Insn, SegmentCode};
use std::collections::{BTreeSet, HashMap};

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
    let locals = local_slots(&body);
    let args = argument_names(f);
    let mut out = Vec::new();

    header(&mut out, program, f, name);
    out.push(Line::new(Kind::Signature, 0, f.frame.signature(name)));
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
    // Frame setup and teardown, which the signature and the declarations
    // above already say, and comparisons whose branch says them better.
    let hidden = frame_scaffolding(&body);

    for (i, (idx, insn)) in body.iter().enumerate() {
        if labels.contains(&insn.offset) {
            out.push(Line::new(Kind::Label, 0, format!("{}:", label_name(insn.offset))));
        }
        // Flags are recorded before anything is skipped: a comparison hidden
        // because the branch below it says it better is exactly the
        // comparison that branch needs to read.
        let sets_flags = is_flag_test(insn) || writes_flags(insn);
        // `sbb r,r` / `inc r` is not arithmetic, it is how a 16-bit compiler
        // writes a comparison into a variable. Left as two instructions it is
        // unreadable; folded, it says what the code is actually computing.
        if let Some((text, eat)) = boolean_idiom(&body, i, flags, &args, &locals) {
            out.push(Line::at(insn.offset, Kind::Statement, text));
            folded.insert(eat);
            flags = None;
            continue;
        }
        if !(consumed.contains(&insn.offset) || hidden.contains(&insn.offset)
            || folded.contains(&insn.offset))
        {
            match statement(program, db, code, *idx, insn, flags, &args, &locals, &labels, f) {
                Some(line) => out.push(line),
                None => out.push(Line::at(
                    insn.offset,
                    Kind::Asm,
                    format!("__asm {{ {} }}", squash(&insn.text)),
                )),
            }
        }
        flags = if sets_flags {
            Some(insn)
        } else if insn.flow == Flow::CondJump {
            flags
        } else {
            None
        };
    }

    out.push(Line::new(Kind::Punct, 0, "}"));
    out
}

/// What the lifter wants the reader to know before they trust any of it.
fn header(out: &mut Vec<Line>, program: &Program, f: &Function, name: &str) {
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
/// The prologue and epilogue are the frame the signature and the local
/// declarations describe; `inc bp` is the Win16 marker for a far frame, not
/// arithmetic; a `nop` is padding; and a comparison immediately above a
/// conditional jump is written into the condition instead.
fn frame_scaffolding(body: &[(usize, &Insn)]) -> BTreeSet<u32> {
    use iced_x86::Register;
    let mut out = BTreeSet::new();
    for (i, (_, insn)) in body.iter().enumerate() {
        let next = body.get(i + 1).map(|(_, n)| *n);
        let hide = match insn.mnemonic {
            Mnemonic::Nop => true,
            // `inc bp` / `dec bp` around the frame, the marker a Win16
            // compiler sets so a debugger can tell a far frame from a near
            // one.
            Mnemonic::Inc | Mnemonic::Dec if insn.op0_register == Some(Register::BP) => true,
            // `push bp` followed by `mov bp,sp`, and the `sub sp,n` that
            // reserves the locals.
            Mnemonic::Push
                if insn.op0_register == Some(Register::BP)
                    && next.is_some_and(|n| {
                        n.mnemonic == Mnemonic::Mov && n.op0_register == Some(Register::BP)
                    }) =>
            {
                true
            }
            Mnemonic::Mov if insn.op0_register == Some(Register::BP) && i < 6 => true,
            Mnemonic::Sub | Mnemonic::Add if insn.op0_register == Some(Register::SP) => true,
            Mnemonic::Pop if insn.op0_register == Some(Register::BP) => true,
            Mnemonic::Leave => true,
            // A flag test whose result is written out by what follows -- a
            // conditional jump, or the sbb idiom -- is said better there.
            _ if is_flag_test(insn) => next
                .is_some_and(|n| n.flow == Flow::CondJump || starts_boolean_idiom(n)),
            _ => false,
        };
        if hide {
            out.insert(insn.offset);
        }
    }
    out
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
            call_text(program, db, code, idx, insn, args, locals),
        ));
    }
    if insn.flow == Flow::CallIndirect {
        return Some(Line::at(insn.offset, Kind::Call, format!("ax = (*{a})();")));
    }

    let text = match insn.mnemonic {
        Mnemonic::Mov | Mnemonic::Lea => format!("{a} = {b};"),
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
        Mnemonic::Nop => return None,
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
fn call_text(
    program: &Program,
    db: &ApiDb,
    code: &SegmentCode,
    idx: usize,
    insn: &Insn,
    args: &HashMap<i32, String>,
    locals: &[(i32, String)],
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

    // No signature, so the name is all we have.
    let target = insn
        .near_target
        .map(|t| Addr {
            segment: insn_segment(program, insn),
            offset: t,
        })
        .and_then(|a| program.function_at(a))
        .map(|f| f.label());
    match target {
        Some(name) => format!("ax = {name}();"),
        None => match insn.fixup.as_ref() {
            Some(fx) => format!("ax = {}();", fx.target),
            None => format!("ax = {}();", squash(&insn.text)),
        },
    }
}

/// Which segment an instruction came from.
///
/// The instruction does not carry it, so it is recovered from the segment
/// whose code contains this exact instruction.
fn insn_segment(program: &Program, insn: &Insn) -> u16 {
    program
        .code
        .iter()
        .find(|(_, c)| {
            c.insns
                .binary_search_by_key(&insn.offset, |i| i.offset)
                .is_ok()
        })
        .map(|(s, _)| *s)
        .unwrap_or(0)
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

    #[test]
    fn a_branch_with_nothing_above_it_says_so() {
        assert!(condition("je", None, &args(), &[]).contains("no comparison"));
    }
}
