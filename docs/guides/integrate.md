# Building an application on ObjectFlow

ObjectFlow holds your business objects and enforces their flow; your application authenticates people, decides who may do what, and calls the engine for everything else. This page takes an application from nothing to a working client, against the engine's service as it runs today. [`wire.md`](wire.md) is the reference for every operation; [`write-a-flow.md`](write-a-flow.md) is how the flows themselves are written.

## 1. What runs today

The engine's core runs as an HTTP service over one of two stores ([`core/README.md`](../../core/README.md)):

- **a sandbox**, a store held in memory, a clock that moves only when told and evaluators answered by fixed answers: how flows are built and tested, and what the examples below run against;
- **a deployment's store**, given `--store <file>`: one SQLite file that survives a restart, the wall clock, and no sandbox controls (ADR-0174). It is enough to pilot a flow on one machine, and a newer engine release takes over its store by an upgrade (§2). Its reads run beside its writes, so a dashboard or an agent reading is never held behind a publish, an import or a clerk's request; its writes run one at a time, as SQLite has one writer, which a deployment whose writes outgrow one machine's disk moves past with PostgreSQL, not yet built (ADR-0178).

Everything below works against both, but for the sandbox's controls, which a deployment does not route.

**What a later release may change** (ADR-0180, binding from release 1.0.0): within `/v1` a release only adds operations, keys, fields, verdict kinds and reasons, so write your client to ignore fields it does not know and to handle a refusal of an unknown `kind` by its `remedy`; a breaking change is `/v2`, served beside `/v1`; and a newer release upgrades your store (§2). `wire.md` states the rule in full.

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

**A new engine release takes over a store by an upgrade** (ADR-0177). Stop the service, copy the store's file (with its `-wal` file, if one is beside it) as the backup, start the new release on it, which answers every operation `503 engine_mismatch` until it is upgraded, and as the operator `POST /v1/upgrade` with `"dry_run": true` to see the steps, then without it. There is no downgrade: a release that fails in service is replaced by restoring the backup. From release 0.2.0 on, a copy of the old release still running is refused from the moment the upgrade commits; one of 0.1.0 is not, so stop it first.

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

**An actor in a final state is refused** (`DESIGN.md` §5.8, ADR-0182): a person who has left, or an agent retired in the store, is refused every request, `unsatisfied`, clause `actor_active`, remedy `dependent`, and offered nothing. Record a departure in the store, and refuse the session or the key at your gateway too, which stays yours. A read checks no actor at all: who may read what is your application's.

## 4. Requests and refusals

A request creates an object (`type`) or takes a transition on one (`object_id`), with inputs in the wire's notation (`wire.md` §2: money `"USD 120.00"`, a date `"2025-12-30"`, a reference by its id, a file by its reference, its content's hash, size and media type, the bytes kept by your application).

```json
POST /v1/request
{"actor": {"id": "operator"}, "object_id": "claim-4", "transition": "add_line",
 "inputs": {"amount": "USD 120.00", "category": "expensecategory-3", "expense_date": "2025-12-30",
            "receipt": {"hash": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", "size": 48213, "media_type": "application/pdf"}},
 "expected_version": 1, "idempotency_key": "line-7f3a"}
```

**A refusal is an answer, never an error.** Branch on `kind`, then on `remedy`, which says what can be done (`wire.md` §3):

- `self_serviceable`: show the person what to correct. An `invalid_input` names each input and why (`"reasons": {"amount": "wrong_currency"}`), so a form marks the field; a `stale` means re-read the object and send again. A required input left out arrives in one of two shapes: `invalid_input` with the reason `missing`, or, where the attribute it writes is optional elsewhere in the type, `unsatisfied` with the clause `<attribute>_provided`; a form maps both to the field (`flow-format.md` §6).
- `delegable`: someone else must act. An agent hands it to a person, or, where the refusal says `proposable: true`, files it as a proposal.
- `temporal`: retry later.
- `dependent`: another object must change first; `objects` names which, in ascending id order, so a screen links to them and an agent works on them first.
- `unreachable_from_here`: nothing the caller can do; do not retry.

An `unsatisfied` refusal also says whether its clause was `unknown`, a value nobody has yet, rather than false, and what it `consulted`; an `invariant_violated` names every object an invariant fails on and the `objects` in conflict, such as the other holder of a unique value (`wire.md` §3, ADR-0179).

