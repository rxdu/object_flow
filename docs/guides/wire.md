# The wire

What an application sends the engine's service and what comes back, operation by operation. It is written from the service as built (`core/crates/objectflow-service`), and every example below is an exchange captured from it on 2026-10-10. The design behind it is ADR-0150 (one `POST` per operation, values in the model's notation) and `docs/design/library-api.md`, whose types the answers carry as JSON; where the two differ, this page says what the service does. [`integrate.md`](integrate.md) walks through using it.

**What `/v1` promises** (ADR-0180, proposed; binding from release 1.0.0). Within `/v1` a release only adds: an operation, an optional request key, an answer field, a verdict `kind`, an `invalid_input` reason, a finding code. It never removes or renames one, changes a field's type or meaning, makes an optional key required, changes a default such as a page's 100, or changes an operation's status codes; `GET /health`'s fields and the evaluator endpoint's call and answer (§4) are held to the same rule, and a flag of the service is renamed or removed only in a major release. So a client **ignores answer fields it does not know**, **handles a refusal of a `kind` it does not know by its `remedy`**, one of the five of §3, which are all there will be, and **sends only the keys this page lists**: the service refuses any other as `400`, which is how a client learns it has met an older service. A change that needs more is `/v2`, served beside `/v1` until a major release. [`wire-v1.json`](wire-v1.json) lists what each operation takes, and a test of the service holds it to the list. The sandbox's controls are not promised.

## 1. Requests, answers and faults

Every operation is `POST /v1/<operation>` with a JSON object for a body and `Content-Type: application/json`; the sandbox's controls are `POST /sandbox/<control>`, which a deployment's store does not route (ADR-0174); and `GET /health` answers `{"release": "objectflow-core 0.2.0", "shape": "sandbox" | "deployment", "version": <the store's>, "now": <its clock>, "store_release": "objectflow-core 0.2.0", "upgrade_due": false}`, the store's release and whether an upgrade is due being ADR-0177's. An answer is `200` with the operation's result. **A refusal is a `200`**, a verdict whose `kind` names it, never an error: a caller branches on `kind`.

A **fault** is a `4xx` or `5xx` whose body is `{"fault": "<what is wrong>"}`, every time:

| Status | When |
|---|---|
| `400` | a body that is not a JSON object, a key the operation does not take (`/v1/get takes no key folow`), or an argument it needs left out |
| `404` | an operation that does not exist (`no operation /v1/nope`) |
| `405` | an operation asked by anything but `POST`, or `/health` by anything but `GET` |
| `409` | the sandbox has no module loaded; an idempotency key reused for a different request (`key_reused: …`) |
| `501` | a construct the core does not yet run (`not run: …`) |
| `503` | `engine_mismatch: …`: the store is at another engine release than this copy of the service, an upgrade due or made by another copy (ADR-0177) |
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
| `file` | an object: its content's SHA-256, its size in bytes and its media type; a name is refused `wrong_type` | `{"hash": "sha256:9f86…", "size": 48213, "media_type": "application/pdf"}` |

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
| `unsatisfied` | `clause`, the guard that failed; `unknown`, whether it read something absent rather than was false; `objects`, what its remedy points at, the objects the clause names in ascending id order; `proposable`, whether filing the request as a proposal would be accepted; `consulted`, the evaluator verdicts and metric values it was decided on (ADR-0179) |
| `invariant_violated` | `failures`: each `object_id`, `type`, `invariant`, `remedy`, on every object an invariant fails on; `objects`, the objects in conflict, such as the other holder of a unique value |
| `invalid_input` | `inputs`, the names at fault, and `reasons`, each's why: `unknown_input`, `missing`, `wrong_type`, `malformed`, `wrong_currency`, `no_such_object`, `out_of_range`, `not_allowed` |
| `unavailable` | `state`, the object's, which the transition is not taken from |
| `unknown_transition` | `transition` |
| `not_requestable` | `parents`, the transitions it is taken only through |
| `not_proposable` | none: a proposal of a transition not marked proposable |
| `stale` | `cause`: `expected_version`, with `expected` and `actual`, the version read and the one it is at; or `retries_exhausted`, remedy `temporal`, where the store's write lock was held past the busy timeout and every retry, by another process (ADR-0178) |
| `not_found` | none |
| `over_limit` | `loop`, `bound` |
| `call_refused` | `step`, `object_id`, and `verdict`, the caused transition's own, whole, its `proposable` false |

