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

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs/design/declaration-syntax.md"


def target():
    """The document to check: the specification, or a path given on the command line."""
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    return pathlib.Path(args[0]) if args else SPEC

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
        self.req_spec = {}      # a machine's required member -> (kind, text after `requires <kind>`)
        self.provides = set()
        self.invariants = set()
        self.derives = set()
        self.base = None
        self.mirror = False
        self.subject = None     # an observation kind's `on` type
        self.collection = None  # an observation kind's `as` name, a part on the subject
        self.clauses = {}       # observation / metric clause keyword -> [(text, line)]
        self.generated = []     # an observation kind's generated `record` creation


def parse(text, base=0):
    decls, cur, lines = [], None, text.split("\n")
    i = 0
    while i < len(lines):
        raw, ln = lines[i], base + i
        if m := re.match(r"^(observation|metric)\s+(\w+)(.*)$", raw):
            if "…" in raw:                     # elided placeholder
                cur = None; i += 1; continue
            cur = Decl(m.group(1), m.group(2), ln)
            on = re.search(r"\bon\s+(\w+)", m.group(3))
            cur.subject = on.group(1) if on else None
            coll = re.search(r"\bas\s+(\w+)", m.group(3))
            cur.collection = coll.group(1) if coll else None
            decls.append(cur); i += 1; continue
        if m := re.match(r"^(machine|type)\s+(\w+)", raw):
            xbase = re.search(r"\bextends\s+(\w+)", raw)
            if "…" in raw:                     # elided placeholder
                cur = None; i += 1; continue
            cur = Decl(m.group(1), m.group(2), ln)
            cur.abstract = " abstract" in raw
            cur.mirror = " mirror" in raw
            cur.base = xbase.group(1) if xbase else None
            decls.append(cur); i += 1; continue
        if cur is None:
            i += 1; continue
        s = raw.strip()
        if cur.kind in ("observation", "metric"):       # §6.8, §6.9: clauses, not members
            if m := re.match(r"^field\s+(\w+)\s*:\s*(.*)$", s):
                cur.attrs[m.group(1)] = (m.group(2), ln)
            elif m := re.match(r"^(recorded by|occurred within|invariant|from|combine|by|window on|value|flag)\b\s*(.*)$", s):
                cur.clauses.setdefault(m.group(1), []).append((m.group(2), ln))
            i += 1; continue
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
                if kind != "capability": cur.req_spec[nm] = (kind, rest)
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
    for d in decls:                              # §6.8: the creation a kind expands into
        if d.kind == "observation":
            body = "".join(f"\n  input {n} : {sp.split()[0] if sp.split() else ''}"
                           for n, (sp, _l) in d.attrs.items())
            body += (f"\n  input subject : {d.subject}\n  input corrects : {d.name}?"
                     "\n  input occurred_at : timestamp?")
            d.generated = [("create", "record", "-> RECORDED", body, d.start)]
    return decls


