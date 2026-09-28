# ADR-0129: A publish notices a metric whose filter it reroutes

- **Status:** Accepted by the author in advance, 2026-09-28: "for all similar items that you have confidence in an recommendation, just accept", said while the author was taken through the items that need their attention.
- **Date:** 2026-09-28
- **Refines:** ADR-0097 (a flow change's impact report), ADR-0116 (the flow checker's steps)
- **Relates to:** PRD F7, M1; `flow-format.md` §7; `metric-scenarios.md` §29, §36, §39

## Context

The lead of `metric-scenarios.md` §29 declares a cycle time counting completions from `PREPARATION` into `DELIVERED`. Version 14 moves the completions to leave `READY`, and from then the metric counts none, so D06's 165 days in preparation never enter it. The metric is still well formed, and its filter still says what it means; what changed is the flow it filters. Publishing checked version 14 and reported only a rule on trial, and the rule-set diff shows the rerouted transition on the delivery and nothing on the metric, whose approver is not told its counts change (`TODO.md`, 2026-09-28).

## Decision

**Publishing notices each declared metric over transitions whose filter compares a transition's `to_state` to a state the version changes the transitions into, or its `from_state` to a state it changes the transitions out of.** The notice names the metric and the state, and says that what the metric counts changes from this version on. A filter on where a transition ends is not noticed when only where it starts changes: the on-time rate, which counts completions into `DELIVERED` from wherever they come, keeps its meaning across version 14. It is a notice and refuses nothing: the metric may be meant to follow the flow. It is the flow checker's `metric` notice, reported beside `migration` (`flow-format.md` §7).

## Alternatives rejected

- **Refusing the publish.** A metric that counts completions into `DELIVERED` from wherever they come is correct across a rerouting, and a refusal would force a change to it.
- **Rewriting the metric's filter.** The engine cannot know what the lead meant, and a declaration changes only by publishing it.
- **Noticing every metric whose filter names a state.** A filter over intervals or objects reads the states themselves, whose meaning a rerouting does not change. The trap is a transition filter, which names a route.
- **Noticing a transition filter whenever a state it names gains or loses any transition.** It would notice the on-time rate across version 14, whose completions still all end in `DELIVERED`, and a notice that fires on every change is one an approver learns to skip.

## Consequences

- `flow-format.md` §7 lists the `metric` notice, `scripts/check-flows.py` reports it, and its self-test plants a rerouting.
- `metric-scenarios.md` §39 reads version 14's notices, which now name the lead's cycle time.
