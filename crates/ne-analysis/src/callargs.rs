//! Call-site argument reconstruction.
//!
//! A Win16 call is a run of pushes followed by a far call. Walking back over
//! the pushes and matching them against the callee's signature turns
//!
//! ```text
//! push 0x42
//! push 0
//! call far KERNEL.GlobalAlloc
//! ```
//!
//! into `GlobalAlloc(GMEM_MOVEABLE|GMEM_ZEROINIT, 0)`, which is the difference
//! between reading a disassembly and understanding it.
//!
//! This is a local, syntactic reconstruction, not data flow: a value computed
//! into a register before the push is reported as unknown rather than guessed
//! at. The renderer marks those, so nothing here is mistaken for certainty.

use ne_core::api::{ApiDb, ArgKind, CallConv, Signature};
use ne_core::Target;
use ne_disasm::{Insn, SegmentCode};

/// One reconstructed argument.
#[derive(Debug, Clone)]
pub struct Arg {
    pub kind: ArgKind,
    /// The constant pushed, where the push carried an immediate.
    pub value: Option<u32>,
    /// A named constant for `value`, when the parameter has a known set.
    pub name: Option<String>,
    /// Raw text of the pushes that supplied it, for the unknown case.
    pub text: String,
}

impl Arg {
    pub fn render(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        match self.value {
            Some(v) if v < 10 => v.to_string(),
            Some(v) => format!("{v:#X}"),
            None => self.text.clone(),
        }
    }
}

/// A call site with its callee resolved and its arguments recovered.
#[derive(Debug, Clone)]
pub struct CallArgs {
    pub module: String,
    pub function: String,
    pub signature: Signature,
    pub args: Vec<Arg>,
    /// Every argument was a literal push, so the rendering is complete.
    pub complete: bool,
}

impl CallArgs {
    pub fn render(&self) -> String {
        format!(
            "{}({})",
            self.function,
            self.args
                .iter()
                .map(Arg::render)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// How far back to look for the pushes feeding a call. Win16 calls take at
/// most a dozen arguments; beyond that the window would start absorbing
/// unrelated code.
const LOOKBACK: usize = 24;

/// Reconstruct the arguments of the call at `index`, if it targets a known API.
pub fn reconstruct(code: &SegmentCode, index: usize, db: &ApiDb) -> Option<CallArgs> {
    let insn = code.insns.get(index)?;
    let fixup = insn.fixup.as_ref()?;
    let (module, signature) = match &fixup.target {
        Target::ImportOrdinal {
            module, ordinal, ..
        } => (module.clone(), db.signature(module, *ordinal)?.clone()),
        Target::ImportName { module, name } => {
            (module.clone(), db.signature_by_name(module, name)?.clone())
        }
        _ => return None,
    };
    if signature.conv == CallConv::Other || signature.args.is_empty() {
        return None;
    }

    // Collect the pushed words immediately before the call, nearest first.
    let mut pushed: Vec<PushedWord> = Vec::new();
    let start = index.saturating_sub(LOOKBACK);
    for prev in code.insns[start..index].iter().rev() {
        match classify_push(prev) {
            Some(p) => pushed.push(p),
            // Anything that is not a push ends the run: a call's arguments are
            // contiguous, and continuing past other work would pick up pushes
            // belonging to an earlier statement.
            None => break,
        }
        if pushed.len() >= signature.stack_words() {
            break;
        }
    }
    if pushed.len() < signature.stack_words() {
        // Some arguments came from somewhere this pass cannot see; the ones
        // that are present are still worth showing.
        while pushed.len() < signature.stack_words() {
            pushed.push(PushedWord::unknown());
        }
    }

    // `pushed` runs backwards from the call. Pascal pushes left to right, so
    // the last push is the last parameter; cdecl pushes right to left, so the
    // last push is the first parameter.
    let mut words: Vec<PushedWord> = pushed.into_iter().take(signature.stack_words()).collect();
    if signature.conv == CallConv::Pascal {
        // Reversing gives push order, which for pascal is parameter order.
        words.reverse();
    }

    let mut args = Vec::with_capacity(signature.args.len());
    let mut w = 0usize;
    let mut complete = true;
    for (i, kind) in signature.args.iter().enumerate() {
        let n = kind.words();
        let slice = words.get(w..w + n).unwrap_or(&[]);
        w += n;
        let value = combine(slice, kind);
        if value.is_none() {
            complete = false;
        }
        let name = value.and_then(|v| {
            db.param_set(&module, &signature.name, i)
                .and_then(|set| set.decode(v))
        });
        args.push(Arg {
            kind: *kind,
            value,
            name,
            text: slice
                .iter()
                .map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join(":"),
        });
    }

    Some(CallArgs {
        module,
        function: signature.name.clone(),
        signature,
        args,
        complete,
    })
}

#[derive(Debug, Clone)]
struct PushedWord {
    value: Option<u32>,
    text: String,
}

impl PushedWord {
    fn unknown() -> PushedWord {
        PushedWord {
            value: None,
            text: "?".into(),
        }
    }
}

/// Recognise a push and, where it carries a literal, its value.
///
/// A `push` of a 32-bit immediate or of a far pointer covers two stack words;
/// splitting it here keeps the word accounting honest.
fn classify_push(insn: &Insn) -> Option<PushedWord> {
    use iced_mnemonic::*;
    if !is_push(insn) {
        return None;
    }
    let operand = insn.text.split_once(' ').map(|(_, o)| o.trim()).unwrap_or("");
    // A push whose operand is a fixup site is an address, not a number.
    if insn.fixup.is_some() {
        return Some(PushedWord {
            value: None,
            text: insn
                .fixup
                .as_ref()
                .map(|f| f.target.to_string())
                .unwrap_or_else(|| operand.to_string()),
        });
    }
    // Only a real immediate counts: a memory operand's displacement is part
    // of an address, not the value being pushed.
    let value = insn.immediate;
    Some(PushedWord {
        value,
        text: operand.to_string(),
    })
}

mod iced_mnemonic {
    use ne_disasm::Insn;

    /// `push`, in any of its forms, including `push imm` and `push seg`.
    pub fn is_push(insn: &Insn) -> bool {
        insn.mnemonic == iced_x86::Mnemonic::Push
    }
}

/// Join the words of one argument into a value, low word first.
fn combine(words: &[PushedWord], kind: &ArgKind) -> Option<u32> {
    match kind.words() {
        1 => words.first()?.value,
        _ => {
            // Two words: the high word is pushed first, so it comes first in
            // parameter order.
            let hi = words.first()?.value?;
            let lo = words.get(1)?.value?;
            Some((hi << 16) | (lo & 0xFFFF))
        }
    }
}
