# ADR-0003: One state machine per object type; composition rather than parallel regions

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

Real objects often advance along more than one independent axis. An illustrative Deployment might progress through delivery (`planned → provisioning → commissioning → uat → accepted`) and commercially (`quoted → ordered → invoiced → paid`) at the same time, with neither driving the other.

With a single state field, every combination must be named. Five delivery states by four commercial states is twenty, and transitions multiply worse: one delivery transition becomes four, each carrying an identical copy of the same guards, which then drift. A third axis multiplies again.

## Decision

An object type has exactly one state machine. Where a second axis exists, it is resolved by one of:

1. **Composition** — the axis belongs to a related object with its own lifecycle.
2. **Plain fields** — if the axis has no gated transitions, it is not a lifecycle; it is data that changes.

A thing is a *part* of a composite only if both hold: **exclusive membership** (it belongs to exactly one composite) and **bounded lifetime** (it cannot outlive the composite). If either fails it is a reference, not composition. Cardinality settles it without judgement: a robot outlives its contracts, so contracts cannot be a region inside a robot.

## Alternatives rejected

### Orthogonal (parallel) regions

One object carrying several named state machines running concurrently, as in Harel statecharts and XState. Avoids the cross product with 5 + 4 states rather than 20.

Rejected because "the state" stops being a scalar, and every API, history record and guard expression must then name a region — a small complexity spread everywhere. Cross-object guards are a shape the system must support regardless (composites require them), whereas cross-region guards would be a second mechanism doing a similar job.

### Compound cross-product states

Rejected outright: quadratic state growth, guards duplicated per combination, and inevitable drift between the copies.

## Consequences

- Collection guards ("all parts satisfy P", "no part is in state S") become mandatory, since composite guards are inherently of that shape.
- Composition nests. Each level should trust the level below's guards rather than re-checking them: a Deal gates on "all Deployments are `accepted`" and relies on the Deployment's own guards to have enforced task completion.
- An axis that is genuinely 1:1 for an object's whole life *and* has gated transitions of its own would need an object with no natural name. That case has not been observed; if it appears it is the one piece of evidence that would argue for reconsidering regions.
- Objects invented purely to host a lifecycle are a modelling smell. The test is whether the thing has a name people already use.
