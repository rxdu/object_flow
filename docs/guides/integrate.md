# Building an application on ObjectFlow

ObjectFlow holds your business objects and enforces their flow; your application authenticates people, decides who may do what, and calls the engine for everything else. This page takes an application from nothing to a working client, against the engine's service as it runs today. [`wire.md`](wire.md) is the reference for every operation; [`write-a-flow.md`](write-a-flow.md) is how the flows themselves are written.

## 1. What runs today

The engine's core runs as an HTTP service, one request at a time, over one of two stores ([`core/README.md`](../../core/README.md)):

- **a sandbox**, a store held in memory, a clock that moves only when told and evaluators answered by fixed answers: how flows are built and tested, and what the examples below run against;
- **a deployment's store**, given `--store <file>`: one SQLite file that survives a restart, the wall clock, and no sandbox controls (ADR-0174). Its first slice is enough to pilot a flow on one machine, and lacks three things a production deployment needs: an external evaluator is answered unknown, so a guard that asks one is refused `temporal`, until the evaluators' HTTP transport is built; a new engine release cannot yet take over a store an older one created; and it serves one request at a time.

Everything below works against both, but for the sandbox's controls, which a deployment does not route.

Use the core's service. The Python reference runner is the oracle the core is held to; it is reachable only through the Flow Studio's server and does not answer every operation.

## 2. Start the engine and load a flow

```sh
cd core && ~/.cargo/bin/cargo build -p objectflow-service
core/target/debug/objectflow-service --port 8712      # listening on http://127.0.0.1:8712
scripts/.venv/bin/python scripts/check-flows.py people.yaml service.yaml   # check what you load: the load does not
```

```json
POST /sandbox/load
{"files": [{"name": "people.yaml", "text": "…"}, {"name": "service.yaml", "text": "…"}]}

{"loaded": true, "modules": ["people", "service"], "types": ["User", …], "evaluators": [], "now": "2026-01-01T00:00:00Z"}
```

Files go in import order. `/sandbox/reset` empties the store and keeps the modules; `/sandbox/clock` with `{"advance": "2 h"}` moves time; `/sandbox/evaluators` fixes what an external evaluator answers.

A deployment's store starts at version 0, with no flow, and is given one the way every later version is: a `DeclarationChange` drafted with the modules' text, submitted, and published (§7). Its flow is checked as it is published.

## 3. Actors

Every request names who makes it, and the engine records it on the event; it never decides whether that person may (ADR-0114). An actor is an object of a type that marks an actor identity, such as `people.yaml`'s `User` with its `login`, or a type of your own:

```yaml
Employee:
  attributes:
    user_id: { type: identity, actor_kind: human, unique: true }
```

A fresh store knows one actor, `operator`. **Mirror your users into the store**: when a person first appears in your identity provider, request the creation (`Employee.onboard` with their `user_id`) as `operator`, and an agent the same way with an `actor_kind: agent` type. Then name the actor by that identity, or by its object's id:

```json
{"actor": {"id": "alice@corp"}, "type": "Claim", "transition": "draft", "inputs": {…}}
```

An actor the store does not know is refused `unsatisfied`, clause `actor_known`, remedy `dependent`: onboard them first. Whether an actor is a person, an agent or a service is its type's, so every event and metric says truthfully which kind acted.

## 4. Requests and refusals

A request creates an object (`type`) or takes a transition on one (`object_id`), with inputs in the wire's notation (`wire.md` §2: money `"USD 120.00"`, a date `"2025-12-30"`, a reference by its id).

```json
POST /v1/request
{"actor": {"id": "operator"}, "object_id": "claim-4", "transition": "add_line",
 "inputs": {"amount": "USD 120.00", "category": "expensecategory-3", "expense_date": "2025-12-30", "receipt": "taxi.pdf"},
 "expected_version": 1, "idempotency_key": "line-7f3a"}
```

**A refusal is an answer, never an error.** Branch on `kind`, then on `remedy`, which says what can be done (`wire.md` §3):

