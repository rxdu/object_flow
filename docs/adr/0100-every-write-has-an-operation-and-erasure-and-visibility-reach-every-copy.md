# ADR-0100: Every write to the store has an operation, and erasure and visibility reach every copy

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` N2, N4, N5, T1, T3, T4, T5, D8, M5 and UC-17; repairs D251 to D258
- **Date:** 2026-09-23
- **Refined by:** ADR-0101 — the import writes only mirrors and never restores an erased value; `whole_open` guards re-parents only; pruning has a floor. ADR-0103 — `maintain` gains `prune_idempotency`. ADR-0105 — `not_erased` extends the import's bar on erased values to every request. ADR-0106 — a gap in legacy history is reported by coverage, beside completeness.
- **Refines:** ADR-0012, ADR-0015, ADR-0030, ADR-0043, ADR-0054, ADR-0058, ADR-0075, ADR-0087, ADR-0091, ADR-0093, ADR-0094, ADR-0096

## Context

`DESIGN.md` §13 says that anything reaching the database by a path other than the store's breaks the mediated guarantee. A review of the whole record against the PRD found writes with no operation, copies of personal values erasure did not reach, and identities that visibility did not hide (`design/defects.md`).

- **D251. The import had no operation.** None of the seventeen operations imports, and `Request` cannot carry legacy entries, legacy intervals or legacy creation times. Nothing limited the built-in assertion to the port either. A recurring mirror refresh holding the deployment capability could set state on owned types, recorded as `imported` and so hidden from `exceptions()` and from the override metrics (T1, T4). And `.imported` had no column behind it.
- **D252. Pruning and archiving had no operation.** Pruning the attempt log with its rollup, and moving events to an archive, were "deployment actions" with nowhere in the API to happen.
- **D253. Erasure missed six copies:**
  - `of_external_id` values of a personal external identifier, so `lookup` still resolved an erased email;
  - the proposal's own creation event, where step 7 redacted only the proposal's column;
  - values the erased object's events passed down into other objects' personal attributes;
  - corrected observations, which no read reaches;
  - legacy payload fields describing another object;
  - the legacy creation fields.
- **D254. Verdicts named objects their requester could not see.** `Unsatisfied.objects` and `InvariantViolated.objects` come from type-scans, which ignore visibility, so a uniqueness conflict returned the id of an object `get` reports as not found.
- **D255. Three execution steps were missing, and one was wrong.**
  - No step wrote the idempotency record.
  - None discharged an admitted violation.
  - Nothing stopped a part being re-parented into a whole in a terminal state.
  - Step 6's "any failure aborts" made an observing clause on a cascaded transition refuse.
- **D256. The export and subscriptions still assumed positions.**
  - Attempt rows had no writing transaction, so they could not be paged by the settled cursor.
  - An interval updated when it closed was never re-emitted to a consumer that had paged past it.
  - Subscription lag was still `log_head - acknowledged`, which a cursor cannot compute.
- **D257. Reads of mirrors and the harness's evaluators were not accounted for.**
  - A guard reading a mirror had no as-of time and no bound, unlike an evaluator's verdict.
  - The evaluators themselves were not among the store's injected dependencies, so a harness run using them could not be reproduced.
- **D258. The legacy data and the first consumer were thinner than the design assumed.**
  - Legacy intervals could not have gaps, although the audit trail has one.
  - A ported reassignment lost who made it.
  - The cutover's write edges were taken only from the transition registry's side-effect lists, and missed the service layer's own writes.
  - Time-driven transitions need a scheduler the first consumer does not have.

## Decision

### 1. The import is an operation, and it cannot write an owned type

`import_batch(actor, batch, dry_run)` writes objects through the built-in assertion. It requires `OF_IMPORT`.

A batch carries, per object:
- its legacy key, state, attributes and references by legacy key;
- its legacy entries;
- its legacy intervals, each with who made the change where the legacy record says;
- its legacy creation time and creator kind.

It writes a type only while that type is a `mirror`, or while no ordinary request has created an object of it. *(Narrowed by ADR-0101 §1: only while it is a `mirror`. A type nobody had created an object of could otherwise be overridden by a refresh long after cutover.)* Once a type is owned, the import refuses it, so the refresh of a mirror cannot override an owned type.

Every event it writes carries `imported`, in a column of `of_event`. That column is what `.imported` reads, and what `exceptions()` and the override metrics exclude.

### 2. Maintenance is an operation, and nothing depends on it

`maintain(actor, task)` runs one of two tasks, and requires `OF_MAINTAIN`:
- `prune_attempts(before)`, which deletes attempt rows and adds their counts to the rollup in the same transaction;
- `archive_events(before)`, which moves events to the archive table the view reads.

Neither changes governed state or what any read returns. *(Corrected by ADR-0101 §4: pruning removes attempt rows past the store's retention, which the attempt sources and export then no longer return, their counts kept in the rollup; it changes no governed state or history.)* A deployment calls it when it chooses, and correctness never waits on it (N2). With `import_batch` and `maintain`, the API has nineteen operations, and nothing else writes the database.

### 3. Erasure reaches every copy

Beyond ADR-0087, erasure:
1. deletes the `of_external_id` rows of the erased object's personal external identifiers, so `lookup` no longer resolves them;
2. redacts the inputs of each affected proposal in the proposal's own creation event, as well as in its column;
3. follows each of the erased object's events down its caused events, and erases what flowed into other objects' personal attributes, both in those events and on those objects. Check 10's taint analysis finds these flows, as it finds the caller's. A person copied into a delivery's contact is erased with the person;
4. reaches every observation in the subject's collection, corrected ones included;
5. never meets another object's fields in a legacy entry, because the import keeps only fields of the entry's own object;
6. erases the legacy creation fields as it erases any personal value. *(Withdrawn by ADR-0101: the creation time and creator kind are not personal values, and nothing needs erasing there.)*

### 4. A verdict names only objects its requester can see

`Unsatisfied.objects` and `InvariantViolated.objects` list only visible objects, and set `withheld` when others were involved, as a read set does (ADR-0095). The clause is still named, and the verdict already discloses that it failed, so the flag discloses nothing more.

### 5. Execution states every write

- Step 2's idempotency record is written in step 8, with the event.
- Step 7 discharges an admitted violation whose invariant holds again, and records the discharging position.
- A transition that writes an `owner` carries a generated guard, `whole_open`: the destination whole is not in a terminal state. *(Narrowed by ADR-0101 §8: a `do` or `act` writing `owner` on an existing object, a re-parent. A creation is covered by check 11, and a recording by `subject_open`, whose correction exemption this would otherwise defeat.)* It is the counterpart for a re-parent of check 11's rule for a creation.
- An observing clause refuses nothing at any depth, cascaded transitions included. Its would-be refusal is logged like the parent's.

### 6. Exports and subscriptions page by the settled cursor

- `of_attempt` records its writing transaction, and the attempt export pages by the settled cursor.
- The interval export pages by the cursor of the event that last opened or closed each row. A closing therefore re-emits the row, and a consumer keeps the latest version per object, dimension and entry.
- A subscription's lag is the age of the oldest event its filter selects after its acknowledged cursor. Its two thresholds, `lag_warn` and `lag_fail`, are durations.

### 7. A mirror read is as-of, and every evaluator is injected

- Every mirror object carries `.imported_at`, the time its last import wrote it. A guard over a mirror may bound it, as a freshness bound does an evaluator's verdict, and `DESIGN.md` §13 lists mirror reads with evaluators and metrics as as-of guarantees.
- The store takes an evaluator source as a fourth injected dependency, beside its ids, clock and connections, and the harness supplies a deterministic one.

### 8. The port states what it cannot recover

- **Gaps.** A legacy interval may be missing where the legacy record is silent. A metric counts such a gap as unknown time and marks itself incomplete, and the harness reports gaps rather than failing on them.
- **Who.** `of_interval` records who made a change: for a recorded interval that is its event's actor, and for a legacy one the legacy record's actor where there is one.
- **Write edges.** The cutover's write edges are extracted from every write the legacy system makes, service-layer methods included, not only from the registry's side-effect lists (ADR-0093 §2).
- **A scheduler.** The first consumer's cutover plan adds one before any type with time-driven transitions migrates, since the store never writes on read (ADR-0012).

### 9. Erasure's admissions are declared overrides

An erasure admits the invariants it breaks (ADR-0031). This is an override in the PRD's sense:
- it happens only where a type declares `erase`;
- it carries a reason;
- it is recorded as an admission;
- `exceptions(type)` lists it.

T1's "except through an override" therefore covers it.

## Alternatives rejected

- **Import through `request`, with more fields.** Every request would then carry fields that exist for one day of a system's life, and nothing would stop an ordinary caller filling them.
- **Let the import write any type under the capability.** The recurring refresh of a mirror would then be a standing override route with no reason recorded against it.
- **Maintenance as an unmodelled deployment script.** It would be the second write path that `DESIGN.md` §13 says voids the guarantee.
- **Leave downward flows to the requester.** A right-to-erasure request names a person, and only the store knows where its declarations copied that person's values.
- **Name hidden objects in a verdict and trust the caller.** ADR-0030 refuses to disclose existence in a read, and a verdict is a read.
- **Page intervals by their entry alone.** A closing would never reach a consumer that had paged past the opening, which would leave the consumer's copy wrong for ever.

## Consequences

- `library-api.md` gains `import_batch` and `maintain`, an evaluator source, and the verdict and interval fields.
- `DESIGN.md` §2, §6, §7, §8, §10, §11 and §13 change.
- `storage-schema.md` §2, §6 and §9 change.
- `publish-and-import.md` §4 and §7 and `first-consumer-cutover.md` §4a change.
- `adversarial-harness.md` §1 and §4 change.
- ADR-0043, ADR-0093 and ADR-0087 are annotated.
- D251 to D258 are resolved.
