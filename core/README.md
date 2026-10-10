# The core

ObjectFlow's engine in Rust: `objectflow-core`, the library that holds the model, the publish checks, requests, reads, publishing, importing and the store, and `objectflow-service`, the HTTP service over it (ADR-0132, ADR-0150, ADR-0153). The Python reference runner in `scripts/` is the oracle the core is held to; applications use the core's service.

## What runs today

The service serves one of two stores (`DESIGN.md` §2), one request at a time.

- **A sandbox**, by default: one store held in memory, emptied by each load and reset; a clock that starts at `2026-01-01T00:00:00Z` and moves only when `/sandbox/clock` moves it; external evaluators answered by fixed answers set through `/sandbox/evaluators`. It is for building, testing and demonstrating flows, the Flow Studio among them, and nothing it holds survives it.
- **A deployment's store**, given `--store <file>` (ADR-0174): one SQLite file that survives a restart, created at version 0 where it is absent, its flow published through the wire as every version is; the wall clock, never moving backwards; no sandbox controls. It posts each evaluator's call to the endpoint `--evaluator <name>=<url>` names, `{"evaluator", "function", "arguments"}`, and takes `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}`; anything else, a fault, a timeout (`--evaluator-timeout`, 5 s unless given) or an evaluator no endpoint answers, is unknown, refused `temporal`, and said on stderr (ADR-0175). A store an earlier engine release made is served only `POST /v1/upgrade` until the operator runs it, and one of a release this core does not upgrade from does not start (ADR-0177); and it authenticates nobody, so it is bound beyond loopback only behind the application that does.

## A new release

A release is the workspace version in `Cargo.toml`, and it moves exactly when what a store holds or means changes: the store's own tables, the tables a type maps to, how a value is stored in a column, or the built-in types and standard metrics (ADR-0177). Such a change is a new entry in `crates/objectflow-core/src/release.rs`'s `RELEASES`: its name, the release it upgrades from, its migration of a store, what it changes in a sentence, and its fingerprint, which `release::tests` prints when the store's schema or built-ins change without one. Before the release ships, make stores with the one it replaces, by `crates/objectflow-service/tests/fixtures/make-release-store.py` run with that release's service, and add them to `deployment.rs`'s upgrade test, which requires an upgraded store to answer what its release answered.

## Build and run

The toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml); rustup installs it on the first build.

```sh
cd core
~/.cargo/bin/cargo build -p objectflow-service        # or --release
target/debug/objectflow-service --port 8712            # --port 0 takes a free port
```

It writes `listening on http://127.0.0.1:<port>` to stderr once it answers, and `--help` prints its usage. A deployment's store:

```sh
target/release/objectflow-service --store /var/lib/objectflow/store.sqlite --bind 127.0.0.1:8712 \
  --attempt-retention "90 days" --idempotency-retention "30 days" \
  --evaluator xero=https://xero-bridge.internal/evaluate
```

`GET /health` answers the release, the shape, the store's version and its clock. Every operation is `POST /v1/<operation>`, and a sandbox's controls `POST /sandbox/<control>`, with a JSON body; [`docs/guides/integrate.md`](../docs/guides/integrate.md) walks through them, and [`docs/guides/wire.md`](../docs/guides/wire.md) is the reference.

**A load is not checked.** `/sandbox/load` installs the modules it is given without running the publish checks (ADR-0153 decision 5); run `scripts/check-flows.py` over them first, as the Flow Studio's server does. A version published through `/v1/publish` is checked.

## Checks

```sh
core/scripts/check     # rustfmt, clippy with every warning an error, and the tests, as CI runs them
```

The tests hold the core to the reference: `tests/checker_oracle.rs` replays every check the flow checker makes on its planted fixtures (`scripts/check-flows.py --export-plants` writes them), and the Flow Studio's server tests play every example of the design's modules through both the core and the runner and compare each answer whole.