- `self_serviceable`: show the person what to correct. An `invalid_input` names each input and why (`"reasons": {"amount": "wrong_currency"}`), so a form marks the field; a `stale` means re-read the object and send again.
- `delegable`: someone else must act. An agent hands it to a person, or files it as a proposal.
- `temporal`: retry later.
- `dependent`: another object must change first.
- `unreachable_from_here`: nothing the caller can do; do not retry.

**Send the version you read** (`expected_version`), so a decision made on what the person saw cannot overwrite a change they did not see. **Send an idempotency key** on every write, so a retry after a timeout answers the first result (`"replayed": true`) rather than applying twice.

`/v1/check` answers the verdict a request would get without writing anything, for a confirmation step.

## 5. Reading

- `get` an object: every attribute, derived ones too, a set by its members' ids; `null` if it does not exist.
- `history`: its events, each with who, when, what was sent and why (`context`).
- `availability`: what it offers an actor now, each transition available, available with input, or unavailable with the refusal it would meet. Render the buttons from it; build a form from each offer's input types and `declaration`. An offer marked `actor_dependent` may still be refused for who takes it, as an agent offered a person's confirmation is.
- `query` a type with a filter (`state == Claim.SUBMITTED`, `claim.id == "claim-4"`) for a worklist; `available` lists the objects a transition is available to.
- `metric` a declared metric, whole or by its dimensions. A metric that names a person is listed in its type's `declaration` under `person_dimensions`; your application decides who sees those.
- `subscribe`, then poll `pull` and `acknowledge` to react to events: notifications, timers, integrations.

Pages hold everything today; page in your client. There is no text search: index what `export` or `pull` gives you.

## 6. Agents

An agent acts through the same requests as a person, as an actor of an agent type, and is refused by the same rules. What the engine adds for one:

- `availability` says what it may do now, and why not; `actor_dependent` marks what may be refused for who it is.
- Every refusal's `remedy` is its next move, `delegable` among them where a person must act.
- **"Agents draft, people approve"** is a rule a flow states: record who acted (`assign: { location: confirmed_by, expr: actor.kind }`) and constrain it with an invariant (`confirmed_by == human`), as `reimbursement.yaml`'s `confirmed_by_a_person` does; an agent refused by it is told `delegable`. A transition marked `proposable` lets the agent file the request for a person to approve.
- Tool definitions are written from `declaration` and `availability` today; generating them is decided (ADR-0173 decision 10) and not yet built.

## 7. Changing a flow, and bringing in existing data

A new version is published as a `DeclarationChange`: draft it with the new modules' text, `submit` it to run its dry run, read the report (what it migrates, what it would newly refuse, the objects each reaches), and approve it with `publish` at the version you read. Existing data comes in through `import_batch` into types the version declares `mirror`, which `scripts/mirror-version.py` writes from the flow's module, then a publish of the module that makes them the flow's own; `docs/design/publish-and-import.md` §7 works a go-live through, on a deployment's store. Both are `docs/design/publish-and-import.md`.

## 8. What the engine does, and what your application does

| Concern | The engine | Your application |
|---|---|---|
| Who someone is | records the actor named, with its declared kind | authenticates them, and names them |
| Who may do what | enforces the flow's rules, including rules over who acted that the flow states | decides permissions, roles and segregation of duties beyond what the flow records |
| What happens next | answers what each object offers, and why not | renders it; starts every transition: nothing runs by itself |
| Time | records when, and evaluates time in rules | schedules: timers, reminders and escalations request transitions or read `available` |
| Notifications | gives events through subscriptions | sends the email, the chat message, the push |
| External systems | consults an evaluator a rule names; in a sandbox, a fixed answer | answers it, in a deployment (ADR-0173 decision 2); writes to the other system itself |
| Search | filters by members, with indexes the flow declares | full-text search, over an export or the event feed |
| Files | records a reference to a file | stores, serves and deletes the bytes |
| Reporting | computes declared metrics, exports rows | dashboards and analysis beyond them |
| Personal data | marks what is personal, erases it on request, labels person-naming metrics | who may see it |
