#!/usr/bin/env python3
"""Validate the declaration-syntax document against its own rules.

Coverage is not a list someone maintains. Every check below carries a FIXTURE:
a minimal declaration that must produce it. `--self-test` runs them, and the
coverage line printed on a normal run is *derived* from which fixtures fire.
A check that is claimed but unimplemented therefore fails the self-test rather
than appearing in a comment nobody rechecks — which is how a previous version
came to advertise six checks it did not have.
"""
import re, sys, pathlib

DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/declaration-syntax.md"

# ── model of a declaration ──────────────────────────────────────────────────
class Decl:
    def __init__(self, kind, name, start):
        self.kind, self.name, self.start = kind, name, start
        self.abstract = False
        self.machine = None
        self.tracking = None
        self.states = {}        # name -> (modifiers, line)
        self.trans = []         # (kind, name, head, body, line)
        self.attrs = {}         # name -> (type_and_markings, line)
        self.counters = set()
        self.rels = {}          # name -> (kind, decl_text, line)
        self.requires = []      # (kind, name, line)
        self.provides = set()
        self.invariants = set()
        self.derives = set()
        self.base = None


def parse(text, base=0):
    decls, cur, lines = [], None, text.split("\n")
    i = 0
    while i < len(lines):
        raw, ln = lines[i], base + i
        if m := re.match(r"^(machine|type)\s+(\w+)", raw):
            xbase = re.search(r"\bextends\s+(\w+)", raw)
            if "…" in raw:                     # elided placeholder
                cur = None; i += 1; continue
            cur = Decl(m.group(1), m.group(2), ln)
            cur.abstract = " abstract" in raw
            cur.base = xbase.group(1) if xbase else None
            decls.append(cur); i += 1; continue
        if cur is None:
            i += 1; continue
        s = raw.strip()
        if m := re.match(r"^machine\s+(\w+)", s):            cur.machine = m.group(1)
        if m := re.match(r"^tracking\s+(\w+)", s):           cur.tracking = m.group(1)
        if s.startswith("provides capability"):
            acc, j = s, i
            ind0 = len(raw) - len(raw.lstrip())
            while j + 1 < len(lines) and lines[j + 1].strip() \
                  and len(lines[j + 1]) - len(lines[j + 1].lstrip()) > ind0:
                j += 1; acc += " " + lines[j].strip()
            for nm in re.findall(r"(\w+)\s*=", acc): cur.provides.add(nm)
            cur.provided_from = getattr(cur, "provided_from", []) + \
                [(rhs, ln) for rhs in re.findall(r"=\s*(\w+)", acc)]
        if m := re.match(r"^requires\s+(\w+)\s+(.+)$", s):
            kind, rest = m.group(1), m.group(2)
            names = re.findall(r"\w+", rest) if kind == "capability" \
                    else re.findall(r"^\s*(\w+)", rest)
            for nm in names:
                cur.requires.append((kind, nm, ln))
        if m := re.match(r"^invariant\s+(\w+)", s):          cur.invariants.add(m.group(1))
        if m := re.match(r"^derive\s+(\w+)", s):             cur.derives.add(m.group(1))
        if m := re.match(r"^state\s+(\w+)(.*)$", s):         cur.states[m.group(1)] = (m.group(2), ln)
        if s.startswith("states "):
            acc, j = s[7:], i
            while acc.rstrip().endswith(",") and j + 1 < len(lines):
                j += 1; acc += " " + lines[j].strip()
            for part in acc.split(","):
                if sm := re.match(r"\s*(\w+)(.*)", part):
                    cur.states[sm.group(1)] = (sm.group(2), ln)
            i = j + 1; continue
        if m := re.match(r"^attr\s+(\w+)\s*(.*)$", s):       cur.attrs[m.group(1)] = (m.group(2), ln)
        if m := re.match(r"^counter\s+(\w+)", s):            cur.counters.add(m.group(1))
        if m := re.match(r"^(ref|part|owner)\s+(\w+)\s*:(.*)$", s):
            spec, j = m.group(3), i
            ind = len(raw) - len(raw.lstrip())
            while j + 1 < len(lines) and lines[j + 1].strip() \
                  and len(lines[j + 1]) - len(lines[j + 1].lstrip()) > ind:
                j += 1; spec += " " + lines[j].strip()
            cur.rels[m.group(2)] = (m.group(1), spec, ln)
            i = j + 1; continue
        if m := re.match(r"^cascade\s+(\w+)\s+on\b(.*)$", s):
            acc, j = "cascade on" + m.group(2), i
            ind0 = len(raw) - len(raw.lstrip())
            while j + 1 < len(lines) and lines[j + 1].strip() \
                  and len(lines[j + 1]) - len(lines[j + 1].lstrip()) > ind0:
                j += 1; acc += " " + lines[j].strip()
            k = "$sub$" + m.group(1)
            prev = cur.rels.get(k)
            cur.rels[k] = ("part", (prev[1] + " " if prev else "") + acc, ln)
            i = j + 1; continue
        if m := re.match(r"^(create|do|act|assert|erase)\s+(\w+)(.*)$", s):
            head, j0 = m.group(3), i
            while "{" not in head and j0 + 1 < len(lines):
                j0 += 1; head += " " + lines[j0].strip()
            raw = "\n".join(lines[i:j0 + 1])
            i = j0
            sets = re.findall(r"\{[^{}]*\}", head)
            for k, ss in enumerate(sets): head = head.replace(ss, f"@@{k}@@", 1)
            head = head.split("{")[0]
            for k, ss in enumerate(sets): head = head.replace(f"@@{k}@@", ss)
            body, depth, j = "", raw.count("{") - raw.count("}"), i
            if "{" in raw: body = raw[raw.index("{") + 1:]
            while depth > 0 and j + 1 < len(lines):
                j += 1; body += "\n" + lines[j]
                depth += lines[j].count("{") - lines[j].count("}")
            cur.trans.append((m.group(1), m.group(2), head, body, ln))
            i = j + 1; continue
        i += 1
    return decls


