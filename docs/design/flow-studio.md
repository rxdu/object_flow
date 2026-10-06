# The Flow Studio

**Status: accepted, 2026-10-04** (ADR-0149, ADR-0150): the author reviewed it and took every recommendation of §9, and approved §8's dependencies. It is the first application built on the engine: a place to write a flow, see it, and run it under triggers you choose, built in this repository as the test case that follows the implementation and as the sample an evaluator of ObjectFlow can run.

## 1. What it is for

The author's words, 2026-10-04: "a flow editing/view/simulation, so that I can define the flows we've went through already, then simulate how states evolve under different triggers. it will help me better understand the behavior of the system we build."

It has three readers, and each sets a condition on it:

- **The author**, understanding the engine's behaviour. The studio shows what the engine decided and why, a refusal's clause and remedy, the intervals a transition opened, the metrics it moved, and never a paraphrase of a rule.
- **The implementation**, which needs a test case that follows it. Every session in the studio is a sequence of the engine's operations, so it can be replayed against each backend, and saved as examples the flow checker runs.
- **A user evaluating ObjectFlow**, who needs a sample to read. The studio is an upper-layer application and uses the engine as any application must (PRD N6), with no privileged path, so its code is an honest example of building on the engine.

It is not an operations application, it publishes nothing to a deployment's store, and it holds no data but a sandbox's.

## 2. Where it sits

The studio has two parts, a web front end and a server, and the server offers three surfaces, kept apart because the difference between them is what the studio tests:

| Surface | What it is | Examples | Whose |
|---|---|---|---|
| **Engine operations** | The library's operations as the service exposes them, one to one (`library-api.md` §6, ADR-0132) | `request`, `get`, `history`, `events`, `availability`, `declaration`, `metric`, and from ADR-0154 `import_batch` and `lookup`, and from ADR-0155 `publish` | the engine's |
| **Sandbox control** | What a sandbox store needs and a deployment's store never offers: loading a sandbox's modules, the clock, the evaluators' answers, resetting | `sandbox/load`, `sandbox/clock`, `sandbox/evaluators`, `sandbox/reset` | the engine's, accepted as F13 (PRD §12) |
| **Studio tooling** | What the studio does with descriptions and examples files, through this repository's scripts | the module catalogue, the diagram, the checker's findings, copying to and saving in the workspace, appending an example the checker holds | the studio's |

Anything the studio needs that fits none of the three is a finding against the engine's interface, recorded in `TODO.md`, and is never met by reaching into a backend.

**Two backends, one wire.** The first backend is a thin server over the reference runner (`scripts/flowrun.py`), so the studio runs before the core does. The second is the Rust core's service, as its slices land. Both speak the same wire, so the front end does not know which it talks to. Which one answers is the server's choice, `backend` in the studio's configuration or `--backend` when it starts: with the core chosen, the server builds the core's service with the pinned toolchain, starts it as its child on a free loopback port, and forwards every `/v1` and `/sandbox` call to it unchanged, status and body, keeping only its own tooling, so a difference between the backends is never the forwarder's. The page reads which backend answers from `GET /studio/backend` and shows it. The runner is the Python oracle of ADR-0132 decision 2, and the server calls it as the flow checker does, adding no rule of its own, so it does not become a second engine. Where the runner reaches a construct it cannot yet run, it raises `NotRunnable` with the construct's name, and the studio shows "not run: <construct>", as the flow checker reports an example not run.

**Replaying a session on both backends** gives two sequences of verdicts, and a difference is a finding against one of them. That is the comparison of the core with the Python reference that `TODO.md` asks for from the core's first slice.

## 3. The panes

### 3.1 The editor

A module's YAML, edited as text, re-checked as it changes: the flow checker's findings appear against their lines (`flow-format.md` §11.5), and the diagram follows the text. Text first, because a description is "clearly described and version controlled" (PRD F10), and a graphical editor that writes YAML would lose its comments and its order. Saving writes the file, and loading it into the sandbox is a separate act, so a half-written module never replaces a running one.

**Where it writes** (decided 2026-10-04, building slice 3). The studio edits flows in a **workspace**, `apps/flow-studio/flows/` by default (`workspace` in `apps/flow-studio/config.json`), and never the design's example modules, which are the corpus the repository's checkers hold the design to. A design module is copied into the workspace to be edited, its examples file with it, and a copy is never overwritten. The workspace is committed, so the author's flows are version-controlled as PRD F10 asks. A module the workspace holds is used before the design's of the same name wherever another module imports it. The server writes only by a plain file name inside the workspace.

