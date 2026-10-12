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

A deployment's store is made by the service's first start with `--create` (`--store /var/lib/objectflow/store.sqlite --create`); later starts leave it out, so a path that holds no store, mistyped or on a volume not mounted, is refused rather than served as a new, empty store (ADR-0183); a file that holds something else, another application's database among them, is refused with `--create` too, and left as it is. It starts at version 0, with no flow, and is given one the way every later version is: a `DeclarationChange` drafted with the modules' text, submitted, and published (§7). Its flow is checked as it is published.

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
- **The store's own acts are your application's to withhold from an agent.** The engine reads no actor's kind to refuse a request (PRD T6, N7), so what your application passes through in an agent's name is taken as anyone's: `maintain`, `upgrade` and `import_batch` among them, and approving a version with `publish` unless the flow declares `publishing: { taken_by: [human] }`, as the samples do, when the engine refuses an agent's approval `publish_taken_by` itself (ADR-0187); the operator's retirement, which was irreversible, is requested by nothing (ADR-0184). The generated tools offer none of these, so an agent held to its tools cannot ask for them; refuse them in your gateway to an agent that reaches the wire itself. An agent may draft and submit a `DeclarationChange`, whose approval is a person's (PRD UC-15, ADR-0183 decision 7).
- **An agent's reads return what a person's do**, personal attributes among them, since a read authorizes nothing (N7): filter what `get_object` and `query_objects` hand the model by your own rules on who may see what.
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

