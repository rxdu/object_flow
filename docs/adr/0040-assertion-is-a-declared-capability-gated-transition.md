# ADR-0040: The override is an assertion: a declared, capability-gated, recorded transition that may bypass guards but not silently

- **Status:** Accepted — repair of D03, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0075 — a mirror is written only by the built-in assertion the import path uses.

## Context

DESIGN.md stated the project's guarantee absolutely: no state change bypassed the guards. Three documents contradicted it. Imported state is asserted rather than transitioned, and ADR-0015 says plainly that none of it passed through a guard. ADR-0027 lets a person admit an invariant violation at publish with a recorded flag. ADR-0024 and ADR-0001 both name an administrative override for repair.

The override is therefore load-bearing and was never declared. It appeared in no model block, had no guards, no stated authority, and no limit on what it could assert. ADR-0024 called it "itself a transition with guards", which cannot be right, since it must assert states the type's guards forbid. It also appeared to collide with ADR-0016's rule that transitions are addressed by name and never by target state, because an override asserts a state.

An escape hatch that exists only as a phrase is worse than one that is designed. This is the single most security-relevant path in the system, and the threat model of mistakes rather than malice does not excuse leaving it undefined: a fallible operator with an undefined tool is exactly the hazard.

## Decision

An **assertion** is a transition declared on a type and marked `asserting`. It is the only path by which state may be set without satisfying the type's guards, and it is not exempt from being declared, gated or recorded.

1. **Declared, and narrow by declaration.** An asserting transition names the states it may assert, as a declared set rather than "any" unless the author writes "any". A type with no asserting transition cannot be overridden at all.
2. **Addressed by name; the state is an input.** The request names the assertion, and the target state is an input constrained to the declared set. ADR-0016's rule survives intact: no request is addressed by target state.
3. **Its own guards apply.** An assertion carries guards like any transition, and at minimum a required capability and a mandatory free-text `reason` input. What it bypasses is the *other* transitions' guards, not its own.
4. **Invariants are not bypassed by default.** An assertion that would violate an invariant is refused. An assertion declared `may_admit` accepts an explicit list of the invariants the caller is knowingly violating; anything not listed still refuses. Each admitted violation is recorded on the object and remains queryable until it no longer holds.
5. **Recorded with what it skipped.** The event carries source `asserted`, the actor, the principal, the reason, the guards that would have failed, and any admitted invariant violations. History says not only that state was set but what was stepped over.
6. **Auditable in aggregate.** The read surface answers, per type, which objects hold asserted state and which carry admitted violations. An escape hatch nobody can count is an escape hatch nobody controls.
7. **Import and migration use a built-in assertion**, not a declared one, so rule 1 cannot make a type unimportable by omission (ADR-0054). It is gated on a deployment-level capability, and records source `asserted` for import (ADR-0015) or `migrated` for a declaration migration (ADR-0027), per object.

**The guarantee is restated.** No state change bypasses the guards except through a declared, capability-gated, recorded assertion; every assertion names its reason and what it stepped over, and the set of objects holding asserted state is queryable at any time. That is weaker than the original sentence and it is true, which the original was not.

## Alternatives rejected

### Leave the override undeclared

The prior state. Rejected: it is the one path that can produce any state at all, and it was the only part of the model with no declaration, no gate and no audit.

### Forbid assertion entirely

Rejected: import cannot work without it, ADR-0015 depends on it, and a system that cannot repair its own data invites the out-of-band database edit that DESIGN.md's Known Limits already identifies as how history becomes fiction.

### Make assertion an operational tool outside the model

A command-line utility with database access. Rejected for the same reason: it would be the unmediated write path the project exists to close, and nothing would record it.

### Let assertion bypass invariants freely

Rejected: guards are per-transition preconditions and can be wrong about a particular object, which is what repair is for. An invariant is a property of the world the type claims always holds, and silently breaking one makes every guard that relies on it unsound. Explicit admission keeps the escape available and visible.

## Consequences

- The model block gains `asserting` as a transition marking, and DESIGN.md §13's guarantee is rewritten.
- ADR-0024's "the recorded override of ADR-0001, which is itself a transition with guards" is corrected: it is a transition with *its own* guards, which are not the type's.
- Admitted invariant violations are a new queryable state, and a type carrying many is a signal that its declaration disagrees with its data.
- An asserting transition is the highest-value target in any deployment. Its capability should be held by few actors, and the aggregate query of rule 6 is the control that makes that reviewable.
- D03 is resolved.