def analyse(text, base=0, capdecl=None, catdecl=None, reserved=None, world=None):
    """Return findings as (check_number, detail, line)."""
    out = []
    def add(c, d, l): out.append((c, d, l))
    decls = parse(text, base)
    by_name = dict(world or {})
    by_name.update({d.name: d for d in decls})
    for k in list(by_name.values()):          # §6.8: a kind adds its collection to the subject
        subj = by_name.get(k.subject) if k.kind == "observation" else None
        if subj is not None and k.collection and k.collection not in subj.rels:
            subj.rels[k.collection] = ("part", f"{k.name}[] inverse subject", k.start)
            subj.generated_parts = getattr(subj, "generated_parts", set()) | {k.collection}
    capdecl = capdecl if capdecl is not None else set()
    catdecl = catdecl if catdecl is not None else set()
    reserved = reserved or set()

    # 51 — the continuation rule of §9.1
    STARTS = (r"^(module|use|capability|category|enum|sequence|evaluator|machine|type|tracking|"
              r"states|state|provides|summary|visible|attr|counter|ref|part|owner|derive|invariant|"
              r"create|do|act|assert|erase|input|accepts|require|set|clear|add|remove|call|supersede|for|"
              r"cascade|survives|requires|removed|renamed|fn|extends|may|corrects|only|proposable|"
              r"observation|metric|field|recorded|occurred|from|combine|by|window|value|flag|labels|"
              r"backfill|admit|requests)\b")
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
        m = re.match(r"^(enum|sequence|evaluator|machine|type|observation|metric)\s+(\w+)(.*)$", raw)
        if m and "…" not in raw and not re.search(r"\bversion\s+\d", m.group(3)):
            out.append((42, f"{m.group(1)} {m.group(2)} has no version", base + i))

    for d in decls:
        mach = by_name.get(d.machine) if d.machine else None
        mtrans = mach.trans if mach else []
        replaced = []
        if any(t[0] == "create" for t in d.trans):        # §2.1: a binder's creations replace
            replaced = [t for t in mtrans if t[0] == "create"]
            mtrans = [t for t in mtrans if t[0] != "create"]
        trans = d.trans + mtrans
        # a state the replaced creation used to reach is reported, not failed (ADR-0064)
        orphaned = set()
        for _k, _tn, _h, _b, _l in replaced:
            orphaned |= {x for x in re.findall(r"->\s*(\w+)", _h.split("{")[0])}
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
            if not d.mirror and not any(t[0] == "create" for t in trans):
                add(34, f"{d.name} has no creation transition", d.start)
            if d.tracking == "quantity" and not d.counters:
                add(31, f"{d.name} is tracking quantity with no counter", d.start)
            if d.tracking in ("serial", "record") and d.counters:
                add(31, f"{d.name} is tracking {d.tracking} with a counter", d.start)

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
                if re.search(r"^\s*any\s*->", head):
                    froms |= {k for k, v in states.items() if "terminal" not in v[0]}
                if re.search(r"\bat\s+any\b", head):
                    ats |= {k for k, v in states.items() if "terminal" not in v[0]}
                for x in froms | tos | ats:
                    if x not in states and x.lower() != "any":
                        add(19, f"{d.name}.{tn} names undeclared state {x}", ln)
                # check 15: an act is a self-transition, so it neither leaves nor
                # reaches a state; an assertion's targets are not reached by the flow
                if kind == "do":
                    out_s |= {x for x in froms if x in states}
                if kind in ("do", "create"):
                    in_s |= {x for x in tos if x in states}
                for a in ats:
                    if a in states:
                        if "terminal" in states[a][0]:
                            add(40, f"{d.name}.{tn} is an act at terminal state {a}", ln)
                for f in froms:
                    if f in states and "terminal" in states[f][0] and kind == "do":
                        add(15, f"{d.name}.{tn} is a do leaving terminal state {f}", ln)
            for s_, (mods, ln) in states.items():
                if "category" not in mods: add(15, f"{d.name}.{s_} has no category", ln)
                if d.mirror: continue
                for c in re.findall(r"category (\w+)", mods):
                    if catdecl and c not in catdecl: add(19, f"category {c} not declared", ln)
                # a closed state is finished work, and may end a lifecycle (ADR-0103 §8)
                if "terminal" not in mods and "category closed" not in mods and s_ not in out_s:
                    add(15, f"{d.name}.{s_} is non-terminal with no outgoing transition", ln)
                if s_ not in in_s and s_ not in orphaned:
                    add(15, f"{d.name}.{s_} is reachable by nothing", ln)
            if not d.mirror and not any("terminal" in v[0] for v in states.values()):
                add(15, f"{d.name} has no terminal state", d.start)

        # 53 — nothing here may write a type another system owns
        if d.mirror:
            if d.machine:
                add(53, f"{d.name} is a mirror and binds machine {d.machine}, "
                        "whose transitions would make it writable", d.start)
            writes = [t for t in d.trans if t[0] != "erase"]      # erase is allowed (ADR-0101)
            if writes:
                add(53, f"{d.name} is a mirror and declares {len(writes)} transition(s) other than erase",
                    d.start)
            base53 = by_name.get(d.base) if d.base else None
            if base53 is not None and not base53.mirror:    # mirrors may extend mirrors (ADR-0101)
                add(53, f"{d.name} is a mirror and extends {d.base}, which this store owns", d.start)
            for rn, (rk, rspec, rln) in d.rels.items():
                if rk in ("part", "owner") and rspec.split():
                    far = by_name.get(rspec.split()[0].rstrip("?[]"))
                    if far is not None and far.kind == "type" and not far.mirror:
                        add(53, f"{d.name}.{rn} is a mirror composing with {far.name}, "
                                "which this store owns", rln)
        if not d.mirror and d.base and getattr(by_name.get(d.base), "mirror", False):
            add(53, f"{d.name} extends mirror {d.base}, so a type this store owns "
                    "would inherit the shape of one it does not", d.start)
        if not d.mirror:
            for kind, tn, head, body, ln in (trans if d.kind == "type" else d.trans):
                # an input or a relationship end resolves to the type it names
                local = {n: t.rstrip("?[]") for n, t in
                         re.findall(r"^\s*input\s+(\w+)\s*:\s*(\S+)", body, flags=re.M)}
                local.update({n: v[1].split()[0].rstrip("?[]")
                              for n, v in rels.items() if v[1].split()})
                for m53 in re.finditer(r"\b(?:create|call)\s+(?:\w+\s*=\s*)?([\w.]+)\.(\w+)\s*\(", body):
                    root = m53.group(1).split(".")[-1]
                    tgt = by_name.get(local.get(root, root)) or by_name.get(m53.group(1))
                    if tgt is not None and getattr(tgt, "mirror", False):
                        add(53, f"{d.name}.{tn} writes {tgt.name}, which is a mirror "
                                "owned by another system", ln)
            for rn, (rk, rspec, rln) in d.rels.items():
                if rk in ("part", "owner") and rspec.split():
                    far = by_name.get(rspec.split()[0].rstrip("?[]"))
                    if far is not None and getattr(far, "mirror", False):
                        add(53, f"{d.name}.{rn} composes with mirror {far.name}, "
                                "whose lifetime this store does not own", rln)

        # 42 — tracking present
        if d.kind == "type" and not d.abstract:
            tr, anc42, seen42 = d.tracking, d, set()
            while tr is None and anc42 is not None and anc42.base and anc42.base not in seen42:
                seen42.add(anc42.base); anc42 = by_name.get(anc42.base)
                tr = anc42.tracking if anc42 else None
            if tr not in ("serial", "quantity", "record"):
                add(42, f"{d.name} has tracking {tr!r}, declared or inherited, "
                        "which is not serial, quantity or record", d.start)

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

        # 7 — an `indexed` marking on a stored relationship end, which already is one
        for rn, (rk, rspec, rln) in d.rels.items():
            toks = rspec.split()
            if not toks or "indexed" not in toks: continue
            if rk == "owner":
                stored = True
            elif rk == "ref":
                im7 = re.search(r"\binverse\s+(\w+)", rspec)
                if "[]" in toks[0]: stored = False            # a set end never stores
                elif not im7 or "stored" in toks: stored = True  # no inverse, or marked
                else:
                    far7 = by_name.get(toks[0].rstrip("?[]"))
                    fe7 = far7.rels.get(im7.group(1)) if far7 else None
                    stored = bool(fe7 and fe7[1].split() and "[]" in fe7[1].split()[0])
            else:
                stored = False                                   # a part end is derived
            if stored:
                add(7, f"{d.name}.{rn} is a stored relationship end and is marked indexed, which it already is", rln)

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
                rm = re.search(r"input\s+reason\s*:\s*(\w+)", body)
                if rm and rm.group(1) in SCALARS:
                    # an override's reason is a declared enum, so overrides count per reason (ADR-0106)
                    add(29, f"{d.name}.{tn} asserts with a {rm.group(1)} reason, not a declared enum", ln)
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
            for tgt in re.findall(r"^\s*clear\s+([\w.]+)\s*$", body, flags=re.M):
                if "." in tgt:
                    add(17, f"{d.name}.{tn} clears through a path '{tgt}'", ln)
                elif tgt in d.counters:
                    add(17, f"{d.name}.{tn} clears counter '{tgt}'", ln)
                elif tgt in rels:
                    rk17, sp17, _l17 = rels[tgt]
                    first = sp17.split()[0] if sp17.split() else ""
                    if not (rk17 == "ref" and first.endswith("?") and stores(d, sp17, by_name)):
                        add(17, f"{d.name}.{tn} clears relationship end '{tgt}', which is not "
                                "an optional, singular, stored ref", ln)
                elif tgt in d.req_spec:
                    # a machine clears what it requires; each binder's end is checked when
                    # the machine's transitions are analysed as the binder's
                    rk17, rs17 = d.req_spec[tgt]
                    ty17 = rs17.split(":", 1)[1].split() if rk17 != "attr" else rs17.split()[1:]
                    t17 = ty17[0] if ty17 else ""
                    if rk17 not in ("ref", "attr") or not t17.endswith("?") or "[]" in t17:
                        add(17, f"{d.name}.{tn} clears required '{tgt}', which is not an optional, "
                                "singular ref or attribute", ln)
                elif tgt not in attrs:
                    add(19, f"{d.name}.{tn} clears undeclared name '{tgt}'", ln)
                elif not attrs[tgt][0].split()[0].endswith("?"):
                    add(17, f"{d.name}.{tn} clears required attribute '{tgt}'", ln)
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
                    ct = next((t for t in callee.trans + callee.generated if t[1] == tn2), None)
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
    out += data_checks(decls, by_name, text)
    return out


