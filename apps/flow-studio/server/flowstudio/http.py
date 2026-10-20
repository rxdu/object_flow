"""The studio's server: the wire, the sandbox control and the studio's tooling.

Routes (ADR-0150):

- `POST /v1/<operation>`: the library's operations the slices use, `request`,
  `get`, `history`, `events`, `availability`, `metric`, `declaration`,
  `import_batch` and `lookup` (ADR-0154);
- `POST /sandbox/<control>`: load, reset, clock, evaluators;
- `GET /studio/...`: the module catalogue, a module's files, a diagram, and
  which backend answers the wire;
- `POST /studio/...`: check a text, draw a text, save to the workspace, copy a
  design module into it, and add an example a check passes.

A refusal is a 200 carrying the verdict; a fault is a 4xx or 5xx carrying a
`fault` message; a construct the reference runner cannot run is a 501.

With the Rust core chosen as the backend, every `/v1` and `/sandbox` call is
forwarded to the core's service and answered as it answers, a load checked by
the flow checker first (`core.py`).
"""

from __future__ import annotations

import json
import logging
import pathlib
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable
from urllib.parse import parse_qs, urlparse

from . import catalogue
from .catalogue import Catalogue
from .core import FORWARDED, Core
from .sandbox import Fault, Sandbox, checked

log = logging.getLogger("flowstudio")

MAX_BODY = 4 * 1024 * 1024
# the keys each operation takes, as the core's service holds them: a key
# outside them is a fault, so a misspelt one is never read as absent
# (ADR-0173 decision 12, D555)
REQUEST_KEYS = ("transition", "actor", "object_id", "type", "inputs",
                "occurred_at", "propose", "expires_at", "expected_version", "target_version",
                "principal", "idempotency_key", "context", "reads_personal")
TAKES = {
    "/v1/request": REQUEST_KEYS, "/v1/check": REQUEST_KEYS,
    "/v1/get": ("actor", "id", "follow"),
    "/v1/query": ("actor", "type", "filter", "params", "order", "cursor",
                  "fields", "limit", "reads_personal"),
    "/v1/lookup": ("actor", "source", "value", "reads_personal"),
    "/v1/availability": ("actor", "id", "reads_personal"),
    "/v1/available": ("actor", "type", "transition", "cursor", "limit",
                      "reads_personal"),
    "/v1/history": ("actor", "id", "follow", "limit", "cursor"),
    "/v1/events": ("actor", "positions"),
    "/v1/exceptions": ("actor", "type", "cursor"),
    "/v1/declaration": ("actor", "type", "version"),
    "/v1/tools": ("actor", "actor_kind"),
    "/v1/pull": ("actor", "subscription", "cursor", "limit"),
    "/v1/acknowledge": ("actor", "subscription", "cursor"),
    "/v1/metric": ("actor", "name", "bind", "keep", "over", "filter",
                   "params", "cursor", "reads_personal"),
    "/v1/export": ("actor", "source", "cursor", "limit"),
    "/v1/diagnostics": ("actor", "type", "window"),
    "/v1/import_batch": ("actor", "batch", "disposition", "dry_run"),
    "/v1/maintain": ("actor", "task"),
    "/v1/publish": ("actor", "change", "expected_version", "dry_run"),
    "/sandbox/load": ("files", "attempt_retention", "idempotency_retention", "unchecked"),
    "/sandbox/reset": (),
    "/sandbox/clock": ("advance", "set"),
    "/sandbox/evaluators": ("answers",),
}


def actor_shape(actor: Any) -> str | None:
    """What is wrong with a request's actor, or none: an object whose keys are `id` and `principal`, each a
    string, or absent or null, as the service checks it (eighth production review, ADR-0195 decision 7)."""
    if actor is None:
        return None
    if not isinstance(actor, dict):
        return "actor takes an object"
    extra = [k for k in actor if k not in ("id", "principal")]
    if extra:
        return f"actor takes no key {extra[0]}"
    for k in ("principal", "id"):
        if actor.get(k) is not None and not isinstance(actor[k], str):
            return f"{k} takes a string"
    return None


def keys_once(pairs: list) -> dict:
    """A JSON object naming each key once, as the core's service reads one: a key named twice is refused, where the
    last one won (ADR-0190 decision 8)."""
    out: dict = {}
    for k, v in pairs:
        if k in out:
            raise ValueError(f"the key {k} is named twice")
        out[k] = v
    return out


def routes(box: Sandbox, cat: Catalogue) -> dict[str, Callable[[dict], Any]]:
    return {
        "/studio/check": lambda b: cat.check(b.get("files") or []),
        "/studio/draw": lambda b: {"mermaid": cat.diagram(
            "", b.get("type", ""), text=b.get("text", ""))},
        "/studio/save": lambda b: cat.save(b.get("file", ""),
                                           b.get("text", "")),
        "/studio/copy": lambda b: cat.copy(b.get("file", "")),
        "/studio/examples": lambda b: cat.add_example(
            b.get("module", ""), b.get("name", ""), b.get("example") or {}),
        "/v1/request": box.request,
        "/v1/get": box.get,
        "/v1/history": box.history,
        "/v1/events": box.events,
        "/v1/availability": box.availability,
        "/v1/metric": box.metric,
        "/v1/declaration": box.declaration,
        "/v1/tools": box.tools,
        "/v1/import_batch": box.import_batch,
        "/v1/lookup": box.lookup,
        "/v1/publish": box.publish,
        "/sandbox/load": box.load,
        "/sandbox/reset": box.reset,
        "/sandbox/clock": box.clock,
        "/sandbox/evaluators": box.evaluators,
    }


