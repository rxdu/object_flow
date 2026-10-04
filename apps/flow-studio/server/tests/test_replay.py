"""Slice 1's acceptance: every example replays through the wire with the
verdict the reference runner gives it (`docs/design/flow-studio.md` §6)."""

import pytest

from flowstudio import catalogue
from flowstudio.sandbox import build
from flowstudio.scripts import flowrun, flows
from replay import replay

FILES = [m["file"] for m in catalogue.modules() if m["examples"]]

# what an example may wait on, and nothing else (ADR-0150, Consequences)
WAITING = ("import_batch",)


def oracle(module_file: str) -> dict[str, str]:
    """Each example's outcome as the flow checker's step 6 decides it."""
    bundle = catalogue.bundle(module_file)
    library, modules = build([(f["name"], f["text"])
                              for f in bundle["files"]])
    doc = flows.load(bundle["examples"]["text"])
    return {name: outcome
            for name, outcome, _ in flowrun.run_examples(doc, library,
                                                         modules)}


@pytest.mark.parametrize("module_file", FILES)
def test_examples_replay_through_the_wire(wire, module_file):
    want = oracle(module_file)
    got = replay(wire, module_file)
    assert set(got) == set(want)
    for name, (outcome, why) in got.items():
        if outcome == "waiting":
            assert any(w in why for w in WAITING), (name, why)
            continue
        assert outcome == want[name], (name, outcome, why)


def test_every_examples_file_is_replayed():
    assert len(FILES) == 10
