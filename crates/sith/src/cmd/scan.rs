//! Directory scan: what NE binaries are here and what are they.

use crate::style::*;
use anyhow::Result;
use serde_json::json;
use std::path::Path;

pub fn run(dir: &Path, as_json: bool) -> Result<()> {
    let found = ne_core::scan_dir(dir);

    if as_json {
        let v: Vec<_> = found
            .iter()
            .map(|m| {
                json!({
                    "path": m.path.display().to_string(),
                    "module": m.module,
                    "description": m.description,
                    "library": m.is_library,
                    "size": m.file_size,
                    "segments": m.segments,
                    "exports": m.exports,
                    "resources": m.resources,
                    "imports": m.imports,
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
    for m in &found {
        println!(
            "  {:<10}  {:<4}  {:>4}  {:>3}  {:>3}  {}",
            cyan(&m.module),
            if m.is_library { "DLL" } else { "EXE" },
            m.segments,
            m.exports,
            m.resources,
            dim(&m.path.display().to_string())
        );
    }
    println!();
    println!("{} NE binaries", found.len());
    Ok(())
}
