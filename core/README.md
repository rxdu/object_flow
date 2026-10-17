# The core

ObjectFlow's engine in Rust: `objectflow-core`, the library that holds the model, the publish checks, requests, reads, publishing, importing and the store, and `objectflow-service`, the HTTP service over it (ADR-0132, ADR-0150, ADR-0153). The Python reference runner in `scripts/` is the oracle the core is held to; applications use the core's service.

## What runs today

The service serves one of two stores (`DESIGN.md` §2). A deployment's reads run beside its writes, each on a read connection of its own in one read transaction, and its writes one at a time, as SQLite has one writer, each begun `IMMEDIATE` and waiting out another process's lock for `--busy-timeout`, 5 s unless given, before it is retried and then refused `stale`, cause `retries_exhausted` (ADR-0178); a sandbox serves one request at a time. *PostgreSQL as a deployment's store is decided for 1.0.0 and being built (ADR-0213); until it lands, a deployment's store is the SQLite file below.*

- **A sandbox**, by default: one store held in memory, emptied by each load and reset; a clock that starts at `2026-01-01T00:00:00Z` and moves only when `/sandbox/clock` moves it; external evaluators answered by fixed answers set through `/sandbox/evaluators`. It is for building, testing and demonstrating flows, the Flow Studio among them, and nothing it holds survives it.
- **A deployment's store**, given `--store <file>` (ADR-0174): one SQLite file that survives a restart, made at version 0 by a first start with `--create`, its files `0600`, and refused where the path holds none without it, so a mistyped path is never served as an empty store (ADR-0183); its flow published through the wire as every version is; the wall clock, never moving backwards; no sandbox controls. It posts each evaluator's call to the endpoint `--evaluator <name>=<url>` names, `{"evaluator", "function", "arguments"}`, and takes `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}`; anything else, a fault, a timeout (`--evaluator-timeout`, 5 s unless given) or an evaluator no endpoint answers, is unknown, refused `temporal`, and said on stderr (ADR-0175). A store an earlier engine release made is served only `POST /v1/upgrade` until the operator runs it, and one of a release this core does not upgrade from does not start (ADR-0177); and it authenticates nobody, so it is given a `--socket` its application's user alone reaches, any user of the machine reaching a loopback port, and never bound beyond loopback (`SECURITY.md`).

## A new release

A release is the workspace version in `Cargo.toml`, and it moves exactly when what a store holds or means changes: the store's own tables, the tables a type maps to, how a value is stored in a column, or the built-in types and standard metrics (ADR-0177). Such a change is a new entry in `crates/objectflow-core/src/release.rs`'s `RELEASES`: its name, the release it upgrades from, its migration of a store, what it changes in a sentence, and its fingerprint, which `release::tests` prints when the store's schema or built-ins change without one. Before the release ships, make stores with the one it replaces, by `crates/objectflow-service/tests/fixtures/make-release-store.py` run with that release's service, and add them to `deployment.rs`'s upgrade test, which requires an upgraded store to answer what its release answered.

## Build and run

The toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml); rustup installs it on the first build.

```sh
cd core
~/.cargo/bin/cargo build -p objectflow-service        # or --release
target/debug/objectflow-service --port 8712            # --port 0 takes a free port
```

It writes `listening on http://127.0.0.1:<port>` to stderr once it answers, then one line for each answer, each line stamped with the time it was written, and `--help` prints its usage, `--version` its release. A deployment's store, made by its first start with `--create`, which later starts leave out:

```sh
target/release/objectflow-service --store /var/lib/objectflow/store.sqlite --create   # once, to make it
target/release/objectflow-service --store /var/lib/objectflow/store.sqlite --socket /run/objectflow/objectflow.sock \
  --attempt-retention "90 days" --idempotency-retention "30 days" \
  --evaluator xero=https://xero-bridge.internal/evaluate --evaluator-token xero=/etc/objectflow/xero.token
```

