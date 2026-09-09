# ObjectKeeper

A governed object store: your data, and the rules that constrain how it changes.

> **Status: design only.** The store is not implemented. This repository holds the design record — the model, the declaration syntax consumers write against, 73 decisions, and a register of 185 findings, all closed — together with the checkers that verify the record against itself. The author has ruled on ADR-0065 to ADR-0073; ADR-0019 to ADR-0064 are written and await review.

## What it is

ObjectKeeper is a **database + business logic layer**: a reusable substrate for building business applications, where the rules about data are declared alongside the data rather than reimplemented in every consumer.

You define object types — typed attributes, a state machine, guards on the transitions, invariants. ObjectKeeper stores the objects and is the only thing that may change them. Applications, human interfaces and AI agents are all consumers of the same declaration.

```text
agents · applications · human UI          consumers
─────────────────────────────────────
            ObjectKeeper
─────────────────────────────────────
PostgreSQL / SQLite                       storage
```

## Why

The objective is **trust under delegation**: being able to hand a system to people who are not supervised, and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards.

Three properties carry that:

| Property | Meaning |
|---|---|
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path, and there are exactly two ways to set state without satisfying a guard: a **declared** assertion the type carries, and the **built-in** assertion import and migration use, gated on a deployment capability. Both are capability-gated and both are recorded |
| **Declared** | What is allowed is data, inspectable at runtime — not code |
| **Recorded** | Every change is attributed and reconstructable |

The distinction from an ORM is deliberate. An ORM abstracts *mechanism* — it hides SQL, and faithfully executes whatever the caller asks. ObjectKeeper abstracts *authority*: what may change, when, and by whom.

A practical consequence: a single business rule today is expressed as a storage constraint, as backend validation, as frontend form state, as error text, and now as an agent tool description — five copies in four languages, each free to drift. Declared once and inspectable, those become projections of one definition.

## Documentation

| Document | Contents |
|---|---|
| [`docs/DESIGN.md`](docs/DESIGN.md) | Purpose, position in the stack, the model, scope boundaries, known limits |
| [`docs/adr/`](docs/adr/) | Decisions taken, each with the alternatives rejected and why |
| [`docs/design/declaration-syntax.md`](docs/design/declaration-syntax.md) | The language a type is declared in, and the checks publishing runs over it |
| [`docs/design/storage-schema.md`](docs/design/storage-schema.md) | How a declaration becomes tables, and which indexes a guarantee depends on |
| [`docs/design/library-api.md`](docs/design/library-api.md) | The fourteen operations a program calls, and the shapes they take |
| [`docs/design/publish-and-import.md`](docs/design/publish-and-import.md) | What publishing checks and reports, and how production data arrives |
| [`docs/design/`](docs/design/) | The first-consumer walkthrough, five case studies, the catalogue of edge cases, and the defect register |
| [`docs/LESSONS.md`](docs/LESSONS.md) | Operational lessons |
| [`TODO.md`](TODO.md) | Where the design stands, what the author has decided, and what is still open |
| [`scripts/`](scripts/) | Four checkers. One runs the declaration syntax's own rules over every example in the documents; one runs the storage schema's SQL; one executes the library API and holds it against the model; the fourth checks the corpus against itself — cross-references, retired notation, decision back-links, the defect index |

## Scope

The first consumer is the author's own robotics operations platform, rebuilt on ObjectKeeper with its production data ported; see [`docs/DESIGN.md`](docs/DESIGN.md#4-first-consumer-and-case-studies).

ObjectKeeper **decides and records**. It evaluates declared arithmetic over its own data, and it does not own domain formulas such as tax or pricing, cause external effects, orchestrate long-running processes, or render a user interface — those belong to the consumers above it. See [ADR-0007](docs/adr/0007-decide-and-record-not-compute-or-effect.md) for why that boundary is where it is.

## License

[Apache License 2.0](LICENSE).
