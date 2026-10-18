# ADR-0215: A read's deadline bounded at PostgreSQL's client

- **Status:** **Decided** 2026-10-18 by the author ("go with (c)"), option (c) of the question `TODO.md` recorded the same day (PostgreSQL in the release, block 7, i).
- **Date:** 2026-10-18
- **Supersedes in part:** ADR-0213 decision 3 (the client is the `postgres` crate), and its rejection of `tokio-postgres`
- **Refines:** ADR-0184 decision 3 (a read answers within its deadline or is stopped there)
- **Relates to:** the durability review on PostgreSQL, finding 3 (`docs/design/audit/durability-postgresql.md`); `operate.md`, PostgreSQL

## Context

ADR-0184 decision 3 promises a read an answer at its deadline, `503 read_timeout`. The store holds it by checking the deadline between a read's steps (`World::on_time`). A statement that waits on the database is not such a step: on PostgreSQL it waited as long as the server took, and a server that stops answering with its connections open, a process frozen or a disk stalled, takes for ever, its kernel still acknowledging what is sent so neither TCP's keepalives nor `tcp_user_timeout` fire. The durability review found every request held so (finding 3); `/health` was given its own bound on 2026-10-18 (`store_stalled`), the reads were not.

The `postgres` crate, the client ADR-0213 chose, bounds no statement: its source has `connect_timeout` and `tcp_user_timeout` alone, and `tokio-postgres`, which it wraps, applies `connect_timeout` to the socket's connect alone, so a new connection's startup waits on such a server without end too. Nothing armed the facade's interrupt handle, so no cancel request was sent either, which a frozen server would not act on.

## Decision

1. **The facade's PostgreSQL backend calls `tokio-postgres` itself**, blocked on a current-thread runtime of each connection's own, the connection's I/O a task of that runtime, as the `postgres` crate wrapped it; the facade's API stays synchronous, so ADR-0213's reason for the synchronous crate, a synchronous core, holds. The `postgres` crate leaves the tree; `tokio-postgres` was in it already, so no package is added.
2. **A read connection's every statement is bounded by the read's deadline.** Past it the call is dropped, the connection's task aborted, which closes its socket, and the read answered `503 read_timeout`, as a read past its deadline between steps is. The pool drops a closed connection, so the next read makes another.
3. **A connection's making is bounded too**, its startup and the store's checks of the database included: by the read's deadline for a read connection, and by 30 s (`PG_SETUP_TIMEOUT`) for every other, the writer's among them.
4. **A write's connection has no deadline.** A commit cut short has an outcome no one knows, so a write waits for the database as before; the writer's reconnection is bounded by decision 3.
5. **`/health`'s read, stopped at its own deadline, is said as the stall**, `store_stalled`, as its wait past 5 s is.

## Alternatives rejected

- **(a) A stated limit**, `/health` the only signal: leaves ADR-0184's promise broken on PostgreSQL, every read held with the database.
- **(b) The service runs each read on a thread of its own** and answers at the deadline, the thread left blocked until the database answers: every read pays a thread's hop, a stall holds a thread and a connection for each read abandoned, which needs a cap of its own, and a connection's making stays unbounded.
- **A server-side `statement_timeout`**: ends a slow statement on a live server, which the client's deadline does too, and nothing on a frozen one, which never runs its timer.

## Consequences

- `core/Cargo.toml` names `tokio-postgres` where it named `postgres`; `Cargo.lock` loses the `postgres` package.
- `db.rs`: `PgClient`, `Connection::postgres_reading` and `Connection::set_deadline`; the fault `DeadlinePassed`, carried by the store as `deadline_passed` and answered by the service as `read_timeout`, through a world's read of the store too.
- `store.rs`: `Readers::read` takes the read's deadline and gives a kept connection none when it is given back.
- Tests: `a_connection_never_answered_is_given_up_at_its_deadline` (a listener that never answers, no database needed), `a_read_connection_stops_at_its_deadline` (a `pg_sleep` stopped, the connection closed, one without a deadline waiting), `a_stalled_database_is_said_by_health`, whose reads through a proxy that stops passing bytes are answered `503 read_timeout` by their 8 s deadline, a `get` on the connection the pool kept, a `get` on one made and a `query`, and `a_read_stopped_at_the_database_is_a_read_timeout` for the service's two mappings. Six mutants killed: no statement deadline, no bound on a connection's making, and each mapping; the first survived the stall test until its reads came before `/health`'s, whose stuck probe had held the one kept connection, so every read made a new one.
- `operate.md`'s PostgreSQL section says a read is answered at its deadline by a database that stops answering, and a write waits.
