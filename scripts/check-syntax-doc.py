#!/usr/bin/env python3
"""Validate the declaration-syntax document against its own rules.

Three iterations shipped examples that violated the checks the same document
defines. This runs the mechanically decidable subset over every fenced example
and over the document's own claims. Exit non-zero on any finding.
"""
import re, sys, pathlib

DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/declaration-syntax.md"
src = DOC.read_text()
lines = src.split("\n")
findings = []


def add(kind, detail, line=None):
    findings.append((kind, detail, line))


def line_of(sub):
    for i, l in enumerate(lines, 1):
        if sub in l:
            return i
    return None


blocks = [(src[: m.start()].count("\n") + 2, m.group(1))
          for m in re.finditer(r"```text\n(.*?)```", src, flags=re.S)]

# ── document self-claims ────────────────────────────────────────────────────
m = re.search(r"(\w+) top-level forms: (.+?)\.", src)
if m:
    n = {"Seven": 7, "Eight": 8, "Nine": 9, "Ten": 10}.get(m.group(1))
    listed = len(re.findall(r"`(\w+)`", m.group(2)))
    if n != listed:
        add("claim", f"says {m.group(1)} top-level forms, lists {listed}", line_of("top-level forms"))

rw = re.search(r"\n`(module use .+?)`\n", src, flags=re.S)
reserved = set()
if rw:
    words = rw.group(1).split()
    reserved = set(words)
    dups = {w for w in words if words.count(w) > 1}
    if dups:
        add("lexical", f"duplicate reserved words: {sorted(dups)}")

# ── vocabularies ────────────────────────────────────────────────────────────
capdecl = set()
mc = re.search(r"capability ([A-Z_,\s]+?)\ncategory", src)
if mc:
    capdecl = set(re.findall(r"[A-Z][A-Z_]+", mc.group(1)))
for cap in set(re.findall(r"actor\.has\((\w+)\)", src)):
    if cap not in capdecl:
        add("check19", f"capability {cap} used but not declared", line_of(f"actor.has({cap})"))

catdecl = set()
mk = re.search(r"^category\s+(.+)$", src, flags=re.M)
if mk:
    catdecl = {c.strip() for c in mk.group(1).split(",")}
for blk_start, blk in blocks:
    for i, l in enumerate(blk.split("\n")):
        for cat in re.findall(r"category (\w+)", l):
            if cat not in catdecl:
                add("check19", f"category {cat} used but not declared", blk_start + i)

# ── per-machine / per-type state analysis ───────────────────────────────────
for blk_start, blk in blocks:
    if not re.search(r"^\s*(machine|type) ", blk, flags=re.M):
        continue
    name = re.search(r"^\s*(?:machine|type) (\w+)", blk, flags=re.M)
    name = name.group(1) if name else "?"
    states = {}
    for m2 in re.finditer(r"^\s*state (\w+)([^\n]*)$", blk, flags=re.M):
        states[m2.group(1)] = m2.group(2)
    for m2 in re.finditer(r"^\s*states\s+(.+)$", blk, flags=re.M):
        for part in m2.group(1).split(","):
            sm = re.match(r"\s*(\w+)(.*)", part)
            if sm:
                states[sm.group(1)] = sm.group(2)
    if not states:
        continue
    for st, mods in states.items():
        if "category" not in mods:
            add("check15", f"{name}.{st} has no category", blk_start)
    if not any("terminal" in v for v in states.values()):
        add("check15", f"{name} has no terminal state", blk_start)
    outgoing, incoming = set(), set()
    for rawline in blk.split("\n"):
        m2 = re.match(r"\s*(create|do|act|assert|erase)\s+(\w+)(.*)$", rawline)
        if not m2:
            continue
        kind = m2.group(1)
        head = m2.group(3)
        # protect a from-state or to-state set, then cut the body brace
        sets = re.findall(r"\{[^{}]*\}", head)
        for k, sset in enumerate(sets):
            head = head.replace(sset, f"@@{k}@@", 1)
        head = head.split("{")[0]
        for k, sset in enumerate(sets):
            head = head.replace(f"@@{k}@@", sset)
        froms = re.findall(r"\b([A-Z][A-Z_]+)\b(?=\s*->)", head) + \
                re.findall(r"\{\s*([A-Z_,\s]+?)\s*\}\s*->", head)
        tos = re.findall(r"->\s*(\w+)", head) + re.findall(r"->\s*\{\s*([A-Z_,\s]+?)\s*\}", head)
        at = re.findall(r"\bat\s+([A-Z][A-Z_]+)", head)
        for f in froms:
            for x in re.split(r"[,\s]+", f):
                if x in states: outgoing.add(x)
        for t in tos:
            for x in re.split(r"[,\s]+", t):
                if x in states: incoming.add(x)
        for a in at:
            if a in states: outgoing.add(a); incoming.add(a)
        if kind == "create":
            for t in tos:
                for x in re.split(r"[,\s]+", t):
                    if x in states: incoming.add(x)
    for st, mods in states.items():
        if "terminal" not in mods and st not in outgoing:
            add("check15", f"{name}.{st} is non-terminal with no outgoing transition", blk_start)
        if st not in incoming:
            add("check15", f"{name}.{st} is reachable by nothing", blk_start)
    if not re.search(r"^\s*create ", blk, flags=re.M) and re.search(r"^\s*type ", blk, flags=re.M):
        add("checkNEW", f"{name} declares no creation transition", blk_start)

# ── outcome / expression level ──────────────────────────────────────────────
for blk_start, blk in blocks:
    for i, l in enumerate(blk.split("\n")):
        ln = blk_start + i
        s = l.strip()
        if re.search(r"!= null|== null", s):
            add("check21", "comparison against a bare null", ln)
        if re.search(r"\b(count|sum|all|any|none|min|max)\(\s*(?!\w+\s+in\b)[a-z_]+\s+where", s):
            add("check21", f"aggregate without an element binder: {s[:60]}", ln)
        if "$" in s:
            add("lexical", f"'$' input prefix (should be inputs.): {s[:60]}", ln)
        if re.search(r"\bfor\s+\w+\s+in\b", s) and "limit" not in s:
            add("check21", f"for without limit: {s[:60]}", ln)
        if re.search(r"\bdo\s+\w+\s+at\s", s):
            add("grammar", f"'do' used with 'at' (that is 'act'): {s[:60]}", ln)

print(f"checked {len(blocks)} example blocks in {DOC.name}")
if not findings:
    print("clean")
    sys.exit(0)
for kind, detail, ln in findings:
    print(f"  {kind:9} line {ln or '?':>4}  {detail}")
print(f"{len(findings)} finding(s)")
sys.exit(1)
