# Changelog

What each release of ObjectFlow's engine changed, for whoever runs it or builds on it. Releases follow semantic versioning (ADR-0180 decision 6), and from 1.0.0 the wire `/v1`, flow format 1 and the store's upgrade are held to ADR-0180's promise; a change that may refuse what an earlier release accepted is listed under its own heading. Each release is tagged `v<version>`, its service built from the tag with a SHA-256 checksum (`.github/workflows/release.yml`).

## Unreleased, toward 1.0.0

Release 1.0.0 is held until a production review, on the same 36.7k-object store as the first, finds no blocker or major (ADR-0183 decision 1; `docs/design/production-review.md`); the first three found some.

### Changes that may refuse what 0.2.0 accepted

- The operator's retirement is refused `not_requestable`, as its creation is, where it was irreversible (ADR-0184).
- A flow may declare who approves its versions, `publishing: { taken_by: [human] }`: read from the installed version, it refuses an approver of another kind `invariant_violated`, `publish_taken_by`, `delegable`, the store's operator always excepted; modules of a version that declare it differently are refused by both checkers; the samples declare `[human]` (ADR-0187 decision 1, third production review AGENT-APPROVE).
- A store whose file fails SQLite's quick check does not start, where a copy restored beside another state's `-wal` answered health while its reads failed; a prune where the service has no retention for it is refused `no_retention`, where it said `inside_retention`; an upgrade asked of a store with none due is refused to anyone but an operator (`409`) (third production review).
- A date or a timestamp is read as RFC 3339 writes it, each field its digits exactly and the year 0001 to 9999, at UTC: a space, a sign, a one-digit month, a year past 9999, the year 0 and the offset `+0000` are refused `malformed`, alike in the reference runner, which took a one-digit month and any offset.
- A request's own `principal` is read, typed and checked as the actor is, a principal naming no one refused `actor_known`, where it was taken and dropped; it and `actor.principal` must agree (third production review PRINCIPAL).
- A change's dry run that passes `--read-timeout`, a `submit`'s or a `publish` preview's, is answered `503 read_timeout` and records nothing, where a submit's was computed again under the writer's lock and a preview held the writer throughout (third production review PUBLISH-SCALE, D604).
- An effect whose arithmetic of present values has no value, an integer past 64 bits, a negation of the least or a division by zero among them, refuses its request as the generated clause `value_in_range`, `unsatisfied` with the remedy `unreachable_from_here`, naming the object, where the core answered `501 not run` and the reference runner stored the value; a condition may not be named `value_in_range` (ADR-0185, production review F5). The least integer leaves the wire as a string, as every integer past 2⁵³ does, where a release build sent it as a number (D601).
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

- `/metrics` gives the store's file and its `-wal` in bytes; the `-wal` is cut back to 64 MiB when a checkpoint empties it, where it grew to six times the store and stayed; a start that cannot make its store or socket, or bind its address, names the path (third production review N6).
- `--socket <path>`: the service on a Unix socket, made `0660`, in an address's place, so the socket's owner and group are its only callers (ADR-0183 decision 4).
- One line on stderr for each answer, its time, method, route, status and duration; `GET /metrics`, the answers by route and status, the store's version and the clock's lead, in Prometheus's text format; `/health` reading the store's file and answering `clock_lead_seconds` (ADR-0183 decision 8).
- On SIGTERM or SIGINT the service takes no new connection, answers the requests in flight for up to 30 s and exits, where it stopped at once (ADR-0183 decision 9).
- `/health` and `/metrics` say how long the writer has been held and the requests in flight, `/metrics` the process's resident memory; `/health` is `503` while an upgrade is due; a 5xx's reason is logged (OBSERVE-2).
- A clean stop carries the write-ahead log into the store's file; a socket a live service answers on is refused, never taken over; start errors are sentences.
- At `--max-connections` the oldest connection answering nothing is closed to make room, where silent connections reset every newcomer, `/health` among them; a newcomer is answered `503 busy` only when every connection is answering a request; `/health` says `connections_open` and `/metrics` the connections open, closed to make room and refused (ADR-0186, third production review C-1).
- `--idle-timeout`, `--max-connections` and `--read-timeout`: a connection whose request does not arrive is closed, a body that does not is `408`, connections are capped, and a deployment's read passing its deadline is stopped and answered `503 read_timeout` (ADR-0184).
- A request that panics holding the writer no longer leaves every later write `500`: the next write takes the lock back and reopens the store from its file.
- `--version`.
- The checkers notice a correction's free-text reason on a type an erasure reaches, `reason_kept`, since its event keeps the reason and an erasure leaves it (production review F7).
- A refusal tells its caller what its remedy points at (ADR-0179).
- The wire's baseline, `docs/guides/wire-v1.json`, and format 1's frozen corpus, each held by a check (ADR-0180 decision 7).

### Performance

- After a publish, an import batch or any request of half a second or more, the service gives back to the system what it freed, which glibc kept: ten seconds after a publish it holds 25 MB on 37k objects where it held 975 MB, and 36 MB on 110k where 2.95 GB; `objectflow_memory_given_back_total` counts it (ADR-0187 decision 2).
- A query's page in creation order reads from its cursor, a chunk at a time, where each page read and filtered its type whole: a filter on a derived attribute read page by page over 12k claims took 1.6 s where 44.4 s, and its first page 0.03 s where 1.03 s (third production review READ-PAGES).
- A publish's dry run is linear in the store, where checking each object's optional identifiers swept every object without one: on 110k objects a publish adding one attribute submitted in 6.4 s where 41.5 s, and its approval held writes 9.1 s where 44.5 s (D605).
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
