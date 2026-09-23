# ADR-0077: Seven decisions from repairing the whole-record review

- **Status:** Accepted — repair of D189, D191, D192 and D195 to D200, 2026-09-09; pending author review
- **Date:** 2026-09-09
- **Refined by:** ADR-0083 — `state_source` gains `imported`, kept apart from `asserted`. ADR-0089 — the settled position becomes a settled cursor; ADR-0091 — the connection source joins the injected dependencies; ADR-0092 — `Stale` carries its cause.
- **Refines:** ADR-0018, ADR-0027, ADR-0037, ADR-0054, ADR-0056, ADR-0075

## Context

Nine findings of the whole-record review (`docs/design/defects.md`, D187 to D200) were repaired in one pass, the author having set aside the three that need a ruling (D190, D193, D194). Seven of the nine are decisions rather than wording, and a decision that lives only in an edit is not recoverable later, so they are recorded together here in the manner of ADR-0041 and ADR-0054. Two rest on claims about SQLite that were probed rather than reasoned, since the previous claim of that kind in the same document was false (ADR-0076).

## Decisions

### 1. The idempotency key is scoped to the actor's `id`, and a reused key with a different body is a fault (D189)

`ok_idempotency` keys on `(actor_id, key)`. `id` is the one member of the descriptor that is never absent; `principal` is optional (ADR-0025), so a human acting for themselves had no row to write under the previous scope. The first consumer's evidence reads "per principal" (ADR-0014) because its principals are its authenticated parties, which is what `actor.id` is here.

A key applied before by this actor with the **same** request digest replays (ADR-0041, ADR-0076). One applied with a **different** digest is refused as a fault, `KeyReused`, the fifth exception of `library-api.md` §7: a retry is the same request, so a different body under a used key is a defect in the caller's key generation, and neither replaying the other request's result nor applying this one under its key would be honest.

### 2. Import writes every row complete, in one pass, after assigning every id; a stored end's foreign key is checked at commit (D191)

Every object's new id is assigned from the legacy-key mapping **before any row is written**, so every reference, required or optional, resolves to an id at the moment its row is inserted, and an unresolvable legacy key is found at the mapping and not by the database. Rows go in dependency order where one exists, so a failure names the first row that could not be placed. A cycle of required references has no such order and commits in one transaction with its foreign keys checked at commit.

That last clause is a schema decision: **the foreign key on every stored relationship end is `DEFERRABLE INITIALLY DEFERRED` on both backends.** The runtime resolves every reference itself and the constraint is the backstop, so checking it at commit loses nothing, and it is also what lets one outcome create two objects that require each other. SQLite honours the clause, which `scripts/check-schema-doc.py` now demonstrates three ways.

### 3. A guard-class violation cannot be admitted; the dispositions available depend on the class kind (D192)

A violation class carries its kind, `INVARIANT` or `GUARD`. An invariant class may be cleaned upstream, admitted or excluded. A guard class — a required attribute or reference absent, a creation guard unmet — may be cleaned upstream, supplied by the mapping, or excluded, and the dry run refuses `ADMIT` on it, because an admission names an object and an **invariant** (ADR-0054) and the column is `NOT NULL` besides.

### 4. "Declaration version" is the publish's number; "type version" is the number in the text; objects and events record the former (D195)

Each publish installs the whole closure under one **declaration version**, per store and monotonic — `ok_declaration.version`. Each type, machine, enum, sequence and evaluator carries its own **type version** in the text, and advancing one advances every type that depends on it (check 22). An object and an event record the declaration version in force, which fixes the version of every type at that instant; a type version alone would not, since a machine or enum it depends on may have moved. The row column is renamed from `type_version` to `declaration_version` to say what it holds.

### 5. `check` returns its verdict with the guards it did not evaluate; `pull` returns the settled position; `history` yields legacy entries; an event carries the provenance the log stores (D196)

