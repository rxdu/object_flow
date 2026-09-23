# ADR-0032: The expression language gains arithmetic, durations and `sum`/`min`/`max`; domain formulas stay outside

- **Status:** Accepted — taken in autonomous design iteration 4 (2026-09-08); pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0061 — ten decisions from the payments-ledger review; ADR-0068 — money keeps its currency in the declaration. ADR-0047 — expression semantics: three-valued logic, explicit binding and aggregates over types, with division by zero yielding unknown. ADR-0081 — any declared formula over the store's own data is in scope, scores built from it included; only formulas needing outside data or rules stay out. ADR-0084 — a declared metric is a second call a guard may make, and grouped aggregation and windows exist in the metric form.

## Context

ADR-0021 fixed a version-1 language without arithmetic and said the first case that needed it would decide. Three did. The first consumer's available-to-promise is on-hand minus allocated plus inbound; the CRM's deal totals and "days since last activity" are a sum and a date difference; the order system's stock is `reserved := reserved + qty` with a guard `on_hand - reserved >= qty`, its totals are `sum(lines.qty * unit_price)`, and its auto-cancel is `placed_at + 30 min <= now`. Case study: `docs/design/case-study-orders.md` §3.

## Decision

Version 2 of the language adds:

| Construct | Example |
|---|---|
| `+ - * /` on numbers, with the usual precedence; division by zero yields unknown (ADR-0047 corrected this row, which read "a guard failure, never a value") | `on_hand - reserved >= inputs.qty` |
| durations as literals, and `+ -` between a timestamp and a duration, and between two timestamps | `placed_at + 30 min <= now`, `now - last_activity > 14 days` |
| `sum`, `min`, `max` over a relationship, with an expression per element | `sum(lines.qty * lines.unit_price)`, `max(activities.at)` |
| comparison and equality on the results of the above | `payment.amount == total` |

Unchanged: no string operations beyond equality and membership, no user-defined functions, no calls other than a declared external evaluator (ADR-0008), no iteration with an accumulator other than the four aggregates, no recursion. *(ADR-0084 §6 adds a second call, a declared metric, readable in a guard only and consulted before the transaction as an evaluator is; grouped aggregation, time windows, `avg`, `median` and `percentile` exist in the metric form alone, never in a guard, invariant or derived attribute; `DESIGN.md` §5.7.)* The language grows only by an ADR that names the case that forced it.

**The boundary with ADR-0007, restated.** The store evaluates declared arithmetic over its own data where the result is a guard, a derived view, an invariant, or an outcome write of a quantity. It does not own domain formulas — tax, pricing, discounts, currency conversion, scoring — whose inputs or rules live outside the declaration; those are computed by the consumer and supplied as inputs, and guards check them for range and consistency. *(Moved by ADR-0081 §2: the store now evaluates any declared formula over its own data, aggregates across objects and over time and scores built from them included (ADR-0084). What stays outside is a formula that needs data or rules the store does not hold, such as a tax table, a pricing engine or a conversion rate, together with selection, effects, orchestration and open-ended exploration; `DESIGN.md` §5.7, §12.)*

## Alternatives rejected

### Stay at version 1

Rejected: quantity stock cannot be modelled without an outcome that subtracts, and modelling each unit as an object is the first consumer's luxury, not a general answer.

### A general language

Rejected as before (ADR-0007, ADR-0021): a language that can express tax grows without limit. Aggregates and four operators are the observed floor across four case studies.

## Consequences

- `Product.available := on_hand - reserved` replaces a consumer computation; available-to-promise in the first consumer becomes a derived attribute plus a type-scan sum over inbound units.
- Numeric attributes need a declared type (integer, decimal with scale, money with currency); mixing currencies in one expression is a declaration error.
- Every arithmetic guard is still printed as a readable rule.
