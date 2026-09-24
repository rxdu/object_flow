# ADR-0104: The engine is the governed core; managing the flow and using its data belong to upper-layer applications

- **Status:** Accepted — decided by the author, 2026-09-24: "stay the governed core, write it into the PRD, the way we manage the transitions and make use of the datapoints should be built in upper-layer applications"
- **Date:** 2026-09-24
- **Refined by:** ADR-0105 — the core has no delivery worker; posting events is an upper-layer relay.
- **Relates to:** ADR-0007 (the store decides and records; consumers compute), ADR-0012 (the store never writes on read), ADR-0081 (the widened objective), ADR-0102 and ADR-0103 (the unit's journey and its review)

## Context

An operations review of the worked example ([`design/flow-review.md`](../design/flow-review.md)) found that the largest gap between the design and a smooth operation lies around the lifecycle, not in it. Nothing owns the queues. Nothing asks what is due or overdue, chases it, assigns it or tells anyone. Its first recommendation was to design a scheduler and operations agent next.

Built inside the engine, that layer would make ObjectFlow a platform: flows, scheduled jobs, notifications, worklists and screens. That is the category of full metadata-driven stacks. [ObjectStack](https://objectstack.ai/docs/getting-started), for instance, documents record-triggered and scheduled flows, before and after hooks, approvals, scheduled jobs, webhooks and a UI renderer, all derived from one metadata definition ([its automation documentation](https://objectstack.ai/docs/automation)).

The PRD already excluded most of this piece by piece:
- a user interface;
- external effects;
- initiating work;
- choosing among alternatives;
- open-ended analysis.

It never said what the engine is instead, or where those things go. The author decided it.

## Decision

### 1. The engine is the governed core

ObjectFlow holds the declared flows and enforces their rules on every request, from people and agents alike. It records everything the flows produce, together with what users record, and computes the formulas and metrics declared over that data. It answers, by query:
- what may be done now;
- what is due or overdue;
- what is waiting, and on whom.

It contains:
- no scheduler, timer or workflow runner;
- no notifier;
- no worklist;
- no user interface;
- no reaction to its own events.

That is DESIGN.md §2 and ADR-0012, now stated as the product's position rather than as an implementation property.

### 2. Managing the flow and using its data are upper-layer applications

**Managing the flow** covers:
- deciding who requests which transition, and when;
- chasing and escalating;
- scheduling, assigning, notifying;
- presenting worklists, boards and screens.

**Using the datapoints** covers:
- dashboards and alerts;
- analysis beyond the declared metrics;
- decisions taken outside the declared rules.

Both belong to applications built on the engine: the first consumer's own services, agents and screens, or an operations application written for it. The flow review's operating layer is one of them.

The declarations themselves — a lifecycle such as the unit's journey, its rules, its datapoint kinds and metrics — are written by the deployment and executed by the core. Declaring them is not an upper-layer concern.

### 3. An upper-layer application has no privileged path

Such an application acts through the same requests as any caller, under the same rules, and is recorded as an actor of kind `service` or `agent`, with its reasons. It holds:
- no write path of its own;
- no system actor that skips guards;
- no way to change governed state that a person could not use.

Stopping it stops the chasing, and loses nothing from the record. This is T1 applied to automation: an application that bypassed the guards would be the second write path that `DESIGN.md` §13 says voids the guarantee.

### 4. The core supplies what an upper layer needs to ask

The questions an operations application asks must each be one query, not a scan it computes itself:
- **"what may be done now"** — `availability` and `available(transition)`;
- **"what is due, overdue or ageing"** — flags, indexed ageing filters and `query` on stored operands (M7);
- **"who holds what"** — assignment and its metrics (M6);
- **"what just happened"** — subscriptions.

A need an upper layer meets by scanning, because no query answers it, is a gap in the core, and is recorded as one.

### 5. Beside platforms, not one of them

ObjectFlow does not compete on automation, scheduling or screens. What it offers that a platform's automation does not promise is:
- the guarantee that governed state changes only as PRD F2 allows and never reaches a condition an enforced rule forbids (T1);
- the record that makes every decision the engine's rules make explainable, and every flow measurable.

*(Corrected 2026-09-24 by PRD revision 5. This section first said ObjectFlow does not compete on "flows", which are its product, and stated the guarantee as "only through a declared transition or a recorded override", which omits a flow change's mapping and erasure, and the explanation of "every decision", which the PRD limits to the engine's own since revision 2.)*

An application layer, including a metadata platform, may be built on it.

## Alternatives rejected

- **Grow the engine into a platform: scheduled flows, triggers, notifications, screens.** This is the category of stacks such as ObjectStack, entered a long way behind. It contradicts the PRD's non-goals and N2. It also puts code that initiates writes inside the component whose one promise is that nothing writes except a governed request.
- **Build the operating layer into the core as a privileged service.** An internal scheduler with a system actor would be the unrecorded second write path the design has refused since ADR-0012. Its automation would also be judged by rules no one else is held to.
- **Leave the position implicit in the non-goals.** A list of exclusions says what the engine is not, not what it is or where the excluded work goes. This review showed how quickly a sound recommendation drifts across the line when the line is only implied.

## Consequences

- **The PRD, revision 4:**
  - a positioning paragraph in §1;
  - a non-goal and an actor for upper-layer applications;
  - requirement N6;
  - use case UC-20;
  - a note on platforms beside the engine.
  The proposal awaiting the author becomes revision 5. *(Since renamed the proposed revision, PRD §12, when a coherence review took the number 5 the same day.)*
- **`DESIGN.md`:** §1 and §12 name the upper layer.
- **`design/flow-review.md`:** §2.1 and §7 place the operating layer in an upper-layer application. Its first question to the author is answered by this decision.
- **`traceability.md`:** gains rows for N6 and UC-20.
- **`TODO.md`:** records the operations application as work outside this repository's core.
