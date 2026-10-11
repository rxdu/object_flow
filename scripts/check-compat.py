"""Format 1's corpus held (ADR-0180 decision 7): the design's sample modules and their examples as they stood when
format 1 was promised, copied to `docs/design/flow-format/format-1/` and never edited. Every file must match its
digest, every bundle check with no fatal finding, and every example pass on the reference runner; the core holds
the same corpus in `objectflow-core`'s `format_1` test. A change this check refuses breaks format 1: under the
promise, it waits for a major release (ADR-0180 decisions 3 and 6).

Usage: check-compat.py
"""
import hashlib
import importlib.util
import json
import pathlib
import shutil
import sys
import tempfile

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORPUS = ROOT / "docs/design/flow-format/format-1"


def load_checker():
    spec = importlib.util.spec_from_file_location("check_flows", ROOT / "scripts/check-flows.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def held(corpus, flows):
    """What breaks format 1 in a corpus directory, each a sentence, and the number of examples it ran."""
    manifest = json.loads((corpus / "format-1.json").read_text())
    problems, examples = [], 0
    for name, digest in manifest["digests"].items():
        path = corpus / name
        if not path.is_file():
            problems.append(f"{name} is gone from format 1's corpus")
        elif "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            problems.append(f"{name} is not as frozen: format 1's corpus is never edited")
    for path in sorted(corpus.glob("*.yaml")):
        if path.name not in manifest["digests"]:
            problems.append(f"{path.name} was added to format 1's corpus, which holds what was promised and no more")
    if problems:
        return problems, examples
    for bundle in manifest["bundles"]:
        files = [(n, (corpus / n).read_text()) for n in bundle["files"]]
        previous = {n: (corpus / p).read_text() for n, p in (bundle.get("previous") or {}).items()} or None
        found, notes = flows.check(files, previous, None, None, flows.beside(corpus))
        for f in found:
            problems.append(f"{bundle['module']}: {f[0]}:{f[1]} {f[3]}: {f[4]}")
        for n in notes:
            if n[3] == "notrun":
                problems.append(f"{bundle['module']}: the reference runner no longer runs {n[4]}")
        for n, text in files:
            if n.endswith(".examples.yaml"):
                examples += len((yaml.safe_load(text) or {}).get("examples") or {})
    return problems, examples


def self_test(flows):
    """Each way the check fails, planted in a copy: an edited file, a file added, and a module the checker now
    refuses."""
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        for plant, expect in (("edit", "is not as frozen"), ("add", "was added"), ("refuse", "schema")):
            copy = pathlib.Path(tmp) / plant
            shutil.copytree(CORPUS, copy)
            if plant == "edit":
                p = copy / "approvals.yaml"
                p.write_text(p.read_text() + "\n# an edit\n")
            elif plant == "add":
                (copy / "extra.yaml").write_text("module: extra\n")
            else:
                # a description the checker would now refuse, its digest made to match, so only the check finds it
                p = copy / "approvals.yaml"
                p.write_text(p.read_text().replace("types:", "unheard_of: 1\ntypes:", 1))
                m = json.loads((copy / "format-1.json").read_text())
                m["digests"]["approvals.yaml"] = "sha256:" + hashlib.sha256(p.read_bytes()).hexdigest()
                (copy / "format-1.json").write_text(json.dumps(m))
            problems, _ = held(copy, flows)
            caught = any(expect in p for p in problems)
            ok &= caught
            print(f"  planted {plant}: {'caught' if caught else 'MISSED'}")
    return ok


def main():
    flows = load_checker()
    problems, examples = held(CORPUS, flows)
    for p in problems:
        print(f"  {p}")
    manifest = json.loads((CORPUS / "format-1.json").read_text())
    ok = self_test(flows)
    print(f"format 1's corpus: {len(manifest['bundles'])} bundle(s), {len(manifest['digests'])} file(s) as frozen, "
          f"{examples} example(s) on the reference runner: "
          + ("held" if not problems else f"{len(problems)} break(s)"))
    return 1 if problems or not ok else 0


if __name__ == "__main__":
    sys.exit(main())