def analyse(text, base=0, capdecl=None, catdecl=None, reserved=None, world=None):
    """Return findings as (check_number, detail, line)."""
    out = []
    def add(c, d, l): out.append((c, d, l))
    decls = parse(text, base)
    by_name = dict(world or {})
    by_name.update({d.name: d for d in decls})
    capdecl = capdecl if capdecl is not None else set()
    catdecl = catdecl if catdecl is not None else set()
    reserved = reserved or set()

    # 51 — the continuation rule of §9.1
    STARTS = (r"^(module|use|capability|category|enum|sequence|evaluator|machine|type|tracking|"
              r"states|state|provides|summary|visible|attr|counter|ref|part|owner|derive|invariant|"
              r"create|do|act|assert|erase|input|accepts|require|set|add|remove|call|supersede|for|"
              r"cascade|survives|requires|removed|renamed|fn|extends|may|corrects|only|proposable)\b")
    clause_col = None
    for i, raw in enumerate(text.split("\n")):
        line = raw.split("#", 1)[0].rstrip()
        if re.match(r"^\s*enum\b", line):
            clause_col = None; continue
        if "{" in line:
            after = line.split("{", 1)[1]
            if after.strip() and "}" not in after and not line.lstrip().startswith(("<", "|")):
                out.append((51, "a body opens with a clause on the brace line and does not close", base + i))
        st = line.strip()
        if not st or st.startswith(("}", "<", "|", "\u2026")):
            clause_col = None; continue
        ind = len(line) - len(line.lstrip())
        if re.match(STARTS, st):
            clause_col = ind
        elif clause_col is not None and ind <= clause_col:
            out.append((51, f"continuation line not indented deeper than its clause: {st[:48]}", base + i))

    for i, raw in enumerate(text.split("\n")):
        m = re.match(r"^(enum|sequence|evaluator|machine|type)\s+(\w+)(.*)$", raw)
        if m and "…" not in raw and not re.search(r"\bversion\s+\d", m.group(3)):
            out.append((42, f"{m.group(1)} {m.group(2)} has no version", base + i))

    for d in decls:
        mach = by_name.get(d.machine) if d.machine else None
        mtrans = mach.trans if mach else []
        if any(t[0] == "create" for t in d.trans):        # §2.1: a binder's creations replace
            mtrans = [t for t in mtrans if t[0] != "create"]
        trans = d.trans + mtrans
        states = d.states or (mach.states if mach else {})
        attrs, rels, seen = dict(d.attrs), dict(d.rels), set()
        anc = by_name.get(d.base)
        while anc and anc.name not in seen:
            seen.add(anc.name)
            for k, v in anc.attrs.items(): attrs.setdefault(k, v)
            for k, v in anc.rels.items(): rels.setdefault(k, v)
            d.invariants |= anc.invariants; d.derives |= anc.derives
            anc = by_name.get(anc.base)
        provides = set(d.provides)
        if mach:
            for rk, rn, rln in mach.requires:
                if rk == "capability":
                    if rn not in provides:
                        add(16, f"{d.name} binds {mach.name} requiring capability {rn}, provides none", d.start)
                elif rn not in attrs and rn not in rels and rn not in d.invariants and rn not in d.counters:
                    add(16, f"{d.name} binds {mach.name} requiring {rn}, not declared on the binder", d.start)
        for rhs, rln in getattr(d, "provided_from", []):
            if capdecl and rhs not in capdecl:
                add(19, f"{d.name} provides from undeclared capability {rhs}", rln)
        if d.kind == "type" and not d.abstract:
            if not d.machine and not d.states:
                add(16, f"{d.name} neither binds a machine nor declares states", d.start)
            if d.machine and d.states:
                add(16, f"{d.name} both binds a machine and declares states", d.start)
            if not any(t[0] == "create" for t in trans):
                add(34, f"{d.name} has no creation transition", d.start)
            if d.tracking == "quantity" and not d.counters:
                add(31, f"{d.name} is tracking quantity with no counter", d.start)
            if d.tracking == "serial" and d.counters:
                add(31, f"{d.name} is tracking serial with a counter", d.start)

        # per-binder state analysis (machine states + this type's transitions)
        if states and d.kind == "type":
            out_s, in_s = set(), set()
            for kind, tn, head, body, ln in trans:
                froms, tos = set(), set()
                for g in re.findall(r"\{\s*([A-Z_0-9,\s]+?)\s*\}\s*->", head):
                    froms |= {x for x in re.split(r"[,\s]+", g) if x}
                for one in re.findall(r"^\s*([A-Z][A-Z_0-9]*)\s*->", head): froms.add(one)
                for g in re.findall(r"->\s*\{\s*([A-Z_0-9,\s]+?)\s*\}", head):
                    tos |= {x for x in re.split(r"[,\s]+", g) if x}
                for one in re.findall(r"->\s*([A-Z][A-Z_0-9]*)", head): tos.add(one)
                ats = set()
                for g in re.findall(r"\bat\s*\{\s*([A-Z_0-9,\s]+?)\s*\}", head):
                    ats |= {x for x in re.split(r"[,\s]+", g) if x}
                for one in re.findall(r"\bat\s+([A-Z][A-Z_0-9]*)", head): ats.add(one)
                if re.search(r"^\s*any\s*->", head) or re.search(r"\bat\s+any\b", head):
                    froms |= {k for k, v in states.items() if "terminal" not in v[0]}
                for x in froms | tos | ats:
                    if x not in states and x.lower() != "any":
                        add(19, f"{d.name}.{tn} names undeclared state {x}", ln)
                out_s |= {x for x in froms if x in states}
                in_s |= {x for x in tos if x in states}
                for a in ats:
                    if a in states:
                        out_s.add(a); in_s.add(a)
                        if "terminal" in states[a][0]:
                            add(40, f"{d.name}.{tn} is an act at terminal state {a}", ln)
                for f in froms:
                    if f in states and "terminal" in states[f][0] and kind == "do":
                        add(15, f"{d.name}.{tn} is a do leaving terminal state {f}", ln)
            for s_, (mods, ln) in states.items():
                if "category" not in mods: add(15, f"{d.name}.{s_} has no category", ln)
                for c in re.findall(r"category (\w+)", mods):
                    if catdecl and c not in catdecl: add(19, f"category {c} not declared", ln)
                if "terminal" not in mods and s_ not in out_s:
                    add(15, f"{d.name}.{s_} is non-terminal with no outgoing transition", ln)
                if s_ not in in_s: add(15, f"{d.name}.{s_} is reachable by nothing", ln)
            if not any("terminal" in v[0] for v in states.values()):
                add(15, f"{d.name} has no terminal state", d.start)

        # 42 — tracking present
        if d.kind == "type" and not d.abstract:
            tr, anc42, seen42 = d.tracking, d, set()
            while tr is None and anc42 is not None and anc42.base and anc42.base not in seen42:
                seen42.add(anc42.base); anc42 = by_name.get(anc42.base)
                tr = anc42.tracking if anc42 else None
            if tr not in ("serial", "quantity"):
                add(42, f"{d.name} has tracking {tr!r}, declared or inherited, "
                        "which is not serial or quantity", d.start)

        # 43 — extends resolves to an abstract base, acyclically
        if d.base:
            seen43, cur43 = set(), d
            while cur43 and cur43.base:
                if cur43.base in seen43:
                    add(43, f"{d.name} has a cyclic extends chain through {cur43.base}", d.start); break
                seen43.add(cur43.base); cur43 = by_name.get(cur43.base)
            anc43 = by_name.get(d.base)
            if anc43 is not None and not anc43.abstract:
                add(43, f"{d.name} extends {d.base}, which is not abstract", d.start)

        # 47 — a cascade clause states its bound
        for rn, (rk, rspec, rln) in d.rels.items():
            for cl in re.findall(r"cascade on\b.*?(?=cascade on|survives|$)", rspec):
                if "limit" not in cl:
                    add(47, f"{d.name}.{rn} has a cascade clause with no limit", rln)

        # reference pairs: exactly one end stores the value (check 41)
        for rn, (rk, rspec, rln) in d.rels.items():
            if rk != "ref" or not rspec.split(): continue
            im = re.search(r"\binverse\s+(\w+)", rspec)
            if not im: continue
            fname = rspec.split()[0].rstrip("?[]")
            far = by_name.get(fname)
            if far is None: continue
            fe = far.rels.get(im.group(1))
            if fe is None:
                add(41, f"{d.name}.{rn} names inverse {fname}.{im.group(1)}, which {fname} does not declare", rln)
                continue
            if fe[0] != "ref" or not fe[1].split(): continue
            fm = re.search(r"\binverse\s+(\w+)", fe[1])
            if fm and fm.group(1) != rn:
                add(41, f"{d.name}.{rn} and {fname}.{im.group(1)} do not name each other", rln)
                continue
            if d.name > fname: continue          # report the pair once
            near_set, far_set = "[]" in rspec.split()[0], "[]" in fe[1].split()[0]
            if near_set and far_set:
                add(41, f"{d.name}.{rn} and {fname}.{im.group(1)} are both set-valued, so neither end can store the pair", rln)
            elif not near_set and not far_set:
                n = ("stored" in rspec.split()) + ("stored" in fe[1].split())
                if n != 1:
                    add(41, f"{d.name}.{rn} and {fname}.{im.group(1)} are both singular; exactly one must be marked 'stored' ({n} are)", rln)

        # parts and owners
        owner = next(((n, v) for n, v in d.rels.items() if v[0] == "owner"), None)
        for kind, tn, head, body, ln in (trans if d.kind == "type" else d.trans):
            written = set(re.findall(r"\bset\s+(\w+)\s*:=", body)) | \
                      {x for g in re.findall(r"accepts\s+([\w,\s]+)", head) for x in re.split(r"[,\s]+", g) if x}
            if kind == "create":
                if owner:
                    if "only via" not in head:
                        add(11, f"{d.name}.{tn} creates a part but is not 'only via' its whole", ln)
                    else:
                        whole = owner[1][1].split()[0].rstrip("?[]")
                        wd = by_name.get(whole)
                        wholes = {whole}
                        if wd is not None and wd.abstract:      # the family declares the part
                            wholes |= {n for n, x in by_name.items() if x.base == whole}
                        named = {t for t, _ in re.findall(r"(\w+)\.(\w+)", head.split("only via")[1])}
                        if named and not (named & wholes):
                            add(11, f"{d.name}.{tn} is only via {sorted(named)}, not its whole {whole}", ln)
                    if owner[0] not in written:
                        add(18, f"{d.name}.{tn} creates a part without writing owner '{owner[0]}'", ln)
                for pn, (pk, pspec, pln) in rels.items():
                    if pk != "part" or pn.startswith("$sub$") or not pspec.split(): continue
                    ptype = pspec.split()[0]
                    if ptype.endswith("?") or ptype.endswith("[]"): continue     # optional or set
                    if not re.search(r"\bcreate\s+(?:\w+\s*=\s*)?" + re.escape(ptype) + r"\.", body):
                        add(8, f"{d.name}.{tn} never fills required part '{pn}'", ln)
                for an, (spec, aln) in attrs.items():
                    if "?" in spec.split()[0] if spec.split() else False: continue
                    if spec and not spec.split()[0].endswith("?") and "[]" not in spec.split()[0] \
                       and "default" not in spec and "identifier" not in spec and an not in written:
                        add(8, f"{d.name}.{tn} never writes required attribute '{an}'", ln)
            if kind == "assert":
                if not re.search(r"actor\.\w+\(", body): add(29, f"{d.name}.{tn} asserts with no capability guard", ln)
                if not re.search(r"input\s+reason\b", body): add(29, f"{d.name}.{tn} asserts with no reason input", ln)
                if "may admit" in body and not re.search(r"input\s+admits\b", body):
                    add(38, f"{d.name}.{tn} has 'may admit' but no admits input", ln)
            if kind == "erase" and not re.search(r"input\s+reason\b", body):
                add(29, f"{d.name}.{tn} erases with no reason input", ln)
            # 17 — a write must target this object
            for tgt in re.findall(r"^\s*(?:set|add|remove)\s+([\w.]+)\s*:=", body, flags=re.M):
                if "." in tgt: add(17, f"{d.name}.{tn} writes through a path '{tgt}'", ln)
                elif tgt not in attrs and tgt not in rels and tgt not in d.counters \
                     and tgt not in d.derives and tgt not in {n for _, n, _ in d.requires}:
                    add(19, f"{d.name}.{tn} writes undeclared name '{tgt}'", ln)
            for tgt in re.findall(r"^\s*(?:add|remove)\s+(\w+)\s*:=", body, flags=re.M):
                spec = attrs.get(tgt, ("", 0))[0]
                if spec and "[]" not in spec: add(17, f"{d.name}.{tn} adds to non-set '{tgt}'", ln)
            # 20 — guards must be named; no default on an optional input
            for g in re.findall(r"^\s*require\s+(.*)$", body, flags=re.M):
                if not re.match(r"^\w+\s*:", g): add(20, f"{d.name}.{tn} has an unnamed guard", ln)
            for inp in re.findall(r"^\s*input\s+(\w+)\s*:\s*([^\n]*)$", body, flags=re.M):
                if "?" in inp[1] and "default" in inp[1]:
                    add(20, f"{d.name}.{tn} gives a default to optional input '{inp[0]}'", ln)
            # 26 — supersession
            to_states = set(re.findall(r"->\s*([A-Z][A-Z_0-9]*)", head))
            sup = re.search(r"^\s*supersede\s+([^}\n]*)", body, flags=re.M)
            if sup and not any("superseding" in states.get(t, ("", 0))[0] for t in to_states):
                add(26, f"{d.name}.{tn} supersedes into a non-superseding state", ln)
            if sup and sup.group(1).strip() == "this":
                add(26, f"{d.name}.{tn} supersedes itself", ln)
            if not sup:
                for t in to_states:
                    if "superseding" in states.get(t, ("", 0))[0]:
                        add(26, f"{d.name}.{tn} enters superseding state {t} without a supersede", ln)
            # 2 — cascade arguments
            for tgt, tn2, args in re.findall(r"\b(?:call|create)\s+([\w.$]+)\.(\w+)\(([^)]*)\)", body):
                callee = by_name.get(tgt) or by_name.get(tgt.split(".")[-1])
                if callee:
                    ct = next((t for t in callee.trans if t[1] == tn2), None)
                    if ct:
                        need = {n for n, sp in re.findall(r"^\s*input\s+(\w+)\s*:\s*([^\n]*)$", ct[3], flags=re.M)
                                if "?" not in sp}
                        given = set(re.findall(r"(\w+)\s*:=", args))
                        for miss in need - given:
                            add(2, f"{d.name}.{tn} calls {tgt}.{tn2} without required '{miss}'", ln)
                    else:
                        add(19, f"{d.name}.{tn} calls undeclared transition {tgt}.{tn2}", ln)
        # 13 — only via: the named parents must exist, and must reach this transition
        for kind, tn, head, body, ln in d.trans:
            for parent in re.findall(r"only via ([\w.,\s]+?)(?:\{|$)", head):
                for ty, tr in re.findall(r"(\w+)\.(\w+)", parent):
                    t = by_name.get(ty)
                    if t is None:
                        add(13, f"{d.name}.{tn} names unknown parent type {ty}", ln); continue
                    pt = next((x for x in t.trans if x[1] == tr), None)
                    if pt is None and t.machine:
                        pm = by_name.get(t.machine)
                        if pm: pt = next((x for x in pm.trans if x[1] == tr), None)
                    trig = r"cascade on\s+(?:\{[^}]*\b" + tr + r"\b[^}]*\}|" + tr + r"\b)"
                    cascades = re.search(trig + r"[^\n]*?to\s+" + d.name + r"\." + tn + r"\b",
                                         "\n".join(v[1] for v in t.rels.values()))
                    if pt is None and not cascades:
                        add(13, f"{d.name}.{tn} names parent {ty}.{tr}, which does not exist", ln); continue
                    reaches = cascades or (pt and re.search(
                        rf"\b(?:call|create)\s+(?:[\w.$]+\.)?{tn}\s*\(", pt[3]))
                    if not reaches:
                        add(13, f"{d.name}.{tn} names parent {ty}.{tr}, which never reaches it", ln)

        # 35 — machine requires completeness
        if d.kind == "machine":
            req = {n for _, n, _ in d.requires}
            for kind, tn, head, body, ln in d.trans:
                names = set(re.findall(r"\bset\s+(\w+)\s*:=", body)) | \
                        {x for g in re.findall(r"accepts\s+([\w,\s]+)", head) for x in re.split(r"[,\s]+", g) if x} | \
                        set(re.findall(r"actor\.\w+\((\w+)\)", body))
                for n in names:
                    if n and n not in req and n not in capdecl and n != "state":
                        add(35, f"{d.name}.{tn} names '{n}', which the machine does not require", ln)
    return out


