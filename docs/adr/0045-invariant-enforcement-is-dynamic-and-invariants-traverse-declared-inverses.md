# ADR-0045: Invariant enforcement is dynamic; the static analysis is a publish-time report; invariants may only traverse declared inverses

- **Status:** Accepted — repair of D10, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0009

## Context

Defect D10 found two different specifications of the same mechanism. ADR-0009 is static: the runtime determines which transitions could violate an invariant and enforces there. DESIGN.md §6 is dynamic: check every invariant the written objects could violate. They imply different algorithms, different cost and different coverage, and neither was marked normative. ADR-0009 also conceded that its analysis is "a real piece of machinery" without specifying it.

There is a further problem that neither statement addresses. An invariant declared on type A may reference type B. When a transition writes a B, the runtime must find the affected instances of A. If the invariant reached B through an expression the model cannot invert, the affected set is "every A", which is a full type scan per write.

## Decision

1. **Dynamic enforcement is normative.** After a request's outcomes are applied, the runtime checks every invariant that the objects written in that request could violate. DESIGN.md §6 is correct; ADR-0009's "enforces there" is a description of the effect, not of the mechanism.
2. **The static analysis is demoted to a publish-time report.** Publishing a declaration reports, per invariant, which transitions could violate it and which shapes the backend can compile (ADR-0041). It serves legibility and index planning. It is not on the request path, so it may be conservative without cost.
3. **An invariant may traverse only relationships with declared inverses.** This makes the affected set computable by reverse traversal from any written object, in bounded work rather than a type scan. A declaration whose invariant traverses a one-way path is rejected at publish, naming the relationship that needs an inverse.
4. **Enforcement is correct regardless**, because transitions run at serialisable isolation (ADR-0039). Compilation to a database constraint remains an optimisation.

## Alternatives rejected

### Static enforcement, as ADR-0009 read

Rejected: almost every observed invariant mentions `state`, and every transition changes state, so the analysis collapses to "check on every transition of every type mentioned". That is the dynamic rule with an extra step, and it is wrong whenever an outcome writes an attribute the analysis did not predict.

### Allow invariants over arbitrary paths, and scan when the reverse is unavailable

Rejected: it makes the cost of a write depend on a property of an unrelated declaration, invisibly. Requiring an inverse makes the cost visible at publish and is a one-line fix for the type author.

## Consequences

- ADR-0009's decision stands as written for *declaration*: an invariant is stated once against the type. Its consequence about runtime analysis is superseded.
- Type authors must declare inverses on any relationship an invariant traverses. This is a real constraint and it is checked at publish, not discovered at runtime.
- The publish report becomes the place where a type author learns what their invariants cost.
- D10 is resolved.
