#!/usr/bin/env python3
"""Parse and validate every JSON example in the renderers document.

The tool schema is the one a model is handed, so "it parses" is not enough:
its `input_schema` must be a schema a validator accepts, and it must accept
and reject the calls it claims to.
"""
import re
import sys
import json
import pathlib

DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/renderers.md"


def main():
    blocks = re.findall(r"```json\n(.*?)```", DOC.read_text(), flags=re.S)
    if not blocks:
        print("no JSON blocks found"); return 1

    parsed = []
    for i, b in enumerate(blocks, 1):
        try:
            parsed.append(json.loads(b))
        except json.JSONDecodeError as e:
            print(f"{DOC.name}: block {i} is not JSON: {e}"); return 1
    print(f"{DOC.name}: {len(parsed)} JSON blocks parse")

    try:
        import jsonschema
    except ImportError:
        print("  jsonschema not installed; schema validity unchecked")
        return 0

    validator = jsonschema.Draft7Validator
    checked = 0
    for i, doc in enumerate(parsed, 1):
        if not (isinstance(doc, dict) and "input_schema" in doc):
            continue
        schema = doc["input_schema"]
        try:
            validator.check_schema(schema)
        except jsonschema.SchemaError as e:
            print(f"  block {i}: input_schema is not a valid schema: {e.message}")
            return 1
        v = validator(schema)
        required = schema.get("required", [])
        if required and v.is_valid({}):
            print(f"  block {i}: schema requires {required} and accepts {{}}")
            return 1
        if schema.get("additionalProperties") is False and \
           v.is_valid({k: "x" for k in required} | {"__unknown__": 1}):
            print(f"  block {i}: schema forbids extra properties and accepts one")
            return 1
        checked += 1
    print(f"  {checked} tool schema(s) valid, and enforcing what they declare")
    return 0


sys.exit(main())
