# Flow Studio

An editor, viewer and simulator of flows, the first application built on ObjectFlow: the test case that follows the engine's implementation, and a sample of building on it. Its design is [`docs/design/flow-studio.md`](../../docs/design/flow-studio.md); its decisions are ADR-0149 and ADR-0150.

It talks to the engine only through the engine's operations, `POST /v1/<operation>`, and a sandbox's controls, `POST /sandbox/<control>`, so the backend behind it can change: the Python reference runner, or the Rust core.

## Running it

Once the server's environment and the front end's packages are in place (below):

```sh
apps/flow-studio/scripts/dev          # the backend config.json names
apps/flow-studio/scripts/dev runner   # or name one: runner or core
```

It starts the studio's server, waits until it answers, which takes longer the first time the core is built, prints the page's address, `http://127.0.0.1:<web_port>`, and runs the front end's development server. Ctrl-C stops both, and the core's service with them. The page's header says which backend answers.

## Testing it by hand

Each walk below was driven in a browser against both backends on 2026-10-09, with no page error, and says what each step should show, so a difference is a finding. Start each from a fresh page, or *Reset* under *Simulation*, which empties the sandbox. Where the backends word a step differently, both are given.

1. **An example.** Choose `service.yaml`, then the example `a_job_is_not_given_to_an_engineer_who_left`, and *Run to the end*, or *Step* through it. Each setup step reads APPLIED and *Applied.*; the request under test reads REFUSED, *engineer_active does not hold: supply or correct an input and try again.*; the panel ends *Does what the example claims.* `a_job_opens_for_an_active_engineer` ends the same way with its request APPLIED. The type tabs (`InspectionCheck`, `ServiceJob`) switch the diagram, the rule set and the metrics.
2. **Free play.** *Reset*; under *Create* choose `User`, give a login and a name, and *Request*: *This session* reads *User.add Applied.* Choose `ServiceJob`, pick the user as its engineer, and *Request*. Choose the job under *Objects*: *What servicejob-2 offers* marks each transition available, available with input, or unavailable, and `finish` reads *Not available in OPEN; it is taken from another state.* *Take* `start`, which applies at once, as it takes no input; then *Take* `finish` and *Request* with the form left empty: refused, *The input signed_off_by is not valid: supply or correct an input and try again.* on the core, *An input is not valid: …* on the runner, which does not work out which input and says so on the wire. On the `ServiceJob` tab the metrics read `null` where a metric has no value yet, such as an average over no time worked. *acting as* chooses who sends each request, and *Advance* moves the sandbox's clock by the duration given: the `now` beside it and the *Simulation* panel's both read the new time, `2 h` taking `2026-01-01T00:00:00Z` to `2026-01-01T02:00:00Z`, and a request sent after it is stamped so in its object's history.
3. **A proposal.** Choose `delivery.yaml`, then the example `an_approved_delivery_is_handed_over`, and *Step* six times, which applies its setup and leaves its request under test next. Choose the delivery under *Objects*: `complete` offers *Take* and *Propose*. *Propose* files it at once, as it takes no input: `proposal-5 Proposal PENDING` under *Objects*. Choose the proposal, which offers `approve`, `reject` and `withdraw`, and *Take* `approve`: *approve on proposal-5 Applied.*, the delivery DELIVERED and the proposal EXECUTED. A proposal filed before the delivery's checklist is ticked, after `opening_creates_the_checklist`, is refused on approval: *checklist_complete does not hold: another object must change first.*
4. **An import.** Choose `servicedesk.yaml`; under *Import* choose `Employee`, the one mirror type, which *Create* does not list; give a legacy key, `E-17`, and its `employee_id`, and *Import*: *import Employee as E-17 imported*, and `E-17 Employee EMPLOYED` under *Objects*.
5. **A publish.** With `service.yaml` loaded, a user created and a job opened for them (walk 2), choose `service-v2.yaml (design)` under *Publish a version* and *Dry run*. The report reads *service-v2.yaml as version 2: it may be approved*, then three notices, `check:migration`, `check:warn` and `mapping:backfill:ServiceJob.site`, the last reaching one object, the job; then the invariant `ServiceJob.photo_when_done · local`, *could be broken by ServiceJob.finish, ServiceJob.open, ServiceJob.start*, and the six metric dimensions naming a person. *Publish* installs it: *Installed; migrated, each with its migration in its history*, listing the job, whose history, on its choice, ends in a `migrate` row from OPEN to OPEN. *Withdraw* instead leaves the sandbox as it was.
6. **An edit.** Choose `service.yaml` and *Edit*: *the design's module, read-only · the checker finds nothing wrong*. *Copy to the workspace to edit*: `service.yaml` appears under *Your workspace*; choose it there and *Edit*. Change a `from: OPEN` to `from: NOPE`: after a pause the status reads *1 finding*, the line is marked once scrolled into view, and *Load into the sandbox* is disabled. Put it right, then *Save*, *Saved service.yaml to the workspace.*, and *Load into the sandbox*, *Loaded into a fresh sandbox.*
7. **A session saved as an example.** With the workspace's module loaded (walk 6), create a user in free play; under *Save the session as an example* describe it, *Draft it*, which shows the claim drafted from what the last request did, and *It should: save it*: *Saved to service.examples.yaml: the flow checker ran it, and it holds.*

