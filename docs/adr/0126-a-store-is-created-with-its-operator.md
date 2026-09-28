# ADR-0126: A store is created with its operator, the actor its first requests name

- **Status:** Accepted by the author, 2026-09-28: "option 1", chosen from the four options put to the author the same day.
- **Date:** 2026-09-28
- **Refined by:** ADR-0134 — the operator's creation is recorded as the operator's own act, since an event's actor is required and a release is not one.
- **Refines:** ADR-0105 (a store begins at declaration version 0), ADR-0110 (declared actor kinds), ADR-0114 (an unknown actor is refused)
- **Relates to:** PRD F7, T3, T6; `DESIGN.md` §5.8, §5.9, §9, §10; `storage-schema.md` §6; `publish-and-import.md` §1

## Context

Every request names an actor, and a request whose actor names no object of a type that marks an actor identity is refused as `actor_known` (`DESIGN.md` §5.8, §6 step 1). No new store could take a request, at two levels (D452):
- **At version 0.** Creating a store installs the built-in types, `DeclarationChange`, `Proposal`, `Subscription` and the `label` kind, and none of them marks an actor identity (§5.9, §9). The first flow is published by a request on a `DeclarationChange`, which must name an actor that no type can hold.
- **After the first publish.** The deployment's version declares `User`, `Agent` or `Service`, and none of their objects exists. Creating the first is a request, and it must name an actor that does not exist yet.

The fourth recoverability review found the second level, and `TODO.md` recorded it as a question on 2026-09-27, with three options: a creation naming the identity it creates as its own actor (recommended then), an initial actor named when the store is created, and the port alone. Putting the question to the author on 2026-09-28 found the first level. The recommendation of 2026-09-27 does not reach it, and the port does not escape it, since `import_batch` names an actor like every operation (`library-api.md` §6).

## Decision

1. **Version 0 holds a built-in type, `Operator`,** for the people answerable for a store. Its identity attribute is marked `actor human` and is unique. Its states are `ACTIVE`, `live`, and `RETIRED`, `closed` and final. Its creation, `install`, is taken only by creating the store. `retire`, from `ACTIVE` to `RETIRED`, is requested like any transition.
2. **Creating a store takes the identity of its operator.** In its one step it writes version 0 and the operator's object, both attributed to the engine release that created the store, as version 0 already is (`storage-schema.md` §6). *(Refined the same day by ADR-0134: an event's actor is required and a release is not one, so the operator's object is recorded as the operator's own act, the context naming the release; version 0 stays the release's.)*
3. **The operator has no privilege.** The engine evaluates no actor (ADR-0114), and what the operator may do is the upper layer's to decide. The operator is the actor the store's first requests can name: drafting and approving the first `DeclarationChange`, creating the first objects of the deployment's actor types, or running the port. So version 1 passes the governed path like every later version (PRD F7), and it and every actor after it are attributed to someone (PRD T3).
4. **An identity names one actor across every type** (`DESIGN.md` §5.8), so the operator's identity is not reused by a later `User`. A deployment gives the operator an identity of its own, such as `ops:ana`, or lets the person act as the operator throughout.

## Alternatives rejected

- **Creating a store takes the first declaration and the first actors, recorded as the creation's.** It adds nothing to the model, but version 1 and the first actors would be attributed to the engine release and to no one, and version 1 would not pass the governed path (PRD T3, F7).
- **A creation may name the identity it creates as its own actor.** Recommended on 2026-09-27, and withdrawn: it does not reach version 0, where no type holds actors, and with this decision it is not needed after it.
- **The port alone.** `import_batch` names an actor too, and a store with no legacy system has no port.
- **An operator whose kind is `service`, or chosen when the store is created.** A script that provisions a store names the person accountable for it, as a deployment is signed for. A kind chosen per store would make the operator the one actor whose kind its type does not fix (PRD T6).

## Consequences

- `DESIGN.md` §5.9, §9 and §10, `storage-schema.md` §6, `declaration-syntax.md` §2, `renderers.md` §2, `publish-and-import.md` §1 and `library-api.md` §6 name the operator.
- `TODO.md`'s question on the first actor is closed, with the recommendation of 2026-09-27 withdrawn there.
- The journey's `Customer` is unaffected: a mirror is written by the import alone, so a store with no legacy system has no customers until an owned type replaces the mirror (`declaration-syntax.md` §2, ADR-0075). That is by design, as the seventh recoverability review found.
