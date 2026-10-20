#!/usr/bin/env python3
"""What a massif heap snapshot holds, by where it was allocated (TODO.md, MEMORY): the peak snapshot, or the one given,
its bytes summed by the first frames above each allocation that are the engine's own source files, so a site reads as
the engine's code that asked, not the allocator's or a collection's; and, with --marks, by the first of the marks given
that each allocation's chain passes through, such as `read_all` or `publish.rs:628`, to tell one copy of the store from
another. Rust's v0 symbol names, which valgrind does not demangle, are read by their identifiers.

Usage:

    massif-sites.py <massif.out> [--snapshot N] [--frames 3] [--top 30] [--marks read_all publish.rs:628 …]
"""
import argparse
import re
from collections import defaultdict

NODE = re.compile(r"^(\s*)n(\d+): (\d+) (.*)$")


def snapshots(path):
    snap, lines = None, []
    with open(path) as f:
        for line in f:
            if line.startswith("snapshot="):
                if snap is not None:
                    yield snap, lines
                snap, lines = {"n": int(line.split("=")[1])}, []
            elif snap is not None and "=" in line and not line.startswith((" ", "n")):
                k, v = line.rstrip("\n").split("=", 1)
                snap[k] = v
            elif snap is not None:
                lines.append(line.rstrip("\n"))
        if snap is not None:
            yield snap, lines


def frame_name(text):
    # "0x4A3B2C: objectflow_core::world::World::in_order (world.rs:123)"
    m = re.match(r"0x[0-9A-F]+: (.*)", text)
    return m.group(1) if m else text


def leaves(lines):
    """Each leaf of the tree, its bytes and its frames, the allocation's first."""
    stack = []
    for line in lines:
        m = NODE.match(line)
        if not m:
            continue
        depth = len(m.group(1))
        children, size, text = int(m.group(2)), int(m.group(3)), frame_name(m.group(4))
        stack = stack[:depth]
        stack.append(text)
        if children == 0:
            yield size, stack[1:]       # the allocator first, then its callers outward, the root's summary dropped


def v0(name):
    """A v0-mangled Rust name read roughly: its length-prefixed identifiers, joined, generic arguments and all."""
    out, i = [], 0
    while i < len(name):
        m = re.match(r"\d+", name[i:])
        if m and m.group(0) != "0":
            n = int(m.group(0))
            ident = name[i + len(m.group(0)):i + len(m.group(0)) + n]
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", ident or "-"):
                out.append(ident)
                i += len(m.group(0)) + n
                continue
        i += 1
    return "::".join(out)


ROOT = __import__("pathlib").Path(__file__).resolve().parents[2]
# the engine's source files by name, `mod.rs`, `lib.rs` and `main.rs` left out since the standard library's share them
ENGINE_FILES = {p.name for p in (ROOT / "core/crates").glob("*/src/**/*.rs")} - {"mod.rs", "lib.rs", "main.rs"}


def site(fn):
    """The engine's source line a frame is, or None."""
    m = re.match(r"(.*?) \((.*)\)$", fn)
    if not m:
        return None
    name, where = m.group(1), m.group(2)
    if where.split("/")[-1].split(":")[0] not in ENGINE_FILES:
        return None
    name = v0(name) if name.startswith("_R") else name
    return f"{name.split('::')[-1]} [{where.split('/')[-1]}]"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--snapshot", type=int)
    ap.add_argument("--frames", type=int, default=3)
    ap.add_argument("--top", type=int, default=30)
    ap.add_argument("--marks", nargs="*", default=[])
    a = ap.parse_args()
    chosen = None
    for snap, lines in snapshots(a.file):
        if (a.snapshot is None and snap.get("heap_tree") == "peak") or snap["n"] == a.snapshot:
            chosen = (snap, lines)
            if a.snapshot is not None:
                break
    snap, lines = chosen
    total = int(snap["mem_heap_B"])
    print(f"snapshot {snap['n']}: heap {total / 2**20:.1f} MiB, the allocator's overhead "
          f"{int(snap['mem_heap_extra_B']) / 2**20:.1f} MiB")
    by, marked = defaultdict(int), defaultdict(int)
    for size, frames in leaves(lines):
        ours = [s for s in (site(f) for f in frames) if s]
        key = " < ".join(ours[:a.frames]) if ours else "(no engine frame)"
        by[key] += size
        text = " ".join(frames)
        marked[next((k for k in a.marks if k in text), "the rest")] += size
    if a.marks:
        for key, size in sorted(marked.items(), key=lambda kv: -kv[1]):
            print(f"{size / 2**20:8.1f} MiB {100 * size / total:5.1f}%  {key}")
        print()
    for key, size in sorted(by.items(), key=lambda kv: -kv[1])[:a.top]:
        print(f"{size / 2**20:8.1f} MiB {100 * size / total:5.1f}%  {key}")


if __name__ == "__main__":
    main()
