# ADR-0110: ObjectFlow is an internal service; upper layers authenticate and map roles, and an actor's kind is declared with its type

- **Status:** Accepted by the author, 2026-09-25 ("yes to all three, go ahead and write them"), after comparing an embedded library with a service. Against `docs/PRD.md` revision 8: N7, T6 and UC-22, which record the author's words, and N2, N4, T1, T2, T5, F1, F3, F7, D12 and C2. Repairs D381 and D382.
- **Date:** 2026-09-25
- **Refines:** ADR-0025 (the actor descriptor), ADR-0037 (the transport), ADR-0080 (an externally owned type), ADR-0086 (the `actor` identity), ADR-0103 (`requests by <kind>`)
- **Relates to:** ADR-0001 (the store owns persistence), ADR-0012 and ADR-0104 (the governed core initiates nothing), ADR-0079 (a role is a capability), ADR-0091 (a synchronous core, driven deterministically by the harness)

## Context

The record allowed two deployments and chose neither. DESIGN §2 said the core is library-shaped, and that "a deployment becomes a service by exposing the API over a transport". `library-api.md` §1 wrote a Python binding "because the first consumer is Python" and called it "a binding, not the design". Asked whether to implement in C++ or Rust with bindings for other languages, the author compared an embedded library with a service, and said:
- "I'm more inclined to deploy the engine as a service rather than a library so that upper layers rely on the same interface to interact with the flow";
- "treat the engine as an internal service and leave the auth & authz for upper layers to decide";
- "leave the authentication to the upper layer completely";
- "define the actor kind along with the objects, flows etc. and we rely on the upper layer to handle the role mapping".

The comparison found two gaps in the record, whichever shape was chosen:
- **D381.** Nothing stopped two engine releases from serving one store. DESIGN §5.9 makes the meaning of the language the engine's: what `.completes` marks, how a percentile is computed. So two releases could give two readers different values from one definition, which C2 forbids. Several embedded hosts make this likely, and a rolling deploy of a service makes it possible.
- **D382.** The record did not say how an engine release is installed over a store that an earlier release created.

With authentication upstream, a third problem appeared. The descriptor carried the actor's kind, so every rule depending on kind trusted each request's word for it:
- `requests by agent require version, key` (ADR-0103);
- an agent may not be engineer-of-record (PRD UC-18);
- the split of every metric by kind of actor (PRD M4).

## Decision

### 1. The engine is deployed as an internal service

There is one service per store, and every caller reaches it through one interface: upper-layer applications, the first consumer's own backend and agent gateways alike.
- **No caller embeds the core or holds the store's database credentials.** So the mediated property of DESIGN §13 is kept by how the system is deployed, not only by convention.
- **Inside, the core stays a library (PRD N2).** The service is a thin transport exposing the API one-to-one (ADR-0037), and the adversarial harness drives the library directly (N4, ADR-0091).
- **The service initiates nothing,** as ADR-0012 and ADR-0104 require. It is not a runtime with schedulers, timers or relays.

### 2. Authentication, and the mapping of roles to capabilities, belong to upper layers

- **The service authenticates nothing.** The descriptor is the upper layer's, and ObjectFlow validates its shape, not its truth (ADR-0025).
- **Capabilities come from the upper layer.** It maps its own roles onto the capabilities the descriptor carries (ADR-0079).
- **The rules stay in the engine.** What a capability permits, and who may see what, are declared and enforced on every request (PRD F1, T1, T5, F7, D12). If upper layers decided permissions, each would restate the who-may rules. The first consumer's most repeated rule is exactly that: 238 of its 454 guard sites are one permission predicate repeated (`TODO.md`, counted 2026-09-09). Any upper layer learns what an actor may do now from `availability` (F3).
- **PRD T2 names network access to the service** beside the store's database credentials, as out of scope.

### 3. An actor's kind is declared with the type that holds it

