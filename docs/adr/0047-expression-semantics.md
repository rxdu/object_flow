# ADR-0047: Expression semantics — three-valued logic, explicit binding, aggregates over types, declared inputs, declared remedy classes, acyclic derivation

- **Status:** Accepted — repair of D16, D17, D19 to D24, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Amended by:** ADR-0052 — expression semantics amended where re-expressing the case studies found them underspecified. ADR-0063 replaces the remedy-class inference with a priority order that never contradicts a declared class.
- **Refines:** ADR-0021, ADR-0032, ADR-0005

## Context

The expression language had a construct list and no semantics. Review found eight defects: `sum` was required over a type and granted only over a relationship; a grouped aggregate was used and never defined; division by zero was said to produce a verdict rather than a value, which makes expressions non-compositional; null semantics were unstated, and one plausible reading lets erasure grant an erased person self-approval; derived attributes were not required to be acyclic, so mutual definitions were legal; scoping was ambiguous inside predicates; parameter schemas were said to be derived from guards, which is abduction and has no algorithm; and remedy-class assignment had no rule.

## Decision

### 1. Three-valued logic, and null is safe

Comparison with an absent value yields **unknown**, not true or false. Unknown propagates through the operators. **A guard that evaluates to unknown fails**, and reports the clause that was unknown.

This closes a real hole. The redaction marker reads as absent (ADR-0031), so after erasing a requester, `actor.id != requested_by.id` is unknown and the separation-of-duties guard fails rather than passing. The claim in the edge-case catalogue that "erasure never silently satisfies a guard" becomes true instead of aspirational.

### 2. Division by zero yields unknown

Not a verdict. An expression always evaluates to a value or to unknown, so expressions compose inside aggregates, implications and derived attributes. A guard containing a division by zero fails, naming the clause.

### 3. Aggregates work over relationships and over types alike

`sum`, `min`, `max` join `count`, `all`, `any`, `none` in accepting either. This is what ADR-0032's own consequence needed for available-to-promise, and there was no reason for the asymmetry. Aggregates over an empty set: `count` and `sum` are zero, `all` and `none` are true, `any` is false, `min` and `max` are unknown.

### 4. No grouped aggregation

`sum(x for a y)` is not added. The case that used it, a shipment quantity per order line, is a relationship aggregate declared on the line: `sum(allocations.qty) <= qty`. Grouping belongs to the read surface, not to guards.

### 5. Binding is explicit

Every aggregate and predicate binds its element: `none(a in approvals where a.approver == actor.id)`. `this` always means the object the expression is declared on and never rebinds. Free names are a publish error. This removes the ambiguity that made `changed_since(relevant, event)` readable two ways.

### 6. Inputs are declared, not derived

A transition **declares its inputs**: name, type, required or optional. Guards reference them. ADR-0005's stronger claim, that the parameter list is derived from the guards, is withdrawn: recovering an input schema from an arbitrary boolean expression is abduction, not extraction, and no algorithm exists for it.

The anti-drift property that ADR-0005 and ADR-0010 rest on is preserved by a cheaper mechanism: inputs and guards are in **one declaration**, and publishing rejects a guard referencing an undeclared input and warns on a declared input no guard or outcome uses. Where a guard has a recognisable shape, the read surface additionally reports the constraint it implies, such as an upper bound from `inputs.qty <= on_hand - reserved`, as a best-effort hint on the schema rather than as the schema.

### 7. Remedy classes are declared per clause

A guard clause declares its remedy class. Where the shape is unambiguous the class may be omitted and is inferred: a comparison against `now` is `temporal`, a comparison against an input is `self_serviceable`, an aggregate over another type is `dependent`, an `actor.*` test is `delegable`. Publishing warns where a class is omitted and the shape is ambiguous. A failing conjunction reports the first failing clause and its class.

### 8. Derived attributes must be acyclic

The dependency graph among derived attributes is checked at publish and a cycle is rejected. Only the cascade graph was covered before, so `a := b + 1; b := a + 1` was legal and non-terminating.

## Alternatives rejected

- **Two-valued logic with null as a value.** Rejected: it is what makes erasure defeat exclusionary guards, and it makes `x != null` a way of asking two different questions.
- **Division by zero as a verdict.** Rejected: expressions must compose. A verdict is what a *request* yields, not what a subexpression yields.
- **Deriving input schemas from guards.** Rejected as undecidable in general; see 6. The elegance was real and the mechanism was not.
- **Inferring remedy classes entirely.** Rejected: `end_date <= now` is `temporal` when the date is stored and `self_serviceable` when it is supplied, so shape alone cannot decide. Declaration with inference as a shortcut is honest.

## Consequences

- The language now has a semantics that can be given a grammar and a test suite, which is the next artefact (TODO.md).
- ADR-0005's consequence about derived parameter lists is superseded; its decision that guards evaluate over state plus inputs stands.
- `docs/design/edge-cases.md` must drop the false claim about erasure and guards, which decision 1 makes true.
- Every guard in the case studies needs its remedy class declared when they are re-expressed.
- D16, D17, D19, D20, D21, D22, D23 and D24 are resolved.
