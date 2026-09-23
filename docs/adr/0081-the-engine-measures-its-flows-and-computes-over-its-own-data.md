# ADR-0081: The engine measures its flows, and computes any declared formula over its own data

- **Status:** Proposed — evaluated 2026-09-23 at the author's direction, against `docs/PRD.md`; pending author review
- **Date:** 2026-09-23
- **Refines:** ADR-0007

## Context

On 2026-09-23 the author stated an objective the record had not captured: the engine is the foundation on which people and AI agents define flows and **collect data** — data the flow generates, data users record, and compound data from formulas users define — so that business logic can be data-driven, and so that a flow can start imperfect and converge. `docs/PRD.md` states it as requirements and nineteen use cases, and `docs/design/data-driven-engine.md` evaluates the designs against them.

Two positions of the record stand in the way. ADR-0007 keeps computation out of the store: "It does not calculate values", refined since to allow declared arithmetic over the store's own data while excluding domain formulas. And `design/edge-cases.md` puts analytics over the log out of scope, in "a warehouse fed from the log by a subscriber".

## Decision

1. **The engine measures its flows.** It keeps data about its own flows (ADR-0083), takes datapoints users record (ADR-0082), and computes declared formulas and metrics over both (ADR-0084). A fourth property joins Mediated, Declared and Recorded: **Measured** — every flow produces data about itself, users add their own, and everyone reads both through the same declared definitions.
2. **The computation boundary moves from "arithmetic in, formulas out" to "the store's own data in, everything else out."** Any declared formula over data the store holds is in scope, including aggregates across objects and over time, and scores built from them. What stays out:
   - a formula that needs data or rules the store does not hold, such as a tax table or a pricing engine;
   - selection: choosing which engineer, which unit, which order first;
   - effects: sending, raising, posting;
   - orchestration;
   - open-ended exploration.
3. **Exploration is exported, not built.** The engine exports its log, flow datasets and observations in a form suited to exploratory tools, and does not become a query-and-chart product.
4. **One store.** Metrics are computed in the same PostgreSQL or SQLite store as the governed state. Machine telemetry at sensor rates stays outside; a service may record summaries of it as observations.
5. **The engine still initiates nothing** (ADR-0012). It computes a metric when asked. An alert is a flag declared on a metric, which a scheduler or an agent queries and acts on.

## Alternatives rejected

- **Keep analytics a subscriber's job.** It fails the use cases the requirement exists for (`design/data-driven-engine.md` §3.4). A BI tool's definition of a metric drifts from the one a screen and an agent use, which is the five-copies problem ADR-0010 was written about, now for metrics. A rule cannot read a value that lives outside the store (UC-10), and an agent reads a different definition from the person beside it (UC-11).
- **A separate analytic store fed from the log.** It lags the governed state that a data-driven rule must be consistent with, and it is a third backend to build and test, at a scale the first consumer does not need (§3.7).
- **Build exploration into the engine.** A second product, and not one the requirement asks for; the PRD says "potentially dig insights", and an export serves that.
- **Leave ADR-0007's line where it is.** "Arithmetic in, formulas out" cannot place a user-defined rate or score, which the PRD requires (C1), and "own data in, outside data out" can. The second line is also the one ADR-0007's rationale actually rests on: domains differ irreducibly in rules they bring from outside, not in counting what they recorded.

## Consequences

- On acceptance, `DESIGN.md` §1, §3, §12 and §13 state the objective, the fourth property and the boundary; `design/edge-cases.md`'s analytics entry becomes "exploration is exported; declared metrics are in scope".
- ADR-0007 carries the back-link.
- The decisions that implement this are ADR-0082 to ADR-0086.
