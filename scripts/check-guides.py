#!/usr/bin/env python3
"""Hold the user guides (docs/guides/) to the engine they describe.

Three checks:

1. **The tutorials run.** Every ```sh block of quickstart.md, agents.md and
   write-a-flow.md that is followed by a ```json or ```text block is run, in
   order, against a fresh sandbox of the service, and its output compared with
   that block: JSON line by line as values, text as lines the output must
   contain. Each ```yaml block names its file in a first-line comment
   (`# leave.yaml`) and is written there beside the commands, which use the
   helper `of` that quickstart.md defines.
2. **The references name every code.** Each code the engine can answer, read
   from its source rather than listed here, appears in backticks on the page
   that is its reference: every verdict kind, remedy, input reason, generated
   clause, read refusal and fault code in errors.md, every operation and key
   of wire-v1.json in wire.md, and every flag and metric in operate.md.
3. **The indexes are whole.** docs/guides/README.md and the repository's
   llms.txt, the index an agent reads first, each link every guide page.

Usage: check-guides.py [--service <binary>]
       The service defaults to the newer of core/target/debug and
       core/target/release's objectflow-service, so check-all's core gate,
       which builds the debug one, checks the guides against the tree.
"""
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
GUIDES = ROOT / "docs/guides"
CORE = ROOT / "core/crates/objectflow-core/src"
SERVICE = ROOT / "core/crates/objectflow-service/src/main.rs"

# The pages whose commands are run, each against a sandbox of its own.
RUNNABLE = ("quickstart.md", "agents.md", "write-a-flow.md")

# The helper the quickstart defines, which the check defines the same way, so
# the page's commands run as a reader's would.
HELPER = """of() { curl -s "$OF/v1/$1" -H 'Content-Type: application/json' -d "$2"; echo; }"""

# Words the extraction below meets that are not codes: an actor kind and two
# value types a read answers as data.
NOT_CODES = {"human", "state", "timestamp"}

# A fault's prefix that is not a code a client meets: the usage line, a load's
# module and the stop's notice.
NOT_FAULTS = {"usage", "module", "stopping"}

failures = []


def fail(message):
    failures.append(message)


def blocks(text):
    """Each fenced block of a page, as (language, body)."""
    return [(m.group(1), m.group(2)) for m in re.finditer(r"^```(\w*)\n(.*?)^```$", text, re.S | re.M)]


def body(path):
    """A source file without its test module, which names codes as data; a
    method marked for tests alone stays, as the code after it does."""
    text = path.read_text()
    m = re.search(r"^#\[cfg\(test\)\]\s*\n(?:#!?\[[^\n]*\]\s*\n)*mod \w+", text, re.M)
    return text[: m.start()] if m else text


# ── 1. the quickstart runs ──────────────────────────────────────────────


def start(binary):
    """A sandbox on a free port, and its address."""
    proc = subprocess.Popen([str(binary), "--port", "0"], stderr=subprocess.PIPE, text=True)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        line = proc.stderr.readline()
        m = re.search(r"listening on (http://\S+)", line)
        if m:
            return proc, m.group(1)
    proc.kill()
    sys.exit("check-guides: the service did not say where it listens within 10 s")


def json_lines(text):
    return [json.loads(line) for line in text.splitlines() if line.strip()]


