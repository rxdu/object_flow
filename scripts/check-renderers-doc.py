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
    return rendered_as_shown([d for d in parsed if isinstance(d, dict) and "input_schema" in d])


def rendered(files):
    """The tools `scripts/flowtools.py` renders for an agent over module files given in import order, as the Flow
    Studio's sandbox loads them (ADR-0173 decision 10)."""
    import importlib.util
    root = DOC.parents[2]
    sys.path.insert(0, str(root / "scripts"))
    import flowrun
    import flowtools
    spec = importlib.util.spec_from_file_location("check_flows", root / "scripts/check-flows.py")
    flows = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(flows)
    library, modules = {}, {}
    for text in files:
        doc = flows.load(text)
        flows.literals(doc)
        modules[doc["module"]] = doc
        view, _back = flows.view(doc, library)
        library.update(dict(flows.types(view)))
    enums = {n: list(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
    kinds = dict(library)
    kinds.update(flowrun.observation_kinds(modules))
    declared, homes = flowrun.declarations(modules)
    store = flowrun.Store(kinds, enums, declared, homes)
    types = [tn for m in modules.values() for tn in (m.get("types") or {})]
    return {t["name"]: t for t in flowtools.render(store.kinds, store.enums, types, store.decl.metrics, "agent")}


def rendered_as_shown(examples):
    """Each tool the document shows is the one the renderer gives for the design's module it names, but that the
    metrics tool's names are a sample of the renderer's (renderers.md §3)."""
    examples_dir = DOC.parent / "flow-format/examples"
    sets = [["people.yaml", "inventory.yaml", "delivery.yaml"], ["people.yaml", "service.yaml"]]
    tools = {}
    for names in sets:
        tools.update(rendered([(examples_dir / n).read_text() for n in names]))
    for doc in examples:
        got = tools.get(doc["name"])
        if got is None:
            print(f"  the tool {doc['name']} is rendered from none of the design's modules")
            return 1
        if doc["name"] == "read_metric":
            shown = doc["input_schema"]["properties"]["name"]["enum"]
            have = got["input_schema"]["properties"]["name"]["enum"]
            if not set(shown) <= set(have):
                print(f"  read_metric names {sorted(set(shown) - set(have))}, which the renderer does not")
                return 1
            doc = json.loads(json.dumps(doc))
            doc["input_schema"]["properties"]["name"]["enum"] = have
        if doc != got:
            print(f"  the tool {doc['name']} differs from the renderer's:\n{json.dumps(got, indent=2)}")
            return 1
    print(f"  {len(examples)} tool(s) as the renderer gives them")
    return 0


sys.exit(main())
