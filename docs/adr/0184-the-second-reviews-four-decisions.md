# ADR-0184: The second production review's four decisions

- **Status:** **Decided by the author**, 2026-10-12 ("go with your recommendations on the four questions"), on the recommendations `TODO.md` states with each, after the second production review (`design/production-review.md` §6). Decisions 1, 3 and 4 built the same day; decision 2 is the repository owner's to act on.
- **Date:** 2026-10-12
- **Refines:** ADR-0126 decision 1 (the operator's `retire`), ADR-0183 decision 7 (the store's own acts) and decision 10 (idle connections, recorded and not built)
- **Relates to:** `PRD.md` T6, N7; ADR-0114; ADR-0177 decision 6; ADR-0178; ADR-0180; `guides/wire.md`; `guides/integrate.md` §6, §9; `core/README.md`; `SECURITY.md`

## Context

The second production review found the engine still not production-ready. Its blockers and most of its majors needed no decision and were fixed (D594 to D600 and the rest, `TODO.md`). Four needed the author: one because the PRD bounds what the engine may refuse, one because only the repository's owner can act, and two because they change the wire or add dependencies before the promise of ADR-0180 binds.

## Decision

1. **The operator's retirement is requested by nothing.** `Operator.retire` is refused `not_requestable`, as its creation `install` is, offered to no one, and `available` for it refuses `not_requestable`; the operator stays `ACTIVE`. Retiring the store's one operator was irreversible, no transition installing another, and left the store with no one answerable for it; an agent did it through the wire in both reviews. The refusal reads no actor's kind, so PRD T6 and N7 stand; the approval of a version stays the gateway's to refuse to an agent, as ADR-0183 decision 7 says. The built-in type's tables are unchanged (ADR-0177 decision 6): the refusal is in the request path, in both backends.
2. **The repository is pushed and its reporting channel opened.** CI, the advisory scan and the release workflow have never run, and `SECURITY.md` names GitHub's private vulnerability reporting, which was disabled. Both are the repository owner's acts: `git push origin main`, which runs CI for the first time, and private vulnerability reporting enabled in the repository's security settings.
3. **A read has a deadline.** A deployment's read stops when it passes `--read-timeout`, 30 s unless given, and is answered `503`, its fault `read_timeout: …`, its work stopped where it stood: the evaluator and every object read check the deadline every 256 steps. A filter of nested sums ran past 186 s, and a metric 77 s, holding a reader, and an erasure's truncation of the `-wal` behind it (D597), without bound. A sandbox's reads take the writer's lock and have none; the flag without `--store` is refused. `events` takes a page of positions at most, 1,000, where one position repeated 200,000 times built an answer of gigabytes. Both are added to `/v1` before 1.0.0, while ADR-0180's promise does not yet bind.
4. **Connections are served by hyper with a timer.** `axum::serve` sets no timer on hyper's connections, so hyper's limit on reading a request's head never took effect: 700 silent, idle or half-sent connections were open after 70 s, and with the descriptors exhausted `/health` stopped answering. The service now serves each connection through hyper's HTTP/1 builder with a timer: a request's head not read within `--idle-timeout`, 30 s unless given, closes its connection; a body not arriving within it is answered `408`; past `--max-connections`, 512 unless given, a connection is closed at once and the service says so. A graceful stop takes no connection, lets those open finish the request they answer, and closes them. hyper 1.11.1 and hyper-util 0.1.21 become direct dependencies of the service; both were resolved through axum with the features used, so the lockfile adds no crate, only their names in the service's own list; tokio's `time` feature adds none either.

## Alternatives rejected

- **The engine refuses the approval of a version, or the operator's retirement, to a non-human** (option b of the question). Rejected: a refusal by the requester's kind is what PRD T6 and N7 forbid, and would need a PRD revision; refusing the retirement to everyone closes the irreversible failure without one.
- **A new transition that installs another operator**, so a retirement could be undone. Rejected for now: a second operator is a design of its own, ADR-0126 having made the store's operator one; refusing the retirement leaves nothing irreversible meanwhile.
- **A read's deadline as a timer that cancels its thread.** Rejected: a synchronous evaluation cannot be cancelled from outside in safe Rust; the cooperative check stops it within 256 steps of the deadline.
- **A reverse proxy for timeouts**, documented and not built. Rejected: both reviews ranked open connections a major, and a deployment on a Unix socket has no proxy in front.
- **Idle timeouts by a wrapper on each connection's reads**, without hyper's builder. Rejected: it would time out a connection whose request is still being answered, which hyper's own limit on the head does not.

## Consequences

- `wire.md` gains `503 read_timeout`, `408` and the cap on `positions`; `core/README.md` and `integrate.md` §9 the flags `--read-timeout`, `--idle-timeout` and `--max-connections`; `integrate.md` §6 drops the operator's retirement from the acts a gateway must withhold.
- `DESIGN.md` §9 and ADR-0126 say the operator's retirement is requested by nothing; ADR-0183's decision 10 records idle connections as built by this ADR.
- The erased values D597 leaves in the `-wal` while an older read is open stay there at most as long as the read's deadline.
- Held by `the_operators_retirement_is_requested_by_nothing`, `a_read_past_its_deadline_is_stopped` and `idle_and_half_sent_connections_are_closed`, each mutation-checked.