def line_checks(text, base, capdecl, reserved, machine_caps):
    out = []
    for i, l in enumerate(text.split("\n")):
        ln, code = base + i, l.strip().split("#")[0]
        if re.search(r"(==|!=)\s*null|null\s*(==|!=)", code): out.append((21, "comparison against a bare null", ln))
        if re.search(r"\b(count|sum|all|any|none|min|max)\(\s*(?!\w+\s+in\b)[a-z_]+\s+where", code):
            out.append((21, f"aggregate without a binder: {code[:48]}", ln))
        if "$" in code: out.append((21, f"'$' input prefix: {code[:48]}", ln))
        if re.search(r"\bfor\s+\w+\s+in\b", code) and "limit" not in code:
            out.append((21, f"for without limit: {code[:48]}", ln))
        if re.match(r"^\s*do\s+\w+\s+at\s", code): out.append((21, f"'do' with 'at': {code[:48]}", ln))
        for cap in re.findall(r"actor\.\w+\((\w+)\)", code):
            if cap not in capdecl and cap not in machine_caps:
                out.append((19, f"capability {cap} used but not declared", ln))
    return out


# ── fixtures: every claimed check must fire on one of these ─────────────────
FIXTURES = {
  2:  "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { create B.mk2() }\n}\ntype B version 1 {\n tracking serial\n states T category live, U category closed terminal\n create mk2 -> T { input amount : int\n }\n do take T -> U { }\n}",
  8:  ["type W version 1 {\n tracking serial\n states S category live, D category closed terminal\n part p : C inverse w\n create mk -> S { }\n do go S -> D { }\n}",
       "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n attr name string\n create mk -> S { }\n do go S -> D { }\n}"],
  11: "type P version 1 {\n tracking serial\n states S category live, D category closed terminal\n owner w : W inverse parts\n create mk -> S { set w := inputs.w }\n do go S -> D { }\n}",
  15: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go D -> S { }\n}",
  16: "type A version 1 {\n tracking serial\n create mk -> S { }\n}",
  17: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { set other.x := 1 }\n}",
  18: "type P version 1 {\n tracking serial\n states S category live, D category closed terminal\n owner w : W inverse parts\n create mk -> S only via W.add { }\n do go S -> D { }\n}",
  19: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> NOWHERE { }\n}",
  20: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { require actor.has(X) }\n}",
  21: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { require n: x == null }\n}",
  26: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { supersede this }\n}",
  29: "machine M version 1 {\n state S category live\n state D category closed terminal\n assert fix -> { S } { }\n}",
  31: "type A version 1 {\n tracking quantity\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n}",
  34: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n do go S -> D { }\n}",
  35: "machine M version 1 {\n state S category live\n state D category closed terminal\n create mk -> S { }\n do go S -> D { set mystery := 1 }\n}",
  13: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D only via B.nope { }\n}",
  38: "machine M version 1 {\n state S category live\n state D category closed terminal\n assert fix -> { S } { input reason : string\n require may: actor.has(Q) because delegable\n may admit inv }\n}",
  42: ["enum E { A, B }",
       "type A version 1 {\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n}"],
  43: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n}\ntype B extends A version 1 {\n tracking serial\n states T category live, U category closed terminal\n create mk2 -> T { }\n do go2 T -> U { }\n}",
  51: ["type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n act poke at S { require may: actor.has(X) because delegable\n   set n := 1 }\n}",
       "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D {\n require g: a == 1\n and b == 2\n }\n}"],
  47: "type W version 1 {\n tracking serial\n states S category live, D category closed terminal\n part ps : C[] inverse w\n      cascade on go to C.del\n create mk -> S { }\n do go S -> D { }\n}",
  41: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n ref bs : B[] inverse as\n create mk -> S { }\n do go S -> D { }\n}\ntype B version 1 {\n tracking serial\n states T category live, U category closed terminal\n ref as : A[] inverse bs\n create mk2 -> T { }\n do go2 T -> U { }\n}",
  40: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n act poke at D { }\n}",
}


