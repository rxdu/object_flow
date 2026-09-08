# ADR-0058: Every terminal transition of a whole must dispose of its parts, and re-parenting is checked against both wholes

- **Status:** Accepted — repair of D61 and D62, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0024, ADR-0045, ADR-0046, ADR-0057

## Context

Two questions surfaced when someone built a shipment model in the declaration syntax. Both are about composition, which the model has always described as "exclusive membership and a lifetime bounded by the whole" without saying what enforces the second half.

**A part is orphaned by any terminal transition its cascade does not name.** ADR-0056 gave a composition a `cascade on <transition>` clause so the deletion cascade names a trigger. A whole with two terminal transitions, say `dispatch` and `cancel`, names one. Its parts then survive the other, alive under a closed whole, which is exactly what "lifetime bounded by the whole" forbids.

**Re-parenting escapes the source whole's invariants.** ADR-0046 makes splitting an object expressible by writing a part's owner. After that write the part points at the destination, so ADR-0045's reverse traversal finds only the destination, and an invariant on the source such as "a shipment has at least one line" is never re-checked. ADR-0057's part-event index has the same ambiguity: which whole is stamped when a part changes owner.

## Decisions

### 1. Every terminal transition disposes of the parts

A composition declares one `cascade on` clause per terminal transition of the whole, or marks the part `survives` to say deliberately that it outlives its whole. Publishing rejects a terminal transition covered by neither, naming it.

`survives` is deliberately awkward to write, because a part that outlives its whole is usually a modelling error and occasionally a real one: a shipment's audit photographs may be meant to outlive the shipment. Making it explicit turns a silent orphan into a decision someone made.

### 2. Re-parenting writes both wholes

A transition that writes a part's `owner` is treated as writing **both** the source whole and the destination whole. Consequently:

- the invariants of both are checked after the outcome (ADR-0045), so a source falling below its minimum is caught;
- both have their part-event position stamped (ADR-0057), so an approval on either is invalidated;
- the event on the part records both, so history says where it came from as well as where it went.

Neither whole's guards run: this is not a transition on them, and requiring their consent would make every split a three-party negotiation. What it is, is a write to them for the purposes of invariants and indexes, which is the smallest thing that makes the source's rules hold.

## Alternatives rejected

- **Infer the cascade from every terminal transition.** Rejected: which part transition to drive is a modelling choice — a cancelled order voids its lines, a completed one archives them — and inference would pick one silently.
- **Forbid re-parenting.** Rejected: it is what makes splitting an object expressible, and split appeared in three of five case studies.
- **Run the wholes' guards on a re-parent.** Rejected: a guard is a precondition on a requested transition, and neither whole's transition was requested. Invariants are the right instrument, because they are properties that must hold however the object was written.

## Consequences

- The syntax's `part` clause repeats for each terminal transition, and gains `survives`.
- Publishing gains two checks: a terminal transition with no disposition, and, for re-parenting, that both wholes' invariants are analysable.
- The storage schema's part-event position is written for two objects on a re-parent.
- D61 and D62 are resolved.