### 3.2 The viewer

- **The lifecycle diagram** of each type, in the notation of `scripts/flow-diagram.py`, UML's state machine notation, so the studio's diagram and the documents' are the same drawing. In the first slice the server draws it from the module with that script, as studio tooling. Drawing it from the engine's `declaration`, so that it shows the version the sandbox runs, is later work, and the test that the two drawings agree comes with it.
- **The rule set**, as `renderers.md` §2 lays it out, so a guard is read as written, never restated (`renderers.md` §1).
- **Each object**: its state, its attributes, its tracked members, and its history, every event with its actor, kind of actor, occurred and recorded times and cause.

### 3.3 The simulator

Objects in a sandbox store, and the triggers of §4. For the object selected, the **live offer** (`renderers.md` §4): every transition it may take now, and for every other, the clause that blocks it and its remedy. A refused request shows its verdict, the clause, the remedy and the objects it names (`DESIGN.md` §5.5, §6). A metrics strip shows the type's metrics, read again after every step.

## 4. Triggers

A trigger is one of the examples format's own steps (`flow-format.md` §11.3), so the studio and an examples file share a vocabulary:

- **a request** by a chosen actor, the operator or an actor object, with inputs and, optionally, an occurred time;
- **a clock advance**, an `after`;
- **an evaluator's answer**, a stub, for a guard that asks an external evaluator.

Importing legacy data and publishing a new version, to watch a migration, are later slices (§6). Time never runs on its own: a session is repeatable only if every change of the clock is a step.

## 5. Sessions and examples

A session is everything done to the sandbox since it was reset, and is saved as one example of the module (`flow-format.md` §11.4): its steps, the evaluators' answers, the clock as each step's `after`, and its last free request as the request under test. An example claims what a request must do, in its author's words, so saving does not turn every outcome into an expectation. The studio proposes the expectation it observed, the verdict with its clause and remedy, or the state, and the author confirms it and writes the description. A saved file is checked by `scripts/check-flows.py` like any other, which is how an exploration becomes a test.

## 6. Slices

1. **Load and step.** Choose one of the eleven example modules in `docs/design/flow-format/examples/` and its examples file, see each type's diagram and the rule set, and step through one example a step at a time, seeing after each step the verdict, every object's state and the history. The server offers `request`, `get`, `history` and `declaration` over the runner, and sandbox load, clock, evaluators and reset. **Done when** every example in the ten examples files, 163 on 2026-10-04, replays through the studio's wire with the verdict the flow checker gives it, an example the runner cannot run being "not run" in both, checked by a test that drives the wire, not the runner. **Done, 2026-10-04**, in `apps/flow-studio/server/` and `apps/flow-studio/web/`. The server's replay test passes 161 of the 163 examples through the wire with the reference runner's verdicts and fails none; the page's own player, run against the real server by the front end's test, gives the same. The other two use an import step, and wait for importing (§6, later). The 43 that first waited on two findings against the interface pass since ADR-0151 decided them: an `events` operation, through which a caller learns what its request created and caused, and an invariant failure that names its object's type.
2. **Free simulation.** Fire any transition on any object as any actor, with the live offer and refusals in full. It needs `availability`, which the runner does not yet offer and which is written into the runner first, as the library states it, since the runner is the oracle the core is compared with. A metrics strip. **Done, 2026-10-04.** What an offer decides before its inputs are given was unstated, and ADR-0152 states it. The runner offers it, each rule proven by a self-test that fails with the rule reverted. The server adds `availability` and `metric` to the wire. The page shows the live offer for any object and takes what it offers through a form built from the offer's input schema, creates objects of any loaded type, acts as any actor the session holds, moves the clock, answers the evaluators, logs the session's requests, and reads the type's metrics after every step. Two limits remain. A metric is read over every row with nothing grouped, since the runner reads a metric as a guard does, and a grouped read is reported as not run. And an offer lists recording but not labelling, which the runner does not yet offer.
3. **Edit.** The editor of §3.1, with live findings and diagram, reloading the sandbox from the edited module, and saving a session as examples (§5). **Done, 2026-10-04.** The page edits a module in CodeMirror, the checker's findings marked against their lines after a pause in typing, the diagram drawn from the text. Saving to the workspace and loading into the sandbox are separate, and loading is disabled while the checker finds the text wrong. A session is drafted from everything done since the sandbox was reset. The draft names objects as the example's aliases do, or by type, carries the clock as `after`, merges the evaluators' answers, leaves out a refused request among the steps, and proposes the expectation from what the last request did. It is appended to the module's examples file only once the flow checker runs it and finds it holds. Building it found D517, a flow checker rule that contradicted the order of refusals.
4. **The Rust backend.** The same sessions against the core's service as its slices land, and a report of where the two backends' verdicts differ. **Begun 2026-10-04** with the core's first slice (ADR-0153). The replay plays every example against the core's service. 21 pass with the runner's outcome and none fails; the rest are not run, each naming what it reaches. A ratchet keeps what passes. **The page switches backends from 2026-10-06** (§2): the configuration names the core, and the front end's own tests play every example, a free session and a session saved as an example through each backend, a launcher refusing a backend other than the one asked for. Both backends answer a sandbox's load with the same types, modules and evaluators for every design module, held by a test; the core had listed its built-in types, sorted, where the runner lists the modules' own types in the order declared.