def run_page(name, binary):
    """Run a page's commands against a sandbox of its own, and answer how many
    were compared."""
    page = (GUIDES / name).read_text()
    if name == "quickstart.md" and HELPER not in page:
        fail("quickstart.md: the helper `of` is not defined as the check defines it")
    found = blocks(page)
    files = {}
    for lang, text in found:
        if lang != "yaml":
            continue
        m = re.match(r"# ([\w.-]+\.yaml)\n", text)
        if not m:
            fail(f"{name}: a yaml block names no file in its first line, as `# leave.yaml`")
            return 0
        files[m.group(1)] = text
    with tempfile.TemporaryDirectory() as work:
        work = pathlib.Path(work)
        for file, text in files.items():
            (work / file).write_text(text)
        (work / "scripts").symlink_to(ROOT / "scripts")
        proc, address = start(binary)
        try:
            ran = 0
            for (lang, command), (out_lang, expected) in zip(found, found[1:]):
                if lang != "sh" or out_lang not in ("json", "text"):
                    continue
                ran += 1
                got = subprocess.run(
                    ["bash", "-c", f"set -o pipefail\nOF={address}\n{HELPER}\n{command}"],
                    cwd=work, capture_output=True, text=True, timeout=60,
                )
                first = command.strip().splitlines()[0][:70]
                if got.returncode != 0:
                    fail(f"{name}: `{first}` exited {got.returncode}: {got.stderr.strip()[:300]}")
                    continue
                if out_lang == "json":
                    try:
                        same = json_lines(got.stdout) == json_lines(expected)
                    except json.JSONDecodeError as e:
                        fail(f"{name}: `{first}`: an answer is not JSON ({e}): {got.stdout[:300]}")
                        continue
                    if not same:
                        fail(f"{name}: `{first}` answered\n    {got.stdout.strip()}\n  where the page says\n    {expected.strip()}")
                else:
                    missing = [line for line in expected.splitlines() if line.strip() and line not in got.stdout]
                    if missing:
                        fail(f"{name}: `{first}` printed no line {missing[0]!r}")
            if ran == 0:
                fail(f"{name}: no command with an answer to compare was found")
            return ran
        finally:
            proc.terminate()
            proc.wait(timeout=40)


def run_restore(binary):
    """Run operate.md's backup and restore as it writes them, on a deployment of a store made for it: the one-off
    backup command, the copy's check, the timer unit's backup command, and the restore's steps with the mark a copy
    made otherwise is given, the paths swapped for the store's and the copies'; the restored store said to be one.
    Where the page's commands change, this is the gate that they still work (ADR-0207)."""
    page = (GUIDES / "operate.md").read_text()
    one_off = re.search(r"^python3 -c \"(import sqlite3; c = sqlite3\.connect\('/backup/store\.sqlite'\).*)\"$",
                        page, re.M)
    checked = re.search(r"`python3 -c \"(import sqlite3; c = sqlite3\.connect\('file:/backup/store\.sqlite\?immutable=1'"
                        r"[^`]*)\"` prints", page)
    timer = re.search(r"^ExecStart=/usr/bin/python3 -c \"(.*)\"$", page, re.M)
    mark = re.search(r"`python3 -c \"(import sqlite3; c = sqlite3\.connect\('/var/lib/objectflow/store\.sqlite'\);"
                     r"[^`]*)\"`", page)
    for found, what in ((one_off, "the one-off backup"), (checked, "the copy's check"), (timer, "the timer's backup"),
                        (mark, "the restore's mark")):
        if found is None:
            fail(f"operate.md: {what} is not where the check reads it")
            return 0
    with tempfile.TemporaryDirectory() as work:
        work = pathlib.Path(work)
        store, copy, copies = work / "store.sqlite", work / "backup.sqlite", work / "copies"
        copies.mkdir()

        def swapped(code):
            return (code.replace("/var/lib/objectflow/store.sqlite", str(store))
                    .replace("/backup/store.sqlite", str(copy))
                    .replace("/var/backups/objectflow", str(copies)).replace("%%", "%"))

        def python(code):
            return subprocess.run([sys.executable, "-c", swapped(code)], capture_output=True, text=True, timeout=60)

        proc = subprocess.Popen([str(binary), "--store", str(store), "--create", "--port", "0"],
                                stderr=subprocess.PIPE, text=True)
        try:
            while not re.search(r"listening on http", proc.stderr.readline()):
                pass
            for code, what in ((one_off.group(1), "the one-off backup"), (timer.group(1), "the timer's backup")):
                ran = python(code)
                if ran.returncode != 0:
                    fail(f"operate.md: {what} exited {ran.returncode}: {ran.stderr.strip()[:300]}")
            said = python(checked.group(1))
            if not said.stdout.startswith("ok objectflow-core "):
                fail(f"operate.md: the copy's check printed {said.stdout.strip()!r}, {said.stderr.strip()[:200]}")
            if not list(copies.glob("store-*.sqlite")):
                fail("operate.md: the timer's backup made no copy")
        finally:
            proc.terminate()
            proc.wait(timeout=40)
        # two restores: the backup's copy, which its command marked, and the file copied with the service stopped,
        # which step 4 marks; each the -wal and -shm gone, in the store's place, and started
        stopped = work / "stopped.sqlite"
        stopped.write_bytes(store.read_bytes())
        for source, marks, what in ((copy, False, "the backup's copy"), (stopped, True, "a copy taken stopped")):
            for end in ("-wal", "-shm"):
                pathlib.Path(f"{store}{end}").unlink(missing_ok=True)
            store.write_bytes(source.read_bytes())
            if marks:
                ran = python(mark.group(1))
                if ran.returncode != 0:
                    fail(f"operate.md: the restore's mark exited {ran.returncode}: {ran.stderr.strip()[:300]}")
            proc = subprocess.Popen([str(binary), "--store", str(store), "--port", "0"], stderr=subprocess.PIPE,
                                    text=True)
            try:
                # the start's notes follow its listening line, and the store's version closes them
                lines = []
                while not any(" at version " in line for line in lines[-1:]):
                    line = proc.stderr.readline()
                    if not line:
                        break
                    lines.append(line)
                if not any("is a restored copy" in line for line in lines):
                    fail(f"operate.md: {what}, restored as the page says, was not said to be a restored copy: "
                         + "".join(lines)[-400:])
            finally:
                proc.terminate()
                proc.wait(timeout=40)
    return 5


