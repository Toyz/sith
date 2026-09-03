//! Relocation reports.

use crate::style::*;
use anyhow::Result;
use ne_core::NeFile;
use serde_json::json;
use std::collections::BTreeMap;

pub fn run(ne: &NeFile, segment: Option<u16>, per_site: bool, as_json: bool) -> Result<()> {
    let segs: Vec<_> = ne
        .segments
        .iter()
        .filter(|s| segment.is_none_or(|n| s.index == n))
        .filter(|s| !s.relocs.is_empty())
        .collect();

    if as_json {
        let v: Vec<_> = segs
            .iter()
            .map(|s| {
                json!({
                    "segment": s.index,
                    "fixups": ne.fixups(s).iter().map(|f| json!({
                        "site": f.site,
                        "addr_type": f.addr_type.to_string(),
                        "additive": f.additive,
                        "target": f.target.to_string(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    for s in segs {
        let fixups = ne.fixups(s);
        println!(
            "{}",
            heading(&format!(
                "segment {} ({}) — {} records, {} patch sites",
                s.index,
                s.kind().as_str(),
                s.relocs.len(),
                fixups.len()
            ))
        );
        if per_site {
            for f in &fixups {
                println!(
                    "  {:04X}  {:<8} {}{}",
                    f.site,
                    dim(f.addr_type.as_str()),
                    f.target,
                    if f.additive { yellow(" [additive]") } else { String::new() }
                );
            }
        } else {
            // Collapse to one line per target: what a segment talks to is
            // usually more interesting than where each patch lands.
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for f in &fixups {
                *counts.entry(f.target.to_string()).or_insert(0) += 1;
            }
            let mut rows: Vec<_> = counts.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for (target, n) in rows {
                println!("  {:>5}  {}", dim(&n.to_string()), target);
            }
        }
        println!();
    }
    Ok(())
}
