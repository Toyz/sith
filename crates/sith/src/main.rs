//! `sith` -- a command-line toolkit for 16-bit Windows NE binaries.

mod cmd;
mod style;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ne_core::{ExportIndex, NeFile, OrdinalDb};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sith",
    version,
    about = "Reverse engineering toolkit for 16-bit Windows NE executables",
    long_about = "Reads Windows 3.x NE (New Executable) binaries: headers, segments, \
relocation chains, imports and exports, resources, and fixup-aware disassembly."
)]
struct Cli {
    /// Colorise output.
    #[arg(long, global = true, default_value = "auto", value_parser = ["auto", "always", "never"])]
    color: String,

    /// Emit JSON instead of text, where the command supports it.
    #[arg(long, global = true)]
    json: bool,

    /// Directory of sibling NE files, scanned so ordinal imports of the
    /// project's own DLLs resolve to real export names.
    #[arg(long, global = true, value_name = "DIR")]
    index: Option<PathBuf>,

    /// Extra ordinal names as a {"MODULE.ordinal": "Name"} JSON map; these
    /// override the built-in Win16 table.
    #[arg(long, global = true, value_name = "FILE")]
    ordinals: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Header, segments, imports, exports and resource summary.
    Info { file: PathBuf },

    /// Segment table.
    Segments { file: PathBuf },

    /// Referenced modules and the symbols imported from each.
    Imports { file: PathBuf },

    /// Exported entry points.
    Exports { file: PathBuf },

    /// The full entry table, exported or not.
    Entries { file: PathBuf },

    /// Relocation fixups, with chains expanded to individual patch sites.
    Relocs {
        file: PathBuf,
        /// Restrict to one segment (1-based).
        #[arg(short, long)]
        segment: Option<u16>,
        /// One line per patch site instead of one per target.
        #[arg(long)]
        sites: bool,
    },

    /// Disassemble a segment with fixups resolved.
    Dis {
        file: PathBuf,
        /// Segment number (1-based).
        #[arg(short, long)]
        segment: u16,
        /// Start offset within the segment.
        #[arg(long, value_parser = parse_num)]
        start: Option<u32>,
        /// End offset (exclusive).
        #[arg(long, value_parser = parse_num)]
        end: Option<u32>,
        /// Disassemble the single function starting at this offset.
        #[arg(short, long, value_parser = parse_num)]
        func: Option<u32>,
        /// Decode as 32-bit code, for segments promoted through DPMI.
        #[arg(long = "bits32")]
        bits32: bool,
        /// Assembly syntax.
        #[arg(long, default_value = "nasm", value_parser = ["nasm", "intel", "masm"])]
        syntax: String,
    },

    /// Render a function as C-shaped pseudocode.
    Pseudo {
        file: PathBuf,
        /// Segment number (1-based).
        #[arg(short, long)]
        segment: u16,
        /// Offset of the function within the segment.
        #[arg(short, long, value_parser = parse_num)]
        func: Option<u32>,
        /// Segments holding 32-bit code, comma separated.
        #[arg(long, value_name = "LIST")]
        bits32: Option<String>,
    },

    /// Discovered functions, with the evidence for each start.
    Funcs {
        file: PathBuf,
        #[arg(short, long)]
        segment: Option<u16>,
        /// Segments holding 32-bit code, comma separated.
        #[arg(long, value_name = "LIST")]
        bits32: Option<String>,
    },

    /// Call graph: every function and what it calls.
    Callgraph {
        file: PathBuf,
        #[arg(short, long)]
        segment: Option<u16>,
        #[arg(long, value_name = "LIST")]
        bits32: Option<String>,
    },

    /// Call sites of a symbol, matched as a substring.
    Xref {
        file: PathBuf,
        /// Symbol to look for, e.g. `GlobalAlloc` or `MYDLL.`.
        name: String,
        #[arg(long, value_name = "LIST")]
        bits32: Option<String>,
    },

    /// Printable strings in segments.
    Strings {
        file: PathBuf,
        #[arg(short, long)]
        segment: Option<u16>,
        /// Shortest run to report.
        #[arg(long, default_value_t = 4)]
        min: usize,
    },

    /// Hex dump a segment, a resource, or a file range.
    Hex {
        file: PathBuf,
        #[arg(short, long)]
        segment: Option<u16>,
        /// Absolute file offset, when no segment is given.
        #[arg(short, long, value_parser = parse_num)]
        offset: Option<u32>,
        #[arg(short, long, value_parser = parse_num, default_value = "256")]
        len: u32,
    },

