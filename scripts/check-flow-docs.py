#!/usr/bin/env python3
"""Hold the YAML in the design documents to the flow checker (ADR-0121).

1. A ```yaml block beginning `module:` begins a module, and one whose first
   line is `# <module>, continued` continues the module of that name begun
   earlier in the same document. Each module is checked by
   scripts/check-flows.py, after the modules it imports. A module a document
   begins twice is two versions of it, and the later is checked against the
   earlier, as the format's examples are against the version before.
2. A module's imports resolve among the design documents' modules, never the
   format's examples, which are a corpus of their own. A module written in
   YAML is checked before the one importing it. A module still written in the
   text notation must declare each name imported from it, and the text checker
   resolves the rest at step 4.
3. Every other yaml block is an excerpt, and MUST be part of a checked module
   or example: each key it shows is there, at one place, with the same value,
   and the keys beside it may be left out.
4. A creation report quoted in a document with YAML modules is the report
   those modules produce (ADR-0109).

Run with no arguments to check every document, or name the documents to
report on. Exit status is non-zero if anything is reported.
"""
import importlib.util
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "docs/design/flow-format/examples"
CONTINUED = re.compile(r"^# (\w+), continued\s*$")

# A module declared in more than one document names its home here, as
# check-syntax-doc.py's MODULE_HOMES does for the text notation.
MODULE_HOMES = {}


