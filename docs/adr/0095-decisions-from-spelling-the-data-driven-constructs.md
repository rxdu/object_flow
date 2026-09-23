# ADR-0095: Six decisions from spelling the data-driven constructs, among them what a reader is shown of a decision's record

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` T5, L2, L5, D4, D5, D6, D12 and UC-19; repairs D216 and D217
- **Date:** 2026-09-23
- **Refined by:** ADR-0096 — the same filter applies to an attempt's read set and consulted values, and to a refusal's, and a recorded metric value is tested against every type the metric reads, not its source type alone.
- **Refines:** ADR-0030, ADR-0082, ADR-0083, ADR-0084, ADR-0088

## Context

The author asked for the design to be iterated until every PRD requirement is covered. Writing the grammar for ADR-0082 to ADR-0086 into `design/declaration-syntax.md` (iteration 19) meant deciding what those ADRs had left to the syntax. Mapping PRD T5 in `design/traceability.md` then found two defects in decisions already accepted.

PRD T5: "Visibility rules apply to every read, including datapoints and metrics; a metric does not reveal what its reader could not see."

**D216.** ADR-0088's read set lists the id of every object a guard read, a type-scan's matches included. `history`, `pull` and `export` return an event under the visibility of the event's own object. A reader who may see the object therefore learns the ids of objects they may not see, and ADR-0030 says why that matters: "existence is information".

**D217.** ADR-0084 puts a metric guard's value on the event beside the guard's name. `storage-schema.md` §4 records only the value's as-of time, and `Event` has only `as_of`. The value a decision used was not recorded, which L2 requires.

Neither can be fixed by computing a metric guard under the requester's visibility. A rule whose verdict depended on who asked would not be a rule, and ADR-0030 already settles this for type-scan guards: they "evaluate over all objects regardless of the requesting actor's visibility; a guard is a rule of the type, not a view". The fix belongs on the read side.

## Decision

### 1. A metric in a guard reads every row, as a type-scan guard does

A metric referenced by a guard is computed over every row its source holds, not under the requester's visibility. The value it produced and the value's as-of time are both recorded on the event, keyed by the guard's name. Everywhere else a metric is computed under its reader's visibility, as ADR-0084 decided.

### 2. A reader is shown a decision's record only as far as their visibility reaches

`history`, `pull` and `export` show an event's read set, and the evaluator verdicts and metric values it records, filtered for the reader:
- an object the reader cannot see is left out of the read set;
- a recorded metric value is left out unless the reader can see every current object of the metric's source type; *(widened by ADR-0096 §3 to every type the metric reads, its paths included)*
- the read set says that something was withheld.

The flag is kept on purpose. Without it, a partial read set re-evaluates to a different verdict and nothing says why. The flag discloses only that the decision read something the reader cannot see, and the verdict itself already disclosed that when it named the clause. An auditor who must re-evaluate every decision reads as an actor that can see everything, which is the pattern ADR-0030 already gives a subscriber that must see everything.

### 3. Backdating is bounded by every value the request changes, and a request has one occurred time

A request's occurred time may not precede the start of the current interval of any state or tracked value it changes. ADR-0083 bounded it by "the object's previous state change", which leaves a negative interval possible on a tracked value an `act` changed after that. Every transition the request cascades to records the same occurred time, since one request is one change.

`backdatable` is legal on a `create`, a `do` or an `act`, and is a publish error on:
- an `assert`, whose time is when the store was told;
- an `erase`, which is not a step of the flow being measured;
- an `only via` transition, which is never requested and so is never given an occurred time.

### 4. A label is readable only as a metric's source

ADR-0082 forbade a label in a guard, an invariant or a visibility predicate. A derivation a guard reads, or an attribute an outcome copied a label into, would carry the label into a guard at one remove. So a label may be named only as `<Type>.labels` in a metric's `from`, and anywhere else is a publish error (check 55).

### 5. A correction hides what it corrects from every read of the kind

Through the subject's part, which guards read, and as a metric's source alike, a read of an observation kind sees only the observations nothing has corrected. `history` and `export` keep both. A single rule means a guard and a metric cannot disagree about which result stands (PRD C2).

### 6. An observation kind's shape is fixed by checks

- `recorded by` is mandatory, since a kind anyone could record would let a gate that reads it be opened by anyone (UC-19).
- The subject declares exactly one set-valued part of the kind, with `inverse subject`, since that part is what erasure and `changed_since` reach.
- `occurred within` is optional. Omitted, the occurred time is the recorded time.

These are checks 54 and 58 of `design/declaration-syntax.md`.

## Alternatives rejected

- **Compute a metric guard under the requester's visibility.** Two actors would get opposite verdicts from one rule on one object, and the value on the event would still disclose, to a later reader, rows that reader cannot see.
- **Record no metric value, only its as-of time.** The decision could then not be re-evaluated from the record, which is L2.
- **Omit what the reader cannot see, without saying so.** A reader re-evaluating the decision would silently reach a different verdict.
- **Refuse the whole read set to any reader who cannot see all of it.** One invisible match would hide every object a decision read, including the ones the reader may see, and L2 would fail for every reader short of an auditor.
- **Keep ADR-0083's bound as written.** A backdated `act` could open a value's interval before the previous one closed.
- **Allow a label in a derivation or an outcome value.** Each is one step from a guard, which is what ADR-0082's rule exists to prevent.

## Consequences

- `DESIGN.md` §5.4, §5.12 and §6 state decisions 1 to 3.
- `library-api.md` §5 gives `ReadSet` a `withheld` flag and `Event` the recorded values, and §6 says how reads filter them.
- `storage-schema.md` §4 records each consulted value beside its as-of time.
- `declaration-syntax.md` §4.2, §6.8 and §6.9 spell decisions 3 to 6, with checks 54, 55 and 58.
- D216 and D217 are resolved.
