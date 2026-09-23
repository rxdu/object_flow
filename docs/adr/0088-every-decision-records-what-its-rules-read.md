# ADR-0088: Every decision records what its rules read

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` L2, G5, T3 and UC-4
- **Date:** 2026-09-23
- **Refined by:** ADR-0095 — a read filters the read set to what the reader can see and says that something was withheld.
- **Refines:** ADR-0033, ADR-0082

## Context

PRD L2: "Whenever a rule's decision depends on data, the record holds the values it used, so the decision can be re-evaluated from the record alone". G5 says the same of every decision the engine's rules make. A rule is a guard or an invariant (PRD §5).

The design recorded an event's inputs and writes, an evaluator's or metric's value, and — from ADR-0082 — the observations a guard read. It did not record the other objects a guard read, what a type-scan found, what an actor's capabilities answered, or the `now` a guard compared with. So a completion's `settled` guard over its checklist, or a one-open-engagement invariant, could not be re-evaluated from the record.

The obvious shortcut — refold the log up to the decision's position — is wrong on PostgreSQL. A position is not commit order (D210; ADR-0089's probe), so the objects' state "as of that position" is not what the decision saw.

## Decision

### 1. Every event records the read set of the rules evaluated for it

The read set is:

- **every object a guard or invariant read, with the version it read.** That includes the object itself, related objects, the elements of an aggregate, and every object a type-scan matched. A type-scan that matched nothing records an empty set;
- **for each `actor.has(C)`,** the capability asked and the answer;
- **the value of `now`,** which is fixed once per request;
- **as already recorded,** the inputs, and each evaluator's verdict and each metric's value, with their as-of times.

### 2. Re-evaluation uses per-object order, which is strict

An object's versions are ordered by `object_seq`, which the schema makes unique per object (`storage-schema.md` §2). Folding each read object up to its recorded version, and evaluating the rule over those states with the recorded `now`, answers, and capability results, gives the same verdict. No global order is needed.

### 3. Observing clauses and refusals are included

An observing clause's would-be refusal carries its read set. An attempt-log row carries the read set of its failing clause: ids, versions and capability answers, never inputs. So "which rule is in the way" can be explained as well as counted.

### 4. The read set holds identities, not values

It records ids and versions. The values are in the objects' histories, which erasure redacts (ADR-0087), so the read set needs nothing from erasure. A decision re-evaluated after an erasure sees absence where the erased value was, and the record says an erasure happened.

### 5. ADR-0082's narrower rule is subsumed

Recording the observations a guard read is this rule applied to observations, which are objects.

## Alternatives rejected

- **Record the values read, not the versions.** It duplicates data the objects' histories already hold, and it copies personal values into places erasure would then have to reach.
- **Refold by global position.** Wrong on PostgreSQL (D210): a read object's state at the decision's position is not the state the decision saw.
- **Record read sets only for data-driven rules (L1, L5).** G5 says every decision the engine's rules make, and a capability check is exactly the decision an auditor asks about first.
- **Record nothing, and rely on the rules being deterministic.** Rules are deterministic over their inputs, and the inputs are what is not recorded.

## Consequences

- `DESIGN.md` §6 and §7 describe the read set.
- `storage-schema.md` gives `ok_event` and `ok_attempt` a `reads` column.
- `library-api.md`'s `Event` gains `reads`.
- The adversarial harness gains a failure condition: an event whose rules, re-evaluated over its read set, give a different verdict.
- Its cost is bounded by what rules read. Aggregates over parts are bounded by declared limits, and type-scans match the few objects a uniqueness or existence rule finds. It is to be measured with the rest (`design/data-driven-engine.md` §7).
