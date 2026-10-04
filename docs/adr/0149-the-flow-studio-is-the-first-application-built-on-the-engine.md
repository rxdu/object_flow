# ADR-0149: The Flow Studio is the first application built on the engine

- **Status:** Accepted, 2026-10-04, by the author ("go with your recommendations on all five"), given the five recommendations below as the answers to five questions; the PRD's wording of decision 1 is revision 21, drafted for the author's review, and the studio's design is `design/flow-studio.md`, proposed for review before any code.
- **Date:** 2026-10-04
- **Refines:** ADR-0015 (the operations platform is no longer the first application built on the engine, and no rebuild of it is planned here), ADR-0132 (the studio drives the engine through the service's interface, and is how the Rust core's first slices are compared with the Python reference)
- **Relates to:** PRD revision 21 and §12; `design/flow-studio.md`; `DESIGN.md` §4; TODO "Run the same flows through the Rust core and the Python checkers"

## Context

The author asked for a smaller application than the operations platform, built in this repository, "on one hand, as the test case along the implementation, on the other hand, as the sample use for users who may use the object_flow app": a flow editor, viewer and simulator, "so that I can define the flows we've went through already, then simulate how states evolve under different triggers. it will help me better understand the behavior of the system we build".

Until now the record named one first consumer, the author's operations platform, and treated it in two roles at once: the case study whose production code and requirements are the design's evidence (ADR-0015, PRD §5), and the first deployment, to be rebuilt on the engine with its data ported. The second role set the PRD's success measures (§8, "the first consumer runs on the engine"), part of its release order (§10) and the cutover tasks in `TODO.md`.

What the studio can build on already exists: eleven example modules with examples files (`docs/design/flow-format/examples/`), the examples format, whose steps are requests by an actor with inputs and an occurred time, clock advances and stubbed evaluator answers (`design/flow-format.md` §11), the reference runner that executes them (`scripts/flowrun.py`), the flow checker (`scripts/check-flows.py`), the diagram script (`scripts/flow-diagram.py`), and the library's nineteen operations, among them `request`, `availability`, `history`, `declaration` and `metric` (`design/library-api.md` §6).

## Decision

1. **The Flow Studio is the first application built on the engine; the operations platform stays the case study.** The platform's requirements and production code remain the design's evidence, and its flows are among those the studio loads, but it is not a target of this repository and no switch-over from its legacy system is planned. The record keeps the name "first consumer" for the platform, defined in the PRD's glossary as the case study. The PRD's measures of success and its release order are restated in revision 21. The cutover tasks are deferred, not withdrawn, since import (N5) stays a requirement for any deployment that moves from a legacy system.

2. **The studio reaches the engine through one service boundary, and the backend behind it changes.** First a thin server over the Python reference runner, so the studio works before the core exists, then the Rust core's service as its slices land. The studio uses the engine's operations only, as the service exposes them (ADR-0132). What it needs that is not an operation is either sandbox control, the clock, the evaluators' answers and loading a sandbox's modules, which no deployment's service offers, or the studio's own tooling, the diagram, the checker's findings and the examples file. Anything else it needs is a finding against the interface, recorded, never a private hook. A session replayed on both backends compares their verdicts, which is the comparison ADR-0132 decision 2 calls for. Where the runner cannot run a construct, the studio says "not run" and names it, as the runner does, and never guesses.

3. **Editing is text first.** A module's YAML is edited as text, re-checked as it changes, with the checker's findings and a diagram that follows the text. The diagram can be clicked, to fire a transition or read a guard, but it never writes the description.

4. **A trigger is a request, a clock advance or an evaluator's answer.** A request is made by a chosen actor, with inputs and an optional occurred time. Those three are the examples format's own steps, so a session can be saved as examples and checked like any other. Importing legacy data and publishing a new version, to watch a migration, come in later slices.

5. **The studio lives in `apps/flow-studio/`**, as a TypeScript web front end following the author's web conventions (Vite, Vitest, pnpm, Tailwind) and a backend behind the boundary of decision 2. Its dependencies are named in `design/flow-studio.md` for the author's approval before any is added.

## Alternatives rejected

- **For decision 1:** keeping the operations platform as the first application, which the author declined; dropping the platform altogether, which would lose the design's only production evidence; renaming "first consumer" across the record, 375 occurrences in 92 files on 2026-10-04 (`grep -rio "first consumer" docs README.md TODO.md scripts`), ADRs among them, which would rewrite history to fix a name.
- **For decision 2:** waiting for the Rust core, which would leave the author with nothing to run for weeks; calling the runner's functions from the studio directly, which would make the studio a second user of the oracle's internals rather than a user of the interface, and leave nothing to swap when the core lands.
- **For decision 3:** graphical editing that writes YAML, which loses comments and layout and is a project of its own; a form per construct, which would restate the format's rules in the front end, the restatement ADR-0148 removed from the documents.
- **For decision 4:** time running on its own, which makes a session unrepeatable and cannot be saved as an example.
- **For decision 5:** a Python-only terminal or notebook interface, quicker to build but weak at diagrams and step-by-step interaction; a separate repository, which would put the sample out of step with the engine it shows.

## Consequences

- PRD revision 21 states decision 1. Its §12 proposes the requirement and use case the studio needs and no row states, a sandbox store with a clock the caller sets and evaluators the caller answers.
- `DESIGN.md` §4 and the README say which application is built first. The cutover tasks in `TODO.md` are marked deferred.
- `design/flow-studio.md` gives the studio's design, its slices and the dependencies awaiting approval. The first slice loads an example module, draws its diagram and steps through one of its examples against the runner. It is done when every example of every module replays through the studio's wire with the verdicts the flow checker gives.
