# ObjectKeeper

A governed object store: your data, and the rules that constrain how it changes.

> **Status: design only.** The store is not implemented. This repository holds the design record — the model, the declaration syntax consumers write against, 102 decisions, and a register of 281 findings, 276 closed and 5 open — together with the checkers that verify the record against itself. The latest findings — from the design review, the design evaluation, the mapping of the design against the PRD, an independent review of that mapping, and a five-slice review of the whole record, all on 2026-09-23 — are resolved. [`docs/PRD.md`](docs/PRD.md) states the product's requirements and is the baseline every design choice is checked against; ADR-0081 to ADR-0096, accepted by the author on 2026-09-23 — the last ten decided at the author's direction and checked against the design before acceptance — and ADR-0097 to ADR-0101, decided at the author's direction the same day and awaiting acceptance, are the design that meets them, and [`docs/design/traceability.md`](docs/design/traceability.md) shows each requirement met, except C5, whose target the PRD leaves to measurement, and C2, UC-8 and UC-10, which the design found unsatisfiable as written beside T5 and for which PRD §12 proposes a revision to the author. An audit of the first consumer's production code on 2026-09-24 ([`docs/design/first-consumer-audit.md`](docs/design/first-consumer-audit.md)) found D275 to D281, five of them open, and ADR-0102, proposed the same day, writes the unit's whole journey from request to loan and service ([`docs/design/unit-journey.md`](docs/design/unit-journey.md)). The author has ruled on ADR-0065 to ADR-0073; ADR-0019 to ADR-0064, ADR-0074 to ADR-0080 and the six implementation documents of 2026-09-09 await review.

## What it is

ObjectKeeper is a **database + business logic layer**: a reusable substrate for building business applications, where the rules about data are declared alongside the data rather than reimplemented in every consumer.

You define object types — typed attributes, a state machine, guards on the transitions, invariants. ObjectKeeper stores the objects and is the only thing that may change them. Applications, human interfaces and AI agents are all consumers of the same declaration.

It also collects data about the flows it runs — every transition, refusal, override and change of hands — takes the datapoints users record, and computes declared metrics over both. That lets business logic be driven by data, whether a person or an agent drives it, and lets a flow start imperfect and converge. [`docs/PRD.md`](docs/PRD.md) states the requirements, and is the baseline every design choice is checked against.

```text
agents · applications · human UI          consumers
─────────────────────────────────────
            ObjectKeeper
─────────────────────────────────────
PostgreSQL / SQLite                       storage
```

## Why

The objective is **trust under delegation**: being able to hand a system to people who are not supervised, and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards.

Four properties carry that:

| Property | Meaning |
|---|---|
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path, and there are exactly two ways to set state without satisfying a guard: a **declared** assertion the type carries, and the **built-in** assertion import and migration use, gated on a deployment capability. Both are capability-gated and both are recorded |
| **Declared** | What is allowed is data, inspectable at runtime — not code |
| **Recorded** | Every change and every recorded datapoint is attributed and reconstructable |
| **Measured** | Every flow produces data about itself, users add their own, and everyone reads both through the same declared metrics |

The distinction from an ORM is deliberate. An ORM abstracts *mechanism* — it hides SQL, and faithfully executes whatever the caller asks. ObjectKeeper abstracts *authority*: what may change, when, and by whom.

A practical consequence, measured rather than asserted. In the first consumer, 454 sites refuse an operation. **176 of them are redundant** — the same rule expressed again somewhere else — and 51 distinct rules appear at more than one site, across route checks, service methods, model hooks and database constraints. One rule, "a delivery must be in preparation", is written five times in four files. Declared once and inspectable, those become projections of one definition.

## Documentation

| Document | Contents |
|---|---|
| [`docs/PRD.md`](docs/PRD.md) | What the product must do: requirements, the use cases every design is tested against, and what they change in the current design. The baseline every design choice is checked against: revision 3, 2026-09-23, its Inferred rows open to the author's correction |
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
| [`docs/design/`](docs/design/) | The first-consumer walkthrough, five case studies, the catalogue of edge cases, and the defect register |
| [`docs/LESSONS.md`](docs/LESSONS.md) | Operational lessons |
| [`TODO.md`](TODO.md) | Where the design stands, what the author has decided, and what is still open |
| [`scripts/`](scripts/) | Six checkers and a probe. One runs the declaration syntax's own rules over every example in the documents; one runs the storage schema's SQL; one executes the library API and holds it against the model; one validates the agent tool schemas; one holds the traceability map against the PRD, failing while any requirement is not covered; the sixth checks the corpus against itself — cross-references, retired notation, decision back-links, the defect index, the counts the status lines quote — and runs the other five. `probe-metric-latency.py` measures metric reads on SQLite, as indicative evidence for PRD C5, and asserts nothing |

## Scope

The first consumer is the author's own robotics operations platform, rebuilt on ObjectKeeper with its production data ported; see [`docs/DESIGN.md`](docs/DESIGN.md#4-first-consumer-and-case-studies).

ObjectKeeper **decides, records and measures**. It evaluates any declared formula over its own data, metrics across objects and time included. It does not own formulas that need data or rules it does not hold, such as a tax table or a pricing engine. Nor does it choose among alternatives, cause external effects, orchestrate long-running processes, or render a user interface — those belong to the consumers above it. See [ADR-0007](docs/adr/0007-decide-and-record-not-compute-or-effect.md) and [ADR-0081](docs/adr/0081-the-engine-measures-its-flows-and-computes-over-its-own-data.md) for why that boundary is where it is.

## License

[Apache License 2.0](LICENSE).
