# ADR-0105: The core sends nothing, and every write to governed state names its route

- **Status:** Accepted — decided 2026-09-24 at the author's direction ("go ahead with the five reviewers once the PRD is fixed"), against `docs/PRD.md` revision 5: F1, F2, F4, F5, T1, T5, L2, L3, L6, N2, N6 and UC-17, UC-20; repairs D290 to D298, and, as corrected by an independent verification, D354, D355, D357, D359, D360 and D363 to D365. The author's own acceptance is pending.
- **Date:** 2026-09-24
- **Refines:** ADR-0007, ADR-0013, ADR-0022, ADR-0027, ADR-0030, ADR-0034, ADR-0036, ADR-0040, ADR-0043, ADR-0054, ADR-0060, ADR-0077, ADR-0083, ADR-0087, ADR-0088, ADR-0097, ADR-0098, ADR-0100, ADR-0104

## Context

PRD revision 5 was made to agree with itself first. Five reviewers then read the whole design record against it, each over one slice:
- flows and trust;
- data, formulas and metrics;
- logic, convergence and the non-functional requirements;
- every use case end to end;
- the record's coherence as one text.

They reported ninety-one findings. After deduplication, and after each was re-read at its cited lines, sixty-four stand ([`design/defects.md`](../design/defects.md), D290 to D353). Two of the slice-4 findings depended on production's behaviour, and both were confirmed in `~/RduWs/wr_inventory_management` at `4109939`.

This decision's first draft was then verified independently, against the PRD, ADR-0104 and the whole working tree. The verification found that its refusal record could not replay an invariant's refusal (D354), that it counted overrides per free-text reason again (D355), that an `admit` could not yield for a compiled invariant (D357), that `not_erased` had no stated scope (D359), and five smaller faults. This version is the repaired one.

Most of the sixty-four are corrections in place. Two groups need decisions:
- **This decision:** where the engine's boundary lies, and how each route of F2 is recorded.
- **ADR-0106:** what the record measures.

The questions this decision settles, each found by at least one reviewer:
1. The design keeps a push delivery worker inside a deployment (`DESIGN.md` §2, §7; ADR-0013, ADR-0043). ADR-0104 and UC-20 say the engine sends nothing and reacts to none of its events. The worker also writes delivery errors through none of the six operations `DESIGN.md` §13 calls the only writes.
2. `Subscription` and `Proposal` are built-in types with no declared authority and, for a proposal, no visibility.
   - Anyone could create a subscription that is delivered as any descriptor.
   - Anyone could acknowledge another's cursor.
   - A proposal showed its target's id and proposed inputs to readers who cannot see the target, which D263 fixed for `DeclarationChange` and never for `Proposal`.
3. An empty store has no declaration version, so the first `DeclarationChange`, which is drafted against the installed version, cannot exist. The built-in module was not versioned with any flow.
4. PRD F2 names four routes into governed state: a transition, an override, a flow change's mapping, and erasure. The design called a migration an assertion, which the PRD calls an override. It then recorded the migration as `migrated`, without a reason, and counted it nowhere.
   - Nothing said whether a mapping is checked against the invariants.
   - "Resolved by a mapping or admitted" had no admission form at publish.
   - `state_source = 'migrated'` could never be written.
5. Erasure redacts values on objects its taint analysis reaches without recording an event on them. A subscriber never sees the change, and a stale `expected_version` still passes.
   - Nothing stopped an externally owned type's sync from writing an erased value back, which UC-17 forbids.
6. Only `Unsatisfied` carried a remedy class. PRD F4 asks every refusal to say what to do next.
7. An attempt row records the failing clause's read set and never an input. A refusal decided on a request value — an amount, a date, the occurred time — cannot be re-evaluated, which L2 and ADR-0088's own rejected alternative require.
8. UC-20 and ADR-0104 record an upper-layer application's actions "with its reason". A request has no reason, only a `context` described as the route it came by.
9. A time-driven transition is requested by a scheduler above the store. While that scheduler is late or stopped, the transition is dated when it finally runs, and time in the earlier state is overstated for good. UC-20 says stopping the application "loses nothing from the record".

## Decision

### 1. The core delivers events by `pull` alone, and posting them is an upper-layer relay

A subscription is read by `pull` and advanced by `acknowledge`: at-least-once, in each object's order, under the reader's own visibility (PRD L6).

The core has no delivery worker, no endpoint and no delivery errors. A deployment that wants events posted to a URL runs a **relay**, an upper-layer application. The relay pulls as its own actor, posts, and acknowledges once the post succeeded, with no privileged path (ADR-0104 §3). Its failures are its own to record and alert on.