# ── 2. the references name every code ───────────────────────────────────


def verdict_codes():
    verdict = body(CORE / "verdict.rs")
    model = body(CORE / "model.rs")
    kinds = set(re.findall(r'Verdict::\w+(?: \{ \.\. \})?(?: \| Verdict::\w+)* => "([a-z_]+)"', verdict))
    reasons = set(re.findall(r'InputReason::\w+ => "([a-z_]+)"', verdict))
    remedies = set(re.findall(r'"([a-z_]+)" => Remedy::', model))
    return kinds, reasons, remedies


def generated_clauses():
    """The clause and invariant names flow-format.md §3 reserves for the
    generated guards and invariants, the patterned ones as written."""
    spec = (ROOT / "docs/design/flow-format.md").read_text()
    line = next(l for l in spec.splitlines() if "a condition MUST NOT be named" in l)
    return set(re.findall(r"`([a-z_<>]+)`", line.split("since those are")[0]))


def read_refusals():
    found = set()
    for f in sorted(CORE.glob("*.rs")):
        s = body(f)
        for pat in (r'\brefuse\("([a-z_]+)"', r'\brefused\("([a-z_]+)"', r'refused: Some\("([a-z_]+)"',
                    r'Ok\(Err\("([a-z_]+)"\)\)', r'Some\("([a-z_]+)"\.(?:into|to_string)\(\)\)',
                    r'refused_with\("([a-z_]+)"\)', r'unreachable\("([a-z_]+)"\)',
                    r'clause: "([a-z_]+)"\.into\(\)'):
            found |= set(re.findall(pat, s))
    return found - NOT_CODES


def fault_codes():
    s = body(SERVICE)
    store = body(CORE / "store.rs")
    consts = dict(re.findall(r'pub const ([A-Z_]+): &str = "([a-z_]+)";', s + store))
    codes = set(re.findall(r'"([a-z]+(?:_[a-z]+)*): ', s))
    codes |= {consts[c] for c in re.findall(r'"\{([A-Z_]+)\}: ', s) if c in consts}
    codes |= set(re.findall(r'\d{3} => "([a-z_]+)"', s))
    codes |= set(re.findall(r'_ => "([a-z_]+)"', s.split("fn fault_code")[1].split("\n}\n")[0]))
    codes |= set(re.findall(r'"code": "([a-z_]+)"', s)) - {"load"}
    return codes - NOT_FAULTS


