//! Resource listing, decoding and extraction.

use crate::cmd::hex::dump;
use crate::style::*;
use anyhow::{bail, Result};
use clap::Subcommand;
use ne_core::{render, NeFile, ResId, Resource};
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ResCmd {
    /// List the resource directory.
    List { file: PathBuf },

    /// Decode one resource: menus, dialogs, string tables, accelerators and
    /// version info render as text; anything else hex dumps.
    Show {
        file: PathBuf,
        /// Resource type, by name (`MENU`) or number.
        #[arg(short, long)]
        r#type: String,
        /// Resource id, by name or number. Omit to show every resource of
        /// the type.
        #[arg(short, long)]
        id: Option<String>,
    },

    /// Write every resource to a directory as a usable file.
    Extract {
        file: PathBuf,
        outdir: PathBuf,
        /// Write raw resource bodies instead of rebuilt .bmp/.ico/.cur files.
        #[arg(long)]
        raw: bool,
    },
}

impl ResCmd {
    pub fn file(&self) -> &PathBuf {
        match self {
            ResCmd::List { file } | ResCmd::Show { file, .. } | ResCmd::Extract { file, .. } => file,
        }
    }
}

pub fn run(ne: &NeFile, cmd: &ResCmd, as_json: bool) -> Result<()> {
    match cmd {
        ResCmd::List { .. } => list(ne, as_json),
        ResCmd::Show { r#type, id, .. } => show(ne, r#type, id.as_deref()),
        ResCmd::Extract { outdir, raw, .. } => extract(ne, outdir, *raw),
    }
}

fn list(ne: &NeFile, as_json: bool) -> Result<()> {
    if as_json {
        let v: Vec<_> = ne
            .resources
            .iter()
            .map(|r| {
                json!({
                    "type": r.type_name(),
                    "id": r.res_id.to_string(),
                    "offset": r.offset,
                    "length": r.length,
                    "flags": r.flags,
                    "flag_names": r.flag_names(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    if ne.resources.is_empty() {
        println!("{}", dim("no resources"));
        return Ok(());
    }
    println!(
        "{}",
        dim("  type            id                  offset     size  flags")
    );
    for r in &ne.resources {
        println!(
            "  {:<14}  {:<18}  {:08X} {:>8}  {}",
            yellow(&r.type_name()),
            r.res_id.to_string(),
            r.offset,
            r.length,
            dim(&r.flag_names().join(" "))
        );
    }
    Ok(())
}

fn show(ne: &NeFile, type_arg: &str, id_arg: Option<&str>) -> Result<()> {
    let matches: Vec<&Resource> = ne
        .resources
        .iter()
        .filter(|r| type_matches(r, type_arg))
        .filter(|r| id_arg.is_none_or(|id| id_matches(r, id)))
        .collect();
    if matches.is_empty() {
        bail!(
            "no resource matches type {type_arg:?}{}",
            id_arg.map(|i| format!(" id {i:?}")).unwrap_or_default()
        );
    }
    for r in matches {
        println!(
            "{}",
            heading(&format!(
                "{} {}  ({} bytes at {:#x})",
                r.type_name(),
                r.res_id,
                r.length,
                r.offset
            ))
        );
        match render::resource_text(ne, r) {
            Some(text) => print!("{text}"),
            None => {
                let data = ne.resource_data(r);
                // Cap the dump: RCDATA blobs run to hundreds of kilobytes and
                // `sith hex` is the right tool for reading all of one.
                let shown = data.len().min(1024);
                print!("{}", dump(&data[..shown], 0));
                if shown < data.len() {
                    println!(
                        "{}",
                        dim(&format!(
                            "... {} more bytes (use `sith hex --offset {:#x} --len {}`)",
                            data.len() - shown,
                            r.offset,
                            r.length
                        ))
                    );
                }
            }
        }
        println!();
    }
    Ok(())
}

fn type_matches(r: &Resource, arg: &str) -> bool {
    if let Ok(n) = arg.parse::<u16>() {
        if r.type_id == ResId::Id(n) {
            return true;
        }
    }
    r.type_name().eq_ignore_ascii_case(arg)
}

fn id_matches(r: &Resource, arg: &str) -> bool {
    let arg = arg.trim_start_matches('#');
    match &r.res_id {
        ResId::Id(n) => arg.parse::<u16>().is_ok_and(|v| v == *n),
        ResId::Name(s) => s.eq_ignore_ascii_case(arg),
    }
}

pub fn extract(ne: &NeFile, outdir: &PathBuf, raw: bool) -> Result<()> {
    std::fs::create_dir_all(outdir)?;
    let mut written = 0usize;
    // Resource names are not unique -- a resource script can attach the same
    // name to several entries of one type (four RCDATA DLGINCLUDE blocks is
    // typical) -- so repeated labels get a numeric suffix instead of
    // overwriting each other.
    let mut used: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &ne.resources {
        let base = r.label();
        let seen = used.entry(base.clone()).or_insert(0);
        *seen += 1;
        let label = if *seen == 1 {
            base
        } else {
            format!("{base}_{}", *seen)
        };
        let (bytes, ext) = if raw {
            (ne.resource_data(r).to_vec(), "bin")
        } else {
            (ne.resource_file_bytes(r), r.extension())
        };
        let path = outdir.join(format!("{label}.{ext}"));
        std::fs::write(&path, &bytes)?;
        written += 1;

        // Text-shaped resources also get a decoded sidecar, which is what
        // anyone reading a menu or dialog actually wants.
        if !raw {
            if let Some(text) = render::resource_text(ne, r) {
                if !text.trim().is_empty() && !matches!(r.type_id.as_id(), Some(2 | 3 | 1 | 12 | 14))
                {
                    std::fs::write(outdir.join(format!("{label}.txt")), text)?;
                    written += 1;
                }
            }
        }
    }
    println!(
        "wrote {} files for {} resources to {}",
        written,
        ne.resources.len(),
        outdir.display()
    );
    Ok(())
}