`Subscription` keeps its filter, its lag thresholds and its lifecycle. It loses `endpoint` and `deliver_as`, and its runtime position loses `last_error` and `last_error_at`.

A deployment becomes a service by exposing the API over a transport (ADR-0037), and in no other way.

**Order.** `pull` returns events in settled-cursor order, `(transaction, position)`. Per-object order survives that ordering because a later writer of an object commits in a later transaction (`storage-schema.md` §7, ADR-0089). A reader that acknowledges only what it has handled therefore meets L6's "at least once and in order for each object".

### 2. Who may subscribe, read a subscription, and end a proposal

- **`OF_SUBSCRIBE`**, a seventh built-in capability, gates creating a subscription.
  - A subscription names its **reader**, the creator by default. Only that actor may `pull` and `acknowledge` it, so no one can move another reader's cursor past events it has not handled.
  - The reader, the creator, or a holder of `OF_SUBSCRIBE` may revoke it.
  - `pull` applies the caller's visibility, as every read does.
- **A proposal is visible** to its proposer and to whoever can see its target.
  - A proposed creation has no target. It is visible to its proposer and to actors who pass the creation's actor guards, the clauses over `actor` — the people who could approve it.
  - Within a visible proposal, inputs naming objects the reader cannot see are withheld, as a verdict's are (ADR-0100).
- **Ending a proposal.** Its proposer may `withdraw` it. An actor who passes the proposed transition's actor guards may `reject` it: whoever could carry it out may decline it, and no capability is invented for the purpose. Approval stays as ADR-0036 decided: executing the recorded request as the approver, with every guard re-evaluated.

### 3. A store begins at declaration version 0, and the built-ins are versioned like any declaration

**Version 0.** Creating a store installs declaration version 0. It holds:
- the built-in types `Subscription`, `Proposal` and `DeclarationChange`;
- the seven built-in capabilities;
- the `label` kind and the `closed` category;
- the standard metric definitions of the engine release that created the store.

Version 0 has its own `of_declaration` row, written by the store's construction — the one write that precedes every operation — and attributed to the engine release, since no one wrote its rules. The first `DeclarationChange` is drafted against version 0 and published by its lifecycle like any other, so the first flow is installed by the same path as every later one, and F7's approval rules govern it from the release that ships them (PRD §10 puts them in the third, and until then a flow change is a publish without them). `DeclarationError` then means that the installed version will not load, never that none is installed.

**Pinned built-ins.** Every declaration version records the built-in module it was published with, which is part of its closure. An engine release that changes a built-in definition takes effect in a store only through a publish, whose impact report lists what changed. Examples are a standard metric's formula and a built-in type's lifecycle.

So an event's declaration version fixes the standard metrics' definitions as it fixes everything else (PRD §5, Flow: "versioned together and change only by publishing"). It does not fix the meaning of the language they are written in — what `.completes` marks, how a percentile is computed. That is the engine's; a metric is computed on read under the engine's current meaning over all history, and a decision that changes it says so, as ADR-0106 does. *(Narrowed by the verification, D364: the first draft said the version fixed the standard metrics wholly.)*

### 4. A flow change's mapping is its own route, checked against the invariants; an admission at publish is an override

**A migration is F2's third route, not an assertion.** It is recorded:
- with source `migrated`;
- caused by the publish event, whose approval under F7 is its authority;
- with the publishing actor as its actor.

It leaves `state_source` as it was, which is why `migrated` is no longer one of that column's values. `DESIGN.md` §1, §3, §8 and §13, and the harness's claim, state the guarantee in F2's four routes.

**Mappings are checked.** The dry run applies every mapping in simulation — a removed state, a removed member, a `backfill` — and evaluates every invariant over the objects the mappings write, as step 7 of a request does (`DESIGN.md` §6).
- A violation is a DECISION finding naming the objects.
- The publish re-runs the same check in its transaction and refuses as `impact_unchanged` on any violation the dry run did not report, so what a dry run reports is what a publish does.

**Resolving a violation.** A violation the dry run reports, from a new invariant or from a mapping, is resolved in one of three ways:
- the objects' own transitions, before the publish;
- a different mapping;
- **`admit <Type>.<invariant> because <Enum>.<MEMBER>`**, a mapping form carried by the publish, whose reason is a member of a declared enum, as an assertion's is, so overrides stay countable per reason. *(Corrected by the verification, D355: the first draft took a free-text reason, which ADR-0106 refuses to count by.)*

An `admit`:
- is stored in `of_migration`;
- records an admission, with its reason, on each violating object's migration event. An object that no other mapping changes receives a `migrated` event carrying only the admission.