- **Connections and reads**: a connection whose request's head does not arrive within `--idle-timeout` (30 s unless given) is closed, a body that does not is answered `408`, and at `--max-connections` (512) the oldest connection answering nothing is closed to make room, so silent or leaked connections never shut out the application or `/health`, and only when every one is answering a request is a newcomer answered `503 busy` and closed; each is said, and counted in `/metrics` (ADR-0186); set the unit's `LimitNOFILE` above the cap, with room for the 16 refusals it may be writing at once; a deployment's read that passes `--read-timeout` (30 s) is stopped and answered `503 read_timeout` (ADR-0184).
- **Its address**: the service authenticates nobody (ADR-0114), so who can reach it is who can act. Give it `--socket <path>` to listen on a Unix socket, made `0660`, so the socket's owner and group, the service's user and your application's, are its only callers, as a systemd unit's `RuntimeDirectory=` and `Group=` arrange; a socket a stopped service left is replaced, a file of another kind never (ADR-0183). Otherwise it binds loopback unless `--bind` says otherwise; put it behind the application that authenticates, never on a network your users reach.
- **Health and logs**: `GET /health` answers the release, the store's version, its clock, the store's release, whether an upgrade is due, `clock_lead_seconds`, how long the writer has been held (`writer_held_seconds`, `0` while it is free), the requests in flight and the connections open, opening the store's file to answer, so a store gone from under the service answers `500`, and one due an upgrade `503`. A writer held for minutes means a publish or an import is running, or a stall: alert on it. A full disk is not in `/health`: watch the store's volume, and a write failing with a fault, as the service says on its standard error. The service writes one line on its standard error for each answer, `<time> <method> <route> <status> <duration> ms`, a health check that answers `200` left out, and each fault it meets, an evaluator's endpoint unreachable among them, naming the evaluator's function and never its arguments or the endpoint's answer, which may hold personal data. `GET /metrics` gives the answers by route and status, their seconds, the requests in flight, the connections open, those closed to make room and those refused at the cap, how long the writer has been held, the process's resident memory, the store's version and the clock's lead, in Prometheus's text format; a fault of the service's own, a 5xx, is said on its standard error with its reason and time. A process that starts the service with its standard error piped must read it to its end, as journald does: a pipe left full stops the service at its next line. One line for each answer passes journald's default rate limit at about 430 answers a second: set `LogRateLimitIntervalSec=0` on the unit, or a higher burst, so lines are not dropped.
- **The clock**: the store's clock never moves back (ADR-0174 decision 4), so if the host's clock runs ahead and returns, every event is stamped at the latest time it reached until the wall clock passes it. The service says so at its start, on each answer's line (`clock_lead=<n>s`), in `/health` and in `/metrics`; correct the host's clock and the lead runs down.
- **Stopping it**: on SIGTERM, as systemd stops a unit, or SIGINT, the service takes no new connection, answers the requests in flight and exits, saying so on its standard error; one still running after 30 s is cut, so give a unit a `TimeoutStopSec` above that. A process killed outright cuts what was in flight, its client answered nothing; a write acknowledged is durable, so send each write with an idempotency key and retry an unanswered one with it, which applies it once (ADR-0183 decision 9).
- **Its files**: a store `--create` makes is `0600`, its `-wal` and `-shm` too, since it holds personal data; the service says at its start where a store's file is readable by others.
- **Backups of a running store**: SQLite's online backup copies a consistent store while the service writes, as Python's `sqlite3.connect(store).backup(sqlite3.connect(copy))` does, and so does `VACUUM INTO '<copy>'` on a connection of your own; each copy, taken while writes ran, passed `PRAGMA integrity_check` on 2026-10-11. Copying the file alone is safe only with the service stopped; a clean stop carries the `-wal` into the file. A backup holds what the store holds, personal data among it: take it as the service's user with `umask 077`, so the copy is `0600` as the store is. **Restore** by stopping the service, removing the store's `-wal` and `-shm` files, and putting the copy in the store's place: a `-wal` left beside a restored copy is replayed onto it, and the store fails SQLite's quick check, which the service runs at each start, so it does not start and says why (production review RESTORE-WAL, third review N5); the check reads the whole file, about 0.3 s for 126 MB and 0.8 s for 387 MB. The `-wal` is cut back to 64 MiB whenever a checkpoint empties it, and `/metrics` gives the store's file and its `-wal` (`objectflow_store_bytes`, `objectflow_wal_bytes`): watch the volume they are on.
- **What an erasure reaches**: the values the flow marks `personal`, and nothing else. Text a clerk typed into an input not marked `personal`, which an event keeps with what it wrote, a correction's `reason` among them, and a request's `context` keep what was typed, the attempt of a refused request included; an erasure's own event keeps only what it erased. Mark every free-text input that may name a person `personal`, and never put personal data in `context` (production review F7).
- **Erasure and the bytes on disk**: an erasure overwrites what it erased in the store's file and truncates the write-ahead log after it commits, so neither file holds an erased value (D584), except that a read begun before the erasure and still open keeps the log's earlier frames, the erased values among them, until it ends, when the next write truncates it (D597); a backup taken before it does, so expire backups within the period your privacy rules allow, or encrypt them.
- **What grows**: the attempt log and the idempotency records are pruned by `POST /v1/maintain {"actor": …, "task": {"name": "prune_attempts" or "prune_idempotency", "before": "<timestamp>"}}`, within the retention the service was started with (`--attempt-retention`, `--idempotency-retention`), a prune inside it refused `inside_retention` and one where the service was started without that retention `no_retention`, since it then prunes nothing, and one naming an actor the store does not know, or one in a final state, refused `actor_known` or `actor_active`, as a request is; who may maintain is your application's to decide; the event log is kept whole, since archiving it is not built on one SQLite file (`archive_events` answers `501`).
- **What a release maintains** (ADR-0182): a major version is maintained for 12 months after the next major's first release, with security fixes in a release of its latest minor version, and `/v1` is served throughout beside any `/v2`.
- **Memory**: a publish holds the store's objects in memory as it simulates the version, and an approval as it installs: on the 37k-object store, 126 MB on disk, a submit peaks at about 0.5 GB and an approval at about 1.1 GB, and on 110k objects, 387 MB, at about 1.5 and 3 GB, measured 2026-10-12. After a publish, an import batch or any request of half a second or more, the service gives what it freed back to the system, which glibc keeps otherwise: ten seconds after a publish it held 25 MB on the first store and 36 MB on the second, where it held 975 MB and 2.95 GB (ADR-0187); `objectflow_resident_bytes` and `objectflow_memory_given_back_total` show it. Size a unit's `MemoryMax` at about ten times the store's file, for a publish's peak, and measure your own.
- **A request's size**: a body is at most 2 MiB, refused `400` past it (`length limit exceeded`), measured 2026-10-11; an import is sent in batches beneath it, about 3,000 objects of a claim's size each.
- **Load**: writes are one at a time, each a durable commit; ADR-0178 states what was measured on one machine, which a deployment measures again on its own. On a store of 36.7k objects, two years of a 600-person company's claims (release build, a shared i7-8700, NVMe), a `get` and a worklist query took 1 to 11 ms, a write about 10 ms, a publish adding one attribute about 2 s of submit, which runs beside the writes, and about 3 s of approval, which holds them, and the go-live import of the 36k objects 36 s, in batches of 3,000; on 110k objects, six years of claims, the submit about 6 s and the approval about 9 s, measured 2026-10-12. A lookup by an attribute is fast when the attribute is `indexed`, a reference or unique: mark `indexed` what an import's legacy key, a worklist's filter or an application's lookup reads by. A publish's time grows with the store, linearly: plan one outside the busiest hours. A submit's dry run and a `publish` preview (`dry_run: true`) run beside the writes under `--read-timeout`, and one that passes it is answered `503 read_timeout` and records nothing: on a store whose dry run takes longer, give a longer `--read-timeout`. A stop waits 30 s for the requests in flight, so an approval longer than that is cut and rolled back: stop the service after a publish, not during one.

