#!/usr/bin/env python3
"""Regenerate the Win16 ordinal database from Wine's 16-bit .spec files.

Wine keeps each 16-bit module in dlls/<name>16/<name>16.spec. The NE module
reference table names them differently (krnl386.exe16 is "KERNEL"), so the
directory -> module mapping is explicit for the three renamed ones and
mechanical for the rest.
"""
import json, re, sys, urllib.request, concurrent.futures

RAW = "https://raw.githubusercontent.com/wine-mirror/wine/master/dlls/%s/%s.spec"

RENAMED = {"krnl386.exe16": "KERNEL", "user.exe16": "USER", "gdi.exe16": "GDI"}

DIRS = """avifile.dll16 comm.drv16 commdlg.dll16 compobj.dll16 ctl3d.dll16
ctl3dv2.dll16 ddeml.dll16 dispdib.dll16 display.drv16 gdi.exe16 imm.dll16
keyboard.drv16 krnl386.exe16 lzexpand.dll16 mmsystem.dll16 mouse.drv16
msacm.dll16 msvideo.dll16 ole2.dll16 ole2conv.dll16 ole2disp.dll16
ole2nls.dll16 ole2prox.dll16 ole2thk.dll16 olecli.dll16 olesvr.dll16
rasapi16.dll16 setupx.dll16 shell.dll16 sound.drv16 storage.dll16 stress.dll16
system.drv16 toolhelp.dll16 twain.dll16 typelib.dll16 user.exe16 ver.dll16
w32sys.dll16 win32s16.dll16 win87em.dll16 winaspi.dll16 windebug.dll16
wineps16.drv16 wing.dll16 winnls.dll16 winsock.dll16 wintab.dll16""".split()


def module_name(d):
    if d in RENAMED:
        return RENAMED[d]
    return re.sub(r"\.(dll|drv|exe)16$", "", d).upper()


# "123 pascal FunctionName(word ptr) Impl"; also "123 stub Foo".
LINE = re.compile(
    r"^\s*(\d+)\s+(\S+)\s+(?:-\S+\s+)*([A-Za-z_@][\w@.]*)\s*(?:\(([^)]*)\))?")


def fetch(d):
    mod = module_name(d)
    try:
        with urllib.request.urlopen(RAW % (d, d), timeout=30) as fh:
            text = fh.read().decode("utf-8", "replace")
    except Exception as e:
        return mod, {}, {}, str(e)
    out = {}
    sigs = {}
    for line in text.splitlines():
        line = line.split("#", 1)[0]
        m = LINE.match(line)
        if not m:
            continue
        ordv, kind, name, args = int(m.group(1)), m.group(2), m.group(3), m.group(4)
        if kind == "variable":
            continue
        if name.startswith("__wine"):
            continue
        key = "%s.%d" % (mod, ordv)
        out[key] = name
        if args is not None:
            sigs[key] = {
                "n": name,
                "cc": kind,
                "a": args.split(),
            }
    return mod, out, sigs, None


merged = {}
merged_sigs = {}
report = []
with concurrent.futures.ThreadPoolExecutor(max_workers=12) as ex:
    for mod, table, sigs, err in ex.map(fetch, DIRS):
        report.append((mod, len(table), err or ""))
        merged.update(table)
        merged_sigs.update(sigs)

# Anything already hand-verified in the seed file wins over a Wine stub name.
if len(sys.argv) > 2:
    seed = json.load(open(sys.argv[2]))
    for k, v in seed.items():
        merged.setdefault(k, v)


def sortkey(k):
    m, n = k.rsplit(".", 1)
    return (m, int(n))


ordered = {k: merged[k] for k in sorted(merged, key=sortkey)}
with open(sys.argv[1], "w") as fh:
    json.dump(ordered, fh, indent=0, sort_keys=False)
    fh.write("\n")

# The signature table lives beside the ordinal table, keyed the same way.
sig_path = sys.argv[1].replace("win16_ordinals", "win16_api")
ordered_sigs = {k: merged_sigs[k] for k in sorted(merged_sigs, key=sortkey)}
with open(sig_path, "w") as fh:
    json.dump(ordered_sigs, fh, indent=0, sort_keys=False)
    fh.write("\n")
print("signatures %d -> %s" % (len(ordered_sigs), sig_path))
for mod, n, err in sorted(report):
    print("%-10s %5d  %s" % (mod, n, err))
print("total %d" % len(ordered))
