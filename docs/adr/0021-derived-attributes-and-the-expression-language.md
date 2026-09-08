# ADR-0021: Derived attributes are named expressions, never stored; the expression language is small and grows only by decision

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

The first consumer computes slot state (`UNFILLED` / `FILLED` / `FULFILLED`) from foreign keys and the bound unit's status and deliberately does not store it (`wr:app/models/enums.py`, `SlotState`); its operations design says "demand is derived, not stored". Guards read such values: "no unfilled required slot" gates delivery completion. ADR-0004 rejects derived state, and TODO.md challenge 3 asked whether that rejection covers this. Separately, no document said what guards may express. Walkthrough §2.3 tabulated every guard the first consumer needs. Walkthrough §4 D.

## Decision

**Derived attributes.** *(Semantics and the acyclicity requirement are given by ADR-0047; queryability by ADR-0048.)* A type may declare a named expression over its own attributes, its relationships and the clock. It is never stored; it is evaluated on read and when a guard references it. It has no transitions and cannot be written. ADR-0004 stands as rejecting derived *lifecycle state* — a state with transitions nobody requested; a derived attribute is a view.

**The expression language, version 1.** Guards, derived attributes, invariants and outcome filters share one language:

| Construct | Example |
|---|---|
| literals; attribute paths across relationships | `model.label_photo_required`, `binding.delivery.state` |
| comparison, null test, membership | `end_date <= now`, `reason in CancellationReason` — *the `reason != null` example that stood here is broken under ADR-0047's three-valued comparison; ADR-0053 replaces it with `reason is not null`* |
| boolean logic and implication | `a and b`, `not a`, `a → b` |
| `count`, `all`, `any`, `none` over a relationship, with a predicate | `none(slots where role in (PRIMARY, INCLUDED) and unit == null)` |
| the same over a type, with a predicate (a type scan) | `none(Service where unit == this and state != CANCELLED)` |
| `now`, `actor.*`, `inputs.*`, `this` | `actor.has(DELIVERY_COMPLETE)` |
| conditional expression, for derived attributes only | `unit == null ? UNFILLED : …` |
| a named external evaluator (ADR-0008) | `xero.invoice_valid(order_id)` |

**Not in version 1:** arithmetic, string operations, user-defined functions, and any call other than a declared external evaluator. Every guard observed in the first consumer fits without them. The language grows only by an ADR that names the use case that forced it. *Version 2 (ADR-0032, iteration 4) added arithmetic, durations and `sum`/`min`/`max` after three case studies asked for them; the rest of the exclusions stand.*

## Alternatives rejected

### Computed stored attributes

Rejected by ADR-0007 and again here: a stored computed value can disagree with its inputs, and the mechanism that keeps it current is a second, unguarded writer.

### Leave all derivation to consumers

Rejected: slot state would be computed in the UI, the API and the completion guard separately — the five-copies problem ADR-0010 exists to end.

### A general expression language from the start

Rejected by ADR-0007's own reasoning: a language that can express tax calculation grows without limit and imports the domain-specific residue. Starting from the observed floor keeps the escape-hatch discipline of ADR-0008.

## Consequences

- ADR-0004 is refined in wording only.
- The read surface returns derived attributes alongside stored ones; the readable rule set prints them as definitions.
- A type-scan predicate in a guard or invariant is a global query and needs an index; this is the "global queries versus maintained projections" question in TODO.md, now with concrete instances.
- Available-to-promise, sums and totals remain consumer computations supplied as inputs where a guard must check them.
- TODO.md challenge 3 is closed.

## Amendment (design iteration 2, 2026-09-07; pending author review)

Outcome writes take the form `attribute := expression`, in the version-1 language above: an input, a literal, `now`, `actor.id`, or an attribute path over `this` and its relationships. Examples from the ticket case study: `resolved_at := now`, `resolution := null` on reopen, `assignee := project.lead`. This adds no construct to the language; it states where expressions may appear.