UNITS = r"(?:s|min|h|days|weeks)"
GENERATED = ("subject", "corrects", "occurred_at", "recorded_at", "recorded_by_kind",
             "created_at", "created_by_kind")
SOURCES = ("labels", "intervals", "transitions", "attempts", "attempt_counts")
# the built-in scalar types of §3.1: a reason typed as one of these is not an enum (check 29)
SCALARS = ("string", "bool", "int", "decimal", "money", "timestamp", "duration", "identity", "file", "event")


def stores(d, spec, by_name):
    """Whether a singular `ref` end stores its value (§3.3), as check 7 decides it."""
    toks = spec.split()
    if not toks or "[]" in toks[0]:
        return False
    im = re.search(r"\binverse\s+(\w+)", spec)
    if not im or "stored" in toks:
        return True
    far = by_name.get(toks[0].rstrip("?[]"))
    fe = far.rels.get(im.group(1)) if far else None
    return bool(fe and fe[1].split() and "[]" in fe[1].split()[0])


def data_checks(decls, by_name, text):
    """Checks 54 to 61: the constructs of ADR-0082 to ADR-0087 and ADR-0092."""
    out = []
    def add(c, d, l): out.append((c, d, l))
    evaluators = set(re.findall(r"^evaluator\s+(\w+)", text, flags=re.M)) | \
        {n for n, x in by_name.items() if x.kind == "evaluator"}
    everything = list(by_name.values())

    # 54 — observation kinds
    for d in decls:
        if d.kind != "observation":
            continue
        if "recorded by" not in d.clauses:
            add(54, f"observation {d.name} has no 'recorded by'", d.start)
        for fn, (spec, fln) in d.attrs.items():
            toks = spec.split()
            if fn in GENERATED:
                add(54, f"{d.name}.{fn} is named as a member every observation already has", fln)
            if "unit" in toks and not (toks and re.match(r"(int|decimal)\b", toks[0])):
                add(54, f"{d.name}.{fn} declares a unit and is not int or decimal", fln)
        for inv, iln in d.clauses.get("invariant", []):
            expr = inv.split(":", 1)[1] if ":" in inv else inv
            if re.search(r"\b(subject|actor|inputs|now|this|referrers)\b|\b[a-z_]\w*\.\w", expr):
                add(54, f"{d.name}: invariant reads beyond its own fields: {inv.strip()[:48]}", iln)
        subj = by_name.get(d.subject) if d.subject else None
        if d.subject is None:
            add(54, f"observation {d.name} names no subject with 'on'", d.start)
        elif subj is None:
            add(19, f"observation {d.name} is on undeclared type {d.subject}", d.start)
        if d.collection is None:
            add(54, f"observation {d.name} names no collection with 'as'", d.start)
    for d in decls:
        for rn, (rk, sp, rln) in d.rels.items():
            if rk != "part" or not sp.split() or rn in getattr(d, "generated_parts", set()):
                continue
            kind = by_name.get(sp.split()[0].rstrip("?[]"))
            if kind is not None and kind.kind == "observation":
                add(54, f"{d.name}.{rn} declares a part of observation {kind.name}, "
                        "which the kind's 'as' adds; the subject does not declare it", rln)

    # 56 — metrics; 58 — the members they read
    for d in decls:
        if d.kind != "metric":
            continue
        if "from" not in d.clauses and "combine" not in d.clauses:
            add(56, f"metric {d.name} has neither 'from' nor 'combine'", d.start)
        if "value" not in d.clauses:
            add(56, f"metric {d.name} has no 'value'", d.start)
        src_decl, binder, dataset = None, None, False
        for fr, fln in d.clauses.get("from", []):
            m = re.match(r"(\w+)\s+in\s+(\w+)(?:\.(\w+)(?:\((\w+)\))?)?", fr)
            if not m:
                add(56, f"metric {d.name}: 'from' is not '<binder> in <source>'", fln); continue
            binder, base_, suffix, member = m.groups()
            src_decl = by_name.get(base_)
            if suffix is not None and suffix not in SOURCES:
                add(56, f"metric {d.name} reads {base_}.{suffix}, which is not a source", fln)
            if src_decl is None:
                add(19, f"metric {d.name} reads undeclared {base_}", fln)
            elif suffix is None and src_decl.kind not in ("type", "observation"):
                add(56, f"metric {d.name} reads {base_}, which is neither a type nor an observation kind", fln)
            if member and src_decl is not None and not tracked(src_decl, member, by_name):
                add(58, f"metric {d.name} reads intervals of {base_}.{member}, which is not tracked", fln)
            if suffix is not None:
                src_decl = None                  # a dataset's rows are not the type's members
                dataset = True
        for dims, bln in d.clauses.get("by", []):
            for dim in dims.split(","):
                expr = dim.split("=", 1)[1] if "=" in dim else dim
                for path in re.findall(rf"\b{binder}((?:\.\w+)+)", expr) if binder else []:
                    # a dataset row's `.object` is the row's object, not a hop (§6.9)
                    hops = path.count(".") - (1 if dataset and path.startswith(".object") else 0)
                    if hops > 2:
                        add(56, f"metric {d.name}: dimension '{dim.strip()}' is {hops} hops", bln)
                    first = path.split(".")[1]
                    if src_decl is not None and "personal" in src_decl.attrs.get(first, ("", 0))[0].split():
                        add(56, f"metric {d.name}: personal member '{first}' is a dimension", bln)
        # a metric reads no personal value anywhere: filter, value or flag (ADR-0106)
        if src_decl is not None and binder:
            for key in ("from", "value", "flag"):
                for text_, tln in d.clauses.get(key, []):
                    for first in re.findall(rf"\b{binder}\.(\w+)", text_):
                        if "personal" in src_decl.attrs.get(first, ("", 0))[0].split():
                            add(56, f"metric {d.name}: reads personal member '{first}' in its {key}", tln)

    # per-transition: 57, 58, 61, and a metric reference's window and dimensions (56)
    metrics = {x.name: x for x in everything if x.kind == "metric"}
    for d in decls:
        mach = by_name.get(d.machine) if d.machine else None
        states = d.states or (mach.states if mach else {})
        for kind, tn, head, body, ln in d.trans:
            reqs = require_clauses(body)
            if kind in ("assert", "erase") and any(re.search(r"\bobserve\b", r) for r in reqs):
                add(57, f"{d.name}.{tn} is an {kind} with an observing clause", ln)
            if "backdatable" in head:
                if not re.search(rf"\bbackdatable\s+within\s+\d+\s*{UNITS}\b", head):
                    add(58, f"{d.name}.{tn} is backdatable without a duration in s, min, h, days or weeks", ln)
                if kind in ("assert", "erase") or "only via" in head:
                    add(58, f"{d.name}.{tn} is backdatable and is {'only via' if 'only via' in head else 'an ' + kind}", ln)
            for x in re.findall(r"\btime_in\(\s*(\w+)\s*\)", body):
                if x not in states:
                    add(58, f"{d.name}.{tn}: time_in({x}) names no state", ln)
            for x in re.findall(r"\bentered_at\(\s*(\w+)\s*\)", body):
                if x not in states and not tracked(d, x, by_name):
                    add(58, f"{d.name}.{tn}: entered_at({x}) names neither a state nor a tracked member", ln)
            for r in reqs:
                for mn, margs in re.findall(r"\bmetric\(\s*(\w+)\s*((?:,[^()]*)?)\)", r):
                    md = metrics.get(mn)
                    if md is None:
                        add(19, f"{d.name}.{tn} reads undeclared metric {mn}", ln); continue
                    if "over last" in margs and "window on" not in md.clauses:
                        add(56, f"{d.name}.{tn}: 'over last' on metric {mn}, which declares no 'window on'", ln)
                    dims = {x.split("=")[0].strip() for c, _l in md.clauses.get("by", []) for x in c.split(",")}
                    for bound in re.findall(r"(\w+)\s*:=", margs):
                        if bound not in dims:
                            add(56, f"{d.name}.{tn} binds '{bound}', not a dimension of metric {mn}", ln)
            # 61 — an argument the calling outcome writes
            written = set(re.findall(r"^\s*set\s+(\w+)\s*:=", body, flags=re.M))
            for tgt, tn2, args in re.findall(r"\b(?:call|create)\s+([\w.$]+)\.(\w+)\(([^)]*)\)", body):
                callee = by_name.get(tgt) or by_name.get(tgt.split(".")[-1])
                ct = next((t for t in (callee.trans + callee.generated if callee else [])
                           if t[1] == tn2), None)
                if ct is None:
                    continue
                for an, aexpr in re.findall(r"(\w+)\s*:=\s*([^,]+)", args):
                    if not any(re.search(rf"(?<![\w.])(?:this\.)?{w}\b", aexpr) for w in written):
                        continue
                    for r in re.findall(r"^\s*require\s+([^\n]*)$", ct[3], flags=re.M):
                        calls = [a for e, a in re.findall(r"\b(\w+)\.\w+\(([^)]*)\)", r) if e in evaluators]
                        calls += re.findall(r"\bmetric\(([^)]*)\)", r)
                        if any(re.search(rf"\binputs\.{an}\b", a) for a in calls):
                            add(61, f"{d.name}.{tn} binds {tgt}.{tn2}'s '{an}' from a value its outcome "
                                    "writes, which an evaluator or metric guard reads", ln)

    # 58 — an observation kind's bound
    for d in decls:
        for dur, oln in d.clauses.get("occurred within", []):
            if not re.fullmatch(rf"\d+\s*{UNITS}", dur.strip()):
                add(58, f"{d.name}: occurred within '{dur.strip()}' is not in s, min, h, days or weeks", oln)

    # 59 — assignee and the actor identity
    for d in decls:
        for an, (spec, aln) in d.attrs.items():
            toks = spec.split()
            if "actor" in toks[1:] and toks[0].rstrip("?") != "identity":
                add(59, f"{d.name}.{an} is marked actor and is not an identity", aln)
        for rn, (rk, spec, rln) in d.rels.items():
            if "assignee" not in spec.split():
                continue
            if rk != "ref" or not stores(d, spec, by_name):
                add(59, f"{d.name}.{rn} is an assignee and is not a singular stored ref", rln)
            tgt = by_name.get(spec.split()[0].rstrip("?[]"))
            if tgt is not None:
                ids = [a for a, (sp, _l) in tgt.attrs.items()
                       if sp.split() and sp.split()[0].rstrip("?") == "identity" and "actor" in sp.split()[1:]]
                if len(ids) != 1:
                    add(59, f"{d.name}.{rn} is an assignee; {tgt.name} marks {len(ids)} actor identities, not one", rln)

    # 60 — erasure follows the supersession chain (ADR-0087)
    superseded = set()
    for x in everything:
        for _k, _tn, _h, b, _l in x.trans:
            for op in re.findall(r"^\s*supersede\s+inputs\.(\w+)", b, flags=re.M):
                if im := re.search(rf"^\s*input\s+{op}\s*:\s*(\w+)", b, flags=re.M):
                    superseded.add(im.group(1))
    for d in decls:
        if d.kind != "type":
            continue
        mach = by_name.get(d.machine) if d.machine else None
        states = d.states or (mach.states if mach else {})
        attrs, anc, seen = dict(d.attrs), by_name.get(d.base), set()
        while anc and anc.name not in seen:
            seen.add(anc.name)
            for k, v in anc.attrs.items(): attrs.setdefault(k, v)
            anc = by_name.get(anc.base)
        personal = [a for a, (sp, _l) in attrs.items() if "personal" in sp.split()]
        chained = d.name in superseded or any("superseding" in v[0] for v in states.values())
        erases = any(t[0] == "erase" for t in d.trans + (mach.trans if mach else []))
        if personal and chained and not erases:
            add(60, f"{d.name} holds personal {personal[0]!r}, takes part in supersession and declares no erase", d.start)
    return out