A refusal names only objects that exist: where a rule fails on the object the refused request was creating, its failure's `object_id` is `null`, since no such object exists and the next one created takes the id it would have had (D581). A satisfied request answers its own object's id; an object its effect created is found by its creation event among the positions it answers, `POST /v1/events {"positions": […]}`, each naming its object and transition.

**Send the version you read** (`expected_version`), so a decision made on what the person saw cannot overwrite a change they did not see. **Send an idempotency key** on every write, so a retry after a timeout answers the first result (`"replayed": true`) rather than applying twice.

`/v1/check` answers the verdict a request would get without writing anything, for a confirmation step.

## 5. Reading

- `get` an object: every attribute, derived ones too, a set by its members' ids; `null` if it does not exist.
- `history`: its events, each with who, when, what was sent and why (`context`).
- `availability`: what it offers an actor now, each transition available, available with input, or unavailable with the refusal it would meet. Render the buttons from it; build a form from each offer's input types and `declaration`. An offer marked `actor_dependent` may still be refused for who takes it, as an agent offered a person's confirmation is.
- `query` a type with a filter (`state == Claim.SUBMITTED`, `claim.id == "claim-4"`) for a worklist, ordered by `order` (`wire.md` §4); `available` lists the objects a transition is available to. A filter compares a member with a value of its own type: a state or an enumeration's member by name, never as text, and a time against `now`, `rejected_at <= now - 180 days`, since there is no timestamp literal; one that compares a member with a literal its type cannot hold is refused `malformed_filter` (D580).
- `metric` a declared metric, whole or by its dimensions. A metric that names a person is listed in its type's `declaration` under `person_dimensions`; your application decides who sees those.
- `subscribe`, then poll `pull` and `acknowledge` to react to events: notifications, timers, integrations. A subscription starts at its own creation, with nothing older (`DESIGN.md` §7): to catch up first, read what you need, then subscribe, or read the log with `export`.
- The standard metrics per assignee are named `<Type>.<metric>.<reference>`, the reference marked `assignee`, such as `Interview.open_work.interviewer`, open interviews per interviewer; `declaration` lists them with the person dimensions.
- The built-in attributes that hold JSON, a `DeclarationChange`'s `source`, `report` and `evidence` and a `Proposal`'s `inputs`, are answered as JSON, as their inputs are written (ADR-0182).

**Reads come in pages** (ADR-0179): `query`, `available`, `pull` and `export` answer at most `limit` items, 100 unless you ask for up to 1000, and the `cursor` of the next page; send it back until it is `null`. A page goes on after the last object it gave, so a worklist read while clerks work meets each new object once. There is no text search: index what `export` or `pull` gives you.

## 6. Agents

An agent acts through the same requests as a person, as an actor of an agent type, and is refused by the same rules. What the engine adds for one:

- `availability` says what it may do now, and why not, decided for the agent where a transition taking no input may be refused for who takes it (ADR-0176); `actor_dependent` marks one awaiting inputs that may still be.
- Every refusal's `remedy` is its next move, `delegable` among them where a person must act.
- **"Agents draft, people approve"** is a rule a flow states: a transition marked `taken_by: [human]` records the kind of actor that took it and refuses another kind `delegable` (`flow-format.md` §4.8, ADR-0176), and one also marked `proposable` lets the agent file the request for a person to approve, whose approval takes it as theirs. A rule over a value rather than a transition records who acted by hand and constrains it with an invariant, as `reimbursement.yaml`'s `confirmed_by_a_person` does. **Nothing an agent may take is forbidden unless the flow says so**: mark a type `taken_by: [human]`, which every transition of it declaring none takes, and open to agents what they may take with `taken_by: [human, agent]` (ADR-0182); or state it on every transition a person must take, approvals, overrides, erasures and payments among them; and keep an allowlist of the agent's tools in your application until you have.
- `tools` gives an agent's tool definitions, rendered from the flow for its actor kind: one per transition it may request, but those a rule over who acted refuses it, which it is handed for proposing only where they are proposable, one per observation kind, one reading any metric, and three reads it plans with, `get_object`, `object_availability` and `query_objects`, each a JSON Schema in the wire's notation (`docs/design/renderers.md` §3). Hand them to the model, send each write as a request in the agent's name, and each read as the operation it names. The tools are what the flow allows an agent, not what your application permits it: filter them by your own rules.

