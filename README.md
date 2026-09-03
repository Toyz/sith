# sith

A reverse-engineering toolkit for 16-bit Windows NE (New Executable) binaries —
the `.EXE`, `.DLL` and `.DRV` files that Windows 3.x ran. It is a Rust
workspace: a parsing library, a fixup-aware disassembler, an analysis pass, a
command-line tool, and a GUI browser.

It is not tied to any one project. Point it at any NE binary.

## Why

NE binaries have a trap in them. Relocations are stored as *chains threaded
through the code itself*: the operand bytes of an unfixed far call hold the
offset of the next fixup site, not the address of the callee. Disassemble the
raw bytes and you get output that is confident and wrong — every intersegment
call points somewhere plausible and false.

`sith` walks the chains, resolves each patch site, and names the target. On top
of that it knows the Win16 API: 3,231 ordinal names and 1,922 signatures
generated from Wine's 16-bit `.spec` files, plus a curated table of parameter
constants. So a call site reads

```
0707  9A5F070000   call 0:075Fh  ; USER.LoadCursor(0, IDC_ARROW)
0711  9AFFFF0000   call 0:0FFFFh  ; GDI.GetStockObject(BLACK_BRUSH)
0121  9A4F010000   call 0:014Fh  ; GDI.BitBlt(word [1734h], word [bp-0Ah], …, SRCCOPY)
```

instead of `call 0:0FFFFh`.

## Install

```sh
cargo build --release
# binaries land in target/release/{sith,sith-gui}
```

## Command line

```sh
sith info      FILE                  header, segments, imports, exports, resources
sith segments  FILE
sith imports   FILE                  imported symbols per module, with usage counts
sith exports   FILE
sith entries   FILE                  the entry table, exported or not
sith relocs    FILE [-s N] [--sites] fixups, collapsed by target or per patch site
sith dis       FILE -s N             disassemble, with fixups and API calls resolved
sith funcs     FILE [-s N]           discovered functions and the evidence for each
sith callgraph FILE [-s N]
sith xref      FILE NAME             call sites of a symbol
sith strings   FILE [-s N]
sith hex       FILE [-s N] [-o OFF]
sith res       list|show|extract FILE
sith extract   FILE OUTDIR           every segment and resource as a usable file
sith scan      DIR                   summarise every NE binary under a directory
sith ordinals  [MODULE] [ORDINAL]    query the built-in Win16 API table
```

Useful flags: `--json` on the informational commands, `--index DIR` to resolve
ordinal imports of sibling DLLs, `--ordinals FILE` to override names,
`--color auto|always|never`.

Disassembly options: `--start`/`--end` to bound the range, `-f OFFSET` for a
single function, `--bits32` for segments that promote themselves through DPMI,
`--syntax nasm|intel|masm`.

## GUI

```sh
sith-gui [FILE]
```

- Tabbed views over a workspace of files, with back/forward history.
- A navigator with segments, functions grouped by segment, resources with live
  thumbnails, and every sibling module found beside the file.
- A disassembly listing with a branch-arrow gutter, function banners, resolved
  fixups and reconstructed API calls.
- An inspector showing the selected instruction's bytes, its fixup, the API
  signature and argument values, and everything that references it.
- A call-graph explorer: pan and zoom, callers and callees, click to re-centre.
- A strings browser that cross-references each string to the code that loads it.
- Resource preview for bitmaps, icons and cursors, with export to real `.bmp`,
  `.ico` and `.cur` files; menus, dialogs, string tables, accelerators and
  version info decode to text.

Keys: `Ctrl+O` open, `Ctrl+G` go to address or symbol, `Ctrl+P` find symbol,
`Ctrl+W` close tab, `Alt+←`/`Alt+→` history, `↑`/`↓` move, `Enter` follow.

## Crates

| crate | what it is |
| --- | --- |
| `ne-core` | NE container parsing: header, segments, relocation chains, entry table, names, resources, DIB decoding, the Win16 ordinal and API databases |
| `ne-disasm` | `iced-x86` decoding with each instruction annotated by the fixup covering its operand bytes |
| `ne-analysis` | function discovery, call graph, cross-references, data references, call-site argument reconstruction |
| `sith` | the command-line tool |
| `sith-gui` | the `egui` browser |

## What it knows about the format

Header and program flags, self-loading modules, the segment table with its
alignment shift, all six relocation address types and all four target types,
additive fixups, movable and fixed entry-table bundles, resident and
non-resident name tables, and the resource tree.

Resources decode rather than merely dump: `RT_BITMAP` to `.bmp`, `RT_ICON` and
`RT_CURSOR` (including group directories) to `.ico` and `.cur`, DIBs at 1, 4, 8,
16, 24 and 32 bpp including `BI_RLE4` and `BI_RLE8`, string tables, menus,
dialog templates, accelerator tables and `VS_VERSIONINFO` in its 16-bit ANSI
form.

## Where the API data comes from

`tools/fetch_ordinals.py` regenerates `crates/ne-core/data/win16_ordinals.json`
and `win16_api.json` from Wine's 16-bit `.spec` files, which are authoritative.
Hand-written tables in circulation get this wrong — one common table has GDI.27
as `BitBlt` when it is `Rectangle`, and GDI.34, the real `BitBlt`, as
`CreateBrushIndirect`.

`crates/ne-core/data/win16_constants.json` is curated by hand: it binds named
flag and enumeration sets (`GMEM_*`, `MB_*`, `WS_*`, ROP codes, `IDC_*`, …) to
specific parameters of specific functions, which is knowledge no machine
-readable source carries.

## Accuracy notes

Two things in here are heuristics, and both say so where they are shown:

- **Recovered intersegment offsets.** An `ADDR_SEGMENT` fixup patching a bare
  `mov ax, seg X` has no offset word to read. Where an offset cannot be
  recovered and validated against the target segment's size, the target renders
  as `segNN:????` rather than as a plausible guess.
- **String and argument references.** 16-bit code loads a data pointer as a
  bare immediate, so tying a string to its code, or an argument to its value,
  is pattern matching rather than proof. Arguments that were not literal pushes
  are shown as their operand text, and partial reconstructions are marked.

## Tests

```sh
cargo test
```

The parser is exercised against a hand-assembled NE image built byte by byte in
the test, so a failure names the field that moved. The DIB decoder was
validated pixel-for-pixel against ImageMagick on RLE4, RLE8 and uncompressed
resources.

## Licence

MIT OR Apache-2.0.
