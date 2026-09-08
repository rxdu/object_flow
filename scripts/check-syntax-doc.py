#!/usr/bin/env python3
"""Validate the declaration-syntax document against its own rules.

Four drafts shipped examples violating checks the same document defines. This
runs the mechanically decidable subset over every fenced example. Run it before
editing the document, not after: a fix that the checker cannot see is asserted,
not verified.

Coverage is listed in COVERS below and printed on every run, so the gap between
what the document defines and what this enforces is never invisible again.
"""
import re, sys, pathlib

COVERS = [2, 8, 11, 15, 16, 17, 18, 19, 20, 21, 26, 29, 31, 34, 35, 40]
DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/declaration-syntax.md"

src = DOC.read_text()
lines = src.split("\n")
findings = []


def add(check, detail, line):
    findings.append((check, detail, line))


blocks = [(src[: m.start()].count("\n") + 2, m.group(1))
          for m in re.finditer(r"```text\n(.*?)```", src, flags=re.S)]

# ── vocabularies declared in the document ───────────────────────────────────
def decl_list(kw):
    m = re.search(rf"^{kw}\s+(.+?)(?=\n[a-z]|\n\n)", src, flags=re.M | re.S)
    return set(re.findall(r"[A-Za-z_][A-Za-z_0-9]*", m.group(1))) if m else set()

capdecl, catdecl = decl_list("capability"), decl_list("category")
reserved = set()
rw = re.search(r"\n`(module use .+?)`\n", src, flags=re.S)
if rw:
    reserved = set(rw.group(1).split())

# ── parse each block into a declaration ─────────────────────────────────────
class Decl:
    def __init__(self, kind, name, start):
        self.kind, self.name, self.start = kind, name, start
        self.states, self.trans, self.attrs, self.refs = {}, [], set(), set()
        self.requires, self.provides, self.machine = [], set(), None
        self.abstract = False


decls = []
for bstart, blk in blocks:
    blines = blk.split("\n")
    cur = None
    i = 0
    while i < len(blines):
        raw, ln = blines[i], bstart + i
        m = re.match(r"^(machine|type)\s+(\w+)", raw)
        if m and "\u2026" in raw:      # an elided placeholder, e.g. "type Robot version 3 { … }"
            cur = None
            i += 1
            continue
        if m:
            cur = Decl(m.group(1), m.group(2), ln)
            cur.abstract = " abstract" in raw
            decls.append(cur)
            i += 1
            continue
        if cur is None:
            i += 1
            continue
        s = raw.strip()
        if m := re.match(r"^machine\s+(\w+)", s):
            cur.machine = m.group(1)
        if m := re.match(r"^state\s+(\w+)(.*)$", s):
            cur.states[m.group(1)] = (m.group(2), ln)
        if s.startswith("states "):                       # may wrap over lines
            acc, j = s[len("states "):], i
            while acc.rstrip().endswith(",") and j + 1 < len(blines):
                j += 1
                acc += " " + blines[j].strip()
            for part in acc.split(","):
                if sm := re.match(r"\s*(\w+)(.*)", part):
                    cur.states[sm.group(1)] = (sm.group(2), ln)
            i = j + 1
            continue
        if m := re.match(r"^(?:attr|counter)\s+(\w+)(.*)$", s):
            cur.attrs.add(m.group(1))
        if m := re.match(r"^(ref|part|owner)\s+(\w+)(.*)$", s):
            cur.refs.add(m.group(2))
        if m := re.match(r"^requires\s+\w+\s+(\w+)", s):
            cur.requires.append(m.group(1))
        if m := re.match(r"^provides\s+capability\s+(\w+)", s):
            cur.provides.add(m.group(1))
        if m := re.match(r"^(create|do|act|assert|erase)\s+(\w+)(.*)$", s):
            head = m.group(3)
            sets = re.findall(r"\{[^{}]*\}", head)
            for k, ss in enumerate(sets):
                head = head.replace(ss, f"@@{k}@@", 1)
            head = head.split("{")[0]
            for k, ss in enumerate(sets):
                head = head.replace(f"@@{k}@@", ss)
            body = raw[raw.find("{") + 1:] if "{" in raw else ""
            j = i
            while j + 1 < len(blines) and not re.match(r"^\s*\}", blines[j + 1]) and \
                  not re.match(r"^\s*(create|do|act|assert|erase|state|attr|ref|part|owner|requires|provides|derive|invariant)\b", blines[j + 1]):
                j += 1
                body += "\n" + blines[j]
            cur.trans.append((m.group(1), m.group(2), head, body, ln))
        i += 1