## 7. Changing a flow, and bringing in existing data

A new version is published as a `DeclarationChange`: draft it with the new modules' text, and each module's examples as `<module>.examples` beside it, which the dry run runs, a failing one refusing the submit `dry_run_clean`; `submit` it to run its dry run, read the report (what it migrates, what it would newly refuse, the objects each reaches), and approve it with `publish` at the version you read. Existing data comes in through `import_batch` into types the version declares `mirror`, which `scripts/mirror-version.py` writes from the flow's module, then a publish of the module that makes them the flow's own; `docs/design/publish-and-import.md` §7 works a go-live through, on a deployment's store. Both are `docs/design/publish-and-import.md`.

## 8. What the engine does, and what your application does

| Concern | The engine | Your application |
|---|---|---|
| Who someone is | records the actor named, with its declared kind | authenticates them, and names them |
| Who may do what | enforces the flow's rules, including rules over who acted that the flow states | decides permissions, roles and segregation of duties beyond what the flow records |
| What happens next | answers what each object offers, and why not | renders it; starts every transition: nothing runs by itself |
| Time | records when, and evaluates time in rules | schedules: timers, reminders and escalations request transitions or read `available`. Keep each deadline as an attribute the flow writes, `expires_at` assigned `now + 7 days` and guarded `now >= expires_at`, and let the scheduler query `expires_at <= now`: a version that changes the period changes the attribute, never a copy in the scheduler (`write-a-flow.md` §5) |
| Notifications | gives events through subscriptions | sends the email, the chat message, the push |
| External systems | consults an evaluator a rule names, posting the call to the endpoint a deployment configures; in a sandbox, a fixed answer | answers it there, `{"verdict": "satisfied"}` or `"unsatisfied"` (ADR-0175); writes to the other system itself |
| Search | filters by members, with indexes the flow declares | full-text search, over an export or the event feed |
| Files | records a reference to a file, `{hash, size, media_type}` | stores, serves and deletes the bytes, and computes the hash it sends |
| Reporting | computes declared metrics, exports rows | dashboards and analysis beyond them |
| Personal data | marks what is personal, erases it on request, labels person-naming metrics | who may see it |

## 9. Running a deployment

- **Its address**: the service binds loopback unless `--bind` says otherwise and authenticates nobody (ADR-0114); put it behind the application that authenticates, never on a network your users reach.
- **Health**: `GET /health` answers the release, the store's version, its clock, the store's release and whether an upgrade is due; the service writes each fault it meets, an evaluator's endpoint unreachable among them, on its standard error, one line each.
- **Backups of a running store**: SQLite's online backup copies a consistent store while the service writes, as Python's `sqlite3.connect(store).backup(sqlite3.connect(copy))` does, and so does `VACUUM INTO '<copy>'` on a connection of your own; each copy, taken while writes ran, passed `PRAGMA integrity_check` on 2026-10-11. Copying the file alone is safe only with the service stopped, its `-wal` file beside it. Restore by stopping the service and putting the copy in the store's place.
- **Erasure and the bytes on disk**: an erasure overwrites what it erased in the store's file and truncates the write-ahead log after it commits, so neither file holds an erased value (D584); a backup taken before it does, so expire backups within the period your privacy rules allow, or encrypt them.
- **What grows**: the attempt log and the idempotency records are pruned by `POST /v1/maintain {"actor": …, "task": {"name": "prune_attempts" or "prune_idempotency", "before": "<timestamp>"}}`, within the retention the service was started with (`--attempt-retention`, `--idempotency-retention`), a prune inside it refused `inside_retention`; the event log is kept whole, since archiving it is not built on one SQLite file (`archive_events` answers `501`).
- **What a release maintains** (ADR-0182): a major version is maintained for 12 months after the next major's first release, with security fixes in a release of its latest minor version, and `/v1` is served throughout beside any `/v2`.
- **Load**: writes are one at a time, each a durable commit; ADR-0178 states what was measured on one machine, which a deployment measures again on its own.

