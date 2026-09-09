#!/usr/bin/env python3
"""Corpus-wide coherence checks over the design record.

Complements scripts/check-syntax-doc.py, which checks declarations inside one
document.  This checks the things that only go wrong *between* documents:
cross-references that stop resolving, notation that was renamed and survived
somewhere, and decision records that were amended without saying so.

Run with no arguments.  Exit status is non-zero if anything is reported.
"""
import re
import sys
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
ADR = ROOT / "docs/adr"

# Notation that was renamed.  Each entry is (retired spelling, what to write
# instead, files allowed to still contain it and why).  The register records
# history and quotes defects verbatim, so it is exempt throughout.
RETIRED = [
    (r"\ba\.event\b", "a.at_event", {"defects.md", "LESSONS.md"}),
    (r"\btime-gated\b", "sweepable (ADR-0048)", {"defects.md"}),
    (r"unreachable-from-here", "unreachable_from_here", {"defects.md", "0056", "0059"}),
    (r"\blet\s+\w+\s*=\s*create\b", "create <name> = <Type>.<transition>(…)",
     {"defects.md", "0046"}),        # 0046 is the record that introduced it, annotated in place
    (r"\?\s+\w+\s+:\s+\w+\s*`", "if … then … else", {"defects.md", "0021", "0053"}),
]

findings = []


def docs():
    yield from sorted(ROOT.glob("docs/**/*.md"))
    yield ROOT / "README.md"
    yield ROOT / "TODO.md"


def adr_numbers():
    return {int(m.group(1)) for p in ADR.glob("[0-9]*.md")
            if (m := re.match(r"(\d+)", p.name))}


def check_references(nums, maxcheck):
    for p in docs():
        rel = p.relative_to(ROOT)
        for i, line in enumerate(p.read_text().split("\n"), 1):
            for m in re.finditer(r"ADR-(\d{4})", line):
                if int(m.group(1)) not in nums:
                    findings.append(f"{rel}:{i}  ADR-{m.group(1)} does not exist")
            for m in re.finditer(r"\b[Cc]heck (\d+)\b", line):
                n = int(m.group(1))
                if n == 0 or n > maxcheck:
                    findings.append(f"{rel}:{i}  check {n} is out of range (1..{maxcheck})")
            for m in re.finditer(r"\]\((?!http)([^)#]*?)(#[^)]*)?\)", line):
                target = (p.parent / m.group(1)).resolve() if m.group(1) else p
                if not target.exists():
                    findings.append(f"{rel}:{i}  broken link {m.group(1)}")
                elif m.group(2) and target.suffix == ".md":
                    want = m.group(2)[1:]
                    heads = {re.sub(r"[^a-z0-9 -]", "", h.lower()).replace(" ", "-")
                             for h in re.findall(r"^#+ (.+)$", target.read_text(), re.M)}
                    if want not in heads:
                        findings.append(f"{rel}:{i}  anchor #{want} does not resolve in {m.group(1) or p.name}")


def check_retired():
    for p in docs():
        rel, text = p.relative_to(ROOT), p.read_text()
        for pattern, replacement, exempt in RETIRED:
            if any(e in p.name for e in exempt):
                continue
            for i, line in enumerate(text.split("\n"), 1):
                if re.search(pattern, line):
                    findings.append(f"{rel}:{i}  retired notation; write {replacement}")


def check_adr_headers(nums):
    for p in sorted(ADR.glob("[0-9]*.md")):
        n = int(re.match(r"(\d+)", p.name).group(1))
        head = "\n".join(p.read_text().split("\n")[:12])
        if not re.search(r"^# ADR-%04d: " % n, head, re.M):
            findings.append(f"{p.name}: title does not match its number")
        for field in ("**Status:**", "**Date:**"):
            if field not in head:
                findings.append(f"{p.name}: no {field.strip('*:')}")
    missing = sorted(set(range(min(nums), max(nums) + 1)) - nums)
    if missing:
        findings.append(f"gaps in ADR numbering: {missing}")


def check_amendments():
    text = {int(re.match(r"(\d+)", p.name).group(1)): p.read_text()
            for p in ADR.glob("[0-9]*.md")}
    for n, body in sorted(text.items()):
        head = "\n".join(body.split("\n")[:12])
        for kind, back in (("Amends", "Amended by"), ("Refines", "Refined by"),
                           ("Supersedes", "Superseded by")):
            m = re.search(r"\*\*%s:\*\*\s*(.+)" % kind, head)
            if not m:
                continue
            for t in re.findall(r"ADR-(\d{4})", m.group(1)):
                target = text.get(int(t), "")
                if back not in target and f"ADR-{n:04d}" not in target:
                    findings.append(f"ADR-{n:04d} {kind.lower()} ADR-{t}, "
                                    f"which does not say so in return")


