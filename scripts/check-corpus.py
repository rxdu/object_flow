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
     {"defects.md", "0046"}),
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
            for m in re.finditer(r"\bcheck (\d+)\b", line):
                n = int(m.group(1))
                if n == 0 or n > maxcheck:
                    findings.append(f"{rel}:{i}  check {n} is out of range (1..{maxcheck})")
            for m in re.finditer(r"\]\((?!http)([^)#]+?)(?:#[^)]*)?\)", line):
                if not (p.parent / m.group(1)).resolve().exists():
                    findings.append(f"{rel}:{i}  broken link {m.group(1)}")


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
        m = re.search(r"\*\*Amends:\*\*\s*(.+)", head)
        if not m:
            continue
        for t in re.findall(r"ADR-(\d{4})", m.group(1)):
            if "Amended by" not in text.get(int(t), ""):
                findings.append(f"ADR-{n:04d} amends ADR-{t}, which does not say so in return")


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


def check_declarations():
    """Every document that states declarations must pass the syntax checker.

    Decision records are exempt: they are historical, and several deliberately
    quote the notation of their own moment with a note saying what replaced it.
    """
    checker = ROOT / "scripts/check-syntax-doc.py"
    for p in sorted(ROOT.glob("docs/design/*.md")):
        if "```text" not in p.read_text():
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
    check_declarations()

    print(f"corpus: {len(nums)} decision records, {maxcheck} checks defined")
    if not findings:
        print("coherent")
        return 0
    for f in findings:
        print("  " + f)
    print(f"{len(findings)} finding(s)")
    return 1


sys.exit(main())
