# ObjectFlow

Governed flows for business objects. People and agents move objects only along declared transitions; every change is checked, recorded and measured.

> **Status: design only.** The store is not implemented. This repository holds the design record — the model, the declaration syntax deployments write their flows in, 108 decisions, and a register of 378 findings, 378 closed and 0 open — together with the checkers that verify the record against itself.
>
> [`docs/PRD.md`](docs/PRD.md), at revision 7, states the product's requirements and is the baseline every design choice is checked against. [`docs/design/traceability.md`](docs/design/traceability.md) shows each requirement met, with one exception: C5's target is left by the PRD to measurement.
>
> - **Accepted by the author:** ADR-0081 to ADR-0096, ADR-0104, the engine as the governed core, ADR-0107, the name ObjectFlow, which supersedes ADR-0011, and ADR-0108, cascades declared clearly enough not to surprise, whose principle is the author's and whose mechanism was written at the author's direction.
> - **Decided at the author's direction, awaiting the author's own acceptance:** ADR-0097 to ADR-0101, ADR-0103, and ADR-0105 and ADR-0106, from a review of the whole record against PRD revision 5 on 2026-09-24.
> - **Proposed:** ADR-0102, which writes the unit's whole journey from production ([`docs/design/unit-journey.md`](docs/design/unit-journey.md)).
> - **Ruled on by the author:** ADR-0065 to ADR-0073.
> - **Awaiting review:** ADR-0019 to ADR-0064, ADR-0074 to ADR-0080 and the six implementation documents of 2026-09-09.

## What it is

ObjectFlow is a **database + business logic layer**: a reusable substrate for building business applications, where the rules about data are declared alongside the data rather than reimplemented in every application.

You define object types — typed attributes, a state machine, guards on the transitions, invariants. ObjectFlow stores the objects and is the only thing that may change them. Applications, human interfaces and AI agents all read and act through the same declaration.

It also collects data about the flows it runs — every transition, refusal, override and change of hands — takes the datapoints users record, and computes declared metrics over both. That lets business logic be driven by data, whether a person or an agent drives it, and lets a flow start imperfect and converge. [`docs/PRD.md`](docs/PRD.md) states the requirements, and is the baseline every design choice is checked against.

```text
upper-layer applications                  manage the flow, use its data:
  agents · services · human UI            schedules, chasing, worklists,
                                          alerts, dashboards
─────────────────────────────────────
            ObjectFlow                  the governed core: declared flows,
                                          enforced rules, the record, metrics
─────────────────────────────────────
PostgreSQL / SQLite                       storage
```

**A governed core, not a platform.** How a flow is managed — who requests which transition and when, chasing, scheduling, assigning, notifying — and how its datapoints are put to use are built in upper-layer applications. They act through the same requests and rules as anyone else, with no path of their own (PRD §1, N6; ADR-0104). ObjectFlow does not compete with full metadata-driven platforms on automation, scheduling or screens. What it offers beneath them is the guarantee that governed state changes only as PRD F2 allows and never reaches a condition an enforced rule forbids, and the record that explains every decision its rules make (ADR-0104 §5).

## Why

The objective is **trust under delegation**: being able to hand a system to people who are not supervised, and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards.

Four properties carry that:

| Property | Meaning |
|---|---|
| **Mediated** | No path to governed state but the PRD's four, each recorded: a declared transition whose guards pass; an override — a **declared** assertion, the **built-in** assertion the import uses, or an admission; a flow change's migration, authorised by the publish's approval; and an erasure. There is no second write path (ADR-0105) |
| **Declared** | What is allowed is data, inspectable at runtime — not code |
| **Recorded** | Every change and every recorded datapoint is attributed and reconstructable |
| **Measured** | Every flow produces data about itself, users add their own, and everyone reads both through the same declared metrics |

The distinction from an ORM is deliberate. An ORM abstracts *mechanism* — it hides SQL, and faithfully executes whatever the caller asks. ObjectFlow abstracts *authority*: what may change, when, and by whom.

A practical consequence, measured rather than asserted. In the first consumer, 454 sites refuse an operation. **176 of them are redundant** — the same rule expressed again somewhere else — and 51 distinct rules appear at more than one site, across route checks, service methods, model hooks and database constraints. One rule, "a delivery must be in preparation", is written five times in four files. Declared once and inspectable, those become projections of one definition.