`--idle-timeout`, `--max-connections` and `--read-timeout` bound a connection, how many are served and a read (ADR-0184), and `--max-reads` how many reads that grow with the store, a query, a metric or a page of a feed among them, run at once, 4 unless given (ADR-0189); at the cap the oldest connection answering nothing is closed to make room, and a newcomer is told `503 busy` only when every one is answering (ADR-0186). `--evaluator-token <name>=<file>` sends an evaluator a bearer token read from a file its owner alone reads, never logged (ADR-0188). `--metrics-bind <host:port>` serves `GET /metrics` and `GET /health` alone on an address of their own, so a service on a socket can be scraped. `--socket` listens on a Unix socket, made `0660`, in the address's place, so the socket's owner and group are the only callers (ADR-0183); `--bind 127.0.0.1:8712` listens on an address instead. `GET /health` answers the release, the shape, the store's version, its clock and how far that leads the wall clock, reading the store's file to answer; `GET /metrics` gives the answers counted by route and status, in Prometheus's text format. Every operation is `POST /v1/<operation>`, and a sandbox's controls `POST /sandbox/<control>`, with a JSON body; [`docs/guides/integrate.md`](../docs/guides/integrate.md) walks through them, [`docs/guides/wire.md`](../docs/guides/wire.md) is the reference, and [`docs/guides/operate.md`](../docs/guides/operate.md) is how to run a deployment.

**A load is not checked.** `/sandbox/load` installs the modules it is given without running the publish checks (ADR-0153 decision 5); run `scripts/check-flows.py` over them first, as the Flow Studio's server does. A version published through `/v1/publish` is checked.

## How fast

Measured on 2026-10-16 by [`scripts/profile/throughput.py`](../scripts/profile/throughput.py), twice, on the release build, a store the service made on an NVMe disk of a 12-core machine other work shared (load about 2.5), over HTTP on the loopback. A write, the smallest a flow makes, took 10.7 ms at p50 and 14 ms at p95 from one client, 95 a second. Requests waiting for the writer commit together (ADR-0208), so 8 clients reached about 265 a second, at p95 40 ms, and 32 clients 760 to 800 a second, at p95 64 to 69 ms. An import of 10,000 objects of a mirror type of two attributes took 0.6 s to dry-run and 2.0 s to run, and one of 1,620 took 0.1 s and 0.3 s. A flow's guards reading other objects, cascades, invariants and metrics add to each; the third hiring review measured about 80 ms a write and 37 s for 1,620 imported objects of its flow, on a debug build before ADR-0208. `scripts/profile/load.py` plays a team's mixed load against a profile store (Gate P).

## Checks

```sh
core/scripts/check     # rustfmt, clippy with every warning an error, and the tests
scripts/check-all      # every gate of the repository, the core's among them: what CI runs on each push
```

CI (`.github/workflows/check.yml`) runs `scripts/check-all` on every push and pull request, with `RUSTFLAGS="-D warnings"`, and checks the core's dependencies against the RustSec advisory database (ADR-0183 decision 2).

## Releases

A release is the author's act: the version raised in `core/Cargo.toml`'s `[workspace.package]`, `CHANGELOG.md`'s section for it written, the commit's check passed, and the commit tagged `v<version>` and pushed. The tag's workflow (`.github/workflows/release.yml`) refuses a tag whose commit's `check-all` has not passed, runs the core's and the service's tests on the oldest supported Ubuntu, builds the service from the tag there, refuses a tag that names another version than the binary's, and publishes the archive and its SHA-256 checksum as the tag's GitHub release. A vulnerability is reported as `SECURITY.md` says.

The tests hold the core to the reference: `tests/checker_oracle.rs` replays every check the flow checker makes on its planted fixtures (`scripts/check-flows.py --export-plants` writes them), and the Flow Studio's server tests play every example of the design's modules through both the core and the runner and compare each answer whole.
