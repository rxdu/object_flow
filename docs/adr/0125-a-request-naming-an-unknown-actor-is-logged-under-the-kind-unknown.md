# ADR-0125: A request naming an unknown actor is logged under the kind unknown

- **Status:** Accepted by the author, 2026-09-28: "accept, keep the identity". Decided the same day at the author's direction: "write the scenarios for UC-22 to UC-24", under the standing direction to "continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed".
- **Date:** 2026-09-28
- **Refines:** ADR-0110 (declared actor kinds), ADR-0114 (an unknown actor is refused as `actor_known`), ADR-0106 (the kind `unknown`)
- **Relates to:** PRD T3, T6, UC-22; `DESIGN.md` §5.8, §5.12; `storage-schema.md` §6; `metric-scenarios.md` §41, §44

## Context

A request whose actor, or principal, names no object of a type that marks an actor identity is refused by the generated guard `actor_known`, remedy `dependent` (`DESIGN.md` §5.8, §6 step 1). The attempt log holds every request that did not apply, and each row records an actor and a kind of actor, neither of which may be absent (`storage-schema.md` §6, `of_attempt`). PRD T3 asks that refusal records be attributed, and that their counts be kept by kind of actor.

A request refused as `actor_known` has no resolvable actor, so it has no kind. Nothing said whether it is logged, and if it is, under what kind. UC-22's gateway sends a request from `helper`, an agent it set up and nobody issued in the store, and the refusal counts of its scenario depend on the answer. Found writing UC-22's scenario (D450).

## Decision

**A request refused as `actor_known` is logged like any refusal.** Its attempt records the actor's identity as the request named it, and the principal likewise. Its kind is the actor's where the actor is known and only the principal is not, and otherwise `unknown`, the kind a legacy row that names no actor already has (`DESIGN.md` §5.12, ADR-0106). It is counted by the standard `refusals` under that kind, so a caller naming actors the store does not know shows in the counts as `unknown`.

## Alternatives rejected

- **Not logging it.** The attempt log would no longer hold every request that did not apply. A caller that keeps naming actors the store does not know, as UC-22's gateway does, would leave no record for the upper layer to find it by, although the refusal is the evidence of the fault.
- **Allowing an absent kind.** Every reader of `actor_kind` would need a case for it, and `unknown` already means that the record names no actor it can resolve.
- **Logging it under the principal's kind.** The principal may be unknown too, and where it is known the refusal would be counted as that person's when an unresolvable caller made the request.
- **Recording only that the actor was unknown, not the identity named.** The identity is whatever the caller sent, unverified, and the attempt log holds no personal value; a caller that sends, say, a person's email from another system as an actor identity puts that string in the log. Rejected by the author when accepting ("keep the identity"): the named identity is what lets the upper layer find the caller at fault.

## Consequences

- `DESIGN.md` §5.8 and §5.12 and `storage-schema.md` §6 say so.
- `metric-scenarios.md` §41 counts `helper`'s refusal under `unknown`, and `scripts/check-scenarios.py` records it so.
