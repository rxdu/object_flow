"""Slice 1's acceptance: every example replays through the wire with the
verdict the reference runner gives it (`docs/design/flow-studio.md` §6)."""

import pathlib
import tempfile

import pytest

from flowstudio.catalogue import Catalogue
from flowstudio.sandbox import build
from flowstudio.scripts import flowpublish, flowrun, flows
from replay import replay

# the design's modules, read through a catalogue whose workspace is empty
DESIGN = Catalogue(pathlib.Path(tempfile.mkdtemp()))
FILES = [m["file"] for m in DESIGN.modules()
         if m["examples"] and m["source"] == "design"]

# what an example may wait on, and nothing else (ADR-0150, Consequences)
def oracle(module_file: str) -> dict[str, str]:
    """Each example's outcome as the flow checker's step 6 decides it."""
    bundle = DESIGN.bundle(module_file)
    files = [(f["name"], f["text"]) for f in bundle["files"]]
    library, modules = build(files)
    examples = bundle["examples"]
    doc = flows.load(examples["text"])
    # the version run, and a `publish` step's files read beside it, as the
    # checker gives them (ADR-0166 decision 3)
    source = {flowpublish.declared_name(t) or n: t
              for n, t in files + [(examples["name"], examples["text"])]}
    return {name: outcome
            for name, outcome, _ in flowrun.run_examples(
                doc, library, modules, examples["name"],
                flows.beside(flows.EXAMPLES), flows, source)}


@pytest.mark.parametrize("module_file", FILES)
def test_examples_replay_through_the_wire(wire, module_file):
    want = oracle(module_file)
    got = replay(wire, module_file)
    assert set(got) == set(want)
    # every example runs through the wire since the import did (ADR-0154)
    for name, (outcome, why) in got.items():
        assert outcome == want[name], (name, outcome, why)


def test_every_examples_file_is_replayed():
    assert len(FILES) == 10
