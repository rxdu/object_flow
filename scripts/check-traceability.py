#!/usr/bin/env python3
"""Hold the traceability table against the PRD, and report what is not covered.

The PRD is the baseline (docs/DESIGN.md, preamble): a design choice is checked
against it, never the reverse. docs/design/traceability.md maps every PRD
requirement and use case to the parts of the design that meet it. This checks
that the map is complete and honest in the ways a script can decide:

  - every requirement in PRD section 6 and every use case in section 7 appears
    exactly once, and nothing else does;
  - every citation resolves: a DESIGN section heading, an ADR file, a document,
    and a document's section heading;
  - a covered row says nothing remains, and any other row says what does;
  - a covered row cites a document that owns an implementation, as the map's
    definition of covered requires: DESIGN.md states a mechanism, and one of
    the implementation documents says how it is built;
  - a row is `unverifiable` only where the PRD itself says the requirement is
    not yet verifiable, so the status cannot be used to park a hard row;
  - the "Current:" line counts the rows as they are.

Exit status is non-zero if any of that fails, or if any row is partial or a gap.
`--structure` checks the map without requiring full coverage, which is what the
corpus checker runs while the design is being iterated.
"""
import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[1]
PRD = ROOT / "docs/PRD.md"
MAP = ROOT / "docs/design/traceability.md"
DESIGN = ROOT / "docs/DESIGN.md"
STATUSES = ("covered", "partial", "gap", "unverifiable")
OWNERS = ("declaration-syntax.md", "storage-schema.md", "library-api.md",
          "publish-and-import.md", "renderers.md", "adversarial-harness.md",
          "first-consumer-cutover.md")


def headings(path):
    """Section numbers a document declares, as '5.11' or '10'."""
    return set(re.findall(r"^#{2,3} (\d+(?:\.\d+)*)\.? ", path.read_text(), re.M))


def prd_ids():
    text = PRD.read_text().split("## 11. Revision history")[0]
    s6 = text[text.index("## 6. Requirements"):text.index("## 7. Use cases")]
    s7 = text[text.index("## 7. Use cases"):text.index("## 8.")]
    reqs = re.findall(r"^\| ([FDCMLVTN]\d+) \|", s6, re.M)
    ucs = re.findall(r"^\*\*(UC-\d+)\.", s7, re.M)
    unverifiable = set(re.findall(r"^\| ([FDCMLVTN]\d+) \|.*not yet verifiable", s6, re.M))
    return reqs, ucs, unverifiable


def rows():
    out = []
    for m in re.finditer(r"^\| ((?:[FDCMLVTN]\d+)|(?:UC-\d+)) \|(.*)$", MAP.read_text(), re.M):
        cells = [c.strip() for c in m.group(2).rstrip().rstrip("|").split("|")]
        out.append((m.group(1), cells))
    return out


def check_citations(rid, cited, findings):
    design_secs = headings(DESIGN)
    for sec in re.findall(r"DESIGN §(\d+(?:\.\d+)*)", cited):
        if sec not in design_secs:
            findings.append(f"{rid}: DESIGN §{sec} does not exist")
    # a comma-separated run after "DESIGN" continues to cite DESIGN sections
    for run in re.findall(r"DESIGN ((?:§\d+(?:\.\d+)*(?:, )?)+)", cited):
        for sec in re.findall(r"§(\d+(?:\.\d+)*)", run):
            if sec not in design_secs:
                findings.append(f"{rid}: DESIGN §{sec} does not exist")
    for n in re.findall(r"ADR-(\d{4})", cited):
        if not list((ROOT / "docs/adr").glob(f"{n}-*.md")):
            findings.append(f"{rid}: ADR-{n} does not exist")
    for doc, secs in re.findall(r"([\w-]+\.md)((?: §\d+(?:\.\d+)*(?:, §\d+(?:\.\d+)*)*)?)", cited):
        path = next((p for p in (ROOT / "docs/design" / doc, ROOT / "docs" / doc) if p.exists()), None)
        if path is None:
            findings.append(f"{rid}: {doc} does not exist")
            continue
        have = headings(path)
        for sec in re.findall(r"§(\d+(?:\.\d+)*)", secs):
            if sec not in have:
                findings.append(f"{rid}: {doc} §{sec} does not exist")


def main():
    structure_only = "--structure" in sys.argv
    reqs, ucs, prd_unverifiable = prd_ids()
    table = rows()
    findings, counts, open_rows = [], {s: 0 for s in STATUSES}, []

    ids = [r for r, _ in table]
    for want, what in ((reqs, "requirement"), (ucs, "use case")):
        for i in want:
            n = ids.count(i)
            if n != 1:
                findings.append(f"{what} {i} appears {n} times in the table")
    for i in ids:
        if i not in reqs and i not in ucs:
            findings.append(f"{i} is in the table and not in the PRD")

    for rid, cells in table:
        if len(cells) != 3:
            findings.append(f"{rid}: expected 3 cells after the ID, found {len(cells)}")
            continue
        cited, status, remains = cells
        if status not in STATUSES:
            findings.append(f"{rid}: status {status!r} is not one of {STATUSES}")
            continue
        counts[status] += 1
        if not cited:
            findings.append(f"{rid}: cites nothing")
        check_citations(rid, cited, findings)
        if status == "covered" and remains:
            findings.append(f"{rid}: covered, yet says something remains")
        if status == "covered" and not any(o in cited for o in OWNERS):
            findings.append(f"{rid}: covered, and cites no document that owns an implementation")
        if status == "unverifiable" and rid not in prd_unverifiable:
            findings.append(f"{rid}: unverifiable, and the PRD does not say it is not yet verifiable")
        if status != "covered":
            open_rows.append((rid, status, remains))
            if not remains:
                findings.append(f"{rid}: {status}, and does not say what remains")

    want = (counts["covered"], counts["partial"], counts["gap"], counts["unverifiable"])
    m = re.search(r"^Current: (\d+) covered, (\d+) partial, (\d+) gaps?, (\d+) unverifiable\.",
                  MAP.read_text(), re.M)
    if not m:
        findings.append("no 'Current: N covered, N partial, N gaps, N unverifiable.' line")
    elif tuple(int(x) for x in m.groups()) != want:
        findings.append(f"the Current line says {m.groups()}, the rows say {want}")

    print(f"traceability: {len(reqs)} requirements and {len(ucs)} use cases; "
          f"{counts['covered']} covered, {counts['partial']} partial, {counts['gap']} gaps, "
          f"{counts['unverifiable']} unverifiable")
    for f in findings:
        print("  " + f)
    for rid, status, remains in open_rows:
        if status == "unverifiable" or not structure_only:
            print(f"  {status:<12} {rid}: {remains}")
    if findings:
        return 1
    blocking = [r for r in open_rows if r[1] != "unverifiable"]
    if blocking and not structure_only:
        return 1
    if blocking:
        print("structure sound")
    else:
        print("every requirement and use case is covered, "
              "except where the PRD says it cannot yet be verified")
    return 0


sys.exit(main())
