# ADR-0210: Five questions decided as recommended

- **Status:** **Decided** 2026-10-16 by the author ("go ahead with your recommendations"), given after the open questions were listed the same day; each decision is the recommendation `TODO.md` recorded for it. E16, E24, E32 and an examples step expecting an import's refusal had no recommendation recorded and are not decided by it; nor is the release gate the author keeps (ADR-0204 §7, §8).
- **Date:** 2026-10-16
- **Refines:** ADR-0204 §1 (how the profile's report readers read); ADR-0100 §7 (a mirror's `imported_at`); ADR-0088 (what a refusal's read set holds)
- **Relates to:** question 16 of 2026-10-14 (`TODO.md`); ADR-0205 decision 13; ADR-0146 decision 7; ADR-0160 (an erasure's walk); D547; `adversarial-harness.md` §1; `design/hiring-third-review.md` E3

## Context

Five questions stood open, each in `TODO.md` with its options and a recommendation:

- **Question 16, Gate P's report readers.** Gate P played four report readers each reading the moment the last read answered, and two agents likewise; back to back, a read's latency is the readers' count over the free cores times its cost, so the gate measured the machine's saturation more than a read's cost. The audit's own paced variant passed at 1x and 2x on the two-year store. On the six-year store every whole-history metric fails C5 even alone (1,167 to 1,585 ms), a metric over a type's whole history being a scan linear in the store.
- **A mirror's `imported_at`.** ADR-0100 §7 gives every mirror object `.imported_at`, "the time its last import wrote it", for a freshness guard; the checker admits it, the store keeps no such column and both backends refuse it at run time, and ADR-0146 decision 7, an unchanged refresh writing nothing, makes "the last import that wrote it" mean the last that changed it, which a freshness guard cannot use.
- **What an erasure reaches through a caller's input.** An input a caller passes into another object's personal attribute is personal for the caller since 2026-10-15 (`input_personal` one hop on): the export omits it, an attempt withholds it, a digest masks it. The caller's own erasure still leaves it on the caller's event, since applying the rule to the erasure's walk also cleared the other object's value.
- **A refusal an effect causes.** ADR-0088 records what a transition's rules read; an effect reads too, and can refuse its request, `value_in_range` or `call_refused`, so such a refusal's record does not explain it (found by the harness's read-set condition).
- **E3, a request naming no type.** A request for a type the installed version does not declare is answered `unknown_transition`, naming the transition, where `query` and `available` answer `unknown_type`.

## Decision

1. **Gate P's readers have a pace** (question 16 (a)). A report reader reads a report about every 10 s, an agent likewise, and a session pauses about 1 s, as the audit's own variant did; `scripts/profile/load.py` plays that by default, and back to back is a stress run reported beside it, not the gate.
2. **A report reads over a window, narrowed by an index on time** (question 16 (d)). A metric read over a window narrows its rows by an index on the time dimension, so its cost follows the window's activity and not the store's age; a whole-history report beyond two years is a stated limit in `operate.md`, not a C5 promise, until totals kept by the writer or reports answered from a recent computation are chosen.
3. **A mirror's `imported_at` is withdrawn before 1.0.0** (option (c)). Both checkers refuse reading it, the name stays reserved against shadowing, and the documents say so, so that it can be added with a use and a meaning, which adding is and removing is not.
4. **A caller's own erasure redacts on its own event an input it passed into another object's personal attribute, and nothing that input flowed into** (option (ii)). The other object's value is that object's to erase.
5. **A refusal's read set holds what its effects read before they stopped** (option (A)), besides its rules' reads, so a refusal replays over its record. A satisfied event's read set stays its rules' reads, what its effects did being recorded as what it wrote (ADR-0209).
6. **A request naming no type of the installed version is answered `unknown_type`**, as the reads are (E3): its remedy `unreachable_from_here` where version 0 is installed, since only the operator's publish helps, and `self_serviceable` otherwise.

## Alternatives rejected

Each question's other options, as `TODO.md` recorded them:

- **Back to back as the gate, metrics made three times cheaper by totals the writer keeps** (question 16 (b)). An incremental aggregate per metric and group, a design of its own, which adds to each write what it takes from each read, against A1.
- **A report answered from a computation at most N seconds old** (question 16 (c)). A product decision on staleness; it can be added later for reports at a scale the window does not cover.
- **`imported_at` derived from the latest `import` event** (option (a)), meaning when an import last changed the object, which no freshness guard can use; **a column every import stamps** (option (b)), a write per unchanged refresh against ADR-0146 decision 7.
- **The erasure's walk following the caller's input** (option (i)), which cleared the other object's value; **as now** (option (iii)), leaving the input on the caller's event after its erasure.
- **Every event's read set holding its effects' reads** (option (B)), growing every event for what `wrote` already explains; **ADR-0088 kept** (option (C)), a refusal by an effect explained by its clause alone.
- **E3 left as it is**, a request answered otherwise than the reads.

## Consequences

- `load.py`'s defaults, `operate.md`, `adversarial-harness.md` and ADR-0204 §1's reading of the profile change with decision 1; the six-year store's whole-history reports are a stated limit with decision 2.
- `declaration-syntax.md` §8, `storage-schema.md` §4 and ADR-0100 §7's `imported_at` are withdrawn with decision 3.
- `storage-schema.md` §9 step 3 says what a caller's erasure reaches with decision 4.
- A refusal's attempt row records, under the rule `effect`, what its effects read before they stopped, with decision 5. *(This said at first that the harness's refusal condition no longer leaves an effect's stop out; it never did, the stop it leaves out being a satisfied request's, which decision 5 leaves as it was.)*
- `wire.md` and `errors.md` give `unknown_type` for a request with decision 6, in both backends.
