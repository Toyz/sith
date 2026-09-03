//! Disassembly of NE segments with relocations resolved.
//!
//! Decoding is done with `iced-x86` rather than by shelling out, so every
//! instruction comes back as structured data: the analysis passes read
//! mnemonics and branch targets directly instead of pattern-matching
//! disassembler text.
//!
//! The value this adds over a raw disassembler is fixup annotation. NE stores
//! relocations as chains threaded through the code, so the operand bytes of
//! an unfixed far call hold a link to the next fixup site rather than the
//! callee address. Raw output for such a call is confident and wrong; here
//! each instruction carries the [`Fixup`] covering its operand bytes.

use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, Mnemonic, OpKind};
use ne_core::{Fixup, NeFile, Segment};
use std::collections::BTreeMap;

/// A byte range standing in for a segment during decoding.
struct RawSeg<'a> {
    index: u16,
    data: &'a [u8],
}

/// Assembly syntax for rendered instruction text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Syntax {
    /// Matches `ndisasm` output, the usual reference for this era of code.
    #[default]
    Nasm,
    Intel,
    Masm,
}

impl Syntax {
    pub fn parse(s: &str) -> Option<Syntax> {
        match s.to_ascii_lowercase().as_str() {
            "nasm" => Some(Syntax::Nasm),
            "intel" => Some(Syntax::Intel),
            "masm" => Some(Syntax::Masm),
            _ => None,
        }
    }

    fn formatter(self) -> Box<dyn Formatter> {
        match self {
            Syntax::Nasm => Box::new(iced_x86::NasmFormatter::new()),
            Syntax::Intel => Box::new(iced_x86::IntelFormatter::new()),
            Syntax::Masm => Box::new(iced_x86::MasmFormatter::new()),
        }
    }
}

/// Control flow leaving an instruction, in the terms the analysis passes care
/// about. `iced-x86` does not distinguish near from far calls in its own
/// `FlowControl`, but the difference decides whether a call stays inside the
/// segment or crosses a fixup, so it is split out here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Next,
    Call,
    CallFar,
    CallIndirect,
    Jump,
    JumpFar,
    JumpIndirect,
    CondJump,
    Return,
    Interrupt,
    Invalid,
}

impl Flow {
    pub fn is_call(self) -> bool {
        matches!(self, Flow::Call | Flow::CallFar | Flow::CallIndirect)
    }

    /// Execution cannot continue into the following instruction.
    pub fn terminates_block(self) -> bool {
        matches!(
            self,
            Flow::Return | Flow::Jump | Flow::JumpFar | Flow::JumpIndirect | Flow::Invalid
        )
    }
}

#[derive(Debug, Clone)]
pub struct Insn {
    /// Offset within the segment.
    pub offset: u32,
    pub len: u8,
    pub bytes: Vec<u8>,
    pub text: String,
    pub mnemonic: Mnemonic,
    pub flow: Flow,
    /// Destination of a near call or jump, as a segment offset.
    pub near_target: Option<u32>,
    /// Fixup covering any byte of this instruction, where one applies.
    pub fixup: Option<Fixup>,
    /// The instruction's immediate operand, when it has exactly one.
    ///
    /// Kept apart from `operand_values` because a memory displacement is not
    /// a pushed constant: reading `push word [bp+6]` as the literal `6` turns
    /// a stack variable into a plausible, wrong argument value.
    pub immediate: Option<u32>,
    /// Immediate and displacement values in the operands.
    ///
    /// In 16-bit code a pointer into the data segment is usually a bare
    /// constant -- `mov ax, 1234h` or `push 1234h` -- with nothing in the
    /// encoding to say it is an address. Recording the constants lets the
    /// analysis match them against known data offsets, which is how a string
    /// gets cross-referenced back to the code that uses it.
    pub operand_values: Vec<u32>,
}

impl Insn {
    pub fn hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    /// The instruction as one text line: offset, bytes, mnemonic, and the
    /// fixup comment that makes an intersegment call readable.
    pub fn render(&self, byte_width: usize) -> String {
        let mut s = format!("{:04X}  {:<width$} {}", self.offset, self.hex(), self.text, width = byte_width);
        if let Some(f) = &self.fixup {
            s.push_str(&format!("  ; {}", f.target));
            if f.additive {
                s.push_str(" [additive]");
            }
        }
        s
    }
}

#[derive(Debug, Clone)]
pub struct SegmentCode {
    pub segment: u16,
    pub bits: u32,
    pub insns: Vec<Insn>,
}

impl SegmentCode {
    /// Index of the instruction starting exactly at `offset`.
    pub fn index_of(&self, offset: u32) -> Option<usize> {
        self.insns.binary_search_by_key(&offset, |i| i.offset).ok()
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub syntax: Syntax,
    /// 16 for ordinary Win16 code; 32 for segments that promote themselves
    /// via DPMI and run 32-bit instructions inside a 16-bit selector.
    pub bits: u32,
    /// Byte range within the segment, defaulting to the whole thing.
    pub start: u32,
    pub end: Option<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            syntax: Syntax::default(),
            bits: 16,
            start: 0,
            end: None,
        }
    }
}

