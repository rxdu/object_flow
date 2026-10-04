"""The studio's server: the wire, the sandbox control and the studio's tooling.

Routes (ADR-0150):

- `POST /v1/<operation>`: the library's operations the slices use, `request`,
  `get`, `history`, `events` and `declaration`;
- `POST /sandbox/<control>`: load, reset, clock, evaluators;
- `GET /studio/...`: the module catalogue, a module's files, a diagram.

A refusal is a 200 carrying the verdict; a fault is a 4xx or 5xx carrying a
`fault` message; a construct the reference runner cannot run is a 501.
"""

from __future__ import annotations

import json
import logging
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable
from urllib.parse import parse_qs, urlparse

from . import catalogue
from .sandbox import Fault, Sandbox

log = logging.getLogger("flowstudio")

MAX_BODY = 4 * 1024 * 1024


def routes(box: Sandbox) -> dict[str, Callable[[dict], Any]]:
    return {
        "/v1/request": box.request,
        "/v1/get": box.get,
        "/v1/history": box.history,
        "/v1/events": box.events,
        "/v1/declaration": box.declaration,
        "/sandbox/load": box.load,
        "/sandbox/reset": box.reset,
        "/sandbox/clock": box.clock,
        "/sandbox/evaluators": box.evaluators,
    }


def studio(path: str, query: dict) -> Any:
    one = {k: v[0] for k, v in query.items()}
    if path == "/studio/modules":
        return catalogue.modules()
    if path == "/studio/module":
        return catalogue.bundle(one.get("file", ""))
    if path == "/studio/diagram":
        return {"mermaid": catalogue.diagram(one.get("file", ""),
                                             one.get("type", ""))}
    raise Fault(404, f"no studio route {path}")


def handler(box: Sandbox, origin: str | None) -> type:
    table = routes(box)

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
            self.answer(lambda: studio(url.path, parse_qs(url.query)))

        def do_POST(self) -> None:  # noqa: N802
            path = urlparse(self.path).path
            if path not in table:
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
            with box.lock:
                self.answer(lambda: table[path](body))

        def answer(self, call: Callable[[], Any]) -> None:
            try:
                self.send(200, call())
            except Fault as f:
                self.send(f.status, {"fault": f.message})
            except catalogue.Unknown as e:
                self.send(404, {"fault": str(e)})
            except Exception as e:  # a fault of the server's own, reported
                log.error("unhandled: %s", traceback.format_exc())
                self.send(500, {"fault": f"{type(e).__name__}: {e}"})

    return Handler


def serve(host: str, port: int, origin: str | None = None,
          box: Sandbox | None = None) -> ThreadingHTTPServer:
    """A server bound to the address given; the caller runs it."""
    return ThreadingHTTPServer((host, port), handler(box or Sandbox(),
                                                     origin))