```json
{"kind": "invalid_input", "remedy": "self_serviceable", "inputs": ["amount", "expense_date"],
 "reasons": {"amount": "wrong_currency", "expense_date": "malformed"}}
```

**`not_computed`.** A field the design names that the core does not yet compute is listed in the answer's `not_computed` rather than given a value that would mislead: today an offer's `consulted`, since an offer asks no evaluator, and an event's `object_seq`, `source`, `taint_version`, `txn` and `flags`. Read a field only where it is not listed. A flag carries `clause`, `remedy`, `unknown` and `objects`, as an unsatisfied clause does.

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

{"kind": "satisfied", "object_id": "claim-4", "version": 1, "events": [5], "replayed": false, "flags": []}
```

- `expected_version`, the version the caller read, refuses the request `stale` if the object has moved (ADR-0023).
- `idempotency_key` makes a retry safe: the same request under the same key answers its first verdict, `replayed: true`; a different request under it is a `409` fault.
- `propose: true` files the request as a `Proposal` instead of taking it, for a transition marked `proposable`; the verdict names the proposal, and a person approves it with `{"object_id": "<proposal>", "transition": "approve"}`, which takes the request as the approver.
- `context` is the caller's reason, kept with the event.

### Reads

| Operation | Keys | Answers |
|---|---|---|
| `get` | `id`, `follow` | the object, or `null`; no `id` is a `400` |
| `history` | `id`, `follow` | `{"events": [...], "not_computed": [...]}`, its legacy entries first, or `null`; no `id` is a `400` |
| `events` | `positions` | `{"events": [...]}`, each position's event or `null` |
| `availability` | `actor`, `id` | each transition's offer to the actor, or `null` |
| `query` | `actor`, `type`, `filter`, `order`, `fields`, `cursor`, `limit` | `{"items": [...], "cursor": "<next page>" or null, "refused": null}` |
| `available` | `actor`, `type`, `transition`, `cursor`, `limit` | a page of the objects the transition is available to |
| `exceptions` | `actor`, `type`, `cursor` | a page of the objects holding an admitted violation |
| `declaration` | `type`, `version` | the type as declared, with its enumerations, `version`, `source_ref`, `source_digest` and `person_dimensions` |
| `tools` | `actor_kind` | `{"tools": [...]}`: each `{name, description, input_schema}` an agent of that kind is handed, sorted by name (`renderers.md` §3) |
| `metric` | `actor`, `name`, `bind`, `keep`, `over`, `filter`, `cursor` | `{"rows": [{"dimensions": …, "value": …}], "flags": [...], "history_complete": …, "refused": null}` |
| `lookup` | `actor`, `source`, `value` | the object an external identifier names (`source` an external system, or `legacy` with `"<Type>:<key>"`), or `null` |
| `export` | `actor`, `source`, `cursor`, `limit` | `{"lines": [...], "cursor": …}`: JSON lines of `events`, an observation kind, or `<Type>.intervals`, `.attempts`, `.transitions` or `.labels`, personal values left out |
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

`availability` is `available`, `available_with_input` or `unavailable`, with the refusal it would meet; `flags` are the warnings taking it would raise; `unevaluated`, the evaluators it would ask. **`actor_dependent: true`** says the transition writes who takes it into what an invariant reads, so it may be refused for who takes it. One that takes no input and asks no evaluator is decided for who asks: the request is simulated with no inputs, as `check` would, so an agent is offered no approval marked `taken_by: [human]` and sees why, `unavailable` with the `invariant_violated` it would meet (ADR-0176). One awaiting inputs cannot be decided until they are given, so an application `check`s it with them, or lets the request answer. `available`'s page carries `actor_dependent` for its transition, since a sweep is not simulated row by row.

Every offer of an object carries the built-in `record` with `"creates": "label"`: a label, a name a person or an agent puts on any object, which a metric reads (`flow-format.md` §4.9). A form for it takes `name` and an optional `note`.

An **event**, as `history`, `events` and `pull` answer it:

```json
{"position": 3, "object_id": "transfer-2", "transition": "settle", "from_state": "SENT", "to_state": "SETTLED",
 "changes_state": true, "actor_id": "operator", "actor_kind": "human", "principal": null, "declaration_version": 1,
 "recorded_at": "2026-10-10T09:31:02Z", "occurred_at": "2026-10-10T09:31:02Z", "imported": false,
 "payload": {"inputs": {}}, "cause": null, "context": null,
 "reads": {"objects": {}, "scans": {}, "now": "2026-10-10T09:31:02Z"},
 "consulted": {"cleared": {"bank.cleared(T-1)": "satisfied"}}, "as_of": {"cleared": "2026-10-10T09:31:02Z"},
 "retries": 0}