def doc_checks(src):
    """Checks over the document itself rather than over a declaration."""
    out = []
    H1, H10 = "## 1. Shape of a file", "## 10. What the checker verifies"
    if H1 not in src or H10 not in src:
        return out
    a10, b10 = src.index(H1), src.index(H10)
    body = re.sub(r"```text\n.*?```",
                  lambda m: "\n" * m.group(0).count("\n"), src[a10:b10], flags=re.S)
    norm = re.compile(
        r"\b(is a publish error|are publish errors|is rejected|are rejected"
        r"|[Pp]ublishing rejects|[Pp]ublishing enforces|[Pp]ublishing verifies"
        r"|[Pp]ublishing checks|which publishing checks|is an error|are both rejected"
        r"|is mandatory|may not be|may not name|may not call|may not read|may not traverse"
        r"|may declare no|must be present|must reach|must be marked|must be declared"
        r"|is not allowed|is not permitted|What is not allowed|usable \*\*only|usable only)\b")
    def sentence(text, i, j):
        """The sentence around [i, j): bounded by a newline, a table cell, or '. '."""
        a = max((text.rfind(x, 0, i) for x in ("\n", " | ")), default=-1)
        for m2 in re.finditer(r"\.\s+(?=[A-Z\u2014`*])", text[:i]):
            a = max(a, m2.end())
        ends = [x for x in (text.find("\n", j), text.find(" | ", j)) if x != -1]
        m3 = re.search(r"\.\s+(?=[A-Z\u2014`*])|\.$", text[j:])
        if m3: ends.append(j + m3.end())
        b = min(ends) if ends else len(text)
        return text[max(a, 0):b]

    for m in norm.finditer(body):
        win = sentence(body, m.start(), m.end())
        if not re.search(r"check \d+|checks \d+|§10", win):
            ln = src[:a10 + m.start()].count("\n") + 1
            quote = re.sub(r"\s+", " ", body[max(0, m.start() - 60):m.end() + 30]).strip()
            out.append((52, f"normative statement cites no check: \u2026{quote}\u2026", ln))
    return out


