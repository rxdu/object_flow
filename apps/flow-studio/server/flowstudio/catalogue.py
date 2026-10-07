"""The studio's own tooling: the flows it can open, edit and check.

None of this is the engine's: it reads and writes description files, as an
author's editor would (`docs/design/flow-studio.md` §2, §3.1, §5).

Two places hold flows. The design's example modules are read-only, since
they are the corpus the repository's checkers hold the design to. The
workspace holds the author's own flows, and copies of the design's to edit;
only the workspace is written, and only by a name that stays inside it.
"""

from __future__ import annotations

import pathlib
import re

import yaml

from .scripts import EXAMPLES, diagrams, flows

# every module is checked with the people module first, as the flow checker
# checks the design's examples (`scripts/check-flows.py`, main)
BASE = "people.yaml"
SOURCES = ("workspace", "design")
NAME = re.compile(r"^[a-z0-9][a-z0-9_-]*(\.examples)?\.yaml$")


class Unknown(Exception):
    """A file, type or source the catalogue does not hold."""


class Refused(Exception):
    """A write the catalogue will not make."""


class Catalogue:
    def __init__(self, workspace: pathlib.Path):
        self.workspace = workspace
        self.workspace.mkdir(parents=True, exist_ok=True)

    # ── where a file is ───────────────────────────────────────────────────
    def root(self, source: str) -> pathlib.Path:
        if source == "workspace":
            return self.workspace
        if source == "design":
            return EXAMPLES
        raise Unknown(f"no source {source}")

    def path(self, source: str, name: str) -> pathlib.Path:
        if not NAME.match(name or ""):
            raise Unknown(f"no module file {name}")
        return self.root(source) / name

    def _doc(self, source: str, name: str) -> dict:
        p = self.path(source, name)
        if name.endswith(".examples.yaml") or not p.is_file():
            raise Unknown(f"no module file {name} in the {source}")
        return flows.load(p.read_text())

    def _file_of(self, module: str) -> tuple[str, str]:
        """The file that declares a module, the workspace's before the
        design's: the one named after it, since a second version of a module
        keeps its name (`inventory-v2.yaml`)."""
        for source in SOURCES:
            if self.path(source, f"{module}.yaml").is_file():
                return source, f"{module}.yaml"
        raise Unknown(f"no file declares the module {module}")

    # ── reading ───────────────────────────────────────────────────────────
    def modules(self) -> list[dict]:
        """Every module the studio can open, the workspace's first."""
        out = []
        for source in SOURCES:
            for path in sorted(self.root(source).glob("*.yaml")):
                if path.name.endswith(".examples.yaml"):
                    continue
                try:
                    doc = flows.load(path.read_text())
                except yaml.YAMLError:
                    doc = None
                examples = path.with_name(path.stem + ".examples.yaml")
                out.append({
                    "file": path.name,
                    "source": source,
                    "editable": source == "workspace",
                    "module": (doc or {}).get("module"),
                    "imports": list((doc or {}).get("imports") or {}),
                    "types": list((doc or {}).get("types") or {}),
                    "examples": examples.name if examples.exists() else None,
                })
        return out

    def bundle(self, name: str, source: str = "design") -> dict:
        """A module's files in import order, its own last, and its examples,
        as text and as the flow checker's strict loader reads them, so a
        browser needs no YAML parser of its own. A module it imports is
        taken from the workspace where the workspace has it."""
        order: list[tuple[str, str]] = []

        def visit(src: str, file: str) -> None:
            if (src, file) in order or any(f == file for _, f in order):
                return
            for m in self._doc(src, file).get("imports") or {}:
                visit(*self._file_of(m))
            order.append((src, file))

        base = next(s for s in SOURCES if self.path(s, BASE).is_file())
        visit(base, BASE)
        visit(source, name)
        examples = self.path(source, name.replace(".yaml", ".examples.yaml"))
        return {
            "files": [{"name": f, "source": s,
                       "text": self.path(s, f).read_text()}
                      for s, f in order],
            "examples": {"name": examples.name, "text": examples.read_text(),
                         "doc": flows.load(examples.read_text())}
            if examples.exists() else None,
        }

    def diagram(self, name: str, type_name: str, source: str = "design",
                text: str | None = None) -> str:
        """A type's lifecycle as the documents draw it (`flow-diagram.py`),
        from a file or from text being edited."""
        doc = flows.load(text) if text is not None \
            else self._doc(source, name)
        if not isinstance(doc, dict) or type_name not in (doc.get("types")
                                                          or {}):
            raise Unknown(f"the module declares no type {type_name}")
        return diagrams.diagram(doc, type_name)

    @staticmethod
    def check(files: list[dict], read=None) -> dict:
        """The flow checker's findings over files given in import order: what
        the editor marks against its lines (`flow-format.md` §11.5); `read`
        reads a file an example's `publish` step names."""
        found, notes = flows.check([(f["name"], f["text"]) for f in files],
                                   None, None, None, read)
        as_json = [{"file": f, "line": line, "severity": sev, "code": code,
                    "message": msg} for f, line, sev, code, msg in found]
        as_json += [{"file": f, "line": line, "severity": sev, "code": code,
                     "message": msg} for f, line, sev, code, msg in notes]
        return {"findings": as_json, "fatal": len(found)}

    # ── writing, the workspace only ───────────────────────────────────────
    def save(self, name: str, text: str) -> dict:
        """A module or examples file written to the workspace, as it is: a
        half-written module is the author's to keep, and loading it into the
        sandbox is a separate act (`flow-studio.md` §3.1)."""
        path = self.path("workspace", name)
        path.write_text(text)
        return {"saved": name}

    def copy(self, name: str) -> dict:
        """A design module, and its examples file, copied to the workspace
        to be edited; an existing copy is never overwritten."""
        src = self.path("design", name)
        if not src.is_file():
            raise Unknown(f"no design module {name}")
        dst = self.path("workspace", name)
        if dst.exists():
            raise Refused(f"the workspace already holds {name}")
        dst.write_text(src.read_text())
        copied = [name]
        ex = src.with_name(src.stem + ".examples.yaml")
        if ex.exists() and not self.path("workspace", ex.name).exists():
            self.path("workspace", ex.name).write_text(ex.read_text())
            copied.append(ex.name)
        return {"copied": copied}

    def beside(self, source: str):
        """A reader of the files beside an examples file of a source, which
        an example's `publish` step names, each found in that source and
        else in the design's, as a module's imports are (ADR-0166 decision
        3); none for a name that is not a plain module file's."""
        def read(name: str) -> str | None:
            for s in dict.fromkeys((source, "design")):
                try:
                    p = self.path(s, name)
                except Unknown:
                    return None
                if p.is_file():
                    return p.read_text()
            return None
        return read

    def add_example(self, module_file: str, name: str, example: dict) -> dict:
        """An example appended to a workspace module's examples file, created
        if it has none, and written only if the flow checker passes the file
        with it, the example run against the rules among its steps
        (`flow-format.md` §11.5): an example is a claim, and one that does
        not hold is not saved."""
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name or ""):
            raise Refused("an example's name is a snake_case identifier")
        if not str(example.get("description", "")).strip():
            raise Refused("an example says what it shows, in its description")
        doc = self._doc("workspace", module_file)
        path = self.path("workspace",
                         module_file.replace(".yaml", ".examples.yaml"))
        if path.exists():
            text = path.read_text()
            held = flows.load(text) or {}
            if name in (held.get("examples") or {}):
                raise Refused(f"the examples file already has {name}")
            keys = re.findall(r"^([A-Za-z_][\w-]*):", text, re.M)
            if not keys or keys[-1] != "examples":
                raise Refused("the examples file does not end with its "
                              "examples, so one cannot be appended")
        else:
            text = f"examples_for: {doc['module']}\n\nexamples:\n"
        body = yaml.safe_dump({name: example}, sort_keys=False,
                              allow_unicode=True, width=100)
        text = text.rstrip("\n") + "\n\n" + "".join(
            f"  {line}\n" if line else "\n" for line in body.splitlines())
        bundle = self.bundle(module_file, "workspace")
        files = [{"name": f["name"], "text": f["text"]}
                 for f in bundle["files"]] + [{"name": path.name,
                                               "text": text}]
        checked = self.check(files, self.beside("workspace"))
        if checked["fatal"]:
            return {"saved": False, "findings": checked["findings"],
                    "text": text}
        path.write_text(text)
        return {"saved": True, "file": path.name, "text": text}
