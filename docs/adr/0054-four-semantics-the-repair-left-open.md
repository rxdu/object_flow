# ADR-0054: Four semantics the repair left open — parent ordering, partial `check`, built-in assertion, and discharged admissions

- **Status:** Accepted — repair of D43 to D46, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0038, ADR-0040, ADR-0027, ADR-0049

## Context

A coherence review of the repaired record found four questions the repair had answered in one document and left open, or answered twice differently, in another. Each is a semantic decision rather than a wording slip.

## Decisions

### 1. The parent's outcome applies before any cascade (D43)

ADR-0038 sequenced the cascades against each other and never said where the parent's own state change and attribute writes fall. DESIGN.md §6 implies step 5 before step 6, but no ADR states it.

**The parent's outcome is applied first, in full, before the first cascade's guards are evaluated.** A cascaded transition therefore observes the parent in its new state. This is what makes a delivery's completion cascade able to read `DELIVERED` on the delivery it came from, and it is the only ordering under which a cascade's guards see a coherent world.

**ADR-0020's rejected alternative needs a better reason, and has one.** It rejected guarding `unit.sell` on `binding.delivery.state == DELIVERED` because "the Delivery is still `PREPARATION` until commit". Under this decision that is no longer true, so the mechanical objection is gone. The decision stands on the argument that survives: only-via states *who may cause* a transition, directly and inspectably, whereas a state comparison encodes the same authority indirectly and silently permits any other route that can produce that state.

### 2. `check` returns a partial verdict and says so (D44)

ADR-0038 said `check` simulates the same sequence "so that the verdict it returns is the verdict a real request would receive". ADR-0049 said `check` calls no external evaluator. For any transition with an external guard both cannot hold.

**`check` evaluates everything internal, in the same sequence a real request would, and reports the external guards it did not evaluate.** Its result is explicitly a partial verdict. A caller that needs certainty makes the request; `check` answers "would this be refused for a reason I can fix", which is what it is for.

ADR-0037's claim that `check` is cheap is also withdrawn: simulating a cascade with creations and read-modify-write accumulation is proportional to the cascade, not constant. It remains lock-free and side-effect-free, which is the property callers actually rely on.

### 3. Import and migration use a built-in assertion, not a declared one (D45)

ADR-0040 rule 1 says a type with no declared asserting transition cannot be overridden at all. Rule 7 says import and migration are bulk assertion. Together they make a type unimportable and unmigratable unless its author happened to declare an asserting transition, which no document requires and which would be discovered at cutover.

**Two distinct paths.** A **declared** asserting transition is for ordinary administrative repair, is narrow by declaration, and is what rule 1 governs. **Import and declaration migration use a built-in assertion** available only to those paths, gated on a deployment-level capability rather than a per-type declaration, and recorded identically with source `asserted` or `migrated`. A type author cannot make their type unimportable by omission, and cannot widen the repair path by accident either.

### 4. An admitted invariant violation is tolerated until discharged (D46)

ADR-0027 admits a violation at publish "with a recorded flag"; ADR-0040 admits one through `may_admit` on an assertion. Neither says what the next ordinary transition does, and ADR-0045 checks every invariant the written objects could violate after every request, so the next transition touching that object would refuse.

**An admission names an object and an invariant, and suppresses that invariant's check for that object.** It is **discharged automatically** the first time the invariant holds again, and the discharge is recorded. Until then the object is reported by the read surface as carrying an admitted violation, which is ADR-0040's auditability requirement. Any *other* invariant, and the same invariant on any other object, is enforced normally.

Without this an admission is a trap: it lets data in and then freezes the object, and the freeze surfaces as an unrelated guard failure much later.

## Alternatives rejected

- **Apply the parent's outcome last.** Rejected: cascades would read a stale parent, and the delivery-completion case would have to duplicate the parent's new state as an input.
- **Let `check` call evaluators.** Rejected: it turns a cheap advisory read into a third-party round trip, and ADR-0048 excludes evaluators from sweeps for the same reason.
- **Require every type to declare an asserting transition.** Rejected: it makes every type carry a repair backdoor whether or not its author wants one, which is the opposite of ADR-0040's narrowing intent.
- **Refuse any transition on an object with an admitted violation.** Rejected: it makes admission useless, since the object is admitted precisely so that work can continue.

## Consequences

- ADR-0038 gains the parent-ordering rule and loses the overreaching claim about `check`.
- ADR-0040 gains the distinction between declared and built-in assertion, and ADR-0027's publish admission is the built-in path.
- The read surface reports admitted violations and their discharge, and the discharge is an event.
- D43 to D46 are resolved.
