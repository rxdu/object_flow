"""The repository's scripts, which the server runs flows with.

The reference runner and the flow checker are the Python oracle of ADR-0132
decision 2; the server imports them as they are and adds no rule of its own
(`docs/design/flow-studio.md` §2).
"""

import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[4]
SCRIPTS = ROOT / "scripts"
EXAMPLES = ROOT / "docs/design/flow-format/examples"

if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import flowexpr  # noqa: E402
import flowrun  # noqa: E402
import flowpublish  # noqa: E402


def _script(name: str, file: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


flows = _script("check_flows", "check-flows.py")
diagrams = _script("flow_diagram", "flow-diagram.py")

__all__ = ["EXAMPLES", "ROOT", "diagrams", "flowexpr", "flowrun", "flows"]
