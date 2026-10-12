# Changelog

What each release of ObjectFlow's engine changed, for whoever runs it or builds on it. Releases follow semantic versioning (ADR-0180 decision 6), and from 1.0.0 the wire `/v1`, flow format 1 and the store's upgrade are held to ADR-0180's promise; a change that may refuse what an earlier release accepted is listed under its own heading. Each release is tagged `v<version>`, its service built from the tag with a SHA-256 checksum (`.github/workflows/release.yml`).

## Unreleased, toward 1.0.0

Release 1.0.0 is held until a second production review, on the same 36.7k-object store as the first, finds no blocker or major (ADR-0183 decision 1; `docs/design/production-review.md`).

### Changes that may refuse what 0.2.0 accepted

- The operator's retirement is refused `not_requestable`, as its creation is, where it was irreversible (ADR-0184).
- `events` takes 1,000 positions at most.
- A file that holds something other than a store, another application's database among them, is refused with `--create` too, where it was given the store's tables and served.
- A description nesting its collections deeper than 64 levels is refused, a `yaml` finding, where 300,000 levels aborted the service (D598).

- A deployment's store path that holds no store, absent or empty, is refused unless the start gives `--create`, so a mistyped path is never served as an empty store (ADR-0183 decision 3).
- `dry_run` and `propose` take `true` or `false`, and `inputs` an object; anything else is `400`, where `"dry_run": "true"` was read as false and installed a version (ADR-0183 decision 6). Every other key is typed too: a value of another JSON type is `400`, where a numeric `filter` returned every object unfiltered, and `actor` takes `id` and `principal` alone.
- `maintain` names an actor the store knows, as a request does: one it does not know is refused `actor_known`, and one in a final state `actor_active`, where any name was taken (ADR-0183 decision 5).
- An expression nests at most 64 levels and its tree is at most 128 deep, in a request's filter and in a module, where a deep filter overflowed the stack and stopped the service (production review F1).
- A decimal written by an effect, an input or a default is rounded half to even to its declared scale, as `declaration-syntax.md` §8.3 states, where the core stored unscaled units (production review F4).
- A request whose actor or principal is in a final state is refused `actor_active`; a type may declare `taken_by` for every transition declaring none; a query's filter may read any member of its type; every built-in attribute holding JSON is answered as JSON (ADR-0182).
- `taken_by` marks any transition a request takes; a refusal names no object that does not exist; a query's filter is typed against its literals; an attribute named `<reference>_id` beside a single reference is refused (ADR-0181).
- `query`, `available`, `pull` and `export` answer pages of 100 unless asked otherwise, a client that ignores the cursor reading the first page (ADR-0179 decision 5).

### Added

- `--socket <path>`: the service on a Unix socket, made `0660`, in an address's place, so the socket's owner and group are its only callers (ADR-0183 decision 4).
- One line on stderr for each answer, its time, method, route, status and duration; `GET /metrics`, the answers by route and status, the store's version and the clock's lead, in Prometheus's text format; `/health` reading the store's file and answering `clock_lead_seconds` (ADR-0183 decision 8).
- On SIGTERM or SIGINT the service takes no new connection, answers the requests in flight for up to 30 s and exits, where it stopped at once (ADR-0183 decision 9).
- `/health` and `/metrics` say how long the writer has been held and the requests in flight, `/metrics` the process's resident memory; `/health` is `503` while an upgrade is due; a 5xx's reason is logged (OBSERVE-2).
- A clean stop carries the write-ahead log into the store's file; a socket a live service answers on is refused, never taken over; start errors are sentences.
- `--idle-timeout`, `--max-connections` and `--read-timeout`: a connection whose request does not arrive is closed, a body that does not is `408`, connections are capped, and a deployment's read passing its deadline is stopped and answered `503 read_timeout` (ADR-0184).
- A request that panics holding the writer no longer leaves every later write `500`: the next write takes the lock back and reopens the store from its file.
- `--version`.
- The checkers notice a correction's free-text reason on a type an erasure reaches, `reason_kept`, since its event keeps the reason and an erasure leaves it (production review F7).
- A refusal tells its caller what its remedy points at (ADR-0179).
- The wire's baseline, `docs/guides/wire-v1.json`, and format 1's frozen corpus, each held by a check (ADR-0180 decision 7).

### Performance

- A submit's dry run runs beside the writes, where every write waited for it: 83 s for a module of 3,000 types, which also checks ten times faster (D599, D600).
- A person's or an agent's submit of a version, which resolved its submitter by reading every actor for each object, took 6 s where it passed 20 minutes on 37k objects (D594); a per-person metric and a filter on a derived attribute 1 to 1.5 s where 77 to 83 s (D595); `available` reads only the objects its transition is taken from (D596); an erasure's truncation of the write-ahead log waits on no reader, where it made every write take 5 s while a read was open (D597).
- An attribute marked `indexed` has an index, which no release before made, and opening a store makes one it lacks: the go-live import of 36k objects took 36 s where 145 s (D593).
- Reads by index, not by reading a type whole: on a store of 36.7k objects a `get` took 1 ms where it took 184 ms, a 50-claim worklist 11 ms where 737 ms, a write about 10 ms where about 200 ms, and a publish adding one attribute 4 s of dry run and 5 s of approval where 214 s and 220 s held every write (production review PERF-SCAN, PUBLISH-FREEZE).

### Security

- A store `--create` makes is `0600`, its `-wal` and `-shm` too; the service says at its start where a store's file is readable by others.
- An evaluator's failure is logged by its function, never its arguments or the endpoint's answer.
- An erasure leaves nothing it erased in the store's files (ADR-0181 decision 4).

## 0.2.0, 2026-10-10

- The store records the engine release that made it and each that upgraded it, and every operation reads it, so no copy of an earlier release serves a store once it is upgraded; `POST /v1/upgrade` carries a store from 0.1.0 (ADR-0177).

## 0.1.0

- The engine's Rust core and its HTTP service, first numbered 2026-10-04 (ADR-0153): a sandbox's store, and from 2026-10-10 a deployment's on one SQLite file (ADR-0174), with evaluators over HTTP (ADR-0175) and reads beside a writer (ADR-0178). Every store made before 0.2.0 is of this release.
