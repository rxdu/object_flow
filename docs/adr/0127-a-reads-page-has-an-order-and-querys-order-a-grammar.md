# ADR-0127: A read's page has an order, and `query`'s order a grammar

- **Status:** Accepted by the author in advance, 2026-09-28: "for all similar items that you have confidence in an recommendation, just accept", said while the author was taken through the items that need their attention, of which this was the fifth.
- **Date:** 2026-09-28
- **Refines:** ADR-0048 decision 6 (cursor pagination orders by object id), ADR-0084 §5 (`query` filters on the entry time of a tracked value's current interval), ADR-0098 (a metric's read)
- **Relates to:** `DESIGN.md` §10; `library-api.md` §6; `metric-scenarios.md` §3.4, §7

## Context

Writing the metric scenarios found three reads whose order the read surface does not state (`TODO.md`, 2026-09-27):
- **object pages:** `query`, `available` and `exceptions`. ADR-0048 decision 6 already says that cursor pagination orders by object id, "stable and total", but the rule never reached `DESIGN.md` §10 or `library-api.md`, so the question was recorded as if it were open;
- **`query`'s `order`,** a parameter with no form. `metric-scenarios.md` §3.4 wrote `order: "entered_at(state)"` to list the deliveries that have waited longest in their state, which UC-1 asks for and `oldest_open`, giving the age but not the delivery, cannot answer;
- **a metric's rows,** which no rule orders, so `scripts/check-scenarios.py` compares them as sets.

## Decision

1. **An object page ascends by object id** unless `query` names an order, as ADR-0048 decided.
2. **`query`'s `order` is a list of keys, each ascending unless marked `desc`.** A key is an indexed attribute, `created_at`, or `entered_at(<tracked member>)`, when the object took its current state or value. The object id breaks every tie, so a page is total, and an absent value sorts last. A single key may be written alone. For example, `order: "entered_at(state)"`, or, for a shipment, whose `eta` is indexed, `order: ["eta desc", "created_at"]`.
3. **A metric's rows ascend by the dimensions the read keeps, in the order `keep` names them,** each ascending, a time bucket in time order and an absent value last. A read that names no `keep` orders by the declared dimensions in their declared order.

## Alternatives rejected

- **Leaving order to the caller.** A caller can sort one page and not a paged result, and "which open deliveries are oldest in their state" is then a scan the caller computes, which UC-20 and N6 ask the engine to answer in one query.
- **Ordering by any expression.** It would need a sort over every candidate. The keys allowed are what a filter may already use, so the store pages by an index, as ADR-0048 requires of a filter.
- **Ordering a metric's rows by their value.** Its main use, the top groups, is served by ordering the rows a caller already has, and adding it later changes nothing already decided.

## Consequences

- `DESIGN.md` §10 and `library-api.md` §6 state the three orders.
- `metric-scenarios.md` §3.4's `order` is written in this grammar. Its metric tables list their groups in the order its prose explains them, and the script still compares them as sets.