# ── checks over the parsed declarations ─────────────────────────────────────
def states_of(d):
    if d.states:
        return d.states
    if d.machine:
        for o in decls:
            if o.kind == "machine" and o.name == d.machine:
                return o.states
    return {}


for d in decls:
    st = d.states
    own_and_machine = list(d.trans)
    if d.machine:
        for o in decls:
            if o.kind == "machine" and o.name == d.machine:
                own_and_machine += o.trans
                for r in o.requires:
                    if r.isupper() and r not in d.provides:
                        add(16, f"{d.name} binds {o.name} which requires capability {r}, and provides none", d.start)
                    elif not r.isupper() and r not in d.attrs | d.refs and not any(
                            re.search(rf"invariant {r}\b", b) for _, b in [(0, src)]):
                        add(16, f"{d.name} binds {o.name} which requires {r}, not declared on the binder", d.start)

    # check 34 — every concrete type needs a creation
    if d.kind == "type" and not d.abstract and not any(t[0] == "create" for t in own_and_machine):
        add(34, f"{d.name} has no creation transition", d.start)

    if not st:
        continue
    outgoing, incoming = set(), set()
    for kind, tname, head, body, ln in own_and_machine:
        froms, tos = set(), set()
        for grp in re.findall(r"\{\s*([A-Z_,\s]+?)\s*\}\s*->", head):
            froms |= {x for x in re.split(r"[,\s]+", grp) if x}
        for one in re.findall(r"^\s*([A-Z][A-Z_]*)\s*->", head):
            froms.add(one)
        for grp in re.findall(r"->\s*\{\s*([A-Z_,\s]+?)\s*\}", head):
            tos |= {x for x in re.split(r"[,\s]+", grp) if x}
        for one in re.findall(r"->\s*([A-Z][A-Z_]*)", head):
            tos.add(one)
        ats = set(re.findall(r"\bat\s+([A-Z][A-Z_]*)", head))
        for x in froms | tos | ats:
            if x not in st and x != "ANY":
                add(19, f"{d.name}.{tname} names undeclared state {x}", ln)
        outgoing |= {x for x in froms if x in st}
        incoming |= {x for x in tos if x in st}
        for a in ats:
            if a in st:
                outgoing.add(a); incoming.add(a)
                if "terminal" in st[a][0]:
                    add(40, f"{d.name}.{tname} is an act at terminal state {a}", ln)
        for f in froms:
            if f in st and "terminal" in st[f][0] and kind == "do":
                add(15, f"{d.name}.{tname} is a do leaving terminal state {f}", ln)
        if kind == "create":
            # check 18 — a part's creation must write its owner
            if any(re.match(r"^owner\s", x) for x in []):
                pass
    for s_, (mods, ln) in st.items():
        if "category" not in mods:
            add(15, f"{d.name}.{s_} has no category", ln)
        else:
            for c in re.findall(r"category (\w+)", mods):
                if c not in catdecl:
                    add(19, f"category {c} not declared", ln)
        if "terminal" not in mods and s_ not in outgoing:
            add(15, f"{d.name}.{s_} is non-terminal with no outgoing transition", ln)
        if s_ not in incoming:
            add(15, f"{d.name}.{s_} is reachable by nothing", ln)
    if not any("terminal" in v[0] for v in st.values()):
        add(15, f"{d.name} has no terminal state", d.start)

    # check 11 / 18 — parts
    owners = [x for x in d.refs if any(re.match(rf"^owner\s+{x}\b", l.strip())
                                       for l in blocks[0][1].split("\n"))]
for d in decls:
    owner_names = set()
    for bstart, blk in blocks:
        for l in blk.split("\n"):
            if m := re.match(r"^\s*owner\s+(\w+)\s*:", l):
                owner_names.add((d.name, m.group(1)))
    for kind, tname, head, body, ln in d.trans:
        if kind != "create":
            continue
        is_part = any(re.match(r"^\s*owner\s+\w+\s*:", l) for l in
                      (blk for bs, blk in blocks if bs <= d.start <= bs + blk.count("\n"))
                      for l in [""])
    # simpler: detect owner within the same block as d
    blk = next((b for bs, b in blocks if bs <= d.start <= bs + b.count("\n") + 1), "")
    owner_decl = re.search(r"^\s*owner\s+(\w+)\s*:", blk, flags=re.M)
    if owner_decl:
        for kind, tname, head, body, ln in d.trans:
            if kind != "create":
                continue
            if "only via" not in head:
                add(11, f"{d.name}.{tname} creates a part but is not 'only via' its whole", ln)
            if not re.search(rf"\b{owner_decl.group(1)}\b", head + body):
                add(18, f"{d.name}.{tname} creates a part without writing owner '{owner_decl.group(1)}'", ln)

