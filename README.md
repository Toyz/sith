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
sith-gui [FILE|PROJECT]
```

- Tabbed views over a workspace of files, with back/forward history. Clicking
  an imported symbol opens the DLL that exports it, at the export.
- A navigator with segments, functions grouped by segment, resources with live
  thumbnails, and every sibling module found beside the file.
- A disassembly listing with an interactive branch-arrow gutter — hover an arc
  to light up the whole path and both ends, click to follow it — plus function
  banners, resolved fixups and reconstructed API calls.
- An inspector showing the selected instruction's bytes, its fixup, the API
  signature with named arguments, everything that references it, and the box
  where you name it and write your note.
- A call-graph explorer: pan and zoom, callers and callees, click to re-centre.
- A strings browser that cross-references each string to the code that loads it.
- Resource preview for bitmaps, icons and cursors, with export to real `.bmp`,
  `.ico` and `.cur` files; menus, dialogs, string tables, accelerators and
  version info decode to text.
- A command palette over everything in the binary, with scoped search and match
  highlighting.
- Seven themes — Catppuccin Mocha, Macchiato and Latte, Nord, Tokyo Night,
  Gruvbox Dark and Midnight — under View ▸ Theme, remembered between runs.

Keys: `Ctrl+O` open, `Ctrl+S` save project, `Ctrl+P` find anything, `Ctrl+G` go
to an address or symbol, `Ctrl+W` close tab, `Alt+←`/`Alt+→` history, `↑`/`↓`
move, `Enter` follow, `N` name the selected address, `B` bookmark it.

### The command palette

`Ctrl+P` searches functions, segments, resources, imports, strings, sibling
modules and the tool's own commands at once. A leading sigil narrows it:

| prefix | searches |
| --- | --- |
| `>` | commands |
| `@` | functions |
| `#` | strings |
| `$` | resources |
| `seg02:1A40` | an address, offered directly |

Matching is subsequence-based and ranked, so `mwp` finds `MAINWNDPROC`, and the
matched letters are marked in the result so it is clear why an entry is there.

## Projects

Everything the tool derives from a binary can be recomputed at any time. What
cannot is the part that came out of your head. A project file holds exactly
that, keyed by address:

- the names you gave to addresses,
- the notes you attached to them,
- your bookmarks,
- which segments you decided hold 32-bit code.

`File ▸ New project…` opens a three-step wizard: point it at the folder the
program was installed to, tick the modules worth keeping, name the result. The
scan finds the NE binaries among the data files and reports what each one holds
— segments, exports, resources — so the choice can be made without opening any
of them.

`File ▸ Save project` writes a `.sith` file; `Ctrl+S` saves again, and every
later change is written back automatically so nothing is lost to a crash.
Opening a project reopens every binary it refers to, and `sith-gui FILE.sith`
opens one straight from the command line.

The format is JSON, so it diffs, merges and reads as plain text:

```json
{
  "format_version": 1,
  "name": "chips",
  "binaries": [
    {
      "path": "CHIPS.EXE",
      "module": "CHIPS",
      "bits32": [],
      "names": { "02:225C": "MainWndProc" },
      "comments": { "02:225C": "handles WM_PAINT" },
      "bookmarks": ["02:225C"]
    }
  ]
}
```

Binary paths are stored relative to the project file where possible, so a
project folder can be moved, shared or checked in as a unit; a binary that has
moved is still matched by its module name.

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

Bitmap fonts are decoded too, so a `.FON` — an NE library whose only payload is
`RT_FONTDIR` and `RT_FONT` — opens like anything else and previews as a sheet
of its glyphs. Glyph bitmaps are stored *column major*, which is the detail
that turns a naive reader's output into a sheared mess.

### Resources and the code that loads them

A resource is inert until something asks for it, and the ask is always the same
shape: a `LoadBitmap`, `DialogBox` or `LoadString` call naming it. Both idioms
are recovered — an id pushed as a literal, and a far pointer to a string whose
text matches a named resource — so a bitmap says which function draws it and a
loading call links straight to the artwork:

```
sith res list CHIPS.EXE
  type            id                  offset     size  refs  loaded by
  BITMAP          OBJ32_4             0000D800    73728    10  by name  seg02:1389 seg02:14ED …
  MENU            CHIPSMENU           0003FC00      512     2  by name  seg02:090A seg02:22C1
  DIALOG          DLG_GOTO            0003FE00      512     1  by name  seg02:20EA
```

## Where the API data comes from

`tools/fetch_ordinals.py` regenerates `crates/ne-core/data/win16_ordinals.json`
and `win16_api.json` from Wine's 16-bit `.spec` files, which are authoritative.
Hand-written tables in circulation get this wrong — one common table has GDI.27
as `BitBlt` when it is `Rectangle`, and GDI.34, the real `BitBlt`, as
`CreateBrushIndirect`.

Two files are curated by hand, because no machine-readable source carries what
is in them:

- `win16_constants.json` binds named flag and enumeration sets (`GMEM_*`,
  `MB_*`, `WS_*`, ROP codes, `IDC_*`, `DISPLAYDIB_*`, `MMIO_*`, WinG dither
  modes, …) to specific parameters of specific functions.
- `win16_params.json` gives return types and parameter names for the entry
  points that turn up most in application code, including the modules a game
  actually leans on: WinG, DISPDIB, MMSYSTEM's `mmio`/`wave`/`time`/`mci`
  families, SHELL, TOOLHELP, VER, LZEXPAND and COMMDLG.

A names list whose length no longer matches the argument list is dropped rather
than applied: a wrong parameter name is worse than none.

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