Such an admission is an override in the PRD's sense: its Override concept names "the data import and migration path under a deployment capability", and here the capability is `OF_APPROVE_CHANGE`. So it is listed by `exceptions(type)` and counted. An `admit` naming an invariant compiled to a database constraint drops that constraint before the migration writes, since a constraint cannot yield for one row (ADR-0074), and the invariant stays uncompiled while any admission of it stands; the report says so. *(Added by the verification, D357.)*

**What counts as an override.** The row member `.overrides` is true on two kinds of event:
- an assertion's event;
- any event that records an admission — an assertion's, an erasure's, or a publish's `admit`.

`override_counts` counts the overrides that are not the import's, per target state and reason (PRD M1). An erasure's reason is free text that may name the person, so an erasure's events carry the fixed reason `erasure`.

### 5. Erasure records an event on every object it changes, and an erased value cannot be written back

**An event per changed object.** An erasure reaches other objects through check 10's taint analysis. Every object whose personal attribute it redacts receives an event:
- with source `erased`;
- caused by the erasure's own event;
- naming the attributes redacted and never their values.

Its version advances, and the admission for any invariant the redaction breaks is recorded on that event. So a subscriber sees the change, and a request carrying the object's old `expected_version` is refused as `stale`.

An admission recorded this way is an override authorised by the erasing type's `erase`. That is why `DESIGN.md` §8 no longer says an erasure's admission exists only where the admitting object's own type declares `erase`.

An event recorded this way is marked `.redacted`. It is a row of its type's transitions, and the standard metrics leave it out as they leave out imports and migrations, except the override counts where it carries an admission (ADR-0106 §7).

**No writing back.** Once an object's own erasure is recorded — through its type's `erase`, or as a member of an erased supersession chain — a request whose outcome writes one of that object's personal attributes is refused by a generated guard, `not_erased`, remedy `unreachable_from_here`. An object the taint analysis reached was not itself erased, and may be given a new value like any other. *(Scope stated by the verification, D359.)* The import is already barred the same way (ADR-0101). This covers an externally owned type's sync (ADR-0080), which would otherwise restore a value the other system has not yet erased or keeps for its own obligations.

A person who returns is a new object.

### 6. Every refusal names a remedy class

Each verdict carries one, so a caller always knows its next move (PRD F4):

| Verdict | Remedy class | Why |
|---|---|---|
| `unsatisfied` | declared or inferred, as now | |
| `stale`, the version moved | `self_serviceable` | re-read and resend |
| `stale`, retries exhausted | `temporal` | the contention passes |
| `not found` | `unreachable_from_here` | nothing this caller can do reaches it, and saying more would disclose existence |
| `not requestable` | `unreachable_from_here` | the verdict names the parents to request instead |
| `over-limit` | `unreachable_from_here` | the request must be split |
| invariant violated, naming other objects | `dependent` | work on them first |
| invariant violated, over this object alone | `self_serviceable` | change the request |

The two faults recorded in the attempt log are also classed, since both are a caller's mistake with an obvious next move:
- an unknown transition, `unreachable_from_here`;
- a reused key, `self_serviceable`.

`of_attempt.remedy` is therefore always set.

### 7. A refusal's record keeps the request's values and everything it read, so it replays

An attempt row records:
- every **non-personal input** of the request, and its `occurred_at`;
- the read set of everything the request had read when it was refused, including the objects its outcome and cascades reached.

An input check 10 classes as personal — marked, or flowing into a personal attribute — is recorded as withheld, by name only.

A refusal is re-evaluated by replaying the request over those versions and values. An invariant, or a cascaded guard, refuses over state the request produced and rolled back, which only a replay reconstructs. So a refusal re-evaluates from the record exactly as an applied request does (PRD L2, G5). Where it cannot, because a personal value decided it, the record says so. *(Corrected by the verification, D354: the first draft recorded only the inputs the failing clause read, which an invariant's or a cascade's refusal never reads directly.)*

Erasure still needs nothing from the attempt log, since the log still holds no personal value. The harness gains the matching failure row.

### 8. An application's reason is its request's `context`

`context` is the caller's statement of why a request was sent and by which route. For an upper-layer application, that is the rule or job that caused it: `ops-app/sweep:leases-overdue`. It is recorded on the event and on an attempt row, and it is what UC-20's "recorded with its reason" reads.

A transition whose reason is governed data declares a `reason` input, as an assertion must.

### 9. A time-driven transition may record when it fell due

A transition a scheduler requests on a deadline may be marked `backdatable within` the longest delay the deployment tolerates. A warranty's `expire` is one. The scheduler supplies the deadline it read as `occurred_at`, for instance `end_date`.

A late or stopped sweep then dates the change when it fell due, and time in the earlier state is not overstated.

