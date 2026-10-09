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

**Two backends, one wire.** The first backend is a thin server over the reference runner (`scripts/flowrun.py`), so the studio runs before the core does. The second is the Rust core's service, as its slices land. Both speak the same wire, so the front end does not know which it talks to. Which one answers is the server's choice, `backend` in the studio's configuration or `--backend` when it starts: with the core chosen, the server builds the core's service with the pinned toolchain, starts it as its child on a free loopback port, and forwards every `/v1` and `/sandbox` call to it unchanged, status and body, keeping only its own tooling, so a difference between the backends is never the forwarder's. The one exception is a load, which the server checks with the flow checker first, as it does for the runner (ADR-0153 decision 5), so the core never loads what the checker refuses, and the answer carries the checker's notices. The page reads which backend answers from `GET /studio/backend` and shows it. The runner is the Python oracle of ADR-0132 decision 2, and the server calls it as the flow checker does, adding no rule of its own, so it does not become a second engine. Where the runner reaches a construct it cannot yet run, it raises `NotRunnable` with the construct's name, and the studio shows "not run: <construct>", as the flow checker reports an example not run.

**Replaying a session on both backends** gives two sequences of verdicts, and a difference is a finding against one of them. That is the comparison of the core with the Python reference that `TODO.md` asks for from the core's first slice.

## 3. The panes

### 3.1 The editor

A module's YAML, edited as text, re-checked as it changes: the flow checker's findings appear against their lines (`flow-format.md` §11.5), and the diagram follows the text. Text first, because a description is "clearly described and version controlled" (PRD F10), and a graphical editor that writes YAML would lose its comments and its order. Saving writes the file, and loading it into the sandbox is a separate act, so a half-written module never replaces a running one.

**Where it writes** (decided 2026-10-04, building slice 3). The studio edits flows in a **workspace**, `apps/flow-studio/flows/` by default (`workspace` in `apps/flow-studio/config.json`), and never the design's example modules, which are the corpus the repository's checkers hold the design to. A design module is copied into the workspace to be edited, its examples file with it, and a copy is never overwritten. The workspace is committed, so the author's flows are version-controlled as PRD F10 asks. A module the workspace holds is used before the design's of the same name wherever another module imports it. The server writes only by a plain file name inside the workspace.

### 3.2 The viewer

- **The lifecycle diagram** of each type, in the notation of `scripts/flow-diagram.py`, UML's state machine notation, so the studio's diagram and the documents' are the same drawing. In the first slice the server draws it from the module with that script, as studio tooling. After a publish in free play it is drawn from the file declaring the type in the version installed, which is the version the sandbox runs (§6); drawing it from the engine's `declaration` is later work, and the test that the two drawings agree comes with it.
- **The rule set**, as `renderers.md` §2 lays it out, so a guard is read as written, never restated (`renderers.md` §1).
- **Each object**: its state, its attributes, its tracked members, and its history, every event with its actor, kind of actor, occurred and recorded times and cause.

### 3.3 The simulator

Objects in a sandbox store, and the triggers of §4. For the object selected, the **live offer** (`renderers.md` §4): every transition it may take now, and for every other, the clause that blocks it and its remedy. A refused request shows its verdict, the clause, the remedy and the objects it names (`DESIGN.md` §5.5, §6). A metrics strip shows the type's metrics, read again after every step.

## 4. Triggers

A trigger is one of the examples format's own steps (`flow-format.md` §11.3), so the studio and an examples file share a vocabulary:

- **a request** by a chosen actor, the operator or an actor object, with inputs and, optionally, an occurred time;
- **a clock advance**, an `after`;
- **an evaluator's answer**, a stub, for a guard that asks an external evaluator;
- **an import** of a legacy object into a mirror type, an example's `import` step (§6);
- **a publish** of another version over the sandbox's objects, a module of the catalogue or the editor's text, its dry run's report read before it is approved, an example's `publish` step (§6, ADR-0166).

