//! Printable-string extraction.

/// One run of printable bytes found in a buffer.
#[derive(Debug, Clone)]
pub struct FoundString {
    pub offset: u32,
    pub text: String,
    /// The run was followed by a NUL, so it is probably a real C string
    /// rather than an accidental printable stretch of code or data.
    pub nul_terminated: bool,
}

/// Scan for runs of at least `min_len` printable bytes.
///
/// Printable here is 0x20..0x7E plus tab; NE data segments are dense with
/// ASCII, and admitting the high half of Latin-1 would swamp the result with
/// runs of code bytes.
pub fn scan(data: &[u8], min_len: usize) -> Vec<FoundString> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut cur = Vec::new();
    let printable = |b: u8| (0x20..0x7F).contains(&b) || b == b'\t';

    for (i, &b) in data.iter().enumerate() {
        if printable(b) {
            if cur.is_empty() {
                start = i;
            }
            cur.push(b);
            continue;
        }
        if cur.len() >= min_len {
            out.push(FoundString {
                offset: start as u32,
                text: String::from_utf8_lossy(&cur).into_owned(),
                nul_terminated: b == 0,
            });
        }
        cur.clear();
    }
    if cur.len() >= min_len {
        out.push(FoundString {
            offset: start as u32,
            text: String::from_utf8_lossy(&cur).into_owned(),
            nul_terminated: false,
        });
    }
    out
}