    /// Resources: list, decode and extract.
    #[command(subcommand)]
    Res(cmd::res::ResCmd),

    /// Write every segment and resource to a directory.
    Extract {
        file: PathBuf,
        outdir: PathBuf,
        /// Keep resources as raw bytes rather than rebuilding .bmp/.ico files.
        #[arg(long)]
        raw: bool,
    },

    /// Summarise every NE file under a directory.
    Scan { dir: PathBuf },

    /// Query the built-in Win16 ordinal database.
    Ordinals {
        /// Module name, e.g. KERNEL. Omit to list known modules.
        module: Option<String>,
        /// Ordinal to resolve. Omit to list the module's ordinals.
        ordinal: Option<u16>,
    },
}

fn parse_num(s: &str) -> Result<u32, String> {
    let s = s.trim();
    let parsed = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(h, 16)
    } else if let Some(h) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
        u32::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    parsed.map_err(|e| format!("{s:?}: {e}"))
}

fn parse_seg_list(s: &Option<String>) -> std::collections::BTreeSet<u16> {
    s.iter()
        .flat_map(|v| v.split(','))
        .filter_map(|p| p.trim().parse().ok())
        .collect()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    style::init(style::ColorChoice::parse(&cli.color).unwrap_or(style::ColorChoice::Auto));

    // Loading a binary is shared by nearly every command; `scan` and
    // `ordinals` are the two that do not take one.
    let load = |path: &PathBuf| -> Result<NeFile> {
        let mut ne = NeFile::open(path)
            .with_context(|| format!("reading {}", path.display()))?;
        if let Some(p) = &cli.ordinals {
            let db = OrdinalDb::from_json_file(p)
                .with_context(|| format!("reading ordinal table {}", p.display()))?;
            ne.merge_ordinals(&db);
        }
        let index_root = cli.index.clone().or_else(|| {
            // Default to the binary's own directory so sibling DLLs resolve
            // without the caller having to say so.
            path.parent().map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
        });
        if let Some(root) = index_root {
            let mut ix = ExportIndex::new();
            if ix.scan(&root).is_ok() && !ix.is_empty() {
                ne.set_export_index(ix);
            }
        }
        Ok(ne)
    };

    match &cli.command {
        Command::Info { file } => cmd::info::run(&load(file)?, cli.json),
        Command::Segments { file } => cmd::info::segments(&load(file)?, cli.json),
        Command::Imports { file } => cmd::info::imports(&load(file)?, cli.json),
        Command::Exports { file } => cmd::info::exports(&load(file)?, cli.json),
        Command::Entries { file } => cmd::info::entries(&load(file)?, cli.json),
        Command::Relocs {
            file,
            segment,
            sites,
        } => cmd::relocs::run(&load(file)?, *segment, *sites, cli.json),
        Command::Dis {
            file,
            segment,
            start,
            end,
            func,
            bits32,
            syntax,
        } => cmd::dis::run(
            &load(file)?,
            *segment,
            *start,
            *end,
            *func,
            *bits32,
            syntax,
        ),
        Command::Pseudo {
            file,
            segment,
            func,
            bits32,
        } => cmd::analyze::pseudo(&load(file)?, *segment, *func, &parse_seg_list(bits32)),
        Command::Funcs {
            file,
            segment,
            bits32,
        } => cmd::analyze::funcs(&load(file)?, *segment, &parse_seg_list(bits32), cli.json),
        Command::Callgraph {
            file,
            segment,
            bits32,
        } => cmd::analyze::callgraph(&load(file)?, *segment, &parse_seg_list(bits32)),
        Command::Xref { file, name, bits32 } => {
            cmd::analyze::xref(&load(file)?, name, &parse_seg_list(bits32))
        }
        Command::Strings { file, segment, min } => {
            cmd::strings::run(&load(file)?, *segment, *min, cli.json)
        }
        Command::Hex {
            file,
            segment,
            offset,
            len,
        } => cmd::hex::run(&load(file)?, *segment, *offset, *len),
        Command::Res(sub) => cmd::res::run(&load(sub.file())?, sub, cli.json),
        Command::Extract { file, outdir, raw } => {
            cmd::extract::run(&load(file)?, outdir, *raw)
        }
        Command::Scan { dir } => cmd::scan::run(dir, cli.json),
        Command::Ordinals { module, ordinal } => cmd::ordinals::run(module.as_deref(), *ordinal),
    }
}