- **Kind is part of the actor identity marking.** A type that holds actors marks one `identity` attribute `actor human`, `actor agent` or `actor service` (`declaration-syntax.md` §3.1, check 62). A type holds one kind of actor, and a deployment with people and agents declares two types, as the example's `User` and `Agent` do.
- **The descriptor carries no kind.** It names the actor by its identity, with an optional principal and the upper layer's capabilities. The engine resolves the actor, and the principal, to the live object holding that identity, and takes the kind from its type.
- **An actor who is not live is refused.** That covers an unknown identity, a user who has left and a revoked agent. The refusal names the generated clause `actor_live`, remedy `delegable`.
- **An identity names one actor.** It is unique across every type that holds actors and never reused, so attribution in history stays unambiguous.
- **A kind changes only by a publish,** visible in the rule-set diff and approved under F7.
- **A role that a rule reads as data belongs to the upper layer.** An example is the assigned engineer's `role`, which the engineer-of-record rule reads. Where the upper layer's user system owns roles, as the first consumer's does, `User` is externally owned and kept in step by a sync (ADR-0080), so two role lists cannot disagree.

### 4. One engine release serves a store at a time

The store records the engine release it is at (`of_engine_release`), and every request reads it in its transaction. A core of any other release raises `EngineMismatch`, a fault for the deployment rather than a verdict for the caller. So once an upgrade commits, the copies still running the old release refuse instead of computing under different meanings, and a rolling deploy needs no coordination beyond the upgrade itself. **Installing a release is an explicit, recorded step**, like creating the store: it is attributed to the release, and it migrates the store's own tables. What each release migrates is that release's work, and `TODO.md` holds the step's specification.

### 5. What the choice leaves open

- **The transport:** HTTP with an OpenAPI description, or gRPC.
- **The core's language.** With every other language reaching the store through the service, the core needs no bindings, so its language can be chosen for correctness and maintainability alone.
- **Specifying the upgrade step.**

All three are in `TODO.md`.

## Alternatives rejected

- **An embedded library in every host.** Each host would need a binding, every host holds database credentials, and every host must upgrade in lockstep, the likeliest way to two releases on one store. It is also the shape in which "the same interface for every upper layer" holds only by convention.
- **A service by default, with embedding allowed for a sole client.** This was recommended before the author chose. It keeps a second way to reach the store and the per-host upgrade problem for the client that embeds, and gives up the property the author asked for: every upper layer on one interface.
- **An active runtime process** that schedules, relays or notifies. N2, N6 and ADR-0012 rule it out, and ADR-0104 puts that work in upper-layer applications.
- **The engine authenticating callers,** by verifying tokens or knowing which upper-layer service calls. The author placed authentication entirely upstream, and it would bring key management and an identity system into the core.
- **Upper layers deciding permissions.** This would restate the who-may rules in every layer (§2), and F1, T1 and T5 are Musts.
- **The kind carried in the descriptor**, as before. A rule depending on kind would trust each request's claim.
- **The kind fixed by an actor's first request.** It catches a kind that changes, not one that is wrong from the start.
- **A built-in `Actor` type.** It was offered first. The author chose to declare actors with the deployment's own types, whose lifecycles and guards already say who may add a user or issue an agent key.
- **A kind per actor object.** A per-actor value is what could be set wrongly. A kind per type changes only by a publish.

## Consequences

- **`docs/PRD.md` revision 8:** N7, T6 and UC-22; T2 names network access to the service; §1 and §5 describe the deployment and actors.
- **`DESIGN.md`:**
  - §2 states the deployment and the single release;
  - §5.8 declares the kind and refuses an actor who is not live;
  - §5.9 records the release;
  - §6's first step resolves the actor, and its transaction reads the release;
  - §13 keeps the mediated property by deployment and states the service's known limit;
  - §14 redefines an actor.
- **`declaration-syntax.md` iteration 24:** the `actor <kind>` marking, check 62, and an `Agent` type in the example. `UserRole` loses `AGENT`, since the first consumer's agents are ported as `Agent` objects, and so an agent cannot be named engineer-of-record at all.
- **`library-api.md`:** `Actor` loses `kind`, and `EngineMismatch` is the sixth fault. **`storage-schema.md`:** `of_engine_release` and `of_actor`. **`renderers.md`:** the rule set prints an actor type's kind. **`first-consumer-cutover.md`:** the backend is a client, and users and agents are ported as actor objects.
- **Behaviour:**
  - an upper layer that forgets to revoke a departed user's session is refused by the engine;
  - an agent is held to the rules for agents, whatever capabilities its gateway sends;
  - a rule reading a capability is only as good as the upper layer's mapping of roles to capabilities, and the record keeps every descriptor.
- D381 and D382 are resolved by §4, with the upgrade step's specification left in `TODO.md`.