/// Disassemble a segment, annotating each instruction with its fixup.
pub fn disassemble(ne: &NeFile, seg: &Segment, opts: &Options) -> SegmentCode {
    let fixups = ne.fixup_map(seg);
    decode(seg.index, &seg.data, opts, &fixups)
}

/// Decode a byte range with no fixup information.
///
/// Useful for scratch buffers and for tests; real segments should go through
/// [`disassemble`], which resolves the relocation chains.
pub fn disassemble_raw(segment: u16, data: &[u8], opts: &Options) -> SegmentCode {
    decode(segment, data, opts, &BTreeMap::new())
}

fn decode(
    segment: u16,
    data: &[u8],
    opts: &Options,
    fixups: &BTreeMap<u16, Fixup>,
) -> SegmentCode {
    let seg = RawSeg { index: segment, data };
    let end = opts.end.unwrap_or(seg.data.len() as u32).min(seg.data.len() as u32);
    let start = opts.start.min(end);
    let data = &seg.data[start as usize..end as usize];

    let mut decoder = Decoder::with_ip(opts.bits, data, start as u64, DecoderOptions::NONE);
    let mut formatter = opts.syntax.formatter();
    formatter.options_mut().set_uppercase_hex(true);
    formatter.options_mut().set_space_after_operand_separator(false);

    let mut insns = Vec::new();
    let mut instr = Instruction::default();
    let mut text = String::new();
    while decoder.can_decode() {
        let pos = decoder.ip() as u32;
        decoder.decode_out(&mut instr);
        let len = instr.len().max(1) as u8;
        text.clear();
        formatter.format(&instr, &mut text);

        let bytes = data
            .get((pos - start) as usize..(pos - start) as usize + len as usize)
            .unwrap_or(&[])
            .to_vec();

        // The fixup may cover any operand byte, so scan the whole instruction
        // rather than assuming a fixed operand position.
        let fixup = (pos..pos + len as u32)
            .find_map(|o| u16::try_from(o).ok().and_then(|o| fixups.get(&o)))
            .cloned();

        let flow = classify(&instr);
        let operand_values = operand_values(&instr);
        let immediate = immediate(&instr);
        let near_target = match flow {
            Flow::Call | Flow::Jump | Flow::CondJump => Some(instr.near_branch_target() as u32),
            _ => None,
        };

        insns.push(Insn {
            offset: pos,
            len,
            bytes,
            text: text.clone(),
            mnemonic: instr.mnemonic(),
            flow,
            near_target,
            fixup,
            immediate,
            operand_values,
        });
    }
    SegmentCode {
        segment: seg.index,
        bits: opts.bits,
        insns,
    }
}

/// The single immediate operand, if the instruction has exactly one.
fn immediate(instr: &Instruction) -> Option<u32> {
    let mut found = None;
    for i in 0..instr.op_count() {
        let v = match instr.op_kind(i) {
            OpKind::Immediate8 => instr.immediate8() as u32,
            OpKind::Immediate16 => instr.immediate16() as u32,
            OpKind::Immediate32 => instr.immediate32(),
            OpKind::Immediate8to16 => instr.immediate8to16() as u16 as u32,
            OpKind::Immediate8to32 => instr.immediate8to32() as u32,
            _ => continue,
        };
        if found.is_some() {
            return None;
        }
        found = Some(v);
    }
    found
}

/// Immediates and non-zero memory displacements, as candidate addresses.
fn operand_values(instr: &Instruction) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 0..instr.op_count() {
        match instr.op_kind(i) {
            OpKind::Immediate8 => out.push(instr.immediate8() as u32),
            OpKind::Immediate16 => out.push(instr.immediate16() as u32),
            OpKind::Immediate32 => out.push(instr.immediate32()),
            OpKind::Immediate8to16 => out.push(instr.immediate8to16() as u16 as u32),
            OpKind::Immediate8to32 => out.push(instr.immediate8to32() as u32),
            OpKind::Memory => {
                let disp = instr.memory_displacement64();
                if disp != 0 && disp <= u32::MAX as u64 {
                    out.push(disp as u32);
                }
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn classify(instr: &Instruction) -> Flow {
    use iced_x86::FlowControl as FC;
    if instr.is_invalid() {
        return Flow::Invalid;
    }
    let far = matches!(
        instr.op0_kind(),
        OpKind::FarBranch16 | OpKind::FarBranch32
    );
    match instr.flow_control() {
        FC::Next => Flow::Next,
        FC::Call => {
            if far {
                Flow::CallFar
            } else {
                Flow::Call
            }
        }
        FC::IndirectCall => Flow::CallIndirect,
        FC::UnconditionalBranch => {
            if far {
                Flow::JumpFar
            } else {
                Flow::Jump
            }
        }
        FC::IndirectBranch => Flow::JumpIndirect,
        FC::ConditionalBranch => Flow::CondJump,
        FC::Return => Flow::Return,
        FC::Interrupt => Flow::Interrupt,
        _ => Flow::Next,
    }
}

/// Widest byte column needed to print a run of instructions without ragged
/// mnemonics.
pub fn byte_column_width(insns: &[Insn]) -> usize {
    insns.iter().map(|i| i.len as usize * 2).max().unwrap_or(0).min(20)
}
