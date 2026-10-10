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


# a metric's expected value, a row it does not read, and a metric no module
# declares, each of which fails its example (ADR-0173 decision 6)
WRONG_METRICS = [
    ("ServiceJob.handoffs_per_job: [{ month: 2026-01, value: 1 }]",
     "ServiceJob.handoffs_per_job: [{ month: 2026-01, value: 2 }]"),
    ("ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }]",
     "ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }, "
     "{ state: OPEN, value: 0 }]"),
    ("ServiceJob.transition_counts: 3", "ServiceJob.transition_counts: 4"),
    ("ServiceJob.transition_counts: 3", "ServiceJob.transition_count: 3"),
    ("value: 2 h }]", "value: 3 h }]"),
]


def with_metric(old: str, new: str) -> dict:
    """The service module's bundle with one metric expectation changed."""
    bundle = DESIGN.bundle("service.yaml")
    text = bundle["examples"]["text"]
    assert text.count(old) == 1, old
    return dict(bundle, examples=dict(bundle["examples"],
                                      text=text.replace(old, new)))


@pytest.mark.parametrize("old,new", WRONG_METRICS)
def test_a_wrong_metric_expectation_fails(wire, old, new):
    got = replay(wire, "service.yaml", bundle=with_metric(old, new),
                 read=flows.beside(flows.EXAMPLES))
    assert got["a_reassignment_is_measured_as_a_handoff"][0] == "failed"


def identity_bundle() -> dict:
    """An approval recorded from who acts, read by a metric in an example
    by the identity it records, which is also the user's alias (D568)."""
    return {"files": [{"name": "approve.yaml", "text": flows.IDENTITY_FLOW}],
            "examples": {"name": "approve.examples.yaml",
                         "text": flows.IDENTITY_EXAMPLES}}


def test_an_identity_that_is_also_an_alias_reads_as_written(wire):
    got = replay(wire, "approve.yaml", bundle=identity_bundle())
    assert got == {"an_approval_is_counted_by_its_approver": ("passed", "")}


def test_every_examples_file_is_replayed():
    assert len(FILES) == 10
