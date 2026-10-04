"""The studio's own tooling: which flows it can open, and their diagrams.

None of this is the engine's: it reads description files from the
repository, as an author's editor would (`docs/design/flow-studio.md` §2).
"""

from __future__ import annotations

from .scripts import EXAMPLES, diagrams, flows

# every module is checked with the people module first, as the flow checker
# checks the design's examples (`scripts/check-flows.py`, main)
BASE = "people.yaml"


class Unknown(Exception):
    """A file or type the catalogue does not hold."""


def _doc(name: str) -> dict:
    path = EXAMPLES / name
    if name.endswith(".examples.yaml") or not path.is_file():
        raise Unknown(f"no module file {name}")
    return flows.load(path.read_text())


def modules() -> list[dict]:
    """Every example module, with what it imports and its examples file."""
    out = []
    for path in sorted(EXAMPLES.glob("*.yaml")):
        if path.name.endswith(".examples.yaml"):
            continue
        doc = flows.load(path.read_text())
        examples = path.with_name(path.stem + ".examples.yaml")
        out.append({
            "file": path.name,
            "module": doc["module"],
            "imports": list(doc.get("imports") or {}),
            "types": list(doc.get("types") or {}),
            "examples": examples.name if examples.exists() else None,
        })
    return out


def _file_of(module: str) -> str:
    """The file that declares a module: the one named after it, since a
    second version of a module keeps its name (`inventory-v2.yaml`)."""
    if (EXAMPLES / f"{module}.yaml").is_file():
        return f"{module}.yaml"
    raise Unknown(f"no file declares the module {module}")


def bundle(name: str) -> dict:
    """A module's files in import order, its own last, and its examples,
    as text and as the flow checker's strict loader reads them, so a
    browser needs no YAML parser of its own."""
    order: list[str] = []

    def visit(file: str) -> None:
        if file in order:
            return
        for m in _doc(file).get("imports") or {}:
            visit(_file_of(m))
        order.append(file)

    visit(BASE)
    visit(name)
    examples = EXAMPLES / name.replace(".yaml", ".examples.yaml")
    return {
        "files": [{"name": f, "text": (EXAMPLES / f).read_text()}
                  for f in order],
        "examples": {"name": examples.name, "text": examples.read_text(),
                     "doc": flows.load(examples.read_text())}
        if examples.exists() else None,
    }


def diagram(name: str, type_name: str) -> str:
    """A type's lifecycle as the documents draw it (`flow-diagram.py`)."""
    doc = _doc(name)
    if type_name not in (doc.get("types") or {}):
        raise Unknown(f"{name} declares no type {type_name}")
    return diagrams.diagram(doc, type_name)
