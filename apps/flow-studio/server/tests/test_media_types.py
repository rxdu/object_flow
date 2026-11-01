"""A file's media types held on every write, through the studio's server on
the reference runner (ADR-0239): an input of a media type its list does not
take refused `wrong_media_type`; the declaration answering each list; an
import of one left to the core's classes; and a version narrowing a list
noticed, the files held kept, a pending proposal holding one invalidated, and
a backfill or a move giving one refused.
"""

import pytest

from replay import NotRun

OP = {"id": "operator"}

ARCHIVE = """module: archive
categories: [live, closed]
types:
  Folder:
    description: A folder of scans.
    tracking: record
    attributes:
      scan:  { type: file, optional: true, media_types: ["image/*", application/pdf] }
      pages: { type: "file[]", media_types: ["image/*"] }
      loose: { type: file, optional: true }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [pages], optional_inputs: [scan, loose] }
      attach: { kind: internal, from: [OPEN], proposable: true, inputs: { upload: { type: file, media_types: [application/pdf] } }, effect: [ { assign: { location: loose, expr: inputs.upload } } ] }
      drop_page: { kind: internal, from: [OPEN], inputs: { raw: { type: file } }, effect: [ { remove: { location: pages, expr: inputs.raw } } ] }
      add_page: { kind: internal, from: [OPEN], inputs: { raw: { type: file } }, effect: [ { add: { location: pages, expr: inputs.raw } } ] }
      shut: { kind: external, from: [OPEN], to: SHUT }
  Album:
    description: An album of photos.
    tracking: record
    observations:
      photos:
        kind: Photo
        description: A photo the album holds.
        attributes:
          shot: { type: file, media_types: [image/jpeg] }
    states:
      KEPT: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      start: { kind: initial, to: KEPT }
      close: { kind: external, from: [KEPT], to: GONE }
  Ledger:
    description: A ledger kept by another system.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
      sheet: { type: file, optional: true, media_types: [application/pdf] }
    states:
      KEPT: { category: live }
"""


def file(media_type: str, size: int = 48213) -> dict:
    """A file as the wire carries it, by its reference (ADR-0173 decision 11)."""
    return {"hash": "sha256:" + "ab" * 32, "size": size,
            "media_type": media_type}


@pytest.fixture()
def archive(wire):
    assert wire.post("/sandbox/load", {"files": [
        {"name": "archive.yaml", "text": ARCHIVE}]})["loaded"]
    wire.post("/sandbox/reset", {})
    return wire


def request(wire, xn: str, oid: str | None = None, **inputs) -> dict:
    body = {"transition": xn, "actor": OP, "inputs": inputs}
    if oid is None:
        body["type"] = "Folder"
    else:
        body["object_id"] = oid
    return wire.post("/v1/request", body)


def made(wire, **inputs) -> str:
    v = request(wire, "open", **inputs)
    assert v["kind"] == "satisfied", v
    return v["object_id"]


def test_an_input_of_another_media_type_says_why(archive):
    """An attribute input is held to its attribute's list, a set's each
    element, and a declared input to its own; a range takes every subtype
    of its type, and a media type is read in lower case."""
    folder = made(archive, scan=file("IMAGE/PNG"), pages=[file("image/gif")])
    v = request(archive, "open", scan=file("video/mp4"), pages=[])
    assert (v["kind"], v["reasons"]) == (
        "invalid_input", {"scan": "wrong_media_type"}), v
    v = request(archive, "open", pages=[file("image/png"),
                                        file("application/pdf")])
    assert v["reasons"] == {"pages": "wrong_media_type"}, v
    # told with every other input not of its type, as a wrong currency is
    v = request(archive, "open", scan=file("video/mp4"), loose={"hash": "x"},
                pages=[])
    assert v["reasons"] == {"loose": "malformed",
                            "scan": "wrong_media_type"}, v
    v = request(archive, "attach", folder, upload=file("image/png"))
    assert (v["kind"], v["reasons"]) == (
        "invalid_input", {"upload": "wrong_media_type"}), v
    assert request(archive, "attach", folder,
                   upload=file("application/pdf"))["kind"] == "satisfied"
    held = archive.post("/v1/get", {"id": folder})["attributes"]
    assert held["scan"]["media_type"] == "image/png", held
    # an observation's field, recorded by its kind's generated creation
    album = archive.post("/v1/request", {"type": "Album", "actor": OP,
                                         "transition": "start"})["object_id"]

    def recorded(media_type: str) -> dict:
        return archive.post("/v1/request", {
            "type": "Photo", "transition": "record", "actor": OP,
            "inputs": {"subject": album, "shot": file(media_type)}})
    v = recorded("image/png")
    assert (v["kind"], v["reasons"]) == (
        "invalid_input", {"shot": "wrong_media_type"}), v
    assert recorded("image/jpeg")["kind"] == "satisfied"


