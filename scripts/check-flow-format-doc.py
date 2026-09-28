#!/usr/bin/env python3
"""Hold the flow format specification to its implementation.

docs/design/flow-format.md states the YAML flow format; flow.schema.json and
scripts/check-flows.py implement it. This checks that they agree:

1. the reserved keys of §10 are exactly the property names the schema defines;
2. the reserved values of §10 are exactly the schema's enumerated values, the
   two booleans, and the dimensions the checker builds in;
3. the finding codes of §7 are exactly those the checker reports;
4. the declaration order of §3 is the order the checker enforces;
5. every name the checker reserves (§3, §4.3) is stated in the document;
6. every complete example in the document (a yaml block beginning `module:`)
   is a valid description;
7. the example keys and values of §10 are exactly those examples.schema.json
   defines, with the alias `operator` and the clock's `now` the checker reads
   (§11).
"""
import importlib.util
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/design/flow-format.md"
SCHEMA = ROOT / "docs/design/flow-format/flow.schema.json"
EXAMPLES_SCHEMA = ROOT / "docs/design/flow-format/examples.schema.json"
CHECKER = ROOT / "scripts/check-flows.py"


def schema_vocabulary(schema):
    keys, values = set(), set()

    def walk(node):
        if isinstance(node, dict):
            keys.update((node.get("properties") or {}).keys())
            for v in node.get("enum", []):
                values.add(v)
            if "const" in node:
                values.add(node["const"])
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)
    walk(schema)
    return keys, values


def listed(doc, label):
    m = re.search(rf"^\*\*{label}:\*\* (.*)$", doc, re.M)
    return set(re.findall(r"`([^`]+)`", m.group(1))) if m else None


def main():
    doc = (pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DOC).read_text()
    schema = json.loads(SCHEMA.read_text())
    spec = importlib.util.spec_from_file_location("trial", CHECKER)
    trial = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(trial)
    source = CHECKER.read_text()
    findings = []

    keys, values = schema_vocabulary(schema)
    values |= {True, False}
    values = {str(v).lower() if isinstance(v, bool) else v for v in values}
    values |= {"month", "week", "actor"}
    values |= trial.BUILT_IN_CATEGORIES      # a category every vocabulary has, whether or not it is listed
    for label, want in (("Keys", keys), ("Values", values)):
        got = listed(doc, label)
        if got is None:
            findings.append(f"§10 has no **{label}:** line")
            continue
        for w in sorted(want - got):
            findings.append(f"§10 {label.lower()} omits `{w}`, which the implementation defines")
        for w in sorted(got - want):
            findings.append(f"§10 {label.lower()} lists `{w}`, which the implementation does not define")

    ekeys, evalues = schema_vocabulary(json.loads(EXAMPLES_SCHEMA.read_text()))
    evalues |= {"operator", "now"}
    for label, want in (("Example keys", ekeys), ("Example values", evalues)):
        got = listed(doc, label)
        if got is None:
            findings.append(f"§10 has no **{label}:** line")
            continue
        for w in sorted(want - got):
            findings.append(f"§10 {label.lower()} omits `{w}`, which examples.schema.json or the checker defines")
        for w in sorted(got - want):
            findings.append(f"§10 {label.lower()} lists `{w}`, which neither examples.schema.json nor the checker defines")

    # a finding is (path, code, message): the code is the lower-case literal just before the message
    reported = set(re.findall(r'"([a-z]+)",\s*f"', source)) | {"yaml", "schema"}
    section7 = doc[doc.index("## 7. Validity"):doc.index("## 8.")]
    stated = {c for c in re.findall(r"^\|[^|]*\| `([a-z]+)`", section7, re.M) if c != "check"}
    notice = re.search(r"reports \w+ notices[^.]*\.", section7)
    stated |= set(re.findall(r"`([a-z]+)`", notice.group(0))) if notice else set()
    for c in sorted(reported - stated):
        findings.append(f"§7 does not state the finding code `{c}`, which the checker reports")
    for c in sorted(stated - reported):
        findings.append(f"§7 states the finding code `{c}`, which the checker never reports")

    section3 = re.search(r"^## 3\..*?(?=^## 4\.)", doc, re.M | re.S)
    lists = re.sub(r"`,? and `|`, `", ", ", section3.group(0) if section3 else "").replace("`", "")
    for name, order in (("module", trial.MODULE_ORDER), ("machine", trial.MACHINE_ORDER), ("type", trial.TYPE_ORDER),
                        ("metric", trial.METRIC_ORDER)):
        if ", ".join(order) not in lists:
            findings.append(f"§3 does not state the {name} order the checker enforces: {', '.join(order)}")

    reserved = trial.NOT_A_CATEGORY | trial.NOT_A_TRANSITION | trial.NOT_AN_ATTRIBUTE | trial.ONLY_ON_OBSERVATIONS | trial.ONLY_ON_TYPES
    reserved |= trial.OBJECT_MEMBERS | trial.MIRROR_MEMBERS | trial.SHADOWING
    for w in sorted(reserved):
        if f"`{w}`" not in doc:
            findings.append(f"the document never names `{w}`, which the checker reserves or restricts")
    for pattern in ("_provided", "_invariant"):
        if pattern not in doc:
            findings.append(f"the document does not reserve the generated names ending in {pattern}")

    examples = re.findall(r"```yaml\n(module:.*?)```", doc, re.S)
    for i, text in enumerate(examples, 1):
        found, _notes = trial.check([(f"example {i}", text)])
        for f, line, _sev, code, msg in found:
            findings.append(f"§ example {i} is not valid: line {line} {code} {msg}")

    for f in findings:
        print(f"  {f}")
    print(f"flow-format.md: {len(keys)} keys and {len(values)} values against the schema, "
          f"{len(reported)} finding codes, {len(examples)} example(s): "
          + ("consistent" if not findings else f"{len(findings)} finding(s)"))
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
