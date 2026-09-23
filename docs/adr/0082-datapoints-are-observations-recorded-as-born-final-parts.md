# ADR-0082: A user datapoint is an observation, recorded as a born-final part; a label records what is not yet modelled

- **Status:** Accepted by the author, 2026-09-23 — evaluated at the author's direction against `docs/PRD.md`, the baseline for every design choice
- **Date:** 2026-09-23
- **Refines:** ADR-0057, ADR-0058

## Context

The PRD requires users to record their own datapoints easily, without modelling a transition for each kind of fact (D3). A recorded datapoint must never be edited (D4), must say when it happened as well as when it was recorded (D5), must let operators maintain catalogues as data (D7), and must be erasable (D8). It must also let information be recorded before anyone has modelled it (D6). The first consumer's pre-delivery inspection is the test case (UC-4): one result per check per configured robot, against a checklist operations staff maintain per product configuration.

ADR-0042 decided that every write goes through a declared transition, and rejected a separate `update` operation as "a second write path with its own authority model". `design/data-driven-engine.md` §3.1 and §3.2 evaluate four ways to record a datapoint and three ways to record unmodelled information.

## Decision

### 1. An observation kind is declared once, and expands to a born-final part of its subject

```
observation InspectionResult version 1 on ConfiguredRobot {
  field check   : InspectionCheck
  field outcome : CheckOutcome
  field value   : decimal(10,3)?
  field note    : string?
  recorded by   actor.has(PDI_RECORD)
  occurred within 7 days
}
```

The subject declares the collection in one line, `part inspections : InspectionResult[] inverse subject`. The language expands the kind into:

- a `tracking record` type with one terminal state;
- an `owner` end to its subject;
- a generated `record` creation, whose guard is the `recorded by` clause.

The rule set prints the expansion. The grammar belongs to `design/declaration-syntax.md`; this decides the semantics. Fields are typed; a numeric field may state its unit (`unit "V"`), which the rule set prints and a read returns beside the value, without conversion or checking in arithmetic; and a kind may declare invariants over its own fields — a voltage within a range — checked when an observation is recorded, as any local invariant is (D12).

### 2. Recording is an ordinary creation request

An observation therefore has an actor, an event, an idempotency key and a place in `history`, and ADR-0042 holds unchanged: there is no second write path. A transition's outcome may record an observation with an ordinary `create` step, which is how a transition records what it decided on.

### 3. Two part rules bend, for observation kinds only

**An observation is recordable directly.** It is exempt from check 11's requirement that a part be created only via its whole. In its place, it may not be recorded on a subject in a **terminal** state. That keeps check 11's purpose, which is that nothing is added to a settled whole, since a terminal state is by definition one after which nothing further is recorded.

**The part is implied `survives`**, because a born-final object has no transition for a cascade to drive (check 37).

Recording does not write the subject, so it neither bumps the subject's version nor conflicts with a concurrent transition on it.

A guard that aggregates over observations **records, on its event, the observations it read**, so the decision can be re-evaluated from the record (PRD L2). Deriving that set from positions instead would be wrong on PostgreSQL, where a position is not commit order (D210): an observation allocated a lower position can commit after the deciding request's snapshot.

### 4. Nothing is edited

A correction is a new observation of the same kind and subject that names the one it corrects, and aggregates read the uncorrected ones by default. Both stay in history.

### 5. Occurred time is bounded

A kind declares `occurred within <duration>`. The recorder may supply when the fact happened within that bound, and the event keeps both times.

### 6. A built-in `label` records what nobody has modelled yet

Every type may carry labels: a normalised name, an optional note, and the subject's state when the label was applied. Who may label is a deployment capability, and a type may narrow it. Two rules govern them:

- **A label is read by metrics and diagnostics, and never by a guard, an invariant or a visibility predicate.** A rule that depends on a vocabulary nobody declared changes meaning with a typo. A label becomes logic by promotion to a state, an observation kind or an attribute, which is a publish.
- **A label's note is treated as personal.** Unclassified free text is where personal data hides (ADR-0078's recorded limit), so the subject's erasure redacts it.

When a label is promoted to a state, objects already in flight move into it by ordinary requests, so history says who moved each one. Labels are not copied into the new form.

### 7. `changed_since` reaches a part relationship by its own position

This adopts D202's repair, which observations depend on. The last event on each part relationship of an object is recorded in `ok_attribute_write`, keyed by the relationship's name, and `last_part_event` is retired. That lets a sign-off name `inspections`, and be invalidated by a later result, without also being invalidated by its own creation as another part.

## Alternatives rejected

- **An attribute and an action per kind of fact.** One current value cannot hold a result per check, a checklist that is data cannot become attributes without a publish, and every kind costs a declaration and a publish. It fails UC-4, UC-6 and D3 (`design/data-driven-engine.md` §3.1).
- **A `record` operation beside `request`.** This is ADR-0042's rejected `update` operation. It would need its own authority model, idempotency, erasure, subscription feed and harness checks.
- **Schemaless key–value datapoints.** Erasure could not tell which keys are personal, and guards would come to depend on keys nobody declared.
- **An observation as a reference rather than a part.** `changed_since` reaches only parts (ADR-0057), and erasure reaches only parts (check 39). A reference would need both extended for it, where a part gets both for nothing. ADR-0035's approval is already a part of this shape.
- **Free-text notes for unmodelled information**, as the first consumer has today: they cannot be counted or related to the flow (UC-13).
- **Labels readable by guards.** Logic would come to rest on an undeclared vocabulary.

## Consequences

- On acceptance, `DESIGN.md` §5 and §5.3 gain observations and labels.
- `design/declaration-syntax.md` gains the `observation` form, the `label` built-in, `occurred within`, check 11's exemption, and checks forbidding a label in a guard, an invariant or a visibility predicate.
- `design/storage-schema.md` gains per-kind tables through the ordinary type mapping, keys `ok_attribute_write` per part relationship, and drops `last_part_event`.
- `design/renderers.md` gives each observation kind a tool.
- ADR-0035's approval can be restated as an observation kind without changing its meaning.
- D202 is resolved on acceptance.
- ADR-0057 and ADR-0058 carry the back-link.
