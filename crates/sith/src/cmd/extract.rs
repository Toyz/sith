//! Bulk extraction of segments and resources.

use anyhow::Result;
use ne_core::NeFile;
use std::path::Path;

pub fn run(ne: &NeFile, outdir: &Path, raw: bool) -> Result<()> {
    let segdir = outdir.join("segments");
    std::fs::create_dir_all(&segdir)?;
    let mut segs = 0;
    for s in &ne.segments {
        if s.data.is_empty() {
            continue;
        }
        let name = format!(
            "seg{:02}_{}.bin",
            s.index,
            s.kind().as_str().to_ascii_lowercase()
        );
        std::fs::write(segdir.join(name), &s.data)?;
        segs += 1;
    }
    println!("wrote {segs} segments to {}", segdir.display());

    if !ne.resources.is_empty() {
        let resdir = outdir.join("resources");
        crate::cmd::res::extract(ne, &resdir, raw)?;
    }
    Ok(())
}
