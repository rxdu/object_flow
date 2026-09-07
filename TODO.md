# TODO

Status: design only. See [`docs/DESIGN.md`](docs/DESIGN.md) for the model and [`docs/adr/`](docs/adr/) for decisions taken.

## Open model questions

- [ ] **Triggers.** The originating definition named "triggers and constraints" as what drives the lifecycle. Constraints are resolved; triggers are not. Decide whether a trigger *proposes* a transition (which is then gated normally, making automation just another actor) or performs one directly. Direct performance reintroduces cascades, evaluation-order dependence and the loss of a single explanation for why an object is in its state — the failure mode of Jira post-functions and Salesforce workflow rules.
- [ ] **Trigger sources.** Decide which of event / time / external a trigger may react to. Time is the consequential one: "nothing happened, and that is the problem" requires something running, which decides whether this is an embeddable library or a service with a scheduler and recovery semantics.
- [ ] **Events, history and provenance.** Undesigned. Includes the foundational question of whether state is stored or folded from an event log, and whether provenance (actor, source, `observed`/`inferred`/`asserted`, confidence) is per-event or a separate subsystem.
- [ ] **Read and query surface.** Undesigned, and forced by the "database" framing. Includes what a query returns to an agent without exhausting its context, whether the read path can answer lifecycle questions such as "which objects are blocked", and whether the query language is introspectable in the way guards are.
- [ ] **Actors and authority.** Guards already reference actors and roles. Decide what an actor is, whether an agent carries its own identity plus a principal it acts for, whether delegation must attenuate authority, and whether a denied transition should produce a pending proposal rather than an error.
- [ ] **Schema evolution.** Decide what happens to objects already mid-lifecycle when a type gains a guard or an attribute: versioned types, eager migration, or forward-only requirements.
- [ ] **Global queries versus maintained projections.** Guards of the form "no other active Deployment on this Site" require scanning a type rather than traversing from an object. Decide whether that gets a real query path or is denormalised into maintained attributes — noting that a query is slow but truthful while a projection is fast and can silently lie.

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
