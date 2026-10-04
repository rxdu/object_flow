"""Slice 3: the studio's editing tools, over a workspace of the test's own
(`docs/design/flow-studio.md` §3.1, §5)."""

import pytest

from flowstudio.scripts import flows

# an example in the examples format, as the page would write a session
ANA_OPENS_A_JOB = {
    "description": "Ana opens a job for herself, and it is open.",
    "steps": [
        {"request": {"type": "User", "transition": "add", "as": "ana",
                     "inputs": {"login": "ana", "name": "Ana"}}},
    ],
    "request": {"type": "ServiceJob", "transition": "open", "as": "job",
                "actor": "ana", "inputs": {"engineer": "ana"}},
    "expect": {"verdict": "applied", "state": "OPEN"},
}


def fault(call):
    with pytest.raises(Exception) as e:
        call()
    return str(e.value)


def test_the_design_is_read_only_and_a_copy_is_editable(wire, workspace):
    design = [m for m in wire.get("/studio/modules")
              if m["source"] == "design"]
    assert design and not any(m["editable"] for m in design)
    copied = wire.post("/studio/copy", {"file": "service.yaml"})
    assert copied["copied"] == ["service.yaml", "service.examples.yaml"]
    assert (workspace / "service.yaml").is_file()
    mine = [m for m in wire.get("/studio/modules")
            if m["source"] == "workspace"]
    assert [m["file"] for m in mine] == ["service.yaml"] and mine[0]["editable"]
    assert "409" in fault(lambda: wire.post("/studio/copy",
                                            {"file": "service.yaml"}))


def test_a_save_stays_inside_the_workspace(wire, workspace):
    for name in ("../escape.yaml", "/tmp/escape.yaml", "a/b.yaml", "x.txt"):
        assert "404" in fault(lambda n=name: wire.post(
            "/studio/save", {"file": n, "text": "module: x\n"}))
    assert not (workspace.parent / "escape.yaml").exists()
    saved = wire.post("/studio/save", {"file": "draft.yaml",
                                       "text": "module: draft\n"})
    assert saved == {"saved": "draft.yaml"}
    assert (workspace / "draft.yaml").read_text() == "module: draft\n"
    (workspace / "draft.yaml").unlink()


def test_a_text_is_checked_and_drawn_as_it_is_edited(wire):
    bundle = wire.get("/studio/module?file=service.yaml")
    files = bundle["files"]
    clean = wire.post("/studio/check", {"files": files})
    assert clean["fatal"] == 0
    broken = files[-1]["text"].replace("guards:", "guard:", 1)
    found = wire.post("/studio/check", {"files": files[:-1] + [
        {"name": "service.yaml", "text": broken}]})
    assert found["fatal"] > 0
    first = next(f for f in found["findings"] if f["severity"] == "fatal")
    assert first["file"] == "service.yaml" and first["line"] > 0
    drawn = wire.post("/studio/draw", {"text": files[-1]["text"],
                                       "type": "ServiceJob"})
    assert drawn["mermaid"].splitlines()[1] == "stateDiagram-v2"
    assert "400" in fault(lambda: wire.post(
        "/studio/draw", {"text": "a: [", "type": "ServiceJob"}))


def test_an_example_is_saved_only_if_it_holds(wire, workspace):
    if not (workspace / "service.yaml").exists():
        wire.post("/studio/copy", {"file": "service.yaml"})
    path = workspace / "service.examples.yaml"
    before = path.read_text()
    # a claim the checker refuses before running it, and one only running
    # the example against the rules refutes
    for expect, code in (({"verdict": "applied", "state": "WORKING"}, "names"),
                         ({"verdict": "applied", "state": "OPEN",
                           "values": {"handoffs": 3}}, "example")):
        refused = wire.post("/studio/examples", {
            "module": "service.yaml", "name": "ana_opens_a_job",
            "example": dict(ANA_OPENS_A_JOB, expect=expect)})
        assert refused["saved"] is False
        assert any(f["severity"] == "fatal" and f["code"] == code
                   for f in refused["findings"]), refused["findings"]
        assert path.read_text() == before
    held = wire.post("/studio/examples", {
        "module": "service.yaml", "name": "ana_opens_a_job",
        "example": ANA_OPENS_A_JOB})
    assert held["saved"] is True, held.get("findings")
    doc = flows.load(path.read_text())
    assert doc["examples"]["ana_opens_a_job"]["expect"]["state"] == "OPEN"
    assert "409" in fault(lambda: wire.post("/studio/examples", {
        "module": "service.yaml", "name": "ana_opens_a_job",
        "example": ANA_OPENS_A_JOB}))
    assert "409" in fault(lambda: wire.post("/studio/examples", {
        "module": "service.yaml", "name": "Not A Name",
        "example": ANA_OPENS_A_JOB}))
