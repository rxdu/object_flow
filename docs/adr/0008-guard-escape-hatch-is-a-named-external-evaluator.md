# ADR-0008: The guard escape hatch is a named external evaluator returning a structured verdict

- **Status:** Proposed
- **Date:** 2026-09-07

## Context

Some guards depend on facts the store does not hold: parts availability in a supplier system, payment status in Xero. These cannot be expressed over the object graph, but excluding them entirely would push users out of the model at the first genuinely awkward rule.

## Decision (provisional)

The object definition references a guard **by name**. An external evaluator answers it, and is contractually required to return a structured verdict — satisfied, or unsatisfied with a reason and a remedy class — in exactly the same shape as a built-in guard.

Guards are additionally marked **eager** (evaluated whenever available transitions are listed) or **deferred** (checked only at execution). Deferred guards may fail at commit time despite the transition having been listed as available.

## Alternatives rejected

### Inline code or lambdas as guards

Maximum expressiveness, and the obvious escape hatch.

Rejected because an arbitrary predicate can be evaluated but never explained. The moment a guard is a code block, "why can't I do this?" degrades to "a rule said no", and the structured blocked-action output that makes the system usable by agents stops working uniformly. It also destroys portability: the model is no longer data.

### No escape hatch

The store enforces only what is declarable; everything else is the consuming application's problem.

Rejected as too austere — though it remains the right answer for computation and effects (ADR-0007). The referenced-evaluator form is deliberately narrow: it covers guards that need external facts, not general extensibility.

## Rationale

The discipline that keeps a generic system generic is a rule about its escape hatch: **the escape hatch must return the same shape of answer as the built-ins.** Systems that let extensions return richer or poorer answers end up with two classes of behaviour, and every consumer must handle both.

## Consequences

- Listing available transitions must stay cheap and frequent; a guard that round-trips to a third party would otherwise make the primary read path slow and dependent on someone else's uptime. Hence the eager/deferred distinction.
- An alternative to deferring is to mirror the external fact locally as a maintained attribute, trading read-time cost for write-time projection and a cache that can silently lie.

## Evidence from the first consumer

Both patterns this ADR describes appear in the inventory system's ADR-0003 on Xero-owned customer identity. The *external evaluator* shape: on delivery completion, the delivery's `order_id` is validated against Xero — reject a placeholder, confirm the invoice exists, store its resolved contact id — which is a guard over a fact the store does not hold. The *mirror* shape: the local `customers` table becomes a 1:1 mirror of Xero contacts keyed by `ContactID`, synced incrementally, with Xero merges followed automatically — the maintained-attribute alternative named under Consequences, chosen there because name matching measured 83% accurate and a query-time join would re-run the error on every read.

The deferred/eager distinction has a concrete cost there too: Xero access tokens expire after thirty minutes and the sync trigger is unresolved, so a guard that reached Xero on every listing would be both slow and intermittently unavailable.