def check_defect_index():
    """Every register entry must appear in the register's own index table.

    The index reached 62 of 170 before anyone noticed, which is what
    LESSONS.md predicts of a summary maintained beside its body.
    """
    reg = ROOT / "docs/design/defects.md"
    text = reg.read_text()
    for m in re.finditer(r"^### ([DC]\d+)", text, re.M):
        if f"[{m.group(1)}](#" not in text:
            findings.append(f"defects.md: {m.group(1)} is not in the index table")


def check_answer_blocks():
    """Recommendation paragraphs belong only in the answers section.

    A scripted edit keyed on `^\\d+\\. ` once inserted four of them into two
    unrelated numbered lists, including the name-resolution order check 19
    depends on.  Nothing mechanical distinguishes that from a legitimate
    indented continuation -- the shapes are identical -- so this checks the one
    thing that is decidable: where those paragraphs are allowed to be.
    """
    spec = ROOT / "docs/design/declaration-syntax.md"
    lines = spec.read_text().split("\n")
    answers = next((i for i, l in enumerate(lines) if l.startswith("## 11.")), len(lines))
    for i, line in enumerate(lines[:answers], 1):
        if line.lstrip().startswith("**Recommended"):
            findings.append(f"declaration-syntax.md:{i}  an answer paragraph outside §11")


def check_schema_doc():
    """The storage schema's SQL must execute."""
    checker = ROOT / "scripts/check-schema-doc.py"
    doc = ROOT / "docs/design/storage-schema.md"
    if not doc.exists():
        return
    r = subprocess.run([sys.executable, str(checker)], capture_output=True, text=True)
    if r.returncode != 0:
        findings.append("storage-schema.md: SQL does not execute; "
                        "run scripts/check-schema-doc.py")


def check_api_doc():
    """The library API's Python must execute and must match the model."""
    if not (ROOT / "docs/design/library-api.md").exists():
        return
    r = subprocess.run([sys.executable, str(ROOT / "scripts/check-api-doc.py")],
                       capture_output=True, text=True)
    if r.returncode != 0:
        for line in r.stdout.strip().split("\n")[1:]:
            findings.append("library-api.md:" + line.rstrip())


def check_renderers_doc():
    """The renderers document's JSON must parse and its schemas must validate."""
    if not (ROOT / "docs/design/renderers.md").exists():
        return
    r = subprocess.run([sys.executable, str(ROOT / "scripts/check-renderers-doc.py")],
                       capture_output=True, text=True)
    if r.returncode != 0:
        findings.append("renderers.md: " + r.stdout.strip().split("\n")[-1])


def check_declarations():
    """Every document that states declarations must pass the syntax checker.

    Decision records are exempt: they are historical, and several deliberately
    quote the notation of their own moment with a note saying what replaced it.
    """
    checker = ROOT / "scripts/check-syntax-doc.py"
    for p in sorted(ROOT.glob("docs/design/*.md")):
        text = p.read_text()
        if "```text" not in text:
            # a document that once carried declarations and no longer does is
            # more likely a retagged fence than a rewrite; say so rather than
            # silently dropping it from the checked set
            if "```" in text and re.search(r"^\s*(type|machine)\s+\w+\s+version\s+\d", text, re.M):
                findings.append(f"{p.relative_to(ROOT)}: has declaration-shaped lines "
                                "in a fence that is not ```text, so nothing checks them")
            continue
        r = subprocess.run([sys.executable, str(checker), str(p)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            n = len([x for x in r.stdout.split("\n") if x.startswith("  check")])
            findings.append(f"{p.relative_to(ROOT)}: {n} declaration finding(s); "
                            f"run scripts/check-syntax-doc.py on it")


def main():
    nums = adr_numbers()
    spec = (ROOT / "docs/design/declaration-syntax.md").read_text()
    sec = spec[spec.index("## 10. What the checker verifies"):spec.index("\n## 11.")]
    maxcheck = max(int(x) for x in re.findall(r"^\| (\d+) \|", sec, flags=re.M))

    check_references(nums, maxcheck)
    check_retired()
    check_adr_headers(nums)
    check_amendments()
    check_answer_blocks()
    check_defect_index()
    check_declarations()
    check_schema_doc()
    check_api_doc()
    check_renderers_doc()

    print(f"corpus: {len(nums)} decision records, {maxcheck} checks defined")
    if not findings:
        print("coherent")
        return 0
    for f in findings:
        print("  " + f)
    print(f"{len(findings)} finding(s)")
    return 1


sys.exit(main())