Time never runs on its own: a session is repeatable only if every change of the clock is a step. The page reads the clock from the session, once, and shows it beside free play's *Advance* and in the simulator's panel, which therefore agree; until 2026-10-09 the simulator's panel kept its own reading, which an advance in free play left behind.

## 5. Sessions and examples

A session is everything done to the sandbox since it was reset, and is saved as one example of the module (`flow-format.md` §11.4): its steps, the evaluators' answers, the clock as each step's `after`, and its last free request as the request under test. An example claims what a request must do, in its author's words, so saving does not turn every outcome into an expectation. The studio proposes the expectation it observed, the verdict with its clause and remedy, or the state, and the author confirms it and writes the description. A saved file is checked by `scripts/check-flows.py` like any other, which is how an exploration becomes a test.

## 6. Slices

1. **Load and step.** Choose one of the eleven example modules in `docs/design/flow-format/examples/` and its examples file, see each type's diagram and the rule set, and step through one example a step at a time, seeing after each step the verdict, every object's state and the history. The server offers `request`, `get`, `history` and `declaration` over the runner, and sandbox load, clock, evaluators and reset. **Done when** every example in the ten examples files, 163 on 2026-10-04, replays through the studio's wire with the verdict the flow checker gives it, an example the runner cannot run being "not run" in both, checked by a test that drives the wire, not the runner. **Done, 2026-10-04**, in `apps/flow-studio/server/` and `apps/flow-studio/web/`. The server's replay test passes 161 of the 163 examples through the wire with the reference runner's verdicts and fails none; the page's own player, run against the real server by the front end's test, gives the same. The other two use an import step, and wait for importing (§6, later). The 43 that first waited on two findings against the interface pass since ADR-0151 decided them: an `events` operation, through which a caller learns what its request created and caused, and an invariant failure that names its object's type.
2. **Free simulation.** Fire any transition on any object as any actor, with the live offer and refusals in full. It needs `availability`, which the runner does not yet offer and which is written into the runner first, as the library states it, since the runner is the oracle the core is compared with. A metrics strip. **Done, 2026-10-04.** What an offer decides before its inputs are given was unstated, and ADR-0152 states it. The runner offers it, each rule proven by a self-test that fails with the rule reverted. The server adds `availability` and `metric` to the wire. The page shows the live offer for any object and takes what it offers through a form built from the offer's input schema, creates objects of any loaded type, acts as any actor the session holds, moves the clock, answers the evaluators, logs the session's requests, and reads the type's metrics after every step. It shipped with two limits, both gone: the runner backend reads a metric grouped since 2026-10-07, as the core does, the strip reading each metric whole on either, and an offer lists labelling since 2026-10-05.
3. **Edit.** The editor of §3.1, with live findings and diagram, reloading the sandbox from the edited module, and saving a session as examples (§5). **Done, 2026-10-04.** The page edits a module in CodeMirror, the checker's findings marked against their lines after a pause in typing, the diagram drawn from the text. Saving to the workspace and loading into the sandbox are separate, and loading is disabled while the checker finds the text wrong. A session is drafted from everything done since the sandbox was reset. The draft names objects as the example's aliases do, or by type, carries the clock as `after`, merges the evaluators' answers, leaves out a refused request among the steps, and proposes the expectation from what the last request did. It is appended to the module's examples file only once the flow checker runs it and finds it holds. Building it found D517, a flow checker rule that contradicted the order of refusals.
4. **The Rust backend.** The same sessions against the core's service as its slices land, and a report of where the two backends' verdicts differ. **Done 2026-10-06; begun 2026-10-04** with the core's first slice (ADR-0153). The replay plays every example against the core's service. 21 pass with the runner's outcome and none fails; the rest are not run, each naming what it reaches. A ratchet keeps what passes. **The page switches backends from 2026-10-06** (§2): the configuration names the core, and the front end's own tests play every example, a free session and a session saved as an example through each backend, a launcher refusing a backend other than the one asked for. Both backends answer a sandbox's load with the same types, modules and evaluators for every design module, held by a test; the core had listed its built-in types, sorted, where the runner lists the modules' own types in the order declared. **The report of where the backends differ is done the same day**: a test plays every example through both and compares every answer whole, less what either names as not computed, and a ratchet lists each known difference with the `TODO.md` entry accounting for it, so a new one fails. Its first run found 2,542 differences over 1,855 calls; fixing whichever side the record backs left 61, in two classes the reference runner had yet to catch up with, proposals as objects and an erasure's walk down, both done on 2026-10-07, after which the backends answer every example alike and the ratchet lists no difference (`TODO.md`, slice 4).