def tracked(d, member, by_name):
    """An enum attribute or a singular stored relationship end (ADR-0083)."""
    if member in d.attrs:
        toks = d.attrs[member][0].split()
        return bool(toks) and toks[0][:1].isupper() and "[]" not in toks[0]
    if member in d.rels:
        rk, spec, _l = d.rels[member]
        return rk in ("ref", "owner") and (rk == "owner" or stores(d, spec, by_name))
    return False


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
        # 19 — the closed vocabularies of a request rule (ADR-0103)
        if (m := re.match(r"^\s*requests\s+by\s+(.+?)\s+require\s+(.+)$", code)) and not re.search(r"[<…]", code):
            for k in re.split(r"\s*,\s*", m.group(1).strip()):
                if k not in ("human", "agent", "service"):
                    out.append((19, f"requests by names unknown actor kind '{k}'", ln))
            for f in re.split(r"\s*,\s*", m.group(2).strip()):
                if f not in ("version", "key"):
                    out.append((19, f"requests by requires unknown field '{f}'", ln))
        st = code.strip()
        if re.search(r"\blabels\b", st) and not (
                (st.startswith("labels by") and not re.search(r"\blabels\b", st[9:]))
                or re.match(r"^from\s+\w+\s+in\s+\w+\.labels\b", st)):
            out.append((55, f"a label read outside a metric's source: {st[:48]}", ln))
        rule = re.match(r"^(require|invariant|derive|visible|set|add|remove|value|flag|by|from|window)\b", st)
        if rule and "metric(" in st and rule.group(1) != "require":
            out.append((56, f"metric(…) outside a guard: {st[:48]}", ln))
        if rule and rule.group(1) in ("require", "invariant", "derive", "visible", "set", "add", "remove") and \
           re.search(r"\b(avg|median|percentile|day|week|month|quarter|year)\(", st):
            out.append((56, f"a metric-only function outside a metric: {st[:48]}", ln))
        for cap in re.findall(r"actor\.\w+\((\w+)\)", code):
            if cap not in capdecl and cap not in machine_caps:
                out.append((19, f"capability {cap} used but not declared", ln))
    return out


