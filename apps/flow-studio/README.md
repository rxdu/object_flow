# Flow Studio

An editor, viewer and simulator of flows, the first application built on ObjectFlow: the test case that follows the engine's implementation, and a sample of building on it. Its design is [`docs/design/flow-studio.md`](../../docs/design/flow-studio.md); its decisions are ADR-0149 and ADR-0150.

It talks to the engine only through the engine's operations, `POST /v1/<operation>`, and a sandbox's controls, `POST /sandbox/<control>`, so the backend behind it can change: today the Python reference runner, later the Rust core.

## The server

Python's standard library over the repository's reference runner and flow checker, in a virtual environment that sees the system interpreter's PyYAML and jsonschema, which the checkers use.

```sh
cd apps/flow-studio/server
python3 -m venv --system-site-packages .venv
.venv/bin/pip install -r requirements-dev.txt
.venv/bin/python -m flowstudio          # listens on 127.0.0.1, port from ../config.json
.venv/bin/python -m pytest              # the wire's tests, and every example replayed through it
```

The replay test plays every example of the design's example modules through the wire, as a caller would, and holds each to the outcome the flow checker gives it. An example that needs what the interface does not yet offer is reported as waiting, with the reason, never passed (ADR-0150, Consequences).

## Ports

Both are set in [`config.json`](config.json), and neither is a default: the server's, and the web front end's, which the server allows as the one origin that may call it.
