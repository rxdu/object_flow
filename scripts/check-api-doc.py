#!/usr/bin/env python3
"""Execute the library API document's Python, and check it against the model.

Two things go wrong with an API document.  The code stops being valid, which
executing it catches.  And it drifts from the model it is supposed to expose,
which only a comparison catches -- so this reads the operation names out of
DESIGN.md section 10 and requires the Protocol to offer exactly those, plus the
write path and the operational calls the same section names.
"""
import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/design/library-api.md"
MODEL = ROOT / "docs/DESIGN.md"
# not read operations: the write path of section 6, and the two calls section 10
# names as "operational calls that are not object operations"
EXTRA = {"request", "batch", "acknowledge", "publish"}


def main():
    text = DOC.read_text()
    blocks = re.findall(r"```python\n(.*?)```", text, flags=re.S)
    if not blocks:
        print("no Python blocks found"); return 1

    ns: dict = {}
    try:
        exec(compile("\n".join(blocks), str(DOC), "exec"), ns)
    except Exception as e:
        print(f"{DOC.name}: does not execute: {type(e).__name__}: {e}")
        return 1

    store = ns.get("Store")
    if store is None:
        print(f"{DOC.name}: no Store protocol defined"); return 1
    offered = {n for n, v in vars(store).items()
               if callable(v) and not n.startswith("_")}

    model = MODEL.read_text()
    sec = model[model.index("## 10. The read surface"):model.index("## 11.")]
    declared = set(re.findall(r"\*\*(\w+)\*\*\(", sec)) | EXTRA

    missing = sorted(declared - offered)
    extra = sorted(offered - declared)
    print(f"{DOC.name}: {len(blocks)} Python blocks execute, "
          f"{len(offered)} operations")
    if missing:
        print("  the model declares, the API does not offer: " + ", ".join(missing))
    if extra:
        print("  the API offers, the model does not declare: " + ", ".join(extra))
    if missing or extra:
        return 1
    print("the API offers exactly what the model declares")
    return 0


sys.exit(main())
