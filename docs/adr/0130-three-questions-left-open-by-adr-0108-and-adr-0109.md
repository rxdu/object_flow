# ADR-0130: Three questions ADR-0108 and ADR-0109 left open

- **Status:** Accepted by the author in advance, 2026-09-28: "for all similar items that you have confidence in an recommendation, just accept", said while the author was taken through the items that need their attention.
- **Date:** 2026-09-28
- **Refines:** ADR-0108 (a cascade is declared clearly enough not to surprise), ADR-0109 (a creation past its lifecycle's first state)
- **Relates to:** PRD F8, V2, UC-3, UC-21; `DESIGN.md` §10; `library-api.md` §6; `metric-scenarios.md` §38, §39

## Context

Three questions stood in `TODO.md`'s list for the author:
1. **Whether `check` returns what a request would reach**, the objects and transitions of its cascades, and not only its verdict. ADR-0108 left it open as an additive change to `Checked`, and weighed it against PRD T5, since a cascade reaches objects its requester might not see.
2. **How a cascade through a single reference is conditioned.** `where` filters only a loop over a collection, so a unit filling a delivery's last slot cannot cascade the delivery into `READY` only when its slot is the last (ADR-0108, Consequences).
3. **Whether diagnostics count creations by the state they land in, beside overrides** (ADR-0109, "Left open"; PRD V2's "states reached mostly by override").

Since they were left open, ADR-0114 made the engine filter no read by who asks, which removes the first question's T5 concern. UC-21's checked scenario met its acceptance with `check`'s verdict alone (`metric-scenarios.md` §38). On 2026-09-28 the author closed the operations review's decisions, `READY` among them, as the operations platform's own, so the second question's motivating case belongs to a flow that declares a transition for it.

## Decision

1. **`check` returns its verdict, as `library-api.md` §6 has it, and nothing more for now.** It names the unit and the rule a cascade would fail on, which is what UC-21 asks. Returning what a request would reach is additive and waits for a use case that needs it.
2. **A cascade through a single reference takes no condition.** A flow that wants one event when its last part arrives declares a transition an application or a person requests, as the operations review's `mark_ready` does, and a subscription tells the application when to ask (`metric-scenarios.md` §21).
3. **Diagnostics report, for each state, how objects entered it: by a transition, by an override, or by a creation that landed there.** A state reached mostly by creation, such as a unit added as opening stock, then reads as a declared route. It is not counted as an override, and it is not missing from the picture of how the flow is used. Overrides stay counted apart, which UC-3 reads.

## Alternatives rejected

- **Returning a request's reach from `check` now.** No use case asks for it, and the shape of `Checked` would be fixed before one does.
- **A `where` on a cascade through a single reference.** It would let a part's transition move its whole conditionally. That is branching inside an outcome, which ADR-0019 keeps out of outcomes and in guards.
- **Counting a creation past the first state as an override.** ADR-0109 rejected it, since routine opening stock would drown the signal UC-3 reads.
- **Leaving creations out of diagnostics.** A state reached only by creation would look unused beside its transitions, which is the wrong answer to V2's question.

## Consequences

- `DESIGN.md` §10's `diagnostics` reports entries by route.
- ADR-0108's and ADR-0109's open points name this decision, and `TODO.md`'s list for the author loses the three questions.
