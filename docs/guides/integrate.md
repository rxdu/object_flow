# Build an application

How your application uses ObjectFlow: who does what, naming actors, making requests, handling refusals, building screens and worklists, reacting to changes and changing the flow. [The API reference](wire.md) has every operation's keys and answers; [Agents](agents.md) covers AI agents; [Operate](operate.md) covers running the service.

## Who does what

ObjectFlow holds your business objects and enforces their flow. Your application authenticates people, decides what they may do, and calls the engine for the rest.

| Concern | ObjectFlow | Your application |
|---|---|---|
| Who someone is | Records the actor a request names, and its kind. | Authenticates them, and names them. |
| What they may do | Enforces the flow's rules, including rules the flow states about who acts. | Decides permissions and roles beyond those. |
| What happens next | Answers what each object offers, and why not. | Renders it, and starts every transition. Nothing runs by itself. |
| Time | Records when things happen, and evaluates time in rules. | Runs schedules: timers, reminders, escalations. |
| Notifications | Delivers events through subscriptions. | Sends the email, the chat message, the push. |
| Outside systems | Asks your endpoint when a rule needs an answer. | Answers it, and writes to the other system itself. |
| Search | Filters by attributes, with the indexes the flow declares. | Full-text search, over an export or the event feed. |
| Files | Records a file's hash, size and type. | Stores, serves and deletes the bytes. |
| Reporting | Computes declared and standard metrics, and exports rows. | Dashboards and analysis beyond them. |
| Personal data | Marks what is personal, erases it on request, labels metrics that name people. | Decides who may see it. |

## Connect

Every operation is `POST /v1/<operation>` with a JSON body; `GET /health` and `GET /metrics` serve operators. In production the service listens on a Unix socket or on loopback, behind your application (see [Operate](operate.md)).

Write your client for later releases. Within `/v1` a release only adds, so:

- **ignore answer fields you don't know;**
- **handle a verdict `kind` you don't know by its `remedy`;**
- **send only the keys the reference lists.** An unknown key is refused `400`, so a typo is never read as absent, and a key added by a newer release tells you when you have met an older service.

## Actors

Every request names its actor. An actor is an object in the store, of a type whose `identity` attribute has an `actor_kind`:

```yaml
Employee:
  attributes:
    user_id: { type: identity, actor_kind: human, unique: true }
```

**Mirror your users into the store.** A fresh store knows one actor, `operator`. When a person first appears in your identity provider, create their actor object as `operator` (`Employee.onboard` with their `user_id`), and an agent the same way with an `actor_kind: agent` type. Then name the actor by its identity or its object id:

```json
{"actor": {"id": "u-10442"}, "type": "Claim", "transition": "draft", "inputs": {"title": "Taxis"}}
```

- **Choose opaque identities**, such as an employee number or a generated id, never an email or a name: an identity is recorded with every event the actor makes. Write each in one Unicode form. The engine compares identities ignoring the case of ASCII letters (`ADMIN` and `admin` are one actor, `É` and `é` two), resolves a request's actor the same way, and refuses an identity that is empty, has spaces at its ends or characters that show nothing, or is shaped like an object id (`claim-12`).
- **An unknown actor** is refused `unsatisfied`, clause `actor_known`, remedy `dependent`: create its object first.
- **Someone who leaves**: move their actor object to a final state. Every later request naming them is refused `actor_active`. Revoke their session at your gateway too.
- **Acting for someone**: a request made on a person's behalf names them as `principal`, `"actor": {"id": "helper", "principal": "u-10442"}`. The principal is checked as an actor is, and recorded.

Reads check no actor's permission: who may read what is yours to decide.

## Requests

A request creates an object (`type`) or takes a transition on one (`object_id`):

```json
POST /v1/request
{"actor": {"id": "u-10442"}, "object_id": "claim-4", "transition": "add_line",
 "inputs": {"amount": "USD 120.00", "category": "expensecategory-3", "expense_date": "2025-12-30",
            "receipt": {"hash": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", "size": 48213, "media_type": "application/pdf"}},
 "expected_version": 1, "idempotency_key": "line-7f3a"}
```