# ── fixtures: every claimed check must fire on one of these ─────────────────
FIXTURES = {
  2:  "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { create B.mk2() }\n}\ntype B version 1 {\n tracking serial\n states T category live, U category closed terminal\n create mk2 -> T { input amount : int\n }\n do take T -> U { }\n}",
  7:  "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n ref m : M indexed\n create mk -> S { }\n do go S -> D { }\n}",
  8:  ["type W version 1 {\n tracking serial\n states S category live, D category closed terminal\n part p : C inverse w\n create mk -> S { }\n do go S -> D { }\n}",
       "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n attr name string\n create mk -> S { }\n do go S -> D { }\n}"],
  11: "type P version 1 {\n tracking serial\n states S category live, D category closed terminal\n owner w : W inverse parts\n create mk -> S { set w := inputs.w }\n do go S -> D { }\n}",
  15: ["type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go D -> S { }\n}",
       "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n act poke at S { }\n}",
       "type A version 1 {\n tracking serial\n states S category live, R category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n do leave R -> D { }\n assert fix -> { R } {\n  input to : state\n  input reason : string\n  require may: actor.has(X) because delegable\n }\n}"],
  16: "type A version 1 {\n tracking serial\n create mk -> S { }\n}",
  17: ["type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { set other.x := 1 }\n}",
       "machine M version 1 {\n requires ref r : R\n requires capability E\n state S category live\n state D category closed terminal\n create mk -> S { require may: actor.has(E) because delegable }\n do go S -> D {\n  clear r\n }\n}\ntype A version 1 {\n tracking serial\n machine M\n provides capability E = X\n ref r : R\n}"],
  18: "type P version 1 {\n tracking serial\n states S category live, D category closed terminal\n owner w : W inverse parts\n create mk -> S only via W.add { }\n do go S -> D { }\n}",
  19: ["type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> NOWHERE { }\n}",
       # a state literal naming no state of its type
       "type A version 1 {\n tracking record\n states S category live, D category closed terminal\n derive stuck = state == A.NOPE\n create mk -> S { }\n do go S -> D { }\n}",
       # an unqualified state compared with the object's own state (D380)
       "type A version 1 {\n tracking record\n states S category live, D category closed terminal\n attr n string?\n invariant i: state != NOPE or n is not null\n create mk -> S { }\n do go S -> D { }\n}",
       "requests by robot require version, token"],
  20: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { require actor.has(X) }\n}",
  21: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { require n: x == null }\n}",
  26: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { supersede this }\n}",
  29: ["machine M version 1 {\n state S category live\n state D category closed terminal\n assert fix -> { S } { }\n}",
       # an override's reason must be a declared enum (ADR-0106)
       "machine M version 1 {\n state S category live\n state D category closed terminal\n do go S -> D { }\n assert fix -> { S } {\n  input to : state\n  input reason : string\n  require may: actor.has(X) because delegable\n }\n}"],
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
  53: "type M version 1 mirror {\n tracking record\n states A category live\n attr k string\n}\ntype T version 1 {\n tracking record\n states S category live, D category closed terminal\n part ms : M[] inverse t\n create mk -> S { }\n do go S -> D { }\n}",
  47: "type W version 1 {\n tracking serial\n states S category live, D category closed terminal\n part ps : C[] inverse w\n      cascade on go to C.del\n create mk -> S { }\n do go S -> D { }\n}",
  41: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n ref bs : B[] inverse as\n create mk -> S { }\n do go S -> D { }\n}\ntype B version 1 {\n tracking serial\n states T category live, U category closed terminal\n ref as : A[] inverse bs\n create mk2 -> T { }\n do go2 T -> U { }\n}",
  40: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n act poke at D { }\n}",
  54: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n}\nobservation O version 1 on A as os {\n field v : int\n}",
  55: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { require quiet: count(l in labels) == 0 }\n}",
  56: ["type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D { }\n}\nmetric m version 1 {\n from a in A\n}",
       # a metric reads no personal value, not even in its filter (ADR-0106)
       "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n attr email string? personal\n create mk -> S { }\n do go S -> D { }\n}\nmetric m version 1 {\n from a in A where a.email is not null\n value count()\n}",
       # a metric reference on a wrapped guard's continuation line is read too (§9.1)
       "type A version 1 {\n tracking record\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D {\n  require r: 1 == 1\n             or metric(m, nope := 1) > 0 because dependent\n }\n}\nmetric m version 1 {\n from a in A\n by k = a.state\n value count()\n}"],
  57: "machine M version 1 {\n state S category live\n state D category closed terminal\n assert fix -> { S } {\n  input reason : string\n  require may: actor.has(Q) observe because delegable\n }\n}",
  58: "type A version 1 {\n tracking serial\n states S category live, D category closed terminal\n create mk -> S { }\n do go S -> D backdatable within 2 months { }\n}",
  59: "type U version 1 {\n tracking record\n states S category live, D category closed terminal\n attr login identity\n create mk -> S accepts login { }\n do go S -> D { }\n}\ntype A version 1 {\n tracking serial\n states S category live, D category closed terminal\n ref who : U assignee\n create mk -> S accepts who { }\n do go S -> D { }\n}",
  60: "type A version 1 {\n tracking serial\n states S category live, M category closed superseding terminal, D category closed terminal\n attr email string? personal\n create mk -> S { }\n do merge S -> M {\n  input old : A\n  supersede inputs.old\n }\n do go S -> D { }\n}",
  61: "evaluator ev version 1 { … }\ntype B version 1 {\n tracking serial\n states T category live, U category closed terminal\n create mk2 -> T only via A.go {\n  input amount : int\n  require ok: ev.check(inputs.amount)\n }\n do take T -> U { }\n}\ntype A version 1 {\n tracking serial\n states S category live, D category closed terminal\n attr total int?\n create mk -> S { }\n do go S -> D {\n  set total := 1\n  create B.mk2(amount := total)\n }\n}",
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
            found |= {c for c, _, _ in state_literals(fixture, 0, {d.name: d for d in parse(fixture)})}
            if check not in found:
                bad.append((check, sorted(found)))
    for check, fixture in sorted(DOC_FIXTURES.items()):
        n += 1
        if check not in {c for c, _, _ in doc_checks(fixture)}:
            bad.append((check, []))
    for c, got in bad:
        print(f"  FIXTURE FAILS: check {c} never fired (got {got})")
    print(f"self-test: {n - len(bad)}/{n} fixtures fire their check, over {len(FIXTURES) + len(DOC_FIXTURES)} checks")
    got = creation_report(parse(REPORT_FIXTURE[0]))
    if got != REPORT_FIXTURE[1]:
        print(f"  REPORT FIXTURE FAILS: got {got}")
    return not bad and got == REPORT_FIXTURE[1]