**Publishing in free play** (2026-10-08, ADR-0166 decision 4's last slice). Free play publishes another version into the running sandbox as a publish installs one (`publish-and-import.md` §1, §2): a module of the catalogue, the version being its bundle, its imports with it, or the editor's text, which the editor's *Publish into the sandbox* sends with the module's imports. A change is drafted with those files, each keyed by the name it declares, and submitted, which runs its dry run, and the report is shown before anything is installed: the version, whether it may be approved, each finding with its severity and the number of objects it reaches, a submission its dry run refused with what refused it, and what the version declares that its reviewer reads: each invariant with its form and the transitions that could break it, and each metric dimension that names a person (`publish-and-import.md` §2; shown since 2026-10-09). *Publish* approves the change at the version read, which migrates the session's objects, and lists each object a mapping, an admission or a discharge reached, each of which opens its history, its migration in it; *Withdraw* leaves the sandbox as it was. A publish that applies is logged and drafted as an example's `publish` step of the files given (`flow-format.md` §11.3), its clock carried as the step's `after`; a session publishing the editor's unsaved text is refused as an example, since no file beside the examples file holds that version. The session's tests publish `service-v2.yaml` over a job in progress through each backend, see `finish` refused for want of the photo, and save the session as an example the checker runs and finds to hold, the save's check reading the published files from the catalogue; they also show a session publishing unsaved text refused as an example. The panel was used by hand on the page, against the core, a catalogue module and the editor's text, with no page error. After a publish, the page shows the version installed until the sandbox is reset or reloaded: the types it lists and offers for creation are that version's, as the checker names them over its files, each type's diagram is drawn from the file declaring it there (§3.2), and its rule set is read from the engine's declaration, which the reference runner's sandbox gave as the loaded version's until D543. Used by hand on the page against each backend: `photo_attached` reads `audit` before the publish, `deny` after it, and `audit` again after a reset.

**Importing in free play** (2026-10-07). Free play imports an object of a mirror type as an example's `import` step does (`flow-format.md` §11.3): its legacy key, which later steps name it by, the state it arrives in, and its values in the model's notation, an integer, a truth value and an enumeration member typed from its declaration and a reference given by the legacy key of an object imported before it. An import the type cannot hold is refused, naming why, and leaves nothing in the session; one that applies is logged and drafted as that step. A mirror type is offered there and never under *Create*, since a mirror takes no transition but an erasure (`flow-format.md` §4.15; until 2026-10-09 *Create* listed it and showed nothing on its choice). The session's tests import `servicedesk.yaml`'s employee through each backend and save the session as an example the checker holds; the form was used by hand on the page, against the core, an import and a refused one.

**Proposals in free play** (2026-10-07). An offer of a transition marked proposable can be proposed as well as taken, whatever it offers now, since filing checks neither the state nor the guards (`DESIGN.md` §9); a creation marked proposable can be filed as a proposal from the create form. The proposal is an object of the session, and what it offers, its decisions, is taken as any offer is (D535). A session drafts a filing as a `propose` step naming its proposal and a decision as an `approve`, `reject` or `withdraw` step, an approval last being the example's `approve` and its expectation the state it leaves the proposal in; one ending in a filing, a rejection or a withdrawal is refused as an example, which tests a request or an approval (`flow-format.md` §11.4). The session's tests file and approve `delivery.yaml`'s hand-over through each backend and save it as an example the checker holds; the offer's Propose and the decisions were taken by hand on the page, against the core.

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
