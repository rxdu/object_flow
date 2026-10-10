# The wire

What an application sends the engine's service and what comes back, operation by operation. It is written from the service as built (`core/crates/objectflow-service`), and every example below is an exchange captured from it on 2026-10-10. The design behind it is ADR-0150 (one `POST` per operation, values in the model's notation) and `docs/design/library-api.md`, whose types the answers carry as JSON; where the two differ, this page says what the service does. [`integrate.md`](integrate.md) walks through using it.

## 1. Requests, answers and faults

Every operation is `POST /v1/<operation>` with a JSON object for a body and `Content-Type: application/json`; the sandbox's controls are `POST /sandbox/<control>`, which a deployment's store does not route (ADR-0174); and `GET /health` answers `{"release": "objectflow-core 0.1.0", "shape": "sandbox" | "deployment", "version": <the store's>, "now": <its clock>}`. An answer is `200` with the operation's result. **A refusal is a `200`**, a verdict whose `kind` names it, never an error: a caller branches on `kind`.

A **fault** is a `4xx` or `5xx` whose body is `{"fault": "<what is wrong>"}`, every time:

| Status | When |
|---|---|
| `400` | a body that is not a JSON object, a key the operation does not take (`/v1/get takes no key folow`), or an argument it needs left out |
| `404` | an operation that does not exist (`no operation /v1/nope`) |
| `405` | an operation asked by anything but `POST`, or `/health` by anything but `GET` |
| `409` | the sandbox has no module loaded; an idempotency key reused for a different request (`key_reused: …`) |
| `501` | a construct the core does not yet run (`not run: …`) |
| `500` | the service's own fault |

**Unknown keys are refused**, so a misspelt key is never read as absent. The keys each operation takes are in §4.

**An object that does not exist** reads as `null` from `get`, `history` and `availability`; a request naming it is the verdict `not_found`.

## 2. Values

A value is JSON's own where JSON keeps it exactly, and otherwise a string in the model's notation (ADR-0150 decision 2). No value carries its type: a reader takes it from the declaration (`/v1/declaration`).

| Type | On the wire | Example |
|---|---|---|
| absent | `null` | |
| `bool` | a boolean | `true` |
| `int` | a number within ±(2⁵³ − 1), else a string of digits | `3` |
| `string`, an identity | a string; never U+0000 | `"Taxis"` |
| `decimal(p,s)` | a string, at most `s` places, at most 18 digits | `"19.90"` |
| `money(CCC)` | a string, the currency and the amount, at most its minor unit's places | `"USD 19.99"` |
| `timestamp` | RFC 3339 at UTC | `"2026-10-04T09:30:00Z"` |
| `date` | `YYYY-MM-DD` | `"2026-10-04"` |
| `duration` | a count and a unit | `"30 min"`, `"2 days"` |
| enumeration member, state | its name | `"NEW"` |
| reference | the object's id | `"employee-1"` |
| set | an array, no element repeated | `["a-1", "a-2"]` |
| `file` | today a string, its name; its object form is to come (D559) | `"taxi.pdf"` |

A duration a reader is given back is written in whichever unit reads best (`"15 days"`, `"1431 h"`); parse the count and the unit. An object's id is `<type in lower case>-<n>`.

## 3. Verdicts

A request's answer, and each refusal inside an offer. Every refusal carries `remedy`, what its caller can do (`DESIGN.md` §5.5):

| `remedy` | Meaning | What an agent does |
|---|---|---|
| `self_serviceable` | change the request: an input, or re-read and resend | fix it and retry |
| `delegable` | someone else must act: another person, or the approver of a proposal | hand it to a person |
| `temporal` | wait: an evaluator not answered, a time not yet reached | retry later |
| `dependent` | another object must change first | work on that object |
| `unreachable_from_here` | nothing done from here helps | stop |