def test_an_effects_write_of_a_file_not_taken_is_refused(archive):
    """An add of a file its set does not take is refused value_in_range; a
    file it takes is added."""
    folder = made(archive, pages=[])
    v = request(archive, "add_page", folder, raw=file("application/pdf"))
    assert (v["kind"], v["clause"], v["remedy"]) == (
        "unsatisfied", "value_in_range", "unreachable_from_here"), v
    assert request(archive, "add_page", folder,
                   raw=file("image/png"))["kind"] == "satisfied"


def test_the_declaration_answers_each_list(archive):
    """`declaration` answers the media types of each attribute, declared
    input and observation field, so a form can give HTML's accept the same
    list."""
    d = archive.post("/v1/declaration", {"actor": OP, "type": "Folder"})
    assert d["attributes"]["scan"]["media_types"] == ["image/*",
                                                      "application/pdf"]
    assert d["transitions"]["attach"]["inputs"]["upload"]["media_types"] \
        == ["application/pdf"]
    assert "media_types" not in d["attributes"]["loose"]
    d = archive.post("/v1/declaration", {"actor": OP, "type": "Album"})
    assert d["observations"]["photos"]["attributes"]["shot"][
        "media_types"] == ["image/jpeg"]
    d = archive.post("/v1/declaration", {"actor": OP, "type": "Photo"})
    assert d["attributes"]["shot"]["media_types"] == ["image/jpeg"], d


def test_an_imported_file_of_another_media_type_is_left_to_the_core(archive):
    """An imported file of a media type its attribute does not take is a
    value not of its type, a class the core's import decides, so the
    reference runner answers not_run; one it takes is imported."""
    def batch(media_type: str, key: str) -> dict:
        return {"actor": OP, "batch": {"objects": [{
            "type": "Ledger", "legacy_key": key, "state": "KEPT",
            "attributes": {"code": key, "sheet": file(media_type)}}]}}
    with pytest.raises(NotRun):
        archive.post("/v1/import_batch", batch("image/png", "l1"))
    assert archive.post("/v1/import_batch",
                        batch("application/pdf", "l2"))["written"] == 1


def drafted(wire, text: str) -> str:
    v = wire.post("/v1/request", {
        "type": "DeclarationChange", "transition": "draft", "actor": OP,
        "inputs": {"source": {"archive": text},
                   "source_ref": "git:archive@2"}})
    assert v["kind"] == "satisfied", v
    return v["object_id"]


def report_of(wire, text: str) -> tuple[str, dict]:
    """A change of the source drafted and dry run: its id and its report."""
    c = drafted(wire, text)
    return c, wire.post("/v1/publish", {"actor": OP, "change": c,
                                        "dry_run": True})["report"]


def installed(wire, c: str) -> None:
    assert wire.post("/v1/request", {"transition": "submit", "actor": OP,
                                     "object_id": c})["kind"] == "satisfied"
    version = wire.post("/v1/get", {"id": c})["version"]
    v = wire.post("/v1/publish", {"actor": OP, "change": c,
                                  "expected_version": version})
    assert v["verdict"]["kind"] == "satisfied", v


NARROWED = ARCHIVE.replace(
    'scan:  { type: file, optional: true, media_types: ["image/*", application/pdf] }',
    'scan:  { type: file, optional: true, media_types: ["image/*"] }').replace(
    'pages: { type: "file[]", media_types: ["image/*"] }',
    'pages: { type: "file[]", media_types: [image/png, image/jpeg] }').replace(
    "loose: { type: file, optional: true }",
    "loose: { type: file, optional: true, media_types: [application/pdf] }")