def studio(cat: Catalogue, core: Core | None, path: str,
           query: dict) -> Any:
    one = {k: v[0] for k, v in query.items()}
    source = one.get("source", "design")
    if path == "/studio/backend":
        return ({"backend": "core", "service": core.url} if core
                else {"backend": "runner"})
    if path == "/studio/modules":
        return cat.modules()
    if path == "/studio/module":
        return cat.bundle(one.get("file", ""), source)
    if path == "/studio/diagram":
        return {"mermaid": cat.diagram(one.get("file", ""),
                                       one.get("type", ""), source)}
    raise Fault(404, f"no studio route {path}")


def handler(box: Sandbox, cat: Catalogue, origin: str | None,
            core: Core | None = None) -> type:
    table = routes(box, cat)

    class Handler(BaseHTTPRequestHandler):
        server_version = "FlowStudio/0.1"

        def log_message(self, fmt: str, *args: Any) -> None:
            log.info("%s %s", self.address_string(), fmt % args)

        def send(self, status: int, body: Any) -> None:
            data = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            if origin:
                self.send_header("Access-Control-Allow-Origin", origin)
            self.end_headers()
            self.wfile.write(data)

        def do_OPTIONS(self) -> None:  # noqa: N802  the stdlib's name
            self.send_response(204)
            if origin:
                self.send_header("Access-Control-Allow-Origin", origin)
                self.send_header("Access-Control-Allow-Methods", "GET, POST")
                self.send_header("Access-Control-Allow-Headers",
                                 "Content-Type")
            self.end_headers()

        def do_GET(self) -> None:  # noqa: N802
            url = urlparse(self.path)
            self.answer(
                lambda: studio(cat, core, url.path, parse_qs(url.query)))

        def do_POST(self) -> None:  # noqa: N802
            path = urlparse(self.path).path
            forwarded = core is not None and path.startswith(FORWARDED)
            if path not in table and not forwarded:
                self.send(404, {"fault": f"no operation {path}"})
                return
            length = int(self.headers.get("Content-Length") or 0)
            if length > MAX_BODY:
                self.send(413, {"fault": "the body is too large"})
                return
            try:
                body = json.loads(self.rfile.read(length) or b"{}", object_pairs_hook=keys_once)
            except ValueError as e:
                # not JSON, or an object naming a key twice (ADR-0190 decision 8)
                self.send(400, {"fault": f"the body is not JSON: {e}"})
                return
            if not isinstance(body, dict):
                self.send(400, {"fault": "the body is a JSON object"})
                return
            unknown = [k for k in body if path in TAKES and k not in TAKES[path]]
            if unknown and not forwarded:
                self.send(400, {"fault": f"{path} takes no key {unknown[0]}"})
                return
            # an actor, wherever one is given, is an object of an id and a principal, as the service's
            # (ADR-0195 decision 7)
            shape = actor_shape(body.get("actor"))
            if shape and not forwarded:
                self.send(400, {"fault": shape})
                return
            if forwarded:
                assert core is not None
                # the studio reads as the flow's author, so it opts into
                # reading by a personal value, unless the caller says
                # otherwise (ADR-0206)
                if path in TAKES and "reads_personal" in TAKES[path]:
                    body = {**body, "reads_personal": body.get("reads_personal", True)}
                self.answer_forwarded(core, path, body)
                return
            with box.lock:
                self.answer(lambda: table[path](body))

        def answer_forwarded(self, core: Core, path: str, body: dict) -> None:
            """The core's answer, a load checked first as the runner's is,
            so the core never loads what the flow checker refuses."""
            if path != "/sandbox/load":
                self.send(*core.forward(path, body))
                return
            try:
                _files, refused, notices = checked(body)
            except Fault as f:
                self.send(f.status, {"fault": f.message})
                return
            except Exception as e:  # a fault of the server's own, answered as the runner's path answers it
                log.error("unhandled: %s", traceback.format_exc())
                self.send(500, {"fault": f"{type(e).__name__}: {e}"})
                return
            if refused:
                self.send(200, refused)
                return
            status, answer = core.forward(path, body)
            if status == 200 and isinstance(answer, dict):
                answer["notices"] = notices
            self.send(status, answer)

        def answer(self, call: Callable[[], Any]) -> None:
            try:
                self.send(200, call())
            except Fault as f:
                self.send(f.status, {"fault": f.message})
            except catalogue.Unknown as e:
                self.send(404, {"fault": str(e)})
            except catalogue.Refused as e:
                self.send(409, {"fault": str(e)})
            except catalogue.yaml.YAMLError as e:
                self.send(400, {"fault": f"the text is not YAML: {e}"})
            except Exception as e:  # a fault of the server's own, reported
                log.error("unhandled: %s", traceback.format_exc())
                self.send(500, {"fault": f"{type(e).__name__}: {e}"})

    return Handler


def serve(host: str, port: int, workspace: pathlib.Path,
          origin: str | None = None, box: Sandbox | None = None,
          core: Core | None = None) -> ThreadingHTTPServer:
    """A server bound to the address given, editing flows in the workspace
    given, its wire answered by the core when one is given and otherwise by
    the reference runner; the caller runs it."""
    return ThreadingHTTPServer(
        (host, port),
        handler(box or Sandbox(), Catalogue(workspace), origin, core))
