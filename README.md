# ObjectKeeper

A governed object store: your data, and the rules that constrain how it changes.

> **Status: design only, under review.** Nothing is implemented. This repository contains the design record: the model, the declaration syntax consumers write against, the decisions taken, and a defect register whose 170 findings have been resolved but not yet reviewed by the author.

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
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path, and the one way to set state without satisfying a guard is a declared, capability-gated, recorded assertion |
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
| [`docs/design/`](docs/design/) | The first-consumer walkthrough, five case studies, the catalogue of edge cases, and the defect register |
| [`docs/LESSONS.md`](docs/LESSONS.md) | Operational lessons |
| [`TODO.md`](TODO.md) | Where the design stands, what the author has decided, and what is still open |

## Scope

The first consumer is the author's own robotics operations platform, rebuilt on ObjectKeeper with its production data ported; see [`docs/DESIGN.md`](docs/DESIGN.md#first-consumer).

ObjectKeeper **decides and records**. It evaluates declared arithmetic over its own data, and it does not own domain formulas such as tax or pricing, cause external effects, orchestrate long-running processes, or render a user interface — those belong to the consumers above it. See [ADR-0007](docs/adr/0007-decide-and-record-not-compute-or-effect.md) for why that boundary is where it is.

## License

[Apache License 2.0](LICENSE).
