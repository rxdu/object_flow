# TODO

Status: design only. See [`docs/DESIGN.md`](docs/DESIGN.md) for the model and [`docs/adr/`](docs/adr/) for decisions taken.

## Open model questions

- [ ] **Event ordering scope.** Decide whether the log guarantees global ordering across all objects or only per-object ordering. Global order means a serialized log and a throughput ceiling; per-object order is cheap and is usually what consumers actually need. "Events arrive in order" means very different things and should not be left implicit.
- [ ] **Retention versus slow consumers.** No-loss (ADR-0013) means an event a registered subscriber has not consumed cannot be pruned, so one dead subscriber pins the log indefinitely. Decide maximum retention, what happens when a subscriber falls too far behind, and whether that is an alert, an eviction, or backpressure.
- [ ] **Subscription granularity.** By object type, by specific transition, or by a filter over attributes. The first is trivial; the last requires the query engine and makes "which subscribers care about this event?" a per-event query.
- [ ] **Are subscriptions a built-in object type?** A subscription has durable state and a lifecycle (`active`, `lagging`, `dead-lettered`), which by this model makes it an object. Probably built-in rather than user-definable, so the recursion terminates — but decide it rather than inherit it.
- [ ] **Inspectable but unexecuted timing metadata.** ADR-0012 pushes timing rules ("escalate after four hours") into the automation layer, where they can drift from the object type they concern. Decide whether ObjectKeeper carries such rules as queryable declarations it never acts on — preserving one source of truth without a scheduler — or whether timing lives entirely upstairs. Raised in discussion and not answered.
- [ ] **State: stored or folded.** Given the event log of ADR-0013, decide whether current state is a stored column or a fold over events, and where provenance (actor, principal, source, `observed`/`inferred`/`asserted`, confidence) attaches.
- [ ] **Read and query surface.** Undesigned, and now load-bearing rather than merely useful: ADR-0012 moves staleness detection upstairs, so the upper layer needs queries such as "which cases are in `awaiting_customer` and have not moved in four hours". Also covers what a query returns to an agent without exhausting its context, whether the read path can answer "which objects are blocked", and whether the query language is introspectable in the way guards are.
- [ ] **Actors and authority.** Guards already reference actors and roles. Decide what an actor is, whether an agent carries its own identity plus a principal it acts for, whether delegation must attenuate authority, and whether a denied transition should produce a pending proposal rather than an error.
- [ ] **Schema evolution.** Decide what happens to objects already mid-lifecycle when a type gains a guard or an attribute: versioned types, eager migration, or forward-only requirements.
- [ ] **Global queries versus maintained projections.** Guards of the form "no other active Deployment on this Site" require scanning a type rather than traversing from an object. Decide whether that gets a real query path or is denormalised into maintained attributes — noting that a query is slow but truthful while a projection is fast and can silently lie.

## Resolved since last revision

- [x] **Triggers** — ObjectKeeper initiates nothing ([ADR-0012](docs/adr/0012-objectkeeper-does-not-initiate-transitions.md)).
- [x] **Trigger sources** — none; there is no scheduler and no reactive rule ([ADR-0012](docs/adr/0012-objectkeeper-does-not-initiate-transitions.md)).
- [x] **Library or service** — the core is library-shaped; only push delivery requires a background worker ([ADR-0012](docs/adr/0012-objectkeeper-does-not-initiate-transitions.md), [ADR-0013](docs/adr/0013-events-are-recorded-to-a-durable-log-in-the-transition-transaction.md)).

## Confirmations outstanding

- [ ] Confirm or reject ADR-0006 (controlled vs free attributes) — recorded as a working assumption only.
- [ ] Confirm or reject ADR-0008 (named external evaluator as the guard escape hatch).

## Validation

- [ ] Choose a second case study that differs structurally from the first — specifically in **volume and object lifetime**, not just domain vocabulary. The first domain has few, long-lived, richly related objects, which pulls the design toward deep composition and expensive cross-object guards. A high-volume, short-lived domain stresses query, indexing, throughput and retention instead. Designing against one alone produces an abstraction shaped like that one while claiming generality.
- [ ] Define the acceptance test as adversarial rather than happy-path: point an agent at the system and attempt to reach an invalid state. The weaker question — can it complete the workflow — does not test the objective.
- [ ] Establish that an object type's full rule set can be printed as a readable artifact for review by someone who knows the business process. Residual risk is specification gaps, which are only mitigated by legibility.

## Domain questions to answer from real workflows

These determine model scope and are currently unanswered; the examples used in the ADRs are a strawman.

- [ ] What proportion of needed guards are local to one object versus queries over related ones? This decides how large the guard language must be.
- [ ] Do type-level invariants occur in practice, or does everything worth enforcing attach naturally to a specific transition? If the latter, drop ADR-0009 rather than carry it.
- [ ] Does any axis exist that is genuinely 1:1 with an object for its whole life *and* has gated transitions of its own? That is the only evidence that would reopen ADR-0003.
- [ ] Are there transitions where authority differs from capability — outcomes an agent could correctly determine but should not conclude alone? A short list makes proposals an edge case; a long one makes them the main path.
