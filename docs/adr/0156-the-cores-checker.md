# ADR-0156: The core's checker

- **Status:** Accepted, 2026-10-05, by the author ("go with your recommendations on all five"), given five recommendations on how the Rust core takes on the publish checks that ADR-0155 decision 3 sequenced after the approval.
- **Date:** 2026-10-05
- **Refines:** ADR-0116 decision 2 (the internal form, kept until the checks run on the model), ADR-0131 decision 2 (versions computed by publishing), ADR-0155 decisions 1 and 3 (a deployment's publish waits for the core's checks; the checker's slices)
- **Relates to:** ADR-0132 (the core in Rust), ADR-0136 (examples), ADR-0137 (the contradiction checker); `design/publish-and-import.md` §1 step 1; `design/declaration-syntax.md` §1, §10; `design/flow-format.md` §7, §11; `DESIGN.md` §5.9

## Context

Publishing installs versions in the core (ADR-0155), and outside a sandbox it is not run until the core checks a source itself. The checks are Python's today, in four layers, measured on 2026-10-05:
- `scripts/check-flows.py`, 4,571 lines, about 1,000 of them its self-test: the strict load, the schema, the expressions, the names and relationships of a module (steps 1 to 3), the conversion to the internal form (step 4), and the runs of steps 5 and 6;
- `scripts/check-syntax-doc.py`, 1,358 lines: the sixty-four checks of `declaration-syntax.md` §10, which step 4 runs over the internal form the conversion writes;
- `scripts/flowlogic.py`, 712 lines: the contradictions within one type (step 5, ADR-0137);
- `scripts/flowrun.py`, the reference runner, which runs each module's examples (step 6, ADR-0136).

The schema, `flow-format/flow.schema.json`, is JSON Schema draft-07 and uses twenty keywords: `$ref`, `type`, `required`, `additionalProperties`, `properties`, `propertyNames`, `const`, `pattern`, `items`, `oneOf`, `minItems`, `not`, `minProperties`, `enum`, `anyOf`, `minLength`, `minimum`, `uniqueItems`, `if` and `then`, beside the annotations `description`, `title`, `$schema`, `$id` and `definitions`. Its eleven patterns are anchored, built of character classes, optional groups and alternation.

ADR-0116 decision 2 keeps the internal form only "until the publish checks run on the declaration model directly", and `TODO.md` recorded on 2026-09-30 that this is the core's work. Element versions live in that form alone: a description carries none, publishing computes them (ADR-0131 decision 2), and objects and events record only the declaration version, which fixes every rule in force (ADR-0077). No requirement of the PRD reads an element's version.

## Decision

1. **The core runs the checks on its declaration model**, never building the internal form: each of the sixty-four checks keeps its number and its meaning, and the core's findings are held to the Python checker's. The core computes no element version. The declaration version, with the source each version records in `of_declaration`, identifies every rule in force; the versions the internal form writes, and check 22's version clauses, are the reference path's and retire with it, once the core's checks cover the sixty-four and the Python path is retired (ADR-0116 decision 2).
2. **The schema stays the one source of the format's structure.** The core validates a description against `flow.schema.json` itself, read from the repository at build time, with a validator of exactly the keywords the schema uses, which refuses a schema using any other, so a change to the schema cannot quietly stop being checked. Its patterns are matched with the `regex` crate, pinned in the workspace, the dependency this decision adds.
3. **The core runs a module's examples against itself**: in an empty in-memory store holding version 0 and the version under publish, with the clock and the id source injected (`publish-and-import.md` §1 step 1), each expectation matched as `flow-format.md` §11.4 states, by a port of the matcher the Flow Studio's replay uses. The studio's replay goes on holding the core to the reference runner.
4. **The oracle is the Python checker, over every module and every plant.** `scripts/check-flows.py` gains an export of each planted mistake of its self-test as files, the module and, for a migration, its previous version, beside the findings it expects. The core is held to the same findings, by file, line and code, over every module of the design and every exported plant, each slice comparing the codes it covers.
5. **The checker comes in four slices**, each joining the dry run's report as fatal findings and notices once it lands, and each free to land in parts: C1, steps 1 to 3, the strict load, the schema, the expressions, the imports, and the names and relationships of a module, with a migration's rules and its checks against the previous version; C2, the sixty-four checks; C3, the contradictions within one type; C4, the examples. When C4 lands, a deployment's dry run and publish run (ADR-0155 decision 1), and a sandbox's run the core's checks too.

## Alternatives rejected

- **For decision 1:** porting the conversion and the text checker as they are, which builds in Rust an internal form ADR-0116 keeps only until this work, and a parser for a notation no one writes; keeping element versions in the core, computed and recorded, which no requirement reads and the declaration version already makes redundant (ADR-0077).
- **For decision 2:** the `jsonschema` crate, a general validator with a much larger dependency tree, for twenty keywords; rewriting the schema's rules into the core's loader, which makes two statements of the format's structure that can drift apart; hand-written matchers for the eleven patterns, which a new pattern in the schema would silently bypass.
- **For decision 3:** the core calling the reference runner, which makes the engine depend on a second runtime (ADR-0132), and tests the runner rather than the engine that will execute the rules.
- **For decision 4:** the design's modules alone, which are all clean and so prove no check fires; plants written afresh in Rust, which would test the port against its own reading of the checks rather than against the reference.
- **For decision 5:** the whole checker in one slice, which would land thousands of lines with no finding compared until the end; publishing outside a sandbox before C4, which installs a source whose examples no one ran.