def script(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


flows = script("check-flows")
syntax = script("check-syntax-doc")


def fences(text):
    """Each fenced block as (info string, line of its first body line, body lines)."""
    out, info, body, start = [], None, [], 0
    for i, line in enumerate(text.split("\n"), 1):
        if info is None and line.startswith("```"):
            info, body, start = line[3:].strip(), [], i + 1
        elif info is not None and line.strip() == "```":
            out.append((info, start, body))
            info = None
        elif info is not None:
            body.append(line)
    return out


class Module:
    def __init__(self, name, doc, previous=None):
        self.name, self.doc, self.body, self.lines = name, doc, [], []
        self.previous = previous                    # the version this document wrote before it


    def add(self, start, body):
        self.body += body
        self.lines += range(start, start + len(body))

    @property
    def text(self):
        return "\n".join(self.body) + "\n"

    @property
    def label(self):
        return f"{self.doc.name}:{self.name}:{self.lines[0] if self.lines else 0}"

    def doc_line(self, n):
        return self.lines[n - 1] if 0 < n <= len(self.lines) else (self.lines[0] if self.lines else 1)


def within(part, whole, why):
    """Whether part shows some of whole: a mapping's keys are whole's, with values within; anything else equal."""
    if isinstance(part, dict):
        if not isinstance(whole, dict):
            why.append("a mapping where the module has none")
            return False
        for k, v in part.items():
            if k not in whole:
                why.append(f"'{k}' is not there")
                return False
            if not within(v, whole[k], why):
                why.append(f"under '{k}'")
                return False
        return True
    if part != whole:
        why.append(f"{part!r:.60} where the module has {whole!r:.60}")
        return False
    return True


def mappings(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from mappings(v)
    elif isinstance(node, list):
        for v in node:
            yield from mappings(v)


def placed(excerpt, corpus):
    """None if the excerpt is part of a module in corpus, else why not, from the nearest place."""
    nearest = None
    for _label, doc in corpus:
        for node in mappings(doc):
            if set(excerpt) <= set(node):
                why = []
                if within(excerpt, node, why):
                    return None
                if nearest is None or len(why) > len(nearest):
                    nearest = why
    if nearest is None:
        return f"no module has {', '.join(repr(k) for k in excerpt)} side by side"
    return ", ".join(reversed(nearest))


def rel(p):
    return p.relative_to(ROOT)


def check_docs(texts):
    """Findings over documents given as (path, text), as (path, line, message), and what was checked."""
    findings, modules, excerpts, reports, text_modules = [], [], [], [], {}
    for p, doc_text in texts:
        blocks = fences(doc_text)
        begun = {}
        for info, start, body in blocks:
            if info != "yaml" or not body:
                continue
            if m := re.match(r"^module:\s*(\w+)", body[0]):
                begun[m.group(1)] = Module(m.group(1), p, begun.get(m.group(1)))
                modules.append(begun[m.group(1)])
                begun[m.group(1)].add(start, body)
            elif m := CONTINUED.match(body[0]):
                if m.group(1) not in begun:
                    findings.append((p, start, f"continues {m.group(1)}, which this document has not begun"))
                else:
                    begun[m.group(1)].add(start, body)
            else:
                excerpts.append((p, start, body))
        text = [body for info, _s, body in blocks if info == "text"]
        if text and (m := re.match(r"module\s+(\w+)", text[0][0] if text[0] else "")):
            # the text parser reads types, machines, observations and metrics; the other
            # declarations a module may export are named on their own line
            named = {n for body in text for n in re.findall(r"^\s*(?:enum|sequence|evaluator)\s+(\w+)", "\n".join(body), re.M)}
            text_modules.setdefault(m.group(1), []).append(
                (p, named | {d.name for body in text for d in syntax.parse("\n".join(body), 0)}))
        reports += [(p, start, body[1:]) for info, start, body in blocks if info == "" and body and body[0] == syntax.REPORT_HEAD]

    by_name = {}
    for m in modules:
        by_name.setdefault(m.name, []).append(m)

    def home(name):
        later = {id(m.previous) for m in modules if m.previous}
        found = [m for m in by_name.get(name, []) if id(m) not in later]
        if len(found) > 1:
            found = [m for m in found if m.doc.name == MODULE_HOMES.get(name)]
        return found[0] if len(found) == 1 else None

    parsed = {}
    for m in modules:
        try:
            parsed[m.label] = flows.load(m.text)
        except Exception:
            parsed[m.label] = None                  # check-flows reports why, at step 1

    def imports(m):
        doc = parsed.get(m.label)
        return (doc.get("imports") or {}) if isinstance(doc, dict) else {}

    for m in modules:
        order, seen = [], {m.label}

        def visit(x):
            for dep, names in imports(x).items():
                h = home(dep)
                if h is not None:
                    if h.label not in seen:
                        seen.add(h.label)
                        visit(h)
                        order.append(h)
                    continue
                if x is not m:
                    continue
                line = m.doc_line(1 + next((i for i, b in enumerate(m.body) if re.match(rf"^\s*{dep}:|.*\b{dep}:", b)), 0))
                if by_name.get(dep):
                    findings.append((m.doc, line, f"imports {dep}, which {len(by_name[dep])} documents declare "
                                                  "and MODULE_HOMES gives no home"))
                elif dep in text_modules:
                    (tp, declared), *more = text_modules[dep]
                    for n in names:
                        if n not in declared:
                            findings.append((m.doc, line, f"imports '{n}' from {dep}, which {rel(tp)} does not declare"))
                else:
                    findings.append((m.doc, line, f"imports {dep}, which no design document declares"))
        visit(m)
        previous = {m.label: m.previous.text} if m.previous else None
        found, _notes = flows.check([(d.label, d.text) for d in order] + [(m.label, m.text)], previous)
        for f, line, _sev, code, msg in found:
            if f == m.label:
                findings.append((m.doc, m.doc_line(line), f"{code}: {msg}"))

    corpus = [(m.label, parsed[m.label]) for m in modules if parsed.get(m.label) is not None]
    corpus += [(p.name, flows.load(p.read_text())) for p in sorted(EXAMPLES.glob("*.yaml"))]
    for p, start, body in excerpts:
        try:
            excerpt = flows.load("\n".join(body) + "\n")
        except Exception as e:
            findings.append((p, start, f"an excerpt that is not valid YAML: {' '.join(str(e).split())[:120]}"))
            continue
        if not isinstance(excerpt, dict):
            findings.append((p, start, "an excerpt that is not a mapping, so no module can hold it"))
        elif (why := placed(excerpt, corpus)) is not None:
            findings.append((p, start, f"an excerpt that is part of no checked module: {why}"))

    for p, start, body in reports:
        mine = [m for m in modules if m.doc == p and parsed.get(m.label) is not None]
        if not mine:
            continue                                # a report over text declarations is check-syntax-doc.py's
        decls = [d for m in mine for d in syntax.parse(flows.to_text(parsed[m.label])[0], 0)]
        want = syntax.creation_report(decls)
        while body and not body[-1].strip():
            body = body[:-1]
        if body != want:
            findings.append((p, start - 1, "the quoted creation report is not the one the modules produce: "
                                           + " | ".join(want)))

    checked = (len(modules), len({m.doc for m in modules}), len(excerpts),
               len([r for r in reports if any(m.doc == r[0] for m in modules)]))
    return findings, checked


def self_test(texts):
    """Plant a mistake of each kind in a copy of the documents and require it reported."""
    spec = ROOT / "docs/design/flow-format.md"
    base = dict(texts)
    module = base[spec][base[spec].index("```yaml\nmodule: documents"):]
    module = module[:module.index("\n```\n") + 5]
    plain = "# A planted document\n\n" + module
    report = "\n```\n" + syntax.REPORT_HEAD + "\nDocument.start -> PUBLISHED\n```\n"
    split = module.replace("\ntypes:\n", "\n```\n\nProse between the blocks.\n\n```yaml\n# documents, continued\ntypes:\n", 1)

    def planted(old, new, doc=spec):
        assert base[doc].count(old) >= 1, old
        return {**base, doc: base[doc].replace(old, new, 1)}
    trial = ROOT / "docs/design/planted.md"
    plants = [
        ("an excerpt whose value its module does not have", "part of no checked module",
         planted("reserved; a query may filter on it.\n", "reserved.\n")),
        ("an excerpt with a key its module does not have", "part of no checked module",
         planted("  flag_when: { low: \"value < 0.900\" }\n", "  flag_when: { low: \"value < 0.900\" }\n  audience: managers\n")),
        ("a continuation of a module the document has not begun", "has not begun",
         {**base, trial: "```yaml\n# nothing, continued\ntypes: {}\n```\n"}),
        ("an import of a name the text module does not declare", "does not declare",
         {**base, trial: plain.replace("module: documents\n", "module: documents\nimports: { inventory: [Customer, Nonesuch] }\n")}),
        ("an import of a module no document declares", "no design document declares",
         {**base, trial: plain.replace("module: documents\n", "module: documents\nimports: { nowhere: [Customer] }\n")}),
        ("a module the flow checker refuses, at the line it is on", "names:",
         {**base, trial: plain.replace("        to: PUBLISHED\n", "        to: PUBLISH\n")}),
        ("a later version that removes a state with no mapping", "migration",
         {**base, trial: plain + "\n" + module.replace("      DRAFT:\n        description: Being written.\n        category: live\n", "")
                                                      .replace("        to: DRAFT\n", "        to: PUBLISHED\n")
                                                      .replace("        from: DRAFT\n", "        from: PUBLISHED\n")}),
        ("a quoted creation report the modules do not produce", "quoted creation report",
         {**base, trial: plain + report}),
    ]
    ok = True
    for name, expect, docs in plants:
        found, _checked = check_docs(list(docs.items()))
        hit = [f for f in found if expect in f[2] and f[0] in (spec, trial)]
        if expect == "names:":                      # the finding is on the line the mistake is on
            hit = [f for f in hit if docs[trial].split("\n")[f[1] - 1].strip() == "to: PUBLISH"]
        ok &= bool(hit)
        where = f"{rel(hit[0][0])}:{hit[0][1]} {hit[0][2][:90]}" if hit else f"MISSED {[f[2][:60] for f in found][:2]}"
        print(f"  planted {name}: {'caught' if hit else 'missed'}, at {where}")
    found, _checked = check_docs(list({**base, trial: "# A planted document\n\n" + split}.items()))
    clean = not [f for f in found if f[0] == trial]
    ok &= clean
    print(f"  a module split across two blocks is checked as one: {'clean' if clean else [f[2] for f in found if f[0] == trial]}")
    return ok


def main():
    docs = [p for p in sorted(ROOT.glob("docs/**/*.md")) if "adr" not in p.relative_to(ROOT).parts]
    wanted = {pathlib.Path(a).resolve() for a in sys.argv[1:]}
    texts = [(p, p.read_text()) for p in docs]
    findings, (n_modules, n_docs, n_excerpts, n_reports) = check_docs(texts)
    shown = [f for f in findings if not wanted or f[0].resolve() in wanted]
    for p, line, msg in sorted(shown, key=lambda f: (str(f[0]), f[1])):
        print(f"  {rel(p)}:{line} {msg}")
    print(f"flow docs: {n_modules} module(s) in {n_docs} document(s), {n_excerpts} excerpt(s), {n_reports} report(s): "
          + ("consistent" if not shown else f"{len(shown)} finding(s)"))
    ok = wanted or self_test(texts)
    return 1 if shown or not ok else 0


if __name__ == "__main__":
    sys.exit(main())