- **Inputs** use the [wire's notation](wire.md#values): money as `"USD 120.00"`, a date as `"2025-12-30"`, a reference as the object's id, a file as its hash, size and media type.
- **Send `expected_version`**, the version you read, so a decision made on an old screen cannot overwrite a change its user never saw. A mismatch is refused `stale`.
- **Send an `idempotency_key`** with every write, so a retry after a timeout returns the first answer (`"replayed": true`) instead of applying twice.
- **`context`** is your reason, kept with the event, up to 16 KiB. It survives an erasure, so never put personal data in it.
- **`occurred_at`** says when the change really happened, if earlier, within the transition's `backdating_limit`.
- **`check`** takes the same body and answers the verdict without writing anything: use it for a confirmation step.

A satisfied request answers its object's id and the positions of the events it wrote. To find an object its effect created, read those events: `POST /v1/events {"positions": [8, 9]}`.

## Refusals

A refusal is a `200` with a `kind` other than `satisfied`, and a `remedy`. Branch on `kind`, then `remedy` ([every one is listed](errors.md)):

- **`self_serviceable`**: show what to fix. An `invalid_input` names each input and its reason, so a form can mark the field. A required input left out arrives as `invalid_input` with reason `missing`, or, where its attribute is optional elsewhere, as `unsatisfied` with the clause `<attribute>_provided`: map both to the field. A `stale` means re-read the object and decide again.
- **`delegable`**: someone else must act. Where `proposable` is `true`, the request can be filed as a proposal (`"propose": true`) for someone else to approve.
- **`temporal`**: retry later.
- **`dependent`**: another object must change first. `objects` names it, so a screen can link to it.
- **`unreachable_from_here`**: nothing will help. Don't offer it again.

An `unsatisfied` refusal also says whether its rule was `unknown` (a value nobody has yet) rather than false, and what outside answers and metric values it `consulted`.

## Proposals

A transition marked `proposable` can be filed instead of taken: send the request with `"propose": true`, and optionally `expires_at`. The answer names the new `Proposal`. Someone approves it with `{"object_id": "<proposal>", "transition": "approve"}`, which takes the original request as the approver, and `reject` or `withdraw` end it. An approval screen shows the proposed object; send its version as `"target_version"`, so an approval of an object that moved since, a payment grown after it was proposed, is refused `stale` rather than run on what the approver never saw.

## Screens

- **`get`** an object: every attribute, derived ones included, references as ids, its `state` and `version`. An object that does not exist is `null`.
- **`availability`** for an object and an actor: each transition `available`, `available_with_input` or `unavailable`, with the refusal it would meet and its inputs' types. Render buttons from it, and forms from the inputs (which are required is in `declaration`). An offer marked `actor_dependent` may still be refused for who takes it once its inputs are known.
- **`history`**: the object's events, newest last, with who, when, what was sent and why. It comes in pages.
- **`declaration`**: a type as declared, for building forms and labels.

## Worklists

**`query`** a type with a `filter` written in the flow's expression language, an `order`, and the `fields` to return:

```json
POST /v1/query
{"actor": {"id": "u-10442"}, "type": "Claim", "filter": "state == Claim.SUBMITTED and submitted_at <= now - 3 days",
 "order": "entered_at(state)", "fields": ["number", "claimant", "total"], "limit": 50}
```

- Compare a state or enumeration member by name (`Claim.SUBMITTED`), a reference by its id (`claim.id == "claim-4"`), and a time against `now` (there is no timestamp literal).
- A long list of alternatives is a set, `number in {"C-1", "C-2"}`, not a chain of `or`s. A filter has at most 256 terms.
- Order by `created_at`, `entered_at(<tracked member>)` or an attribute holding one value, `desc` for descending. Mark `indexed` in the flow what you filter or order large types by.
- A filter the type cannot answer is refused `malformed_filter`, with a `reason`.

**`available`** lists the objects of a type a given transition is available to now.

**Reads come in pages.** `query`, `available`, `history`, `pull` and `export` answer at most `limit` items (100 by default, up to 1000) and a `cursor`. Send the cursor back with the same read until it is `null`. A page continues after the last item it gave, so an object created meanwhile is met once.

There is no full-text search: index what `export` or `pull` gives you.

## Reacting to changes

A **subscription** delivers events. Create one as an object, then poll it:

```json
POST /v1/request
{"actor": {"id": "notifier"}, "type": "Subscription", "transition": "subscribe", "inputs": {"target_type": "Claim"}}
```

`pull` it for a page of events and a `settled` cursor, handle them, then `acknowledge` the cursor. There is no push: poll. A subscription starts at its creation. To catch up on what came before, read what you need first, then subscribe, or read the log with `export`.

## Schedules and deadlines

The engine starts no transition by itself. Keep each deadline as an attribute the flow writes: the transition that starts the clock assigns `expires_at: now + 7 days`, the transition that ends it is guarded `now >= expires_at`, and your scheduler queries `expires_at <= now` and requests it. A new version that changes the period changes one expression, never a copy in your scheduler.

## Outside systems

A rule can ask an outside system through an **evaluator**. A deployment posts each call to the endpoint its `--evaluator <name>=<url>` names:

```json
POST <your endpoint>
{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-0042"]}
```

Answer `200` with `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}`. If the answer isn't known yet, answer `{"verdict": "unanswered"}`: the guard is then unknown and refused `temporal`, so the caller retries later. Anything else, an error, or no answer within the timeout (5 s by default) is treated the same way. With `--evaluator-token`, each call carries `Authorization: Bearer <token>`.

In a sandbox, set the answers instead: `POST /sandbox/evaluators {"answers": {"xero.bill_paid": "satisfied"}}`.

## Metrics and reports

`metric` reads a declared or standard metric, whole or by its dimensions: `{"actor": …, "name": "Claim.time_in_state", "keep": ["state"], "over": "30 days"}`. Standard metrics per assignee are named `<Type>.<metric>.<reference>`, such as `Interview.open_work.interviewer`. A metric that names a person is listed in its type's `person_dimensions`: keep those behind your own permissions. A read that selects by a personal value, a query's filter or order, a lookup, `available`, `availability`, `check` or a metric's filter or person binding, is refused `personal` unless your gateway sets `"reads_personal": true` in its body, which it does where the person or agent it acts for may read by such values (ADR-0206). `export` gives the log and each type's intervals, transitions, attempts and labels as JSON lines, personal values left out, for your own analysis.

## Changing the flow

A new version is a `DeclarationChange`:

1. **Draft** it with the new modules' text, each module's examples beside it as `<module>.examples`, and a reference to their source:
   ```json
   {"actor": …, "type": "DeclarationChange", "transition": "draft",
    "inputs": {"source": {"leave": "<leave.yaml>", "leave.examples": "<leave.examples.yaml>"}, "source_ref": "git:4f2c1e9"}}
   ```
2. **Submit** it. This runs the dry run: every check, every example, and a simulation over the store's real objects. A failing one refuses the submit `dry_run_clean`; read why with a preview, `POST /v1/publish {"actor": …, "change": "<id>", "dry_run": true}`, and `revise` the draft.
3. **Read the report**: what the version migrates, what it would newly refuse, and the objects each reaches.
4. **Approve** it: `POST /v1/publish {"actor": …, "change": "<id>", "expected_version": <the change's version you read>}`. This installs the version.

Who may approve is set by the flow's `publishing: { taken_by: [human] }`; the operator always may, and where no module sets it, any kind but an agent may. Only an actor that may approve moves on a change another drafted. The store's operator is the identity `operator` in any case of its letters: never give a user or an agent an identity that is it. Data from a system you are replacing comes in through `import_batch`, into types the version marks `mirror`. [`publish-and-import.md`](../design/publish-and-import.md) works a go-live through.

## Testing your application

Run your tests against a sandbox. It serves every operation, plus four controls under `/sandbox/`:

| Control | Body | Does |
|---|---|---|
| `/sandbox/load` | `{"files": [{"name", "text"}, …]}`, imports first | Installs the flow (unchecked: run `scripts/check-flows.py` first) and empties the store. |
| `/sandbox/reset` | `{}` | Empties the store and keeps the flow. |
| `/sandbox/clock` | `{"advance": "2 h"}` or `{"set": "<timestamp>"}` | Moves the clock, which otherwise stands still. |
| `/sandbox/evaluators` | `{"answers": {"<evaluator>.<function>": "satisfied"}}` | Fixes what an outside system answers. |