Walks 6 and 7 write `service.yaml` and `service.examples.yaml` into the workspace, [`flows/`](flows/), which git does not track until they are added; delete them afterwards unless they are to be kept.

## The backends

`backend` in [`config.json`](config.json) names the one the server starts with, and `--backend` overrides it:

- **`core`**, the Rust core's service. The server builds it with the toolchain `core/rust-toolchain.toml` pins, starts it as its child on a free loopback port, and forwards every `/v1` and `/sandbox` call to it unchanged, but for a load, which the flow checker checks first as it does for the runner; the editor, the checker's findings and the diagrams stay the server's. It needs `~/.cargo/bin/cargo`, from rustup, which installs the toolchain the file pins on first use.
- **`runner`**, the Python reference runner, the oracle the core is held to (ADR-0132 decision 2).

## The server

Python's standard library over the repository's reference runner and flow checker, in a virtual environment holding the versions of PyYAML and jsonschema the scripts are pinned to (`scripts/requirements-dev.txt`), and pytest.

```sh
cd apps/flow-studio/server
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
.venv/bin/python -m flowstudio          # listens on 127.0.0.1, port from ../config.json
.venv/bin/python -m pytest              # the wire's tests, and every example replayed through it
```

The replay test plays every example of the design's example modules through the wire, as a caller would, and holds each to the outcome the flow checker gives it, against the runner and against the core. A second test plays them through both backends at once and compares every answer whole; `tests/backend_differences.txt` lists where they are known to differ and why. An example that needs what the interface does not yet offer is reported as waiting, with the reason, never passed (ADR-0150, Consequences).

## The front end

TypeScript and React, built by Vite; pnpm, at the version `package.json` pins, through Node's corepack.

```sh
cd apps/flow-studio/web
corepack pnpm install
corepack pnpm check                     # typecheck, lint, format and tests
```

Its tests play every example, a free session and a session saved as an example through the real server, once with each backend.

## Editing

The studio edits flows in its workspace, [`flows/`](flows/), and never the design's example modules, which the repository's checkers hold the design to; a design module is copied there to be edited. A session in the sandbox can be saved as an example of a workspace module, written only once the flow checker runs it and finds it holds (`docs/design/flow-studio.md` §3.1, §5).

## Ports

Both are set in [`config.json`](config.json), and neither is a default: the server's, and the web front end's, which the server allows as the one origin that may call it. The core's service, a child of the server, takes a free loopback port.