`check` returns a `Checked`, the verdict and the external guards it did not evaluate, since ADR-0054 made its verdict partial and no `Verdict` member could say so. `pull` returns an `EventPage` carrying the **settled position**, the highest position below which no transaction is still in flight, which is as far as a consumer may acknowledge (ADR-0034, `storage-schema.md` §7). *(Replaced by ADR-0089 §3 and §5, repairing D210: nothing in the schema could compute that position, and on PostgreSQL a position is not commit order. `pull` returns a **settled cursor**, a (transaction, position) pair up to which every writing transaction had finished when the reader's snapshot was taken, and `acknowledge` takes it; `DESIGN.md` §7.)* `history` yields `Event | LegacyEntry`. An `Event` carries what `ok_event` stores — actor id, kind and principal, context, declaration version, taint version, and the as-of time of each external verdict, which the payload now holds keyed by guard name — and not a full descriptor, whose capabilities the log never recorded. `Request.context` is a string, as the model always said. A push subscription stores the descriptor its worker delivers as, `deliver_as`, since visibility is applied per actor and a worker has none of its own.

### 6. `state_source` resets only on an event that changes state (D197)

An ordinary transition that **changes state** sets `state_source` back to `observed`; an action leaves it alone. Editing a note on an asserted unit does not make its state any less asserted, and the object stays in `exceptions(type)` until the machine has moved it, which is the control ADR-0040 §6 rests on.

### 7. Object ids are UUIDv7, and the store takes its id source and clock as injected dependencies (D198)

Time-ordered rather than random, as ADR-0018 recommended and left to storage to confirm, because two guarantees now rest on the order: ADR-0038 iterates a cascade in ascending id order so that a request is reproducible, and the harness reduces a failing run by replaying its transcript. With random ids neither holds. Stored in canonical text form on both backends. The embedded time is never read back as a fact about the object, which is what `created_at` is for. The store takes its id source and its clock as injected dependencies, so a test can make both deterministic; even a time-ordered id is not reproducible from a seed without that.

## Alternatives rejected

- **Scope the key to `principal`, falling back to `id`.** Rejected: two agents acting for one principal would then collide on a key string, which is the case ADR-0014's evidence says must not collide, and the fallback makes the scope depend on which optional field a consumer fills.
- **Treat a reused key with a different body as `Unsatisfied`.** Rejected: a verdict answers "may I", and this is "you have already asked something else under this name". It is the same class as `UnknownTransition`.
- **Keep two-pass import and refuse required-reference cycles.** Rejected: two passes could never have written a required reference at all, since its column is `NOT NULL`, so the first draft was wrong before any cycle arose; and refusing cycles would also refuse the same shape at ordinary creation time.
- **Defer only during import, with `SET CONSTRAINTS` on PostgreSQL and `PRAGMA defer_foreign_keys` on SQLite.** Workable, and rejected because a mutually required creation needs the same deferral in an ordinary transaction, and a constraint whose timing differs per code path is a rule with two meanings.
- **Let a guard class be admitted by inventing an admission shape for guards.** Rejected: an admission suppresses an invariant's *check*; a missing required value has nothing to suppress and a `NOT NULL` column will not hold it.
- **Rename the event column to `publish_version` instead.** Rejected as churn: "declaration version" already meant the publish in ADR-0027 and on the event; the row column was the odd one out.
- **Put `unevaluated` on every `Verdict`.** Rejected: six of seven verdicts never have one, and a field that is empty by construction on most values invites `except: pass`.
- **Random ids, with the harness sorting by creation time instead.** Rejected: the cascade order is the store's rule, not the harness's, and the store would need a second creation-order column to honour it.

## Consequences

- `DESIGN.md` §3 names both assertions, §5 carries `mirror` and `survives` in the model block, §5.1 and §6 say the id order is creation order, §5.9 and §14 define the two versions, §6 step 2 scopes the key, §7 and §10 name the settled position, §11 carries staged cutover and one-pass import.
- `library-api.md` gains `Checked`, `EventPage`, `LegacyEntry`, the event's provenance fields and `KeyReused`; `Request.context` is a string.
- `publish-and-import.md` §4 assigns ids before any row is written, §6 carries the class kind and the fifth rule, §8 retracts two-pass import and says why.
- `storage-schema.md` §2 decides the id, §3 renames the version column, defers the stored-end foreign key and says when `state_source` resets, §4 records as-of times, §6 rekeys idempotency and adds `deliver_as`, §7 names the settled position, §11 lists the decisions.
- `adversarial-harness.md` names what the retrier tests and what reproducibility rests on.
- `scripts/check-schema-doc.py` runs the deferred-foreign-key probe; `scripts/check-syntax-doc.py` enforces check 7's stored-end clause, which found D199's line on its first run; `scripts/check-corpus.py` holds the counts of D200 against their sources.
- ADR-0018's open format item is decided, ADR-0056 §4 and ADR-0075's two-pass sentence are annotated, and ADR-0027, ADR-0037, ADR-0054 and ADR-0075 carry the back-link.
- Found while repairing D191 and recorded as D201: ADR-0075 breaks a cycle by the optionality of its edges, which answered the import question that no longer exists and not the stage question it was asked in. *(Fixed later the same day: the exception is struck through in ADR-0075 with the reason, and the cutover document's §2 is rewritten to match its own stage table.)*
- D189, D191, D192 and D195 to D200 are resolved.
