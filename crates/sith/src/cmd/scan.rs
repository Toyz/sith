//! Directory scan: what NE binaries are here and what are they.

use crate::style::*;
use anyhow::Result;
use ne_core::NeFile;
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn run(dir: &Path, as_json: bool) -> Result<()> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(ne) = NeFile::open(&p) {
                found.push((p, ne));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));

    if as_json {
        let v: Vec<_> = found
            .iter()
            .map(|(p, ne)| {
                json!({
                    "path": p.display().to_string(),
                    "module": ne.module_name(),
                    "description": ne.description(),
                    "library": ne.header.is_library(),
                    "segments": ne.segments.len(),
                    "exports": ne.exports().len(),
                    "resources": ne.resources.len(),
                    "imports": ne.module_ref_names(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    if found.is_empty() {
        println!("{}", dim(&format!("no NE binaries under {}", dir.display())));
        return Ok(());
    }
    println!(
        "{}",
        dim("  module      kind  segs  exp  res  path")
    );
    for (p, ne) in &found {
        println!(
            "  {:<10}  {:<4}  {:>4}  {:>3}  {:>3}  {}",
            cyan(ne.module_name()),
            if ne.header.is_library() { "DLL" } else { "EXE" },
            ne.segments.len(),
            ne.exports().len(),
            ne.resources.len(),
            dim(&p.display().to_string())
        );
    }
    println!();
    println!("{} NE binaries", found.len());
    Ok(())
}
