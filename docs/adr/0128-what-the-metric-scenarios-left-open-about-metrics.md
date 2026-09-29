# ADR-0128: What the metric scenarios left open about metrics

- **Status:** Accepted by the author in advance, 2026-09-28: "for all similar items that you have confidence in an recommendation, just accept", said while the author was taken through the items that need their attention.
- **Date:** 2026-09-28
- **Refined by:** ADR-0139 — the scale of an average of integers, which decision 3 needs.
- **Refines:** ADR-0098 and ADR-0101 (a metric and its reader), ADR-0106 (what the record measures), ADR-0124 (a combined metric's inputs over the rows a read selects)
- **Relates to:** PRD C2, D11, M1; `declaration-syntax.md` §1, §6.9, §8.3, §10 check 56; `flow-format.md` §4.9; `metric-scenarios.md` §7, §13, §17, §19, §22, §33; `edge-cases.md`

## Context

Writing the PRD's use cases as checked scenarios left five questions about metrics in `TODO.md`, each with the value that depends on it:
1. **A filter over the row's subject's other datapoints.** First-pass yield counts a unit's first result for a check, found with `none(s in r.subject.pdi_results where …)`. The flow checker accepts it, and `declaration-syntax.md` §6.9 names only "a row's own flow data" as a collection a row may range over.
2. **A window on a combined metric.** UC-10's rule wants first-pass yield over the last 30 days. The yield combines two counts and declares no time dimension, so check 56 refuses `over last` on it, and the scenario reads a per-check rate instead (§19).
3. **Rounding a metric's quotient.** `refused * 1.000 / (refused + applied)` has three places, and §8.3 says a quotient rounds half to even only where it is `set` into an attribute or divides two like quantities. 4 in 11 is 0.364 rounded and 0.363 cut.
4. **A set end read as it was when a span began.** A split by a delivery's configuration reads the models of the units bound to it now, so a delivery whose revoke unbound its units loses its configuration in every past span (§3.5, §7).
5. **Archiving as completion.** Work completes on every transition from an open state into a closed or terminal one (§1), so archiving an untouched Jira issue counts in throughput.

## Decision

1. **A metric may range over a collection of the object its row belongs to,** as a guard on that object may: its observation kinds, its set ends, its intervals and its transitions. For an observation or a label that object is its subject, and for an interval or a transition, its object. The path keeps the two-hop limit of a dimension.
2. **A combined metric may be windowed when every input declares a time dimension.** A guard's or a reader's `over last` then selects each input's rows by that input's own time dimension, and the inputs combine over the rows selected, as ADR-0124 has them combine over a bound dimension. Check 56 refuses `over last` on a combined metric only where an input declares none.
3. **A metric's value is rounded half to even to the scale of its expression's type,** as a `set` of the same value would be: three places for `refused * 1.000 / (refused + applied)`, whose type §8.3's arithmetic gives, and six for a quotient of two like quantities.
4. **A dimension path reads each object as it is now.** `.held` reads a tracked member as it was when a span began, and a set end is not tracked, since PRD D11 leaves references to sets out of what is tracked. So a split by a set end reports today's membership for every past span. `edge-cases.md` records the limit, and it is revisited with the author's review of D11.
5. **Work completes whatever the outcome.** A transition from an open state into a closed or terminal one completes work, whether the object was delivered, cancelled or archived. Every standard metric that counts completions carries the state entered, so a reader separates the outcomes by keeping it.

## Alternatives rejected

- **Refusing a filter that reads the subject's other datapoints.** A first result, and anything else defined against the datapoints recorded before it, could then not be declared. The path is one hop to the subject, within what a dimension may reach.
- **Adding `count(distinct … where …)` so that first-pass yield is one metric with a time dimension of its own.** It widens the value grammar to answer one metric, and every other combined metric would still be unwindowable. With decision 2 the yield is windowed through its inputs.
- **Truncating a metric's quotient.** It would make a rate depend on the digit after the last, and a metric's value differ from the same quotient written into an attribute.
- **Tracking set ends.** It would give every set-valued reference an interval index, the cost PRD D11 declined, for one use the scenarios found.
- **A marking for transitions that end work without completing it.** The state entered already tells the outcomes apart, and a reader keeps it. A marking would add a second vocabulary for what the states say.

## Consequences

- `declaration-syntax.md` §1, §6.9, §8.3 and check 56, and `flow-format.md` §4.9, state the five decisions, and `scripts/check-syntax-doc.py` implements decision 2's check.
- `edge-cases.md` records decision 4's limit.
- `metric-scenarios.md`'s findings that `TODO.md` held these questions say where they were settled, and the five questions leave `TODO.md`.