## Documentation

| Document | Contents |
|---|---|
| [`docs/PRD.md`](docs/PRD.md) | What the product must do: requirements, the use cases every design is tested against, and what they change in the current design. The baseline every design choice is checked against: revision 7, 2026-09-24, its Inferred rows and the two questions of its §10 open to the author |
| [`docs/design/data-driven-engine.md`](docs/design/data-driven-engine.md) | The design evaluation: each question's options tested against the PRD's use cases, and why ADR-0081 to ADR-0086 chose as they did |
| [`docs/design/traceability.md`](docs/design/traceability.md) | Every PRD requirement and use case, with the sections and decisions that meet it; a checker fails while any is not covered |
| [`docs/DESIGN.md`](docs/DESIGN.md) | Purpose, position in the stack, the model, scope boundaries, known limits |
| [`docs/adr/`](docs/adr/) | Decisions taken, each with the alternatives rejected and why |
| [`docs/design/declaration-syntax.md`](docs/design/declaration-syntax.md) | The language a type is declared in, and the checks publishing runs over it |
| [`docs/design/storage-schema.md`](docs/design/storage-schema.md) | How a declaration becomes tables, and which indexes a guarantee depends on |
| [`docs/design/library-api.md`](docs/design/library-api.md) | The nineteen operations a program calls, and the shapes they take |
| [`docs/design/publish-and-import.md`](docs/design/publish-and-import.md) | What publishing checks and reports, and how production data arrives |
| [`docs/design/renderers.md`](docs/design/renderers.md) | The rule set, agent tool schemas and form hints, as projections of one declaration |
| [`docs/design/adversarial-harness.md`](docs/design/adversarial-harness.md) | The acceptance test: what would falsify the guarantee, and how to try |
| [`docs/design/first-consumer-cutover.md`](docs/design/first-consumer-cutover.md) | The stage order for the system being ported, and what would change it |
| [`docs/design/unit-journey.md`](docs/design/unit-journey.md) | The first consumer's unit, from request to service and loan, written from its production code as a checked module |
| [`docs/design/flow-review.md`](docs/design/flow-review.md) | That journey read as an operations manager would, and the decisions it puts to the author |
| [`docs/design/returns-module.md`](docs/design/returns-module.md) | A returns flow published with no rules and then tightened from its own data, as two checked publishes |
| [`docs/design/first-consumer-audit.md`](docs/design/first-consumer-audit.md) | Every business rule the first consumer enforces, set beside the design |
| [`docs/design/`](docs/design/) | The first-consumer walkthrough, five case studies, the catalogue of edge cases, and the defect register |
| [`docs/LESSONS.md`](docs/LESSONS.md) | Operational lessons |
| [`TODO.md`](TODO.md) | Where the design stands, what the author has decided, and what is still open |
| [`scripts/`](scripts/) | Six checkers and a probe. One runs the declaration syntax's own rules over every example in the documents; one runs the storage schema's SQL; one executes the library API and holds it against the model; one validates the agent tool schemas; one holds the traceability map against the PRD, failing while any requirement is not covered; the sixth checks the corpus against itself — cross-references, retired notation, decision back-links, the defect index, the counts the status lines quote — and runs the other five. `probe-metric-latency.py` measures metric reads on SQLite, as indicative evidence for PRD C5, and asserts nothing |

## Scope

The first consumer is the author's own robotics operations platform, rebuilt on ObjectFlow with its production data ported; see [`docs/DESIGN.md`](docs/DESIGN.md#4-first-consumer-and-case-studies).

ObjectFlow **decides, records and measures**. It evaluates any declared formula over its own data, metrics across objects and time included. It does not own formulas that need data or rules it does not hold, such as a tax table or a pricing engine. Nor does it choose among alternatives, cause external effects, orchestrate long-running processes, or render a user interface — those belong to the upper-layer applications above it. See [ADR-0007](docs/adr/0007-decide-and-record-not-compute-or-effect.md) and [ADR-0081](docs/adr/0081-the-engine-measures-its-flows-and-computes-over-its-own-data.md) for why that boundary is where it is.

## License

[Apache License 2.0](LICENSE).
