#!/usr/bin/env python3
"""Every expression in the design's flow modules, with the tree `flowexpr.py`
parses it into, as JSON lines: the reference the Rust core's parser is held to
(ADR-0132 decision 2, ADR-0153).

Each line is {"module": file, "value": the expression as YAML gives it, a
string, a boolean or a number, "tree": the tree, tuples written as arrays}.
The flow checker's strict loader reads each module, as it does when checking.

With `--parse`, it parses the JSON values given on stdin instead, one a line,
writing each with its tree or with the error parsing raises.
"""
import importlib.util
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import flowexpr  # noqa: E402

_spec = importlib.util.spec_from_file_location("check_flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(flows)


def tree(node):
    if isinstance(node, tuple) or isinstance(node, list):
        return [tree(x) for x in node]
    return node


def main():
    if "--parse" in sys.argv[1:]:
        # the values on stdin, one JSON value a line, each with its tree, or
        # with the parse error it raises
        for line in sys.stdin:
            value = json.loads(line)
            try:
                print(json.dumps({"value": value, "tree": tree(flowexpr.parse(value))}))
            except flowexpr.ParseError as e:
                print(json.dumps({"value": value, "error": str(e)}))
        return 0
    for path in sorted((ROOT / "docs/design/flow-format/examples").glob("*.yaml")):
        if path.name.endswith(".examples.yaml"):
            continue
        doc = flows.load(path.read_text())
        for _where, value in flowexpr.expressions(doc):
            print(json.dumps({"module": path.name, "value": value, "tree": tree(flowexpr.parse(value))}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
