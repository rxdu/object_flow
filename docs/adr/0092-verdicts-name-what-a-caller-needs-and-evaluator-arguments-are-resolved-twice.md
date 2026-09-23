# ADR-0092: A verdict names what the caller needs; tool inputs cannot collide; an evaluator's arguments are resolved twice

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` F4, F3, L5 and T1; repairs D214 and D215
- **Date:** 2026-09-23
- **Refines:** ADR-0049, ADR-0077

## Context

PRD F4: "A refusal says which rule refused, and whether the caller should supply something, ask someone, wait, work on another object first, or take a different path."

**D214.** Three shapes fell short of what the model says:
- `Unsatisfied` carries one object and no capability, while a `dependent` verdict names the parts to work on and a `delegable` one may name the capability and offer a proposal (`DESIGN.md` §5.5). The caller learns to "ask someone" without learning whom.
- `Stale` has required `expected` and `actual`, and also stands for exhausted retries, where neither need exist.
- A tool schema puts a transition's inputs beside `object_id`, `expected_version` and `idempotency_key`, so an input with one of those names collides with the request's own field.

**D215.** ADR-0049 consults an evaluator before the transaction and never says what happens when its argument depends on state the transaction changes. ADR-0084 answered that question for metric guards, and not for the evaluators it copied.

## Decision

### 1. `Unsatisfied` names what the remedy points at

It gains:
- `objects`: the objects a `dependent` remedy says to work on first, or the object the failing clause read;
- `capability`: for a clause over `actor.has(C)`, the capability that would satisfy it;
- `proposable`: whether the transition accepts a proposal.

Each is filled where the clause gives it, and otherwise empty.

### 2. `Stale` carries its cause

`cause` is `expected_version` or `retries_exhausted`, with `expected` and `actual` present only for the first.

### 3. A tool schema nests a transition's inputs under `inputs`

The request's own fields stay at the top level, so no input name can collide with them.

### 4. An evaluator's arguments are resolved twice

This is the metric-guard rule of ADR-0084, applied to evaluators:
- Arguments are resolved against committed state and the inputs before the transaction, and again inside it. A mismatch means the world moved, and the request consults again within its serialisation retry bound.
- An argument the request's own outcome writes is refused at publish, since no consultation before the transaction can see a value it has not yet written.

## Alternatives rejected

- **Leave `Unsatisfied` minimal and let callers call `availability`.** A second round trip to learn what the refusal already knew, and an agent that does not make it acts on less than F4 promises.
- **Reserve the three request field names against inputs.** It works, and it makes every future request field a breaking change for inputs already named after it. Nesting costs nothing.
- **Consult evaluators inside the transaction when an argument moves.** It holds locks across a network call, which ADR-0049 exists to prevent.

## Consequences

- `DESIGN.md` §5.5 and §6 say what a verdict carries and how an evaluator's arguments are resolved.
- `library-api.md` §4's shapes change.
- `renderers.md` §3's tool schema nests inputs.
- `declaration-syntax.md` gains the check on an evaluator argument the outcome writes.
- D214 and D215 are resolved.
