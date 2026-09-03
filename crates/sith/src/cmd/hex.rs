//! Hex dumps of segments and file ranges.

use crate::style::*;
use anyhow::{bail, Result};
use ne_core::NeFile;

pub fn run(ne: &NeFile, segment: Option<u16>, offset: Option<u32>, len: u32) -> Result<()> {
    let (data, base, what) = match segment {
        Some(n) => {
            let Some(seg) = ne.segment(n) else {
                bail!("segment {n} does not exist");
            };
            let start = offset.unwrap_or(0) as usize;
            let end = (start + len as usize).min(seg.data.len());
            if start >= seg.data.len() {
                bail!("offset {start:#x} is past the end of segment {n}");
            }
            (
                seg.data[start..end].to_vec(),
                start as u64,
                format!("segment {n}"),
            )
        }
        None => {
            let start = offset.unwrap_or(0) as usize;
            let end = (start + len as usize).min(ne.buf.len());
            if start >= ne.buf.len() {
                bail!("offset {start:#x} is past the end of the file");
            }
            (ne.buf[start..end].to_vec(), start as u64, "file".to_string())
        }
    };

    println!(
        "{}",
        heading(&format!("{what} {:#x}..{:#x}", base, base + data.len() as u64))
    );
    println!("{}", dump(&data, base));
    Ok(())
}

/// Classic 16-byte hex dump with an ASCII gutter.
pub fn dump(data: &[u8], base: u64) -> String {
    let mut out = String::new();
    for (row, chunk) in data.chunks(16).enumerate() {
        let addr = base + row as u64 * 16;
        out.push_str(&format!("{addr:08X}  "));
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => out.push_str(&format!("{b:02X} ")),
                None => out.push_str("   "),
            }
            if i == 7 {
                out.push(' ');
            }
        }
        out.push_str(" |");
        for &b in chunk {
            out.push(if (0x20..0x7F).contains(&b) { b as char } else { '.' });
        }
        out.push_str("|\n");
    }
    out
}
