# ADR-0071: Mandatory bounds and guard names stay; the reported worst-case product goes

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refines:** ADR-0041, ADR-0047
- **Answers:** open question 4

## Context

Two deviations were bundled in one question because both cost the author something on every declaration: every loop must state a bound, and every guard must be named.

The evidence against the bounds was that the numbers are invented. A reviewer's clinical-trial model produced a reported worst-case fan-out around thirty-three thousand, from limits they said they made up, so the figure was precise and meaningless.

The evidence for them is that the loops are real. The first consumer raises a procurement order as `for i in 1..inputs.quantity`, repeating over a caller-supplied count, and its production system exposes a batch creation endpoint (`wr:app/api/robots.py:48`). Without a bound a typo creates a hundred thousand objects in one transaction.

Guard names were never in doubt on evidence: a verdict carries the failing clause's name, which is what the remedy classes, the availability listing and every consumer's error text are built on.

## Decision

**Both stay.** What goes is the **reported worst-case product across nested loops and cascades**, which multiplied invented numbers into a total that looked authoritative and meant nothing.

In its place the publish report gives:

- each loop's own declared bound, attributed to the loop, which is a number the author chose and can defend;
- the **observed** maximum fan-out over the live objects, which the report already reads for the new-invariant scan.

## Alternatives rejected

- **Drop mandatory bounds.** Rejected: the protection is real at exactly the place the first consumer loops, over a caller-supplied count. An unbounded loop in a serialisable transaction is a production incident, not a slow query.
- **Infer a bound.** Rejected for the reason ADR-0050 gives about tracking: an inferred number would be a rule nobody wrote, and the author would discover it by being refused.
- **Keep the product and label it a worst case.** Rejected because it was labelled a worst case and was still read as information. A number computed from invented inputs should not be printed at all.

## Consequences

- The friction stays and is now ratified rather than inherited, which is what the question asked for.
- A consumer reading the publish report gets one number it can act on, the observed maximum, and one it chose, the declared bound, instead of a product of the two.
- Computing the observed maximum needs the live objects, which `docs/design/declaration-syntax.md` §10 already lists among the report's external inputs.
