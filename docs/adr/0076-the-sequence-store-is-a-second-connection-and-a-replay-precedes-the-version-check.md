# ADR-0076: The sequence store is a second connection, its own file on SQLite; and a replay precedes the version check

- **Status:** Accepted — repair of D187 and D188, 2026-09-09; pending author review
- **Date:** 2026-09-09
- **Refines:** ADR-0023, ADR-0029, ADR-0041

## Context

Two defects from the whole-record review of 2026-09-09 (`docs/design/defects.md`), each of which breaks a running system rather than a document, and each found by asking what happens in one concrete case rather than by reading.

**D187.** D181 moved sequence allocation out of the request's transaction so that a rolled-back creation leaves the gap ADR-0029 requires, and the storage schema said "on SQLite it is a second connection". SQLite locks the file, not the row. The mint happens inside the request — after the writes that fix its scope and before the rest of the outcome (`declaration-syntax.md` §3.1) — and by then the request's connection has already written, because the event's position is allocated when the transition begins applying (ADR-0046 §5). A second connection to the same file therefore waits out its busy timeout and fails with `database is locked`, and every creation with an identifier deadlocks against itself. Observed with a two-connection probe against SQLite 3.37, in both journal modes; the schema's "what is verified" paragraph had listed the DDL and not this claim.

**D188.** `DESIGN.md` §6 checked `expected_version` at step 2 and looked up the idempotency key at step 3. A retry is the original request re-sent: it carries the version the original was checked against, and if the original committed, that version is now stale. So a caller doing exactly what ADR-0023 and ADR-0041 ask — supply the version you read, supply a key so a timeout is safe — was refused with `stale` on the one retry the key exists to make harmless. The harness's retrier (`adversarial-harness.md` §2) is scripted to send that request.

## Decisions

### 1. The sequence store is a second connection on every backend, and on SQLite that connection opens a separate database file

The allocation stays outside the request's transaction, as D181 decided, on a connection of its own that commits before the request continues. Where that connection points follows from how the backend locks:

- **PostgreSQL** locks rows. The second connection opens the same database and updates the `ok_sequence` row, which the request's transaction never touches, so nothing contends. An unscoped sequence may be a native `SEQUENCE` instead, whose `nextval` is already non-transactional; a scoped one is a row per scope key, since a native sequence per scope value would be DDL at request time.
- **SQLite** locks the file. `ok_sequence` lives in a **separate database file beside the store**, opened on its own connection and created by the same DDL. A write there does not contend with the store's lock.

A crash between the mint and the request's commit leaves a gap, which is the behaviour ADR-0029 already accepts. `scripts/check-schema-doc.py` runs the probe on every corpus run: a second connection to the store's own file blocks behind an open write transaction and fails, one to a separate file succeeds, and the value it allocated survives the request's rollback.

### 2. The idempotency replay is checked before `expected_version`

The order of `DESIGN.md` §6 becomes: visibility, then replay, then version, then guards. A request whose key has been applied returns the recorded verdict marked as a replay, **whatever `expected_version` it carries**. `stale` is possible only for a request that has not been applied. ADR-0023's rule that `stale` precedes every guard still holds; it now also follows the one check that can say the request already happened.

## Alternatives rejected

### For the sequence

- **Allocate every value the request will need before its transaction opens.** Works for a creation whose scope is an input. It does not work for a creation inside a cascade, whose scope may be read from state the parent's outcome has just written, and pre-reading that outside the transaction reintroduces the unisolated read ADR-0039 closed. Rejected as the larger and leakier change.
- **`ATTACH` the sequence file to the request's own connection.** An attached database joins the connection's transaction, so the allocation would roll back with the request and the sequence would be gapless again, which is what D181 removed. Rejected.
- **Allocate inside the transaction and accept gapless.** Rejected by ADR-0029: a gapless sequence serialises every creation in its scope.
- **Keep the second connection to the same file and retry on `busy`.** Rejected: the request holds the lock the mint is waiting for, so no timeout resolves it.

### For the order

- **Keep the order and tell callers to drop `expected_version` on a retry.** Rejected: a caller cannot know at retry time whether the original committed, which is the whole reason the key exists, and a retry that differs from the original is a different request.
- **Replay before visibility.** Rejected: an object the actor can no longer see must stay `not found`, since existence is information (ADR-0030). The recorded verdict is returned only to an actor who can see the object now.
- **Compare the retry's `expected_version` against the version the original was checked against, and replay only on a match.** Correct and unnecessary: the key already identifies the request, and a key reused with a different body is the digest's job (`ok_idempotency.request_digest`; the key's scope is D189), not the version's.

## Consequences

- `DESIGN.md` §6 is reordered, and a creation request skips steps 1 and 3 rather than 1 and 2.
- `library-api.md` §3 says which of the two fields wins.
- `storage-schema.md` §6 and §11 say where the sequence lives on each backend, and its verified paragraph names the probe. ADR-0029 rule 1, which still read "inside the creating transaction", is annotated in place; it had been superseded by D181 without saying so.
- `scripts/check-schema-doc.py` gains the two-connection probe, so the claim this ADR rests on fails the build if SQLite ever behaves otherwise.
- The harness's retrier now expects a replay for the identical retry, and `stale` only for a request that has not been applied.
- D187 and D188 are resolved.