| `kind` | Fields beside `kind` and `remedy` |
|---|---|
| `satisfied` | `object_id`, `version`, `events` (positions, the request's own first), `replayed`, `flags` |
| `unsatisfied` | `clause`, the guard that failed |
| `invariant_violated` | `failures`: each `object_id`, `type`, `invariant`, `remedy` |
| `invalid_input` | `inputs`, the names at fault, and `reasons`, each's why: `unknown_input`, `missing`, `wrong_type`, `malformed`, `wrong_currency`, `no_such_object`, `out_of_range`, `not_allowed` |
| `unavailable` | `state`, the object's, which the transition is not taken from |
| `unknown_transition` | `transition` |
| `not_requestable` | `parents`, the transitions it is taken only through |
| `not_proposable` | none: a proposal of a transition not marked proposable |
| `stale` | `cause`, `expected`, `actual`: the version read and the one it is at |
| `not_found` | none |
| `over_limit` | `loop`, `bound` |
| `call_refused` | `step`, `object_id`, and `verdict`, the caused transition's own |

```json
{"kind": "invalid_input", "remedy": "self_serviceable", "inputs": ["amount", "expense_date"],
 "reasons": {"amount": "wrong_currency", "expense_date": "malformed"}}
```

**`not_computed`.** A field the design names that the core does not yet compute is listed in the answer's `not_computed` rather than given a value that would mislead: today an `unsatisfied` verdict's `unknown`, `objects`, `proposable` and `consulted`, a flag's, and an event's `object_seq`, `source`, `taint_version`, `txn`, `retries` and `flags`. Read a field only where it is not listed.

## 4. Operations

**The actor.** An operation that acts or reads as someone takes `"actor": {"id": "<who>"}`, the actor's identity (an `Employee`'s `user_id`, say) or its object's id; the store's first actor is `operator`. A request by an actor the store does not know is refused `unsatisfied`, clause `actor_known`.

### Requests

| Operation | Keys | Answers |
|---|---|---|
| `request` | `actor`, `transition`; `type` to create, `object_id` otherwise; `inputs`; `expected_version`, `idempotency_key`, `occurred_at`, `propose`, `expires_at`, `principal`, `context` | a verdict |
| `check` | as `request` | `{"verdict": …, "unevaluated": [...]}`: the verdict it would have, nothing written |

```json
POST /v1/request
{"actor": {"id": "alice@corp"}, "type": "Claim", "transition": "draft",
 "inputs": {"claimant": "employee-1", "title": "Taxis"}, "idempotency_key": "draft-1"}

{"kind": "satisfied", "object_id": "claim-4", "version": 1, "events": [5], "replayed": false, "flags": [],
 "not_computed": ["flags[].unknown", "flags[].objects"]}
```

- `expected_version`, the version the caller read, refuses the request `stale` if the object has moved (ADR-0023).
- `idempotency_key` makes a retry safe: the same request under the same key answers its first verdict, `replayed: true`; a different request under it is a `409` fault.
- `propose: true` files the request as a `Proposal` instead of taking it, for a transition marked `proposable`; the verdict names the proposal, and a person approves it with `{"object_id": "<proposal>", "transition": "approve"}`, which takes the request as the approver.
- `context` is the caller's reason, kept with the event.

### Reads

| Operation | Keys | Answers |
|---|---|---|
| `get` | `id`, `follow` | the object, or `null` |
| `history` | `id`, `follow` | `{"events": [...], "not_computed": [...]}`, its legacy entries first, or `null` |
| `events` | `positions` | `{"events": [...]}`, each position's event or `null` |
| `availability` | `actor`, `id` | each transition's offer to the actor, or `null` |
| `query` | `actor`, `type`, `filter`, `order`, `fields`, `cursor` | `{"items": [...], "cursor": null, "refused": null}` |
| `available` | `actor`, `type`, `transition`, `cursor` | a page of the objects the transition is available to |
| `exceptions` | `actor`, `type`, `cursor` | a page of the objects holding an admitted violation |
| `declaration` | `type`, `version` | the type as declared, with its enumerations, `version`, `source_ref`, `source_digest` and `person_dimensions` |
| `tools` | `actor_kind` | `{"tools": [...]}`: each `{name, description, input_schema}` an agent of that kind is handed, sorted by name (`renderers.md` §3) |
| `metric` | `actor`, `name`, `bind`, `keep`, `over`, `filter`, `cursor` | `{"rows": [{"dimensions": …, "value": …}], "flags": [...], "history_complete": …, "refused": null}` |
| `lookup` | `actor`, `source`, `value` | the object an external identifier names (`source` an external system, or `legacy` with `"<Type>:<key>"`), or `null` |
| `export` | `actor`, `source`, `cursor` | `{"lines": [...], "cursor": …}`: JSON lines of `events`, an observation kind, or `<Type>.intervals`, `.attempts`, `.transitions` or `.labels`, personal values left out |
| `diagnostics` | `actor`, `type`, `window` | `{"diagnostics": [...]}`: where the flow and its record disagree |

An **object**:

```json
{"id": "claim-4", "type": "Claim", "state": "DRAFT", "version": 2, "declaration_version": 1,
 "created_at": "2026-01-01T00:00:00Z", "created_by_kind": "human", "created_dated": true,
 "attributes": {"number": "EXP-00001", "claimant": "employee-1", "title": "Taxis", "lines": ["claimline-5"],
                "submitted_at": null, "total": "USD 120.00", "over_policy_lines": 0},
 "recorded_from": "2026-01-01T00:00:00Z", "open": true}
```

Every attribute is there, derived ones among them; a set reference holds its members' ids, `[]` where there are none.

An **offer**, one per transition, its inputs as types to build a form from (which are required is in `declaration`):

```json
{"transition": "add_line", "creates": null, "availability": "unavailable",
 "verdict": {"kind": "unavailable", "remedy": "unreachable_from_here", "state": "SUBMITTED"},
 "inputs": {"amount": "money(USD)", "category": "ExpenseCategory", "expense_date": "date", "receipt": "file"},
 "unevaluated": [], "flags": [], "actor_dependent": false}
```

`availability` is `available`, `available_with_input` or `unavailable`, with the refusal it would meet; `flags` are the warnings taking it would raise; `unevaluated`, the evaluators it would ask. **`actor_dependent: true`** says the transition writes who takes it into what an invariant reads, so it may be refused for who takes it, which an offer cannot tell: a person's confirmation an agent is offered, say.

A **query filter** is an expression over the type's members (`declaration-syntax.md` §8): `amount > USD 100.00`, `state == ClaimLine.LISTED`, `claim.number == "EXP-00001"`. A reference is compared by its id, `claim.id == "claim-4"`: a filter comparing a reference with a string, or computing with one, is refused `malformed_filter`. A query's refusal is `refused`, a code: `unknown_type`, `malformed_filter`, `unknown_order_key`, `unknown_field`.

**Pages hold everything today**: `query`, `available` and `metric` answer every row on one page with `cursor: null`, and a cursor sent to `query` or `metric` is a fault. Page in the client until the core pages.

### Subscriptions

A subscription is an object: `{"actor": …, "type": "Subscription", "transition": "subscribe", "inputs": {"target_type": "Claim"}}`, optionally with `target_family`, `transitions`, `changes_state_only`, `lagging_after` and `dead_after`.

| Operation | Keys | Answers |
|---|---|---|
| `pull` | `actor`, `subscription`, `cursor` | `{"events": [...], "settled": "<cursor>", "refused": null}` |
| `acknowledge` | `actor`, `subscription`, `cursor` | `{"refused": null}`: everything up to the settled cursor is handled |

There is no push: a consumer polls `pull` and acknowledges what it has handled.

### Changing the declaration, and importing

| Operation | Keys | Answers |
|---|---|---|
| `publish` | `actor`, `change`, `dry_run`, `expected_version` | `{"verdict": …, "report": {...}}`: a `DeclarationChange`'s dry run and its impact report, or its approval at the version read, which installs the version |
| `import_batch` | `actor`, `batch`, `disposition`, `dry_run` | `{"written": …, "findings": [...], "refused_types": [...], "refusal": …}` |
| `maintain` | `actor`, `task` | `{"moved": …, "refused": …}`: `prune_attempts`, `prune_idempotency` or `archive_events` before a time |

A change is an object: `{"type": "DeclarationChange", "transition": "draft", "inputs": {"source": {"<module>": "<its YAML>", …}, "source_ref": "git:…"}}`, then `submit`, which runs its dry run, then `publish` with `expected_version` to approve it. The whole procedure, and importing legacy data into mirror types, is `docs/design/publish-and-import.md`.

### The sandbox's controls

| Control | Keys | Answers |
|---|---|---|
| `/sandbox/load` | `files` (each `name` and `text`, imports first), `attempt_retention`, `idempotency_retention` | `{"loaded": true, "modules": [...], "types": [...], "evaluators": [...], "now": …}` |
| `/sandbox/reset` | none | `{"now": …}`: the loaded modules kept, every object gone |
| `/sandbox/clock` | `advance` (a duration) or `set` (a timestamp) | `{"now": …}` |
| `/sandbox/evaluators` | `answers`: each `"<evaluator>.<function>"` to `satisfied`, `unsatisfied` or `unanswered` | the answers held |

A deployment's service routes none of these: its clock is the wall's, its evaluators are answered at the endpoints it names, each call posted as `{"evaluator", "function", "arguments"}` and answered `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}` (ADR-0175), and its flow arrives by `publish`.
