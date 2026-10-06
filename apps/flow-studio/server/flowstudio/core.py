"""The Rust core's service as the studio's backend (flow-studio.md §2, §6
slice 4).

Both backends speak one wire, so with the core chosen the studio's server
forwards every `/v1/<operation>` and `/sandbox/<control>` call to the core's
service unchanged, status and body, and keeps only its own tooling. The one
exception is a load, which the server checks with the flow checker first, as
it checks one for the runner (ADR-0153 decision 5): the core never loads what
the checker refuses, and the answer carries the checker's notices. Otherwise
it adds nothing to an answer and reads nothing out of one, so a difference
between the backends is the backends', never the forwarder's.

The service is built with the repository's pinned toolchain and started as a
child on a free loopback port; it stops with the studio's server.
"""

from __future__ import annotations

import json
import logging
import pathlib
import re
import subprocess
import threading
import urllib.error
import urllib.request
from typing import Any

log = logging.getLogger("flowstudio")

ROOT = pathlib.Path(__file__).resolve().parents[4]
CORE = ROOT / "core"
CARGO = pathlib.Path.home() / ".cargo/bin/cargo"
BINARY = CORE / "target/debug/objectflow-service"
# the prefixes the core answers; the studio's tooling stays the server's
FORWARDED = ("/v1/", "/sandbox/")
TIMEOUT_S = 60


class CoreUnavailable(Exception):
    """The core's service could not be built or started."""


class Core:
    """The core's service, running as a child of the studio's server."""

    def __init__(self) -> None:
        if not CARGO.exists():
            raise CoreUnavailable(
                f"no cargo at {CARGO}; the core is built with the toolchain "
                "core/rust-toolchain.toml pins (core/README.md)")
        built = subprocess.run(
            [str(CARGO), "build", "-q", "-p", "objectflow-service"],
            cwd=CORE, capture_output=True, text=True)
        if built.returncode != 0:
            raise CoreUnavailable(
                f"the core's service did not build:\n{built.stderr}")
        self.process = subprocess.Popen(
            [str(BINARY), "--port", "0"], stderr=subprocess.PIPE, text=True)
        assert self.process.stderr is not None
        line = self.process.stderr.readline()
        m = re.search(r"listening on (http://127\.0\.0\.1:\d+)", line)
        if not m:
            self.process.kill()
            raise CoreUnavailable(f"the core's service did not start: {line}")
        self.url = m.group(1)
        # a panic's message is the core's only other output; it is logged,
        # and the pipe is drained so the child never blocks writing it
        threading.Thread(target=self.drain, daemon=True).start()

    def drain(self) -> None:
        assert self.process.stderr is not None
        for line in self.process.stderr:
            log.warning("core: %s", line.rstrip())

    def forward(self, path: str, body: dict) -> tuple[int, Any]:
        """The core's answer to a call, its status and body as it gave
        them."""
        req = urllib.request.Request(
            self.url + path, data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
                return resp.status, json.loads(resp.read() or b"null")
        except urllib.error.HTTPError as e:
            raw = e.read()
            try:
                return e.code, json.loads(raw)
            except json.JSONDecodeError:
                text = raw.decode(errors="replace").strip()
                return e.code, {"fault": text or f"the core's service "
                                                 f"answered {e.code} to {path}"}
        except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
            code = self.process.poll()
            state = "running" if code is None else f"exited with {code}"
            return 502, {"fault": f"the core's service did not answer "
                                  f"({state}): {e}"}

    def close(self) -> None:
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