Later: publishing a second version and watching its migration and impact report, `publish-and-import.md` §1 to §3 being the reference; importing; proposals and their approval.

## 7. Layout and stack

```
apps/flow-studio/
  web/        the front end: TypeScript, strict; Vite, Vitest, pnpm, Tailwind
  server/     the backend: the wire, the sandbox control, the studio tooling
```

- **The server** is Python's standard library HTTP server over the runner, the checker and the diagram script, which it imports from `scripts/`, or, with the core chosen, a forwarder of the wire to the core's service, which it starts and stops (§2). It adds no Python dependency: PyYAML and jsonschema are already the checkers'. It listens on `127.0.0.1` only, on a port set explicitly in one configuration file, and the front end's development server's port likewise, never a default. It serves one user and holds only a sandbox store, so it has no authentication.
- **The wire** carries JSON, one `POST /v1/<operation>` per library operation, values in JSON where it keeps them exactly and otherwise in the model's notation, sandbox control under `/sandbox/` and the studio's tooling under `/studio/` (ADR-0150).
- **Tests.** The front end's with Vitest. The server's with pytest, the slice 1 replay test among them. A single `check` script for each part runs typecheck, lint and tests, as the author's web conventions ask.

## 8. Dependencies

Approved by the author on 2026-10-04 (ADR-0150 decision 4), each pinned exactly in `apps/flow-studio/web/package.json` and `server/requirements-dev.txt`, with `@types/node` for the build's configuration. pnpm runs through Node's `corepack`, pinned at 11.28.2, since pnpm 12 ships a native binary the installed corepack 0.33 cannot start. TypeScript is pinned at 6.0.3, the newest typescript-eslint accepts. pytest runs in a virtual environment that sees the system interpreter's PyYAML and jsonschema, which the checkers use.

| Dependency | For | Slice |
|---|---|---|
| `typescript`, `vite`, `vitest`, `eslint`, `prettier`, `tailwindcss` | the toolchain the author's web conventions name | 1 |
| `react`, `react-dom` | the user interface (question 2 of §9) | 1 |
| `mermaid` | drawing the diagram, in the notation the documents already use | 1 |
| `@codemirror/*` | the YAML editor, with findings against lines | 3 |
| `pytest` | the server's tests, in a virtual environment | 1 |

## 9. Questions the author decided

All three on 2026-10-04, as recommended (ADR-0150). Two more, from slice 1, were decided the same day as recommended (ADR-0151): a caller reads the events its request appended through `events`, and an invariant failure names its object's type.

1. **The wire's shape.** ADR-0132 says the service exposes the library one to one, and no OpenAPI description exists yet. I recommend the literal reading: one `POST /v1/<operation>` per library operation, its arguments a JSON object with the library's names, its result the library's shape as JSON, a refusal a 200 with the verdict, since a refusal is a value (`library-api.md` §2), and sandbox control under `/sandbox/`, which a deployment's service does not route. The slice 1 operations would be the first entries of the OpenAPI description. The alternative is resource paths (`GET /objects/{id}`, `POST /objects/{id}/transitions/{name}`), more conventional for HTTP, but a second naming of the same operations to keep in step with the library.
2. **The user interface library.** I recommend React: the sample is meant to be read by people building on ObjectFlow, and React is the one most of them read. The alternative with the fewest dependencies is plain TypeScript with no framework, which is lighter but makes the three panes' state harder to follow.
3. **The PRD's proposed requirement and use case** (PRD §12): a sandbox store with a clock the caller sets and evaluators the caller answers. The engine needs it for the studio to run on the Rust core, and no requirement states it.
