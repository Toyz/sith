//! Printable-string extraction.

use crate::style::*;
use anyhow::Result;
use ne_core::{strings, NeFile};
use serde_json::json;

pub fn run(ne: &NeFile, segment: Option<u16>, min: usize, as_json: bool) -> Result<()> {
    let segs: Vec<_> = ne
        .segments
        .iter()
        .filter(|s| segment.is_none_or(|n| s.index == n))
        .filter(|s| !s.data.is_empty())
        .collect();

    if as_json {
        let v: Vec<_> = segs
            .iter()
            .map(|s| {
                json!({
                    "segment": s.index,
                    "strings": strings::scan(&s.data, min).iter().map(|f| json!({
                        "offset": f.offset,
                        "text": f.text,
                        "nul_terminated": f.nul_terminated,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    for s in segs {
        let found = strings::scan(&s.data, min);
        println!(
            "{}",
            heading(&format!(
                "segment {} ({}) — {} strings",
                s.index,
                s.kind().as_str(),
                found.len()
            ))
        );
        for f in found {
            // A NUL terminator is good evidence of a real string rather than
            // a printable stretch of code, so it is worth marking.
            let mark = if f.nul_terminated { green("\u{2022}") } else { dim(" ") };
            println!("  {:04X} {} {}", f.offset, mark, f.text);
        }
        println!();
    }
    Ok(())
}