# ── mutations: each check must catch a mistake in the document's own examples ──
# A fixture is written in the shape its check expects, so it proves only that
# the check can fire. A mutation breaks one of the specification's own examples
# the way an author would, and the check that claims the rule must catch it.
MUTATIONS = [
  (54, "no recorded by", "  recorded by   actor.has(PDI_RECORD)\n", ""),
  (54, "kind with no collection", "observation InspectionResult version 1 on ServiceJob as inspections {",
       "observation InspectionResult version 1 on ServiceJob {"),
  (54, "subject declares the kind's part", "  attr     photo file?\n",
       "  attr     photo file?\n  part     results : InspectionResult[] inverse subject\n"),
  (54, "unit on a string field", "field note    : string?", 'field note    : string? unit "V"'),
  (54, "kind invariant reads the subject", "invariant in_range: value is null",
       "invariant in_range: subject.photo is null"),
  (55, "label read by a guard", "require none_failed: none(r in inspections",
       "require none_failed: none(r in labels"),
  (56, "three-hop dimension", "by        engineer = r.subject.engineer\n",
       "by        engineer = r.subject.engineer.login.x\n"),
  (56, "unknown metric source", "from      i in ServiceJob.intervals where",
       "from      i in ServiceJob.events where"),
  (56, "metric in a derivation", "  attr     photo file?\n",
       "  attr     photo file?\n  derive   rate = metric(inspection_pass_rate)\n"),
  (57, "observe on an erase", "create add -> ACTIVE accepts login, role { require may: actor.has(SERVICE_ASSIGN) because delegable }",
       "create add -> ACTIVE accepts login, role { require may: actor.has(SERVICE_ASSIGN) because delegable }\n"
       "  erase forget {\n    input reason : string\n"
       "    require may: actor.has(ERASE_PERSONAL) observe because delegable\n  }"),
  (58, "backdated in months", "do start OPEN -> WORKING backdatable within 2 days {",
       "do start OPEN -> WORKING backdatable within 2 months {"),
  (58, "occurred within months", "occurred within 7 days", "occurred within 7 months"),
  (58, "time_in of a member", "require mine: engineer.login == actor.id because delegable\n  }\n  do finish",
       "require mine: engineer.login == actor.id because delegable\n    require slow: time_in(photo) > 1 h\n"
       "  }\n  do finish"),
  (59, "assignee target with no actor", "attr     login identity actor unique", "attr     login identity unique"),
  (59, "set-valued assignee", "ref      engineer : User assignee", "ref      engineer : User[] assignee"),
]


# A module declared in more than one document names its home here. The flow
# review's appendix re-declares inventory_journey as a proposed variant of it.
MODULE_HOMES = {"inventory_journey": "unit-journey.md"}