Beyond the bound, the request is refused by `occurred_within`, and the scheduler sends it undated. The transition then records its own time, and the lateness stays readable from the stored deadline beside it.

So stopping an upper-layer application loses nothing from the record: within the bound the change is dated when it fell due, and beyond it the late change records its own time beside the stored deadline, and only time in the earlier state is overstated, which `DESIGN.md` §13 names as a limit of measurement. UC-20's clause is met either way. *(Reworded by the verification, D365: the first draft said the clause held only within the bound, which confused the record with a measure over it.)*

## Alternatives rejected

- **Keep push delivery as L6's transport.** It is not a notifier in intent, but it is the engine sending: a process in the deployment that reacts to the engine's own events, which ADR-0104 excludes by name and UC-20 excludes by acceptance. It needs a write path for its errors outside the six operations. And it delivers as a descriptor that nothing bounds, so a subscription could be delivered what its creator may not see.
- **Bound `deliver_as` to the creator's capabilities, and keep the worker.** This repairs the privilege and leaves the contradiction with N6, UC-20 and ADR-0104 in place.
- **Invent `OF_REVIEW_PROPOSAL` for rejection.** Anyone who may carry out the transition may already decline it by not approving. A capability that differs from the transition's own authority would let someone reject what they could never have approved, or fail to let someone who could.
- **An unapproved first publish for an empty store.** A second way to install rules, outside F7, used exactly when a deployment's rules are least reviewed.
- **Built-ins outside the declaration version.** An engine upgrade would change what `override_counts` or a standard metric means under events already recorded, with no publish to show it. That breaks the PRD's definition of a flow.
- **Keep calling a migration an assertion.** The PRD now names the mapping as its own route. And an assertion carries a reason and is counted, neither of which a mapping that merely moves a removed state's objects needs. The one part of a publish that is an override — admitting a violation — is now named as one.
- **Strike "or admitted", so a publish can never admit.** A new invariant that forty live objects violate would then block the publish until each object is repaired one request at a time, however long the repairs take. Admitting is how the import already handles the same situation, with a reason and a record.
- **Redact other objects silently, as before.** It changes governed state with no event. That is the unrecorded write F2 forbids, and the reason a subscriber could not tell the value had gone.
- **Record every input on an attempt row.** The attempt log would then hold personal data that erasure must reach, which ADR-0083 designed it never to hold.
- **Propose weakening L2 for refusals.** L2 is a Must, and recording non-personal inputs costs a column.
- **A separate `reason` field on every request.** It would be a second free-text field beside `context`. Callers would fill one or the other, and readers would have to check both.
- **A new `occurs at <operand>` marking for time-driven transitions.** It covers only an outage longer than a bound the declaration already chooses, and it adds a construct for what `backdatable within` does.

## Consequences

- **`DESIGN.md`:**
  - §1, §3, §8 and §13 state the four routes;
  - §2 and §7 lose the delivery worker and state per-object order;
  - §5.5 gives every verdict a remedy class;
  - §5.9 and §9 carry version 0, the pinned built-ins, the checked mappings and `admit`;
  - §6 records the failing clause's request values;
  - §7 makes time-driven transitions backdatable;
  - §8 records an event per redacted object and adds `not_erased`;
  - §9 gives the built-in types their authority and visibility;
  - §14 defines the terms PRD §5 added.
- **`storage-schema.md`:**
  - `t_subscription` loses `endpoint` and `deliver_as`, and gains `reader`;
  - the position table loses its error columns;
  - `state_source` loses `migrated`;
  - `of_migration` gains the `admit` kind;
  - `of_attempt` gains `request_values`;
  - version 0's row is described;
  - §9 adds a step for the events on redacted objects.
- **`library-api.md`:**
  - every verdict carries its remedy class;
  - `Attempt` carries its request values;
  - `DeclarationError` means only a failed load;
  - `context` is described as the caller's reason.
- **`declaration-syntax.md`:**
  - seven built-in capabilities;
  - the `admit` mapping form;
  - `.overrides`;
  - `override_counts` redefined;
  - check 23 covers mappings that violate an invariant.
- **`publish-and-import.md`:** step 3 checks mappings, step 5 applies `admit`, and the report names the objects each mapping rewrites and the changes a publish will supersede.
- **`renderers.md`:** prints `Subscription` and `Proposal` with their authority.
- **`adversarial-harness.md`:**
  - the claim uses F2's four routes;
  - a failure row for a refusal that does not re-evaluate from its record;
  - a row for an erased value written back.
- **ADR-0013 and ADR-0043:** refined; push delivery is an upper-layer relay.
- **ADR-0104 §1:** unchanged. This decision is what makes it true of the design.
