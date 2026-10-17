# ADR-0174: A deployment's service, its first slice

- **Status:** **Accepted** by the author, 2026-10-11, by ADR-0182 ("go with your recommendations on the fifteen questions"). Before: Built at the author's direction, 2026-10-10 ("continue with your recommendation", on a plan naming "the deployment service: an ADR, then a first slice"), for the author's review. Decision 7's transport waits on a dependency the author approves.
- **Date:** 2026-10-10
- **Superseded in part by:** ADR-0213 — decision 2: a deployment's store is PostgreSQL or one SQLite file, PostgreSQL in 1.0.0 for the first deployment.
- **Refined by:** ADR-0177 — a store an earlier release made starts, serving only the upgrade until it runs; one of a release this core does not upgrade from still does not. ADR-0178 — reads beside the writer, on read connections of their own; writes begun `IMMEDIATE` with a busy timeout. ADR-0181 — the store's file holds nothing an erasure erased: `secure_delete` and a truncated log. ADR-0183 — decision 1's store is made only when the start gives `--create`, a path holding none refused otherwise; the service may listen on a Unix socket.
- **Builds:** `DESIGN.md` §2's deployment shape (ADR-0110, ADR-0132), ADR-0173 decision 2
- **Relates to:** `design/procurement-cold-draft.md` (I2, I16), `design/storage-schema.md` §6, `docs/guides/integrate.md`, `core/README.md`, `TODO.md`

## Context

`DESIGN.md` §2 deploys ObjectFlow as an internal service, one per store, which every caller reaches through one interface, and says a deployment's service, opening a store of its own, is not yet built: the core's service serves a sandbox, a store held in memory whose clock and evaluators its caller holds. The procurement review's integrator found that a team could build and test a flow on it and could hold no real record with it: the store is gone at a restart, time moves only when told, and any caller can move it (`procurement-cold-draft.md` I2, I16). ADR-0173 decision 2 decided how a deployment answers an evaluator, over HTTP; nothing decided how a deployment's store is opened, which this ADR does for the first slice: what a team needs to pilot one flow with real data on one machine.

## Decision

1. **One binary, two shapes.** `objectflow-service --store <file>` serves a deployment's store, the file created where it is absent; without `--store` it serves a sandbox, as before. A deployment routes none of the sandbox's controls, so `/sandbox/<control>` is the fault every unknown operation is (`DESIGN.md` §2), and no caller can load, reset, or move its clock.
2. **The store is one SQLite file**, opened with foreign keys, the write-ahead log and full synchronisation, so a write a request was answered for survives a crash of the process or the machine. A new store is created at version 0, holding the built-ins and the operator as a sandbox's is, and its flow arrives as every later version does, a `DeclarationChange` drafted against version 0, submitted and published through the wire (`DESIGN.md` §5.9, `publish-and-import.md` §1). An existing store is reopened at its latest version, its library loaded from the source that version recorded, as an erasure loads an earlier version's (ADR-0146 decision 2).
3. **A store opened by another release is refused.** The service starts only where the store's version 0 records this engine release, and says which release created it otherwise, since every running copy is the same release and a later release is installed by an explicit upgrade (`DESIGN.md` §2, §5.9, ADR-0110), which is not yet built.
4. **The clock is the wall clock, never moving backwards.** Each operation reads UTC now, or the store's last time where the machine's clock has stepped back, so no event is recorded before one already recorded and no interval in a state ends before it began.
5. **The address is configured, and loopback by default.** `--bind <host:port>` names it, `127.0.0.1:8712` unless given, and `--port` still names a loopback port. The service authenticates nothing (`DESIGN.md` §2, ADR-0114): it is bound beyond loopback only behind the application that does.
6. **`GET /health` answers the release, the store's version and its clock**, for a deployment's monitoring and a caller checking what it talks to; a sandbox answers it too, its version 0 until a load.
7. *Replaced 2026-10-10 by ADR-0175, the transport built.* **Until the HTTP transport of ADR-0173 decision 2 is built, a deployment answers every evaluator unknown**, so a guard that asks one is refused `temporal`, as an unreachable endpoint's would be, and the service names those evaluators when it starts. The transport needs an HTTP client with TLS, a new dependency, which is the author's to approve; the recommendation is in `TODO.md`.
8. **Retentions are flags.** `--attempt-retention` and `--idempotency-retention` take a duration, which `maintain` prunes before; a deployment given none prunes nothing, as a store built with none does (`DESIGN.md` §8, ADR-0103).

## Alternatives rejected

- **A second binary for a deployment.** The two shapes share every operation and differ in four routes, the clock and the store's file; a second binary is a second release to keep the same.
- **A configuration file now.** Four values fit flags; the evaluator endpoints of decision 7 are the first configuration flags fit badly, and the file arrives with them.
- **PostgreSQL first.** `DESIGN.md` §2 names both; the first consumer's sizing is a few hundred live objects per type (PRD N3), which one SQLite file holds, and the core's store is written on SQLite today.
- **Answering an evaluator `unsatisfied` until the transport exists.** It refuses as a rule would, with a remedy that tells the caller to change the request; `unknown` refused `temporal` says what is true, that nothing was asked.

## Consequences

- `DESIGN.md` §2 says a deployment's service is built, its first slice, and what it lacks: the evaluator transport, an upgrade between releases, PostgreSQL and more than one request at a time.
- `docs/guides/integrate.md` and `core/README.md` say how to run a deployment, and that its flow is published through the wire.
- `TODO.md` tracks decision 7's transport, the release upgrade and concurrent requests.