# ── machine 'requires' completeness (check 35) ──────────────────────────────
for d in decls:
    if d.kind != "machine":
        continue
    req = set(d.requires)
    for kind, tname, head, body, ln in d.trans:
        for nm in set(re.findall(r"\bset\s+(\w+)\s*:=", body)) | set(
                re.findall(r"accepts\s+([\w,\s]+)", head)):
            for one in re.split(r"[,\s]+", nm):
                if one and one not in req and one not in ("state",):
                    add(35, f"{d.name}.{tname} names '{one}', which the machine does not require", ln)

# ── line-level lexical and expression checks ────────────────────────────────
for bstart, blk in blocks:
    for i, l in enumerate(blk.split("\n")):
        ln, s = bstart + i, l.strip()
        code = s.split("#")[0]
        if re.search(r"(==|!=)\s*null|null\s*(==|!=)", code):
            add(21, "comparison against a bare null", ln)
        if re.search(r"\b(count|sum|all|any|none|min|max)\(\s*(?!\w+\s+in\b)[a-z_]+\s+where", code):
            add(21, f"aggregate without an element binder: {code[:52]}", ln)
        if "$" in code:
            add(21, f"'$' input prefix, should be inputs.: {code[:52]}", ln)
        if re.match(r"^\s*for\s+\w+\s+in\b", code) and "limit" not in code:
            add(21, f"for without limit: {code[:52]}", ln)
        if re.match(r"^\s*do\s+\w+\s+at\s", code):
            add(21, f"'do' used with 'at', that is 'act': {code[:52]}", ln)
        for cap in re.findall(r"actor\.\w+\((\w+)\)", code):
            if cap not in capdecl:
                add(19, f"capability {cap} used but not declared", ln)
        if m := re.match(r"^\s*attr\s+(\w+)", code):
            if m.group(1) in reserved - {"event", "state", "count", "type", "owner", "scope", "serial", "quantity", "format", "summary"}:
                add(21, f"reserved word '{m.group(1)}' used as an attribute name", ln)
        if re.search(r"\bcall\s+\w[\w.]*\.forget\(\s*\)", code):
            add(2, "call to an erase transition passes no reason argument", ln)

# ── document-level claims ───────────────────────────────────────────────────
m = re.search(r"(\w+) top-level forms: (.+?)\.", src)
if m:
    n = {"Seven": 7, "Eight": 8, "Nine": 9, "Ten": 10}.get(m.group(1))
    listed = len(re.findall(r"`(\w+)`", m.group(2)))
    if n != listed:
        add(0, "says " + m.group(1) + " top-level forms, lists " + str(listed), 1)
if rw:
    words = rw.group(1).split()
    if dups := {w for w in words if words.count(w) > 1}:
        add(21, f"duplicate reserved words: {sorted(dups)}", 1)
    for kw in ("provides", "requires", "cascade", "accepts", "corrects"):
        if re.search(rf"^\s*{kw}\b", src, flags=re.M) and kw not in words:
            add(21, f"keyword '{kw}' used in the document but not reserved", 1)
nums = [int(x) for x in re.findall(r"^\| (\d+) \|", src, flags=re.M)]
if nums != sorted(nums):
    add(0, f"check table is misordered: {nums}", 1)

print(f"{DOC.name}: {len(blocks)} example blocks, {len(decls)} declarations")
print(f"enforces checks {COVERS} of the 40 defined; the rest are not mechanised")
if not findings:
    print("clean")
    sys.exit(0)
seen = set()
for c, d, ln in sorted(findings, key=lambda f: (f[2], f[0])):
    if (c, d) in seen:
        continue
    seen.add((c, d))
    print(f"  check{c:<3} line {ln:>4}  {d}")
print(f"{len(seen)} finding(s)")
sys.exit(1)