```

`retries` is how many times the request waited out another process's write lock (ADR-0178). `payload.inputs` holds what the request sent; `reads`, what its rules read; and `consulted` and `as_of`, by guard, each evaluator's verdict or metric's value it was given, by the call with its arguments, and when (`library-api.md` §5).

A **query filter** is an expression over the type's members (`declaration-syntax.md` §8): `amount > USD 100.00`, `state == ClaimLine.LISTED`, `claim.number == "EXP-00001"`. A reference is compared by its id, `claim.id == "claim-4"`: a filter comparing a reference with a string, or computing with one, is refused `malformed_filter`. A query's refusal is `refused`, a code: `unknown_type`, `malformed_filter`, `unknown_order_key`, `unknown_field`. A **metric's filter** reads its rows through the metric's item, `i.state == Claim.SUBMITTED` for `time_in_state`; one naming anything else is refused `malformed_filter`, as are `unknown_metric`, `unknown_dimension`, `filter_on_combined`, `no_window` and `personal` (`library-api.md` §5).

**Pages** (ADR-0179): `query`, `available`, `pull` and `export` answer at most `limit` items, 1 to 1000, **100 unless asked**, with the `cursor` of the next page, `null` where it is the last; send the cursor back, with the same read, until it is `null`. A cursor continues only the read it came from: another `query` or `available` refuses it `malformed_cursor`. A page goes on after the last object given, in the read's order, so an object created between pages is met once and none twice, unless an object's order key changes between pages. `pull` pages by its settled cursor, `export` by its own; a metric's read and `exceptions` answer one page.

### Subscriptions

A subscription is an object: `{"actor": …, "type": "Subscription", "transition": "subscribe", "inputs": {"target_type": "Claim"}}`, optionally with `target_family`, `transitions`, `changes_state_only`, `lagging_after` and `dead_after`.

| Operation | Keys | Answers |
|---|---|---|
| `pull` | `actor`, `subscription`, `cursor`, `limit` | `{"events": [...], "settled": "<cursor>", "refused": null}` |
| `acknowledge` | `actor`, `subscription`, `cursor` | `{"refused": null}`: everything up to the settled cursor is handled |

There is no push: a consumer polls `pull` and acknowledges what it has handled.

### Changing the declaration, and importing

| Operation | Keys | Answers |
|---|---|---|
| `publish` | `actor`, `change`, `dry_run`, `expected_version` | `{"verdict": …, "report": {...}}`: a `DeclarationChange`'s dry run and its impact report, or its approval at the version read, which installs the version |
| `import_batch` | `actor`, `batch`, `disposition`, `dry_run` | `{"written": …, "findings": [...], "refused_types": [...], "refusal": …}` |
| `maintain` | `actor`, `task` | `{"moved": …, "refused": …}`: `prune_attempts`, `prune_idempotency` or `archive_events` before a time |

A change is an object: `{"type": "DeclarationChange", "transition": "draft", "inputs": {"source": {"<module>": "<its YAML>", …}, "source_ref": "git:…"}}`, then `submit`, which runs its dry run, then `publish` with `expected_version` to approve it. The whole procedure, and importing legacy data into mirror types, is `docs/design/publish-and-import.md`; §7 there works a go-live through.

A **batch** (`publish-and-import.md` §4), each object of a mirror type, its references by the other's legacy key:

```json
{"objects": [
   {"type": "PurchaseOrder", "legacy_key": "4300", "state": "OPEN",
    "attributes": {"number": "PO-04300", "supplier": "S-12", "legacy_key": "4300"},
    "created_at": "2025-11-02T08:00:00Z", "created_by_kind": "human",
    "entered": {"state": {"at": "2025-11-03T10:00:00Z", "by": "jo@corp", "by_kind": "human"}},
    "legacy_intervals": [{"dimension": "state", "value": "DRAFT", "entered_at": "2025-11-02T08:00:00Z",
                          "left_at": "2025-11-03T10:00:00Z", "entered_by": "jo@corp", "entered_by_kind": "human"}],
    "legacy_entries": [{"kind": "note", "occurred_at": "2025-11-02T09:00:00Z", "payload": {"text": "…"},
                        "about": {"customer": "Customer:C-7"}}],
    "silent_spans": [["2025-12-20T00:00:00Z", "2026-01-05T00:00:00Z"]]}],
 "sequences": [{"sequence": "order_number", "last": 4300, "scope": ""}],
 "legacy_kinds": {"note": ["text"]}}
