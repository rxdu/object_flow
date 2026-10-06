# Flow Studio

An editor, viewer and simulator of flows, the first application built on ObjectFlow: the test case that follows the engine's implementation, and a sample of building on it. Its design is [`docs/design/flow-studio.md`](../../docs/design/flow-studio.md); its decisions are ADR-0149 and ADR-0150.

It talks to the engine only through the engine's operations, `POST /v1/<operation>`, and a sandbox's controls, `POST /sandbox/<control>`, so the backend behind it can change: the Python reference runner, or the Rust core.

## Running it

Once the server's environment and the front end's packages are in place (below):

```sh
apps/flow-studio/scripts/dev          # the backend config.json names
apps/flow-studio/scripts/dev runner   # or name one: runner or core
```

It starts the studio's server, waits until it answers, which takes longer the first time the core is built, prints the page's address, `http://127.0.0.1:<web_port>`, and runs the front end's development server. Ctrl-C stops both, and the core's service with them. The page's header says which backend answers.

## The backends

`backend` in [`config.json`](config.json) names the one the server starts with, and `--backend` overrides it:

- **`core`**, the Rust core's service. The server builds it with the toolchain `core/rust-toolchain.toml` pins, starts it as its child on a free loopback port, and forwards every `/v1` and `/sandbox` call to it unchanged, but for a load, which the flow checker checks first as it does for the runner; the editor, the checker's findings and the diagrams stay the server's. It needs `~/.cargo/bin/cargo`, from rustup, which installs the toolchain the file pins on first use.
- **`runner`**, the Python reference runner, the oracle the core is held to (ADR-0132 decision 2).

## The server

Python's standard library over the repository's reference runner and flow checker, in a virtual environment holding the versions of PyYAML and jsonschema the scripts are pinned to (`scripts/requirements-dev.txt`), and pytest.

```sh
cd apps/flow-studio/server
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
.venv/bin/python -m flowstudio          # listens on 127.0.0.1, port from ../config.json
.venv/bin/python -m pytest              # the wire's tests, and every example replayed through it
```

The replay test plays every example of the design's example modules through the wire, as a caller would, and holds each to the outcome the flow checker gives it, against the runner and against the core. A second test plays them through both backends at once and compares every answer whole; `tests/backend_differences.txt` lists where they are known to differ and why. An example that needs what the interface does not yet offer is reported as waiting, with the reason, never passed (ADR-0150, Consequences).

## The front end

TypeScript and React, built by Vite; pnpm, at the version `package.json` pins, through Node's corepack.

```sh
cd apps/flow-studio/web
corepack pnpm install
corepack pnpm check                     # typecheck, lint, format and tests
```

Its tests play every example, a free session and a session saved as an example through the real server, once with each backend.

## Editing

The studio edits flows in its workspace, [`flows/`](flows/), and never the design's example modules, which the repository's checkers hold the design to; a design module is copied there to be edited. A session in the sandbox can be saved as an example of a workspace module, written only once the flow checker runs it and finds it holds (`docs/design/flow-studio.md` §3.1, §5).

## Ports

Both are set in [`config.json`](config.json), and neither is a default: the server's, and the web front end's, which the server allows as the one origin that may call it. The core's service, a child of the server, takes a free loopback port.
