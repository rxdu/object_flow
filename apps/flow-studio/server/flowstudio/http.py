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
forwarded to the core's service and answered as it answers (`core.py`).
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
from .sandbox import Fault, Sandbox

log = logging.getLogger("flowstudio")

MAX_BODY = 4 * 1024 * 1024


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
        "/v1/import_batch": box.import_batch,
        "/v1/lookup": box.lookup,
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
                body = json.loads(self.rfile.read(length) or b"{}")
            except json.JSONDecodeError as e:
                self.send(400, {"fault": f"the body is not JSON: {e}"})
                return
            if not isinstance(body, dict):
                self.send(400, {"fault": "the body is a JSON object"})
                return
            if forwarded:
                assert core is not None
                self.send(*core.forward(path, body))
                return
            with box.lock:
                self.answer(lambda: table[path](body))

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
