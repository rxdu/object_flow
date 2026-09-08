# ADR-0053: `is null` and `is not null` are definite predicates; comparison with null stays unknown

- **Status:** Accepted — repair of D42, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Amended by:** ADR-0056 §8 — the conditional is `if … then … else` and implication is `implies`. ADR-0073 adds `clear`, which is how absence is written now that a bare `null` is not assignable.
- **Refines:** ADR-0047

## Context

ADR-0047 made evaluation three-valued: comparison with an absent value yields unknown, unknown propagates, and a guard that evaluates to unknown fails. That was the right decision, and it closed a real hole where erasing a person could satisfy an exclusionary guard.

It also broke every null test in the design. ADR-0021 listed "null test" as a construct with the example `reason != null`, and the case studies use `s.unit != null`, `s.warranty_product != null` and `inputs.fix_version != null`. Under three-valued comparison all of these are unknown whether or not the value is present, so every guard containing one fails permanently. ADR-0047 even names the problem in its rejected alternatives, saying two-valued logic "makes `x != null` a way of asking two different questions", and then leaves no way to ask either.

This is a defect the repair introduced. It is the ordinary consequence of adopting SQL's null semantics without also adopting SQL's `IS NULL`.

## Decision

1. **`is null` and `is not null` are predicates that always return true or false**, never unknown. They test presence, which is a definite fact about an attribute even when its value is not.
2. **Comparison with `null` remains unknown**, and a bare `null` literal in a comparison is a publish error rather than a silent always-unknown expression. A declaration that means to test presence must say so.
3. **The conditional in a derived attribute uses the same predicate**: `unit is null ? UNFILLED : FILLED`.
4. Aggregate predicates use it like any other clause: `none(s in slots where s.role in (PRIMARY, INCLUDED) and s.unit is null)`.

## Alternatives rejected

### Leave comparison two-valued for the null literal only

Special-case `x == null` to mean a presence test while other comparisons stay three-valued. Rejected: it makes one operator mean two things depending on its right operand, which is the ambiguity ADR-0047 rejected two-valued logic to avoid.

### A `present(x)` function

Equivalent in power and worse in reading. It also implies a function-call syntax the language deliberately does not have beyond declared external evaluators.

### Coalescing, so absence takes a default

Rejected: it hides absence rather than testing it, and a guard whose operand silently became a default is exactly the silent satisfaction the three-valued rule exists to prevent.

## Consequences

- Four declaration lines in the case studies and two examples in DESIGN.md are rewritten. They were the only occurrences; a mechanical sweep of every declaration block found no other grammar violation.
- ADR-0021's "null test" row keeps its name and loses its example, which was the broken form.
- The erasure property of ADR-0047 is unaffected and sharpened: an erased value reads as absent, so `is not null` on it is definitely false and any comparison to it is unknown. Both fail a guard, which is the intent.
- D42 is resolved.