```

`type`, `legacy_key` and `state` are required; the rest is what the legacy record can say: when the object was created and by what kind of actor, when it took its current state or a tracked member its value and who changed it (`entered`), its earlier values (`legacy_intervals`), its audit rows (`legacy_entries`, each kind's fields kept through an erasure in `legacy_kinds`), the spans the record is silent over, and each sequence's last number. A mirror's external identifier, `legacy_key` above, holds the key in `attributes` too. The **disposition file** is the dry run's `findings`, each with `disposition` set: `clean_upstream`, `admit` (an invariant), `exclude` or `drop` (a reference or a field), and optionally `decided_by` and `reason` (§6 there). A reference to a legacy key neither the batch nor the store has is a finding too, `"<Type>.<reference>: names nothing"`, so one dry run lists them all (ADR-0176).

A **deployment's evaluator endpoint** (ADR-0175) is posted each call as `{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-0042"]}`, the arguments in the order the function declares them, each in this page's notation, and answers `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}` with a `200`; anything else, or no answer within `--evaluator-timeout`, is the evaluator's unknown, so its guard is refused `temporal`, and the service writes why on its standard error, which is where an integration's fault shows.

### A deployment's upgrade

`GET /health` answers `release`, this copy's engine release, `store_release`, the store's, and `upgrade_due`. A copy started on a store an earlier release made answers every operation but one `503 engine_mismatch`, and that one is **`POST /v1/upgrade`** `{"actor": {"id": "operator"}, "dry_run": true}`: the store's operator carries the store through each release between, or with `dry_run` runs every step and rolls it back, answering

```json
{"from": "objectflow-core 0.1.0", "to": "objectflow-core 0.2.0", "dry_run": false, "upgraded": true,
 "steps": [{"release": "objectflow-core 0.2.0", "changes": "the store records its engine release in of_engine_release, …"}]}
```

after which the copy serves the store. An actor that is not the store's operator is a `409`; a store at this release already answers `upgraded: false` with no steps; a store of a release this copy does not upgrade from, a later one's among them, does not start (`storage-schema.md` §12, ADR-0177).

### The sandbox's controls

| Control | Keys | Answers |
|---|---|---|
| `/sandbox/load` | `files` (each `name` and `text`, imports first), `attempt_retention`, `idempotency_retention` | `{"loaded": true, "modules": [...], "types": [...], "evaluators": [...], "now": …}` |
| `/sandbox/reset` | none | `{"now": …}`: the loaded modules kept, every object gone |
| `/sandbox/clock` | `advance` (a duration) or `set` (a timestamp) | `{"now": …}` |
| `/sandbox/evaluators` | `answers`: each `"<evaluator>.<function>"` to `satisfied`, `unsatisfied` or `unanswered` | the answers held |

A deployment's service routes none of these: its clock is the wall's, its evaluators are answered at the endpoints it names, each call posted as `{"evaluator", "function", "arguments"}` and answered `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}` (ADR-0175), and its flow arrives by `publish`.