def flags():
    s = body(SERVICE)
    usage = s.split("usage: objectflow-service")[1].split('";')[0]
    return set(re.findall(r"(--[a-z][a-z-]+)", usage))


def metrics():
    return set(re.findall(r"# TYPE (objectflow_[a-z_]+)", body(SERVICE)))


def named(page, codes, what, forms=("`{}`",)):
    if not (GUIDES / page).exists():
        if f"{page}: is missing" not in failures:
            fail(f"{page}: is missing")
        return
    text = (GUIDES / page).read_text()
    for code in sorted(codes):
        if not any(form.format(code) in text for form in forms):
            fail(f"{page}: names no {what} {forms[0].format(code)}")


def check_references():
    kinds, reasons, remedies = verdict_codes()
    for got, what in ((kinds, "verdict kinds"), (reasons, "input reasons"), (remedies, "remedies")):
        if not got:
            fail(f"check-guides: read no {what} from the core, so its pattern no longer matches the source")
    named("errors.md", kinds, "verdict kind")
    named("errors.md", reasons, "input reason")
    named("errors.md", remedies, "remedy")
    named("errors.md", generated_clauses(), "generated clause")
    named("errors.md", read_refusals(), "refusal code")
    named("errors.md", fault_codes(), "fault code")
    # a flag as the table writes it, alone or with its argument
    named("operate.md", flags(), "flag", forms=("`{}`", "`{} <"))
    named("operate.md", metrics(), "metric")
    wire = json.loads((GUIDES / "wire-v1.json").read_text())["operations"]
    named("wire.md", {op.split("/")[-1] for op in wire if op.startswith("/v1/")}, "operation")
    keys = {k for op, taken in wire.items() if op.startswith("/v1/") for k in taken}
    named("wire.md", keys, "key")


def check_required():
    """Each operation's row in wire.md names before its `;` exactly the keys wire-v1.json records as required, which
    the service's test holds, where the tables marked `actor` optional on operations that refused a request without
    it (the guides' audit G-93)."""
    required = json.loads((GUIDES / "wire-v1.json").read_text()).get("required", {})
    text = (GUIDES / "wire.md").read_text()
    for op, keys in required.items():
        name = op.split("/")[-1]
        row = re.search(rf"^\| `{name}` \| ([^|]*) \|", text, re.M)
        if row is None:
            fail(f"wire.md: no row for `{name}`")
            continue
        if row.group(1).strip() == "the same":
            continue
        said = set(re.findall(r"`([a-z_]+)`", row.group(1).split(";")[0]))
        if said != set(keys):
            fail(f"wire.md: `{name}` requires {sorted(said)} before its `;`, and wire-v1.json {sorted(keys)}")


def check_indexes():
    pages = sorted(p.name for p in GUIDES.glob("*.md") if p.name != "README.md")
    for index, prefix in ((GUIDES / "README.md", ""), (ROOT / "llms.txt", "docs/guides/")):
        text = index.read_text() if index.exists() else ""
        for page in pages:
            if f"]({prefix}{page})" not in text:
                fail(f"{index.relative_to(ROOT)}: links no {page}")


def main():
    args = sys.argv[1:]
    if args[:1] == ["--service"] and len(args) == 2:
        binary = pathlib.Path(args[1])
    elif not args:
        target = ROOT / "core/target"
        built = [p for p in (target / "debug/objectflow-service", target / "release/objectflow-service")
                 if p.exists()]
        binary = max(built, key=lambda p: p.stat().st_mtime, default=None)
        if binary is None:
            sys.exit("check-guides: build the service first (cargo build -p objectflow-service in core/)")
    else:
        sys.exit(__doc__.split("Usage: ")[1])
    check_references()
    check_required()
    check_indexes()
    ran = {name: run_page(name, binary) for name in RUNNABLE}
    ran["operate.md"] = run_restore(binary)
    for f in failures:
        print(f"  {f}")
    compared = ", ".join(f"{name} {n}" for name, n in ran.items())
    print(f"check-guides: {len(failures)} failure(s); commands compared: {compared}; against {binary}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