def with_imports(src, world):
    """Add what a document's `use` lines import to the declarations it is checked against.

    Publishing checks a module with the closure of its `use` imports (§1), so a
    name imported from another document's module resolves to that declaration.
    A type imported this way brings its machine and base with it. Imported
    declarations are read, never analysed: each is checked in its own document.
    """
    index = {}
    for p in sorted((ROOT / "docs/design").glob("*.md")):
        for m in re.finditer(r"```text\n(.*?)```", p.read_text(), flags=re.S):
            if mm := re.match(r"module\s+(\w+)", m.group(1)):
                index.setdefault(mm.group(1), [])
                if p not in index[mm.group(1)]:
                    index[mm.group(1)].append(p)
    out, merged = [], dict(world)
    for mod, names in re.findall(r"^use\s+(\w+)\.\{([^}]*)\}", src, flags=re.M):
        homes = index.get(mod, [])
        if len(homes) > 1:
            home = [p for p in homes if p.name == MODULE_HOMES.get(mod)]
            if not home:
                out.append((19, f"module {mod} is declared in {len(homes)} documents and has no home", 1))
                continue
            homes = home
        if not homes:
            continue                                  # a module the record does not write out
        theirs = {}
        text = homes[0].read_text()
        for m in re.finditer(r"```text\n(.*?)```", text, flags=re.S):
            for d in parse(m.group(1), 0):
                theirs[d.name] = d
        todo = [n.strip() for n in names.split(",") if n.strip()]
        while todo:
            n = todo.pop()
            if n in merged or n not in theirs:
                continue
            merged[n] = theirs[n]
            todo += [x for x in (theirs[n].machine, theirs[n].base) if x]
    return merged, out


def declaration_findings(src, is_spec):
    """Findings over every ```text block of a document, with the vocabularies it declares."""
    blocks = [(src[: m.start()].count("\n") + 2, m.group(1))
              for m in re.finditer(r"```text\n(.*?)```", src, flags=re.S)]
    vocab = src if is_spec else src + "\n" + SPEC.read_text()
    capdecl, catdecl = set(), set()
    for m in re.finditer(r"^capability\s+((?:.+\n?)+?)(?=\n[a-z]|\n\n)", vocab, flags=re.M):
        capdecl |= set(re.findall(r"[A-Z][A-Z_0-9]*", m.group(1)))
    for m in re.finditer(r"^category\s+(.+)$", vocab, flags=re.M):
        catdecl |= {c.strip() for c in m.group(1).split(",")}
    reserved = set()
    if rw := re.search(r"\n`(module use .+?)`\n", SPEC.read_text(), flags=re.S):
        reserved = set(rw.group(1).split())
    machine_caps = {w for m in re.findall(r"^\s*requires capability ([^\n]+)$", src, flags=re.M) for w in re.findall(r"[A-Z][A-Z_0-9]*", m)}

    world = {}
    for bstart, blk in blocks:
        for d in parse(blk, bstart):
            world[d.name] = d
    findings, ndecl = report_checks(src, list(world.values())), len(world)
    if not is_spec:
        world, imp = with_imports(src, world)
        findings += imp
    for bstart, blk in blocks:
        findings += analyse(blk, bstart, capdecl, catdecl, reserved, world)
        findings += line_checks(blk, bstart, capdecl, reserved, machine_caps)
        findings += state_literals(blk, bstart, world)
    return findings, blocks, ndecl, rw


def require_clauses(body):
    """Each `require` clause of a body with its continuation lines joined (§9.1).

    A clause continues while the next line is indented deeper than the line it
    began on, so a guard that wraps is one clause; reading its first line alone
    let a metric reference on a continuation line escape check 56.
    """
    out, cur, col = [], None, None
    for raw in body.split("\n"):
        line = raw.split("#", 1)[0].rstrip()
        if not line.strip():
            continue
        ind = len(line) - len(line.lstrip())
        if m := re.match(r"^\s*require\s+(.*)$", line):
            if cur is not None: out.append(cur)
            cur, col = m.group(1), ind
        elif cur is not None and ind > col:
            cur += " " + line.strip()
        else:
            if cur is not None: out.append(cur)
            cur, col = None, None
    if cur is not None: out.append(cur)
    return out


REPORT_HEAD = "Creations that land past their lifecycle's first state"


def creation_report(decls):
    """The publish report's lines for creations that land past a first state (ADR-0109).

    A lifecycle's first state is the first its declaration lists. A creation into
    any other state skips the `do` transitions into that state from the states
    the first one leads to without passing through it. For each, the report says
    which of its guard clauses the creation carries, which the type holds as an
    invariant of the same name, and which nothing carries, compared by name; and
    every invariant of the type, since a creation writes the whole object and so
    is checked against all of them. It is a report, not a check: nothing fails.
    """
    by_name = {d.name: d for d in decls}
    out = []

    def ends(kind, head):
        h, _, via = head.partition(" only via ")
        via = re.sub(r"\{[^{}]*\}", "", via).strip()
        if "->" not in h:
            return None, None, via
        a, b = h.split("->", 1)
        to = b.split()[0] if b.split() else None
        frm = [] if kind == "create" else [x.strip() for x in a.strip().strip("{}").split(",") if x.strip()]
        return frm, to, via

    for d in decls:
        if d.kind != "type" or d.mirror or d.abstract:
            continue
        m = by_name.get(d.machine) if d.machine else d
        if m is None or not m.states:
            continue
        states = list(m.states)
        first = states[0]
        terminal = {s for s, (mods, _l) in m.states.items() if "terminal" in mods.split()}
        trans = list(m.trans) + (list(d.trans) if d is not m else [])
        own = [t for t in d.trans if t[0] == "create"] if d is not m else []
        creates = own or [t for t in trans if t[0] == "create"]
        edges = []
        for kind, name, head, body, _ln in trans:
            if kind != "do":
                continue
            frm, to, via = ends(kind, head)
            if to is None:
                continue
            if frm == ["any"]:
                frm = [s for s in states if s not in terminal]
            edges.append((name, frm, to, via, body))
        invs, anc = set(d.invariants), by_name.get(d.base) if d.base else None
        while anc is not None:
            invs |= anc.invariants
            anc = by_name.get(anc.base) if anc.base else None
        for _kind, name, head, body, _ln in creates:
            _f, target, _v = ends("create", head)
            if target is None or target == first:
                continue
            reach, todo = {first}, [first]
            while todo:
                x = todo.pop()
                for _n, frm, to, _via, _b in edges:
                    if x in frm and to != target and to not in reach:
                        reach.add(to); todo.append(to)
            carried = {c.split(":")[0].strip() for c in require_clauses(body)}
            out.append(f"{d.name}.{name} -> {target}")
            for tn, frm, to, via, tb in edges:
                if to != target or not set(frm) & reach:
                    continue
                names = [c.split(":")[0].strip() for c in require_clauses(tb)]
                line = f"  skips {tn}" + (f", only via {via}" if via else "") + ": "
                if not names:
                    out.append(line + "no clause of its own")
                    continue
                groups = [("carried", [n for n in names if n in carried]),
                          ("held by invariants", [n for n in names if n not in carried and n in invs]),
                          ("not carried", [n for n in names if n not in carried and n not in invs])]
                out.append(line + "; ".join(f"{g}: {', '.join(ns)}" for g, ns in groups if ns))
            out.append("  checked on landing: " + (", ".join(sorted(invs)) if invs else "no invariant"))
    return out


