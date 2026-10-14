"""The Rust core's first slice judged by the replay (ADR-0153 decision 6):
every example of the design played through the core's service as through the
reference runner's, and none failing. Each passes with the runner's outcome,
or is not run, naming what it reaches; a failure is a finding against the
core or the runner.

A ratchet holds what the core passes: `core_passing.txt` lists each example
the core has passed, and one that no longer passes fails the test, since a
break that turns a pass into "not run" would otherwise go unseen. An example
newly passing is printed, to be added to the list.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import collections
import pathlib
import re
import subprocess
import tempfile
import threading

import pytest

from flowstudio.catalogue import Catalogue
from flowstudio.scripts import flows
from replay import Wire, replay
from test_replay import WRONG_METRICS, identity_bundle, oracle, with_metric

ROOT = pathlib.Path(__file__).resolve().parents[4]
CORE = ROOT / "core"
CARGO = pathlib.Path.home() / ".cargo/bin/cargo"
DESIGN = Catalogue(pathlib.Path(tempfile.mkdtemp()))
FILES = [m["file"] for m in DESIGN.modules()
         if m["examples"] and m["source"] == "design"]
PASSING = pathlib.Path(__file__).with_name("core_passing.txt")


@pytest.fixture(scope="module")
def core():
    subprocess.run([str(CARGO), "build", "-q", "-p", "objectflow-service"],
                   cwd=CORE, check=True)
    service = subprocess.Popen(
        [str(CORE / "target/debug/objectflow-service"), "--port", "0"],
        stderr=subprocess.PIPE, text=True)
    line = service.stderr.readline()
    m = re.search(r"listening on (http://127\.0\.0\.1:\d+)", line)
    assert m, f"the core's service did not start: {line}"
    # the service writes a line for each answer: read to its end, so a full
    # pipe never stops it
    threading.Thread(target=collections.deque, args=(service.stderr, 0),
                     daemon=True).start()
    yield Wire(m.group(1))
    service.terminate()
    service.wait()


@pytest.mark.parametrize("old,new", WRONG_METRICS)
def test_a_wrong_metric_expectation_fails_in_the_core(core, old, new):
    got = replay(core, "service.yaml", bundle=with_metric(old, new),
                 read=flows.beside(flows.EXAMPLES))
    assert got["a_reassignment_is_measured_as_a_handoff"][0] == "failed"


def test_an_identity_that_is_also_an_alias_reads_as_written_in_the_core(core):
    got = replay(core, "approve.yaml", bundle=identity_bundle())
    assert got == {"an_approval_is_counted_by_its_approver": ("passed", "")}


def test_no_example_fails_in_the_core(core):
    tally = collections.Counter()
    failures, reached = [], collections.Counter()
    passed = set()
    for module_file in FILES:
        want = oracle(module_file)
        # a `publish` step's files read beside the design's examples, as the
        # catalogue would give them (ADR-0166 decision 3)
        got = replay(core, module_file, bundle=DESIGN.bundle(module_file),
                     read=flows.beside(flows.EXAMPLES))
        assert set(got) == set(want)
        for name, (outcome, why) in got.items():
            tally[outcome] += 1
            if outcome == "passed":
                passed.add(f"{module_file} {name}")
            if outcome == "not run":
                reached[why.split(" (")[0][:70]] += 1
            elif outcome != want[name]:
                failures.append(f"{module_file} {name}: the core gives "
                                f"{outcome}, the runner {want[name]}: {why}")
    # the service's migration examples, a file of their own that no change
    # carries, whose publishes read their files beside them (ADR-0166)
    bundle = DESIGN.bundle("service.yaml")
    migration = flows.EXAMPLES / "service-migration.examples.yaml"
    bundle["examples"] = {"name": migration.name, "text": migration.read_text()}
    for name, (outcome, why) in replay(core, "service.yaml", bundle=bundle,
                                       read=flows.beside(flows.EXAMPLES)).items():
        tally[outcome] += 1
        if outcome == "passed":
            passed.add(f"{migration.name} {name}")
        else:
            failures.append(f"{migration.name} {name}: the core gives "
                            f"{outcome}: {why}")
    print(f"\nthe core's first slice: {dict(tally)}")
    for what, n in reached.most_common(12):
        print(f"  {n:3d} not run: {what}")
    assert failures == []
    held = {line.strip() for line in PASSING.read_text().splitlines()
            if line.strip() and not line.startswith("#")}
    for new in sorted(passed - held):
        print(f"  newly passing, to add to {PASSING.name}: {new}")
    assert sorted(held - passed) == [], "examples the core passed no longer pass"