def test_a_narrowed_list_is_noticed_and_keeps_the_files_held(archive):
    """A version whose list takes fewer media types, one added where there
    was none among them, notices it once and then on each object holding a
    file the new list does not take; the files stay, and a remove takes one
    out, while a list taking more, or the same, is not noticed."""
    pdf = made(archive, scan=file("application/pdf"),
               pages=[file("image/png"), file("image/gif")],
               loose=file("text/plain"))
    png = made(archive, scan=file("image/png"), pages=[file("image/png")],
               loose=file("application/pdf"))
    c, report = report_of(archive, NARROWED)
    message = ("Folder.{0} takes fewer media types than before; a file it "
               "holds of another stays until written")
    for a, held in (("scan", [pdf]), ("pages", [pdf]), ("loose", [pdf])):
        code = f"media_types_narrowed:Folder.{a}"
        notice = [f for f in report["findings"] if f["code"] == code]
        assert [(f["severity"], f["message"]) for f in notice] == [
            ("notice", message.format(a))], report["findings"]
        assert report["affected"].get(code) == held, (a, report["affected"])
    installed(archive, c)
    kept = archive.post("/v1/get", {"id": pdf})["attributes"]
    assert kept["scan"]["media_type"] == "application/pdf", kept
    assert request(archive, "drop_page", pdf,
                   raw=file("image/gif"))["kind"] == "satisfied"
    assert [p["media_type"] for p in archive.post(
        "/v1/get", {"id": pdf})["attributes"]["pages"]] == ["image/png"]
    assert archive.post("/v1/get", {"id": png})["attributes"]["scan"][
        "media_type"] == "image/png"
    # a list taking more media types, or the same ones written otherwise
    wider = NARROWED.replace(
        'media_types: ["image/*"] }', 'media_types: ["image/*", text/plain] }'
    ).replace("media_types: [image/png, image/jpeg]",
              "media_types: [image/jpeg, image/png]")
    _c, report = report_of(archive, wider)
    assert not [f for f in report["findings"]
                if f["code"].startswith("media_types_narrowed:")], report


def test_a_pending_proposal_holding_a_file_no_longer_taken_is_invalidated(
        archive):
    """A pending proposal whose file input the version's list no longer
    takes is invalidated, as one holding a value its type no longer holds
    is; one its list still takes stays."""
    folder = made(archive, pages=[])
    v = archive.post("/v1/request", {
        "transition": "attach", "actor": OP, "object_id": folder,
        "inputs": {"upload": file("application/pdf")}, "propose": True})
    assert v["kind"] == "satisfied", v
    _c, report = report_of(archive, ARCHIVE.replace(
        "upload: { type: file, media_types: [application/pdf] }",
        "upload: { type: file, media_types: [image/png] }"))
    assert "proposals_invalidated" in {f["code"] for f in report["findings"]}
    _c, report = report_of(archive, ARCHIVE.replace(
        "upload: { type: file, media_types: [application/pdf] }",
        'upload: { type: file, media_types: ["application/*"] }'))
    assert "proposals_invalidated" not in {
        f["code"] for f in report["findings"]}, report


def test_a_mapping_giving_a_file_not_taken_is_fatal(archive):
    """A backfill, or a value a move's entry gives, of a file its attribute
    does not take is fatal, as a value it cannot hold is; a value a move
    carries is held, not written."""
    folder = made(archive, pages=[], loose=file("video/mp4"))
    _c, report = report_of(archive, ARCHIVE + (
        "migration:\n  description: Each folder's loose file is its scan.\n"
        "  backfill:\n    Folder: { scan: loose }\n"))
    fatal = [f for f in report["findings"]
             if f["code"] == "mapping:backfill:Folder.scan"]
    assert [f["severity"] for f in fatal] == ["fatal"], report["findings"]
    assert report["affected"]["mapping:backfill:Folder.scan"] == [folder]
    binder = ARCHIVE.replace("  Folder:\n    description: A folder of scans.\n",
                             "  Binder:\n    description: A binder of scans.\n")
    _c, report = report_of(archive, binder + (
        "migration:\n  description: Folders are binders.\n  moved_types:\n"
        "    Folder: [ { to: Binder, attributes: { scan: loose } } ]\n"))
    code = "mapping:moved_types:Folder.Binder.scan"
    assert [f["severity"] for f in report["findings"] if f["code"] == code] \
        == ["fatal"], report["findings"]
    _c, report = report_of(archive, binder.replace(
        "loose: { type: file, optional: true }",
        "loose: { type: file, optional: true, media_types: [image/png] }") + (
        "migration:\n  description: Folders are binders.\n  moved_types:\n"
        "    Folder: [ { to: Binder } ]\n"))
    assert not [f for f in report["findings"]
                if f["severity"] == "fatal"], report["findings"]