def report_checks(src, decls):
    """A report block quoted in a document must be the report its declarations produce."""
    out, fence, body, start = [], None, [], 0
    for i, line in enumerate(src.split("\n"), 1):   # pair fences line by line
        if fence is None and line.startswith("```"):
            fence, body, start = line[3:].strip(), [], i
        elif fence is not None and line.strip() == "```":
            if fence == "" and body and body[0] == REPORT_HEAD:
                want = creation_report(decls)
                if body[1:] != want:
                    out.append((0, "the quoted creation report is not the one the declarations "
                                   "produce: " + " | ".join(want), start))
            fence = None
        elif fence is not None:
            body.append(line)
    return out


REPORT_FIXTURE = (
    "type T version 1 {\n tracking serial\n states A category live, B category live, C category closed, D category closed terminal\n"
    " attr n string?\n invariant y: state != C or n is not null\n"
    " create mk -> A { }\n create jump -> C accepts n {\n  require z: inputs.n is not null because self_serviceable\n }\n"
    " do go A -> B {\n  require x: n is not null because unreachable_from_here\n }\n"
    " do end B -> C {\n  require y: n is not null because unreachable_from_here\n"
    "  require z: n is not null because unreachable_from_here\n  require w: true\n }\n"
    " do back C -> A { }\n do drop C -> D { }\n}\n",
    ["T.jump -> C",
     "  skips end: carried: z; held by invariants: y; not carried: w",
     "  checked on landing: y"])


def state_literals(text, base, world):
    """19 — a state literal naming no state: `<Type>.<STATE>`, or a bare name compared with
    the object's own `state` inside a type or machine, which names one of its own states."""
    out, cur = [], None
    for i, raw in enumerate(text.split("\n")):
        line = raw.split("#", 1)[0]
        if m := re.match(r"^(type|machine|observation|metric)\s+(\w+)", line):
            cur = world.get(m.group(2)) if m.group(1) in ("type", "machine") else None
        if cur is not None:
            own = cur.states or (world[cur.machine].states if cur.machine in world else {})
            for st in re.findall(r"(?<![\w.])state\s*(?:==|!=)\s*([A-Z][A-Z0-9_]*)\b(?!\.)", line):
                if own and st not in own:
                    out.append((19, f"{cur.name}: state {st} names none of its own states", base + i))
        for t, st in re.findall(r"\b([A-Z]\w*)\.([A-Z][A-Z0-9_]*)\b", line):
            d = world.get(t)
            if d is None or d.kind not in ("type", "machine"):
                continue                                   # an enum member, or a name declared elsewhere
            states = d.states or (world[d.machine].states if d.machine in world else {})
            if states and st not in states:
                out.append((19, f"{t}.{st} names no state of {t}", base + i))
    return out


def mutation_test(src):
    """Apply each mutation to the specification; its check must newly fire."""
    base = {(c, d) for c, d, _ in declaration_findings(src, True)[0]}
    out, caught = [], 0
    for check, name, old, new in MUTATIONS:
        if src.count(old) != 1:
            out.append((0, f"mutation '{name}' no longer applies: its text is not in the document once", 1))
            continue
        got = {(c, d) for c, d, _ in declaration_findings(src.replace(old, new), True)[0]} - base
        if any(c == check for c, _ in got):
            caught += 1
        else:
            out.append((0, f"mutation '{name}' was not caught by check {check} "
                           f"(got {sorted({c for c, _ in got})})", 1))
    print(f"mutations: {caught}/{len(MUTATIONS)} caught by the check that claims them")
    return out


def main():
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    DOC = target()
    is_spec = DOC.resolve() == SPEC.resolve()
    src = DOC.read_text()
    findings, blocks, ndecl, rw = declaration_findings(src, is_spec)

    if rw and is_spec:
        w = rw.group(1).split()
        if dups := {x for x in w if w.count(x) > 1}: findings.append((21, f"duplicate reserved words {sorted(dups)}", 1))
        for kw in ("provides", "requires", "cascade", "accepts", "corrects", "inputs"):
            if re.search(rf"(^|\s|`){kw}\b", src) and kw not in w:
                findings.append((21, f"keyword '{kw}' used but not reserved", 1))

    findings += doc_checks(src) if is_spec else []
    findings += mutation_test(src) if is_spec else []

    if is_spec and "## 10. What the checker verifies" in src:
        _a = src.index("## 10. What the checker verifies")
        _b = src.find("\n## 11.", _a)
        sec = src[_a:_b if _b != -1 else len(src)]
    else:
        sec = src
    nums = [int(x) for x in re.findall(r"^\| (\d+) \|", sec, flags=re.M)]
    if nums != sorted(nums): findings.append((0, f"check table misordered: {nums}", 1))
    ok = self_test()
    covered = sorted(set(FIXTURES) | set(DOC_FIXTURES))
    ndefined = max(nums) if nums else 0
    print(f"{DOC.name}: {len(blocks)} blocks, {ndecl} declarations")
    if is_spec:
        print(f"partially enforces {len(covered)} of {ndefined} defined checks: {covered}")
        print("each is proven by a fixture; most implement one clause of a multi-clause check, "
              "so this is a floor, not coverage")
    else:
        print(f"checked against {len(covered)} implemented checks: {covered}")
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
