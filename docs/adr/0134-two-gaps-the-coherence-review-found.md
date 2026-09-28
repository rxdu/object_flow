# ADR-0134: Two gaps the coherence review of 2026-09-28 found

- **Status:** Accepted by the author in advance, 2026-09-28, under the author's standing instruction "for all similar items that you have confidence in an recommendation, just accept", given for the author-attention pass of the same day and applied to this review, which the author asked for: "review the existing requirements/design docs as a whole again, ensure it's still clear and coherent".
- **Date:** 2026-09-28
- **Refines:** ADR-0126 (the store's operator), ADR-0133 (erasure required)
- **Relates to:** PRD D8, T3, T6; `DESIGN.md` §5.9, §8, §9, §13; `storage-schema.md` §2, §6; `flow-format.md` §4.13; `declaration-syntax.md` §6.4, §10 check 60

## Context

On 2026-09-28 the author had the requirements and design read whole, in six slices, for whether they still agree. Most of what the readers found was wording that later decisions had left behind (D462 to D477). Two findings needed a choice:
1. **The operator's creation could not be stored as ADR-0126 wrote it.** ADR-0126 decision 2 has creating a store write version 0 and the operator's object, "both attributed to the engine release". Version 0 is a row of `of_declaration`, whose `published_by` names the release. The operator's object is created by an event, and an event's `actor_id` and `actor_kind`, and its object's `created_by_kind`, are required and name an actor (`storage-schema.md` §2). A release is not one, and `ActorKind` has no value for it.
2. **A personal input could be declared where nothing erases it.** An erasure reaches every event payload that carried a personal input, and only through the erasure of that event's object (`DESIGN.md` §8, ADR-0078). ADR-0133 made check 60 require an erasure of a type holding a personal attribute, and counted no inputs. The flow format makes an assertion's explanation in words a personal input (`flow-format.md` §4.13), so the unit journey's `Robot`, the format's `ServiceJob` with its reassignment note and the syntax document's `Robot` held values no request could erase, against PRD D8, a Must.

*A draft of this record, never committed, gave it a third decision: a reused idempotency key had "no remedy class", and was given `self_serviceable`. That was asserted without searching the decision records, and was wrong: ADR-0105 §6 had already decided it, "a reused key, `self_serviceable`". The gap was only that `DESIGN.md` §6, `library-api.md` §7 and `storage-schema.md` §6 did not say so, which is corrected in place (D465). A reader of the decision records found it the same day.*

## Decision

1. **The operator's creation is attributed to the operator.** The event that creates the store's `Operator` object names the operator's identity as its actor, of kind `human` as the type declares, with provenance `observed` and the context `store-creation:<release>`, naming the engine release that created the store. Version 0 stays attributed to that release, since it is the built-in module, which no one wrote and which changes no flow. `storage-schema.md` §6 gives `t_operator` its table.
2. **A type whose transitions take a personal input declares an erasure.** Check 60 counts a personal input as it counts a personal attribute. A machine's transitions count for each type that binds it, and a machine itself, having no objects, is not held to the rule, as an abstract base is not. The journey's `UnitLifecycle`, the format's `ServiceJob` and the syntax document's `Robot` gain an erasure `forget`.

## Alternatives rejected

- **For the operator, actor columns that may be empty for a store's creation.** Every reader of the log, every metric split by kind of actor and every relay reading `pull` would handle an event with no actor, for one row per store, and PRD T3, "every change to governed state … is attributed to an actor", would gain an exception.
- **For the operator, a reserved actor for the engine, of kind `service`.** No type would hold it, so `actor_known` would refuse any request naming it, and the metrics would count a creation by a service that no service made.
- **Leaving personal inputs out of check 60.** A value nothing can erase is what D8 exists to prevent, the gap ADR-0133 closed for attributes.
- **An erasure generated on every type.** ADR-0133 has the erasure declared, where the author of a flow reads it and where it states the parts it reaches (check 39); a generated one would hide both.

## Consequences

- `storage-schema.md` §6 and `DESIGN.md` §5.9, §9 and §13 attribute the operator's creation to the operator.
- `flow-format.md` §4.13 and `declaration-syntax.md` §6.4 and check 60 count personal inputs; `scripts/check-flows.py` and `scripts/check-syntax-doc.py` enforce it, and the flow checker's self-test plants a personal input on a type with no erasure.
