# ADR-0172: The internal form stays as the reference checker's path

- **Status:** Decided by the author, 2026-10-09 ("go with your recommendations"), on a recommendation stated with the question.
- **Date:** 2026-10-09
- **Refines:** ADR-0116 decision 2 (the text language kept as the internal form until the checks run on the model), ADR-0156 decision 1 (the internal form's versions retiring with the Python path)
- **Relates to:** ADR-0131, ADR-0132, ADR-0156 decision 4 (the oracle); `design/core-checker.md` §2; `design/declaration-syntax.md` §10; `TODO.md`

## Context

ADR-0116 retired the text language as a written form and kept it "only as the internal form the conversion produces and the checker reads", until the publish checks run on the declaration model directly. ADR-0156 then had the core run the checks on its own model, never building the internal form, and said the internal form's versions, and check 22's version clauses, "retire with it, once the core's checks cover the sixty-four and the Python path is retired".

On 2026-10-09 the core's checks cover the sixty-four: every check of `declaration-syntax.md` §10 is enforced in both checkers, subsumed or withdrawn (`core-checker.md` §2). The core reads the model and never builds the form, so the condition ADR-0116 set is met on the core's side.

The internal form has two users left, both in the reference: the flow checker's step 4, which converts a module and runs the text checker over it (`scripts/check-flows.py`, `scripts/check-syntax-doc.py`), and the specification's own examples, written in it (`declaration-syntax.md`, 39 blocks). Retiring it now would mean retiring the reference's step 4, which is the oracle the core's step 4 is held to (ADR-0156 decision 4).

## Decision

1. **The internal form stays, as the reference checker's path and the specification's notation.** No one writes it, reviews it or is shown it, as ADR-0116 decision 2 has it; the reference checker goes on converting a module into it for step 4, and the specification goes on stating the model in it. It retires only if the reference checker does, which is not planned.

2. **The core's half of the retirement is done.** The publish checks run on the declaration model in the core, which builds no internal form and computes no element version. The notation checks, 22's version clauses, 38, 42's version and 51, stay the reference's alone, as the versions the internal form carries do.

## Alternatives rejected

- **Port the reference's step 4 to read the YAML**, retiring the conversion. It is a port of every check the text checker enforces, and it would leave the two implementations reading the same representation; today the reference reads the text form and the core the YAML, so they are independent down to what they read, which is what makes the oracle worth having.
- **Drop the reference's step 4**, the core's checks the only ones. The core's sixty-four checks would lose the oracle that held each of them, the recorded fixtures left as a record of what they once agreed on rather than a second implementation.

## Consequences

- ADR-0116 and ADR-0156 name this decision where they spoke of the form's retirement; `core-checker.md` §2 says a notation check is the reference's alone, and `DESIGN.md`'s glossary says the same of type versions.
- `TODO.md`'s item to run the checks on the model and retire the form is done, by this decision.
