# ADR-0031: Erasure redacts declared personal attributes across the object and its history; it is recorded, irreversible, and not deletion

- **Status:** Accepted — taken in autonomous design iteration 3 (2026-09-08); pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0078 — erasure reaches legacy entries, proposal inputs and personal inputs. ADR-0087 — erasure follows the supersession chain and reaches the caller's event. ADR-0051 — a personal value may not be copied into a non-personal attribute, and the redaction marker is absence.
- **Amended by:** ADR-0056 §7 — erasure runs from any state including terminal, and stays optional. *(ADR-0087 §2 makes it mandatory for a type that takes part in supersession, check 60.)* ADR-0060 §5 adds that erasure admits the invariants it breaks.

## Context

A right-to-erasure request requires that a person's data be removed, including from history. The recorded property (DESIGN.md) and ADR-0024's deletion keep history by design. This is the first requirement in the case studies that pushes against that property, and it is a legal one, so it needs a designed path rather than a database edit. Case study: `docs/design/case-study-crm.md` §4.

## Decision

1. An attribute may be declared **`personal`**.
2. A type with personal attributes may declare an **erasure transition**: a transition, guarded on authority like any other, whose outcome is *erase*. *(Narrowed by ADR-0087 §2: a type with a personal attribute that takes part in supersession must declare one, since erasure follows the supersession chain through each member's own `erase`, and publishing rejects it otherwise (check 60). Elsewhere it stays optional; `declaration-syntax.md` §6.4.)* It may leave the object in its current state or move it to a terminal one, as declared.
3. Erasure **replaces every personal value** with a redaction marker on the object and in every recorded event that carried the value — inputs, outcome writes, before-and-after values. It **deletes the content** of any `file` reference held in a personal attribute (ADR-0017) and redacts the reference. *(Refined by ADR-0087 §1, repairing D207: files are content-addressed, so the content is deleted only once every reference to its hash is erased, and an identical file another object holds survives; a deletion that fails is retried by the next erasure request (ADR-0096 §11); `DESIGN.md` §8.)*
4. Erasure **keeps the event skeleton**: identifiers, timestamps, actors, transition names, states, causes, and non-personal values. History remains reconstructable in shape.
5. Erasure is **recorded** as an event carrying the actor, the reason, and the names of the attributes erased — never their values — and is **irreversible**: no override restores an erased value.
6. The redaction marker is absence, stored as SQL NULL (ADR-0051), and reads as unknown in the expression language (ADR-0047), so a guard that depended on the value fails with its usual remedy class rather than being satisfied.

Erasure is not deletion (ADR-0024). A deleted object keeps its values; an erased object may keep its state.

## Alternatives rejected

### Hard-delete the object and its events

Rejected: it destroys references, causal lineage and the non-personal record, and the recorded property has no exception for whole objects.

### Encrypt personal values and discard the key

Achieves erasure without rewriting events. Rejected as the primary mechanism because the redaction must still be visible as such in the record, and because key management would become a second correctness dependency for reads; it remains a valid storage implementation of step 3.

### Leave erasure to the consumer

Rejected: the consumer cannot reach the event log except through ObjectFlow, which is the point of ADR-0001.

## Consequences

- The event log is append-only *except* for redaction in place, which is itself recorded. Push and pull consumers that already received a value are outside ObjectFlow's reach; the erasure event tells them to act.
- Idempotency keys and sequences are unaffected; they never hold personal values by declaration.
- `docs/design/edge-cases.md` records the caveats: dependent guards, and hashes of erased files.