DOC_FIXTURES = {
  52: "## 1. Shape of a file\n\nA duplicate name is rejected.\n\n## 10. What the checker verifies\n",
}


def self_test():
    bad = []
    n = 0
    for check, fixtures in sorted(FIXTURES.items()):
        for fixture in (fixtures if isinstance(fixtures, list) else [fixtures]):
            n += 1
            found = {c for c, _, _ in analyse(fixture, 0, {"X", "Q"}, {"live", "closed"}, set())}
            found |= {c for c, _, _ in line_checks(fixture, 0, {"X", "Q"}, set(), set())}
            if check not in found:
                bad.append((check, sorted(found)))
    for check, fixture in sorted(DOC_FIXTURES.items()):
        n += 1
        if check not in {c for c, _, _ in doc_checks(fixture)}:
            bad.append((check, []))
    for c, got in bad:
        print(f"  FIXTURE FAILS: check {c} never fired (got {got})")
    print(f"self-test: {n - len(bad)}/{n} fixtures fire their check, over {len(FIXTURES) + len(DOC_FIXTURES)} checks")
    return not bad


def main():
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    src = DOC.read_text()
    blocks = [(src[: m.start()].count("\n") + 2, m.group(1))
              for m in re.finditer(r"```text\n(.*?)```", src, flags=re.S)]
    capdecl, catdecl = set(), set()
    for m in re.finditer(r"^capability\s+((?:.+\n?)+?)(?=\n[a-z]|\n\n)", src, flags=re.M):
        capdecl |= set(re.findall(r"[A-Z][A-Z_0-9]*", m.group(1)))
    for m in re.finditer(r"^category\s+(.+)$", src, flags=re.M):
        catdecl |= {c.strip() for c in m.group(1).split(",")}
    reserved = set()
    if rw := re.search(r"\n`(module use .+?)`\n", src, flags=re.S):
        reserved = set(rw.group(1).split())
    machine_caps = {w for m in re.findall(r"^\s*requires capability ([^\n]+)$", src, flags=re.M) for w in re.findall(r"[A-Z][A-Z_0-9]*", m)}

    world = {}
    for bstart, blk in blocks:
        for d in parse(blk, bstart):
            world[d.name] = d
    findings, ndecl = [], len(world)
    for bstart, blk in blocks:
        findings += analyse(blk, bstart, capdecl, catdecl, reserved, world)
        findings += line_checks(blk, bstart, capdecl, reserved, machine_caps)

    if rw:
        w = rw.group(1).split()
        if dups := {x for x in w if w.count(x) > 1}: findings.append((21, f"duplicate reserved words {sorted(dups)}", 1))
        for kw in ("provides", "requires", "cascade", "accepts", "corrects", "inputs"):
            if re.search(rf"(^|\s|`){kw}\b", src) and kw not in w:
                findings.append((21, f"keyword '{kw}' used but not reserved", 1))

    findings += doc_checks(src)

    sec = src[src.index("## 10. What the checker verifies"):] if "## 10. What the checker verifies" in src else src
    nums = [int(x) for x in re.findall(r"^\| (\d+) \|", sec, flags=re.M)]
    if nums != sorted(nums): findings.append((0, f"check table misordered: {nums}", 1))
    ok = self_test()
    covered = sorted(set(FIXTURES) | set(DOC_FIXTURES))
    ndefined = max(nums) if nums else 0
    print(f"{DOC.name}: {len(blocks)} blocks, {ndecl} declarations")
    print(f"partially enforces {len(covered)} of {ndefined} defined checks: {covered}")
    print("each is proven by a fixture; most implement one clause of a multi-clause check, so this is a floor, not coverage")
    seen, uniq = set(), []
    for c, d, ln in sorted(findings, key=lambda f: (f[2], f[0])):
        if (c, d) in seen: continue
        seen.add((c, d)); uniq.append((c, d, ln))
    if not uniq and ok:
        print("clean"); sys.exit(0)
    for c, d, ln in uniq:
        print(f"  check{c:<3} line {ln:>4}  {d}")
    print(f"{len(uniq)} finding(s)")
    sys.exit(1)


main()
