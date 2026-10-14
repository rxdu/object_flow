# API reference

Every operation of the engine's service: what it takes and what it answers. [Build an application](integrate.md) shows how to use them; [Errors and refusals](errors.md) lists every refusal and fault.

## Basics

- Every operation is `POST /v1/<operation>`, with a JSON object as its body and `Content-Type: application/json`. A body is at most 2 MiB.
- An answer is `200` with the operation's result. **A refusal is also `200`**: a verdict whose `kind` names it.
- A fault is a `4xx` or `5xx` whose body is `{"fault": "<reason>", "code": "<code>"}`. Branch on `code`; the reason is text for people and may change.
- **Unknown keys are refused** `400`, and so is **a key named twice** in one object. `null` is the same as leaving a key out.
- A flag such as `dry_run` is `true` or `false`, never a string; an id, a filter or a cursor is a string; `expected_version` is an integer.
- An object that does not exist reads as `null` from `get`, `history` and `availability`; a request naming it is refused `not_found`.
- `GET /health` and `GET /metrics` serve operators ([Operate](operate.md)). A sandbox also serves `POST /sandbox/<control>` ([below](#sandbox-controls)).

**What `/v1` promises**, from release 1.0.0. Within `/v1` a release only adds: an operation, an optional request key, an answer field, a verdict `kind`, an `invalid_input` reason, a refusal or finding code. It never removes or renames one, changes a field's type or meaning, makes an optional key required, changes a default (such as a page's 100), or changes an operation's status codes. `/health`'s fields, the evaluator call and its answer, and the sandbox's controls are held to the same rule, and a service flag is renamed or removed only in a major release. A change that needs more is `/v2`, served beside `/v1`. [`wire-v1.json`](wire-v1.json) lists the keys each operation takes, and a test of the service holds it to that list.

## The actor

An operation that acts or reads as someone takes `"actor": {"id": "<who>"}`: the actor's identity (an `Employee`'s `user_id`, say) or its object's id. The store's first actor is `operator`. A request made for someone else names them as `principal`, in the actor (`"actor": {"id": …, "principal": …}`) or as the request's own key; the two must agree. The principal is checked as the actor is, and recorded.

An actor the store does not know is refused `unsatisfied`, clause `actor_known`. An actor or principal whose object is in a final state is refused `actor_active`. Both have remedy `dependent`.

## Values

A value is JSON's own where JSON keeps it exactly, and otherwise a string in the flow's notation. No value carries its type: read it from `declaration`.

| Type | On the wire | Example |
|---|---|---|
| absent | `null` | |
| `bool` | a boolean | `true` |
| `int` | a number within ±(2⁵³ − 1); a string of digits beyond | `3` |
| `string` | a string of at most 64 KiB in UTF-8, past which it is `out_of_range`; never U+0000 | `"Taxis"` |
| `identity` | a string, as above, that is not empty, has no whitespace at its ends and none but a space within, holds no character that shows nothing (a control or format character, a variation selector, a combining grapheme joiner, a Hangul filler, a tag, a line or paragraph separator, a blank braille pattern, U+17B4 or U+17B5), no private-use character and no noncharacter, and is not shaped as an object id of the store's types, in any case of its letters (`Staff-2` as `staff-2`); else `malformed`. It names one actor across every actor type, compared ignoring the case of its ASCII letters, a letter outside ASCII as written: a second holder is refused `identity_taken`, and a request naming the identity in another ASCII case is that actor's, recorded under the identity it holds. | `"u10442"` |
| `decimal(p,s)` | a string of digits, at most `s` places and `p` digits in all; an input past that is `out_of_range`, an effect's write past it `value_in_range` | `"19.90"` |
| `money(CCY)` | a string: the currency, one space, the amount in at most its minor unit's places | `"USD 19.99"` |
| `timestamp` | RFC 3339 in UTC (`Z` or `+00:00`), to the second or the minute, no fraction of a second | `"2026-10-04T09:30:00Z"` |
| `date` | `YYYY-MM-DD`, the year 0001 to 9999 | `"2026-10-04"` |
| `duration` | a count and a unit: `s`, `min`, `h`, `day(s)`, `week(s)` | `"30 min"`, `"2 days"` |
| enumeration member, state | its name | `"NEW"` |
| reference | the object's id | `"employee-1"` |
| set | an array, no element repeated | `["a-1", "a-2"]` |
| `file` | an object: the content's SHA-256, its size in bytes and its media type; a name is refused `wrong_type` | `{"hash": "sha256:9f86…", "size": 48213, "media_type": "application/pdf"}` |

Dates and times stay within 0001-01-01 to 9999-12-31T23:59:59Z. An effect that would write one past that is refused `value_in_range`, and so is any arithmetic past its type's bound, wherever it is evaluated: a date or a time computed past the calendar, a date moved by part of a day, an integer past 64 bits. A read that meets one, a `get`, a `query` or a `metric`, answers `{"refused": "value_in_range", "reason": …, "object_id": …}`, naming the object and no value; it was answered `null` before 1.0.0 (ADR-0205 decision 2). `availability` and `check` say it as the transition's verdict. A division by zero is unknown, and reads as `null`. A duration you are given back is written in whichever unit reads best (`"15 days"`, `"1431 h"`): parse the count and the unit. An object's id is `<type in lower case>-<number>`.

## Requests

In each table, the keys before a `;` are required and those after it optional; a request missing a required key is answered `400`.

| Operation | Keys | Answers |
|---|---|---|
| `request` | `actor`, `transition`; `type` to create, or `object_id`, with `type` beside it to require the object be of that type (else `not_found`); `inputs`, `expected_version`, `target_version`, `idempotency_key`, `occurred_at`, `propose`, `expires_at`, `principal`, `context` | a verdict |
| `check` | the same | `{"verdict": …, "unevaluated": […]}`: the verdict the request would get, nothing written; `unevaluated` names the conditions whose evaluators it did not ask, whose guards may still refuse the request. A satisfied verdict names no `events`, and a creation's no `object_id`, each listed in its `not_computed`: what the request would be given, another request may be given first. A check on an object names that object |

```json
POST /v1/request
{"actor": {"id": "u10442"}, "type": "Claim", "transition": "draft",
 "inputs": {"claimant": "employee-1", "title": "Taxis"}, "idempotency_key": "draft-1"}

{"kind": "satisfied", "object_id": "claim-4", "version": 1, "events": [5], "replayed": false, "flags": []}
```

- `inputs`: the transition's inputs, by name, in the notation above.
- `expected_version`: the version the caller read. If the object has moved on, the request is refused `stale`.
- `idempotency_key`: at most 256 bytes. The same request under the same key, from the same actor, answers its first verdict with `"replayed": true`; a different request under it is a `409 key_reused` fault. The key belongs to the actor: another actor's request under it is a request of its own. A different request under the key, one differing in a personal value included, is `key_reused`. Once an erasure has removed what its request sent, every request under the key is `key_reused`, so a key confirms no guess of what was erased (ADR-0205 decision 3); a key release 0.2.0 recorded, whose digest held personal values, is refused so too, where it replayed whatever was sent. An erasure does not reach the key itself: make keys of ids or random values, never of a name or other personal data.
- `target_version`: on a proposal's `approve` only, the version of the proposed object its approver read. An approval finding the object moved since is refused `stale`, so what a person approves is what runs; without it, the approval runs on the object as it stands. On any other request it is a `400`.
- `occurred_at`: when the change happened, if earlier than now, within the transition's `backdating_limit`.
- `propose`: `true` files the request as a `Proposal` instead of taking it, for a transition marked `proposable`. The verdict names the proposal. A person approves it with `{"object_id": "<proposal>", "transition": "approve"}`, which takes the request as the approver. `reject`, and `withdraw` by anyone but its proposer, are held to the kinds the transition's `taken_by` names. The proposal records its `proposer` by identity.
- `expires_at`: when a proposal lapses, with `propose` only.
- `context`: at most 16 KiB, the caller's reason, kept with the event. It is kept through an erasure, which removes only values marked `personal`: never put personal data in it.
- `reads_personal`: `true` where the caller may read by a personal value: its refusal by one, a uniqueness or `identity_taken`, then names the object holding it. A read takes it too (Filters and order).

### Verdicts

A verdict is `{"kind": …, "remedy": …}` with the fields of its kind; a satisfied one has no `remedy`. The kinds, their fields and the five remedies are in [Errors and refusals](errors.md#verdict-kinds).

```json
{"kind": "invalid_input", "remedy": "self_serviceable", "inputs": ["amount", "expense_date"],
 "reasons": {"amount": "wrong_currency", "expense_date": "malformed"}}
```

**`not_computed`.** A field the design names but the engine does not compute yet is listed in the answer's `not_computed` instead of being given a misleading value: today an offer's `consulted` (an offer asks no evaluator) and an event's `taint_version`, `txn` and `flags`. The reference runner a sandbox runs on lists more of an event's fields. Read a field only where it is not listed.

## Reads

| Operation | Keys | Answers |
|---|---|---|
| `get` | `id`; `actor`, `follow` | the object, or `null`. With `"follow": true`, the object its supersession chain ends at. |
| `history` | `id`; `actor`, `follow`, `limit`, `cursor` | `{"events": […], "cursor": …, "not_computed": […]}`, a page of the object's events in order, its imported legacy entries first on the first page; or `null`. With `"follow": true`, the events of every object whose chain ends where this one's does. |
| `events` | `positions`; `actor` | `{"events": […]}`: each position's event, or `null`, in the order asked. At most a page's worth of positions, none twice, and at most 8 MiB of events, past which it is a `400` saying how many fit. |
| `availability` | `actor`, `id` | an offer per transition, or `null` |
| `query` | `actor`, `type`; `filter`, `order`, `fields`, `cursor`, `limit` | `{"items": [{"id", "type", "fields"}…], "cursor": …, "refused": …}`; `order` names at most 8 keys and `fields` at most 256, each once |
| `available` | `actor`, `type`, `transition`; `cursor`, `limit` | a page of the objects the transition is available to, with `actor_dependent` |
| `exceptions` | `actor`, `type`; `cursor` | a page of at most 100 of the objects holding an admitted violation of an invariant, in creation order, `{"items": [{"id", "type", "fields"}…], "cursor": …}`, the `cursor` going on from its last where more follow |
| `declaration` | `type`; `actor`, `version` | the type as declared, with its enumerations, `version`, `source_ref`, `source_digest` and `person_dimensions` |
| `tools` | `actor_kind`; `actor` | `{"tools": [{"name", "description", "request", "input_schema"}…]}`: the tools an actor of that kind is handed, sorted by name ([Agents](agents.md)) |
| `metric` | `actor`, `name`; `bind`, `keep`, `over`, `filter`, `cursor` | `{"rows": [{"dimensions", "value"}…], "flags": […], "history_complete": …, "refused": …}` |
| `lookup` | `source`, `value`; `actor` | the object an external identifier names, or `null`. `source` is an external system, or `legacy` with `"<Type>:<key>"`. |
| `export` | `actor`, `source`; `cursor`, `limit` | `{"lines": […], "cursor": …, "more": …}`: JSON lines of `log` (the event log), an observation kind, or `<Type>.intervals`, `.attempts`, `.transitions` or `.labels`, with personal values left out. An `.intervals` row is keyed by `object_id`, `dimension`, `entered_position` and `entered_at`, and a line `{"retracted": true, …}` with that key removes a row a later commit deleted or moved, a correction merging it or a version renaming or retyping its member, before the rows that commit wrote: fold the lines by their key in order, and you hold what the store holds (ADR-0205 decision 9) |
| `diagnostics` | `actor`, `type`, `window` | `{"diagnostics": […]}`: where the flow and its record disagree |

A metric's `over` and diagnostics' `window` are positive durations, and a metric's `keep` names each dimension once; anything else is a `400`. The built-in attributes that hold JSON, a `DeclarationChange`'s `source`, `report` and `evidence` and a `Proposal`'s `inputs`, are answered as JSON.

### An object

```json
{"id": "claim-4", "type": "Claim", "state": "DRAFT", "version": 2, "declaration_version": 1,
 "created_at": "2026-01-01T00:00:00Z", "created_by_kind": "human", "created_dated": true,
 "attributes": {"number": "EXP-00001", "claimant": "employee-1", "title": "Taxis", "lines": ["claimline-5"],
                "submitted_at": null, "total": "USD 120.00", "over_policy_lines": 0},
 "recorded_from": "2026-01-01T00:00:00Z", "open": true}
```

Every attribute is there, derived ones included. A set holds its members' ids, `[]` when empty.

### An offer

One per transition, as `availability` answers it:

```json
{"transition": "add_line", "creates": null, "availability": "unavailable",
 "verdict": {"kind": "unavailable", "remedy": "unreachable_from_here", "state": "SUBMITTED"},
 "inputs": {"amount": "money(USD)", "category": "ExpenseCategory", "expense_date": "date", "receipt": "file"},
 "unevaluated": [], "flags": [], "actor_dependent": false}
```

- `availability` is `available`, `available_with_input` or `unavailable`, with the `verdict` it would meet.
- `inputs` are the input types to build a form from; which are required is in `declaration`.
- `flags` are the warnings taking it would raise; `unevaluated`, the evaluators it would ask.
- A transition taking no input and asking no evaluator is decided for the actor asking: the request is simulated, as `check` would, so an agent sees an approval marked `taken_by: [human]` as `unavailable` with the refusal it would meet. **`actor_dependent: true`** marks one awaiting inputs that may still be refused for who takes it: `check` it with the inputs, or let the request answer.
- Every object offers the built-in `record`, `"creates": "label"`: a label anyone may put on any object, which metrics read. It is a request of the type `label`, its object named by the input `subject`: `{"actor": …, "type": "label", "transition": "record", "inputs": {"subject": "claim-7", "name": "checked"}}`, with an optional `note` and an optional `corrects`, naming an earlier label it corrects.

### An event

As `history`, `events` and `pull` answer it:

```json
{"position": 3, "object_id": "transfer-2", "transition": "settle", "from_state": "SENT", "to_state": "SETTLED",
 "changes_state": true, "actor_id": "operator", "actor_kind": "human", "principal": null, "declaration_version": 1,
 "recorded_at": "2026-10-10T09:31:02Z", "occurred_at": "2026-10-10T09:31:02Z", "imported": false,
 "payload": {"inputs": {}}, "cause": null, "context": null,
 "reads": {"objects": {}, "scans": {}, "now": "2026-10-10T09:31:02Z"},
 "consulted": {"cleared": {"bank.cleared(T-1)": "satisfied"}}, "as_of": {"cleared": "2026-10-10T09:31:02Z"},
 "retries": 0}
```

`payload.inputs` holds what the request sent, `reads` what its rules read, and `consulted` and `as_of`, by guard, each outside answer or metric value it used and when. `retries` counts the times the request waited out another process's lock on the store.

### Pages

`query`, `available`, `history`, `pull` and `export` answer at most `limit` items, from 1 to 1000, **100 unless asked**, with the `cursor` of the next page, `null` on the last; a page of `query`, `history`, `pull` or `export` also ends at 8 MiB, one item at least, with a cursor going on. Send the cursor back, with the same read, until it is `null`. A cursor continues only the read it came from: another `query` or `available` refuses it `malformed_cursor`. A page continues after the last object it gave, in the read's order, so an object created between pages is met once and none twice, unless an object's order key changes between pages.

`pull` and `export` page through a log that grows, so their cursor is never `null`: `more` is `true` where a page ended at its limit or its 8 MiB, so ask again at once, and `false` where it holds everything there is now, so the cursor you keep reads what is added later. Read `more`, not the page's length: a page ended by its bytes holds fewer than `limit` items while more wait. A page of `export` never ends among the rows that share one cursor, an `.attempts` source's rows of one transaction or an `.intervals` source's rows of one event, so it may pass `limit` by those rows. A metric's read and `exceptions` answer one page.

### Filters and order

A `filter` is a boolean expression of the flow's language over the type's members ([`declaration-syntax.md`](../design/declaration-syntax.md) §8):

- a state or an enumeration member by name: `state == Claim.SUBMITTED`;
- a reference by its id: `claim.id == "claim-4"`;
- money, numbers and text as literals: `amount > USD 100.00`, `claim.number == "EXP-00001"`;
- a time against `now`, since there is no timestamp literal: `submitted_at <= now - 30 days`;
- a set of literals for a list of alternatives, up to 1,000: `number in {"C-1", "C-2"}`.

A filter is refused `malformed_filter`, with a `reason` naming what it did but never a value an object holds, when it: does not parse; compares a member with a literal its type cannot hold (a time with text, a reference with a string, the state with text); is not a boolean or adds text to a number; gives `and`, `or`, `implies` or `not` an operand that is not a boolean, such as `title == "x" and title`; holds a literal past what its type holds; calls an evaluator or a metric; reads the request (`actor`, `inputs`, `this_event`); divides by a zero; nests deeper than 64 levels or has a tree deeper than 128; nests aggregates more than two deep; writes a set of more than 1,000 members; or holds more than 256 terms, a set of literals counting as one and a comparison of a member with a value three, so about 64 such comparisons chained by `or`.

`order` is a key or a list of keys, each ascending unless followed by ` desc`: `created_at`, `entered_at(<tracked member>)`, or an attribute holding one value. The object id breaks ties, and an absent value sorts last. Any other key is refused `unknown_order_key`.

A **metric's filter** reads its rows through the metric's item, such as `i.state == Claim.SUBMITTED` for `time_in_state`; one naming anything else is refused `malformed_filter`, and so is one past a query filter's bounds: more than 256 terms, aggregates nested more than two deep, a set of more than 1,000 members, or a metric called within it (ADR-0205 decision 15).

**A read that selects by a personal value is opted into** with `"reads_personal": true` in its body: a query whose `filter` or `order` reads one, a `lookup` by a personal identifier, `available`, `availability` and `check` where a guard or an invariant deciding the answer reads one, and a metric whose `filter` reads one or whose `bind` names a dimension that names a person. Without it, whoever asks, each is refused `personal`, before anything is evaluated. A personal value is one a flow marks `personal`, a derived value reading one, and, where an actor type marks its identity personal, any value of type `identity`. The gateway sets the opt-in where the caller may read by such a value; a request may set it too, so that a refusal by a personal value, a uniqueness or `identity_taken`, names the object holding it, which it otherwise leaves out (ADR-0206). Where an actor type marks its identity personal, a metric's rows name each actor by its object, never its identity.

## Subscriptions

A subscription is an object: `{"actor": …, "type": "Subscription", "transition": "subscribe", "inputs": {"target_type": "Claim"}}`, optionally with `target_family`, `transitions`, `changes_state_only`, `lagging_after` and `dead_after`. `revoke` ends it. It is refused `invalid_input`, the input `not_allowed`, where it would select nothing or its durations make no sense: a `target_type` no type of the installed version has, by its exact name; a transition in `transitions` no type it selects declares; a `lagging_after` or `dead_after` that is not positive; or a `dead_after` before its `lagging_after` (ADR-0205 decision 10).

| Operation | Keys | Answers |
|---|---|---|
| `pull` | `actor`, `subscription`; `cursor`, `limit` | `{"events": […], "settled": "<cursor>", "more": …, "refused": …}` |
| `acknowledge` | `actor`, `subscription`, `cursor` | `{"refused": …}`: everything up to the cursor is handled. `"0"` replays the log from its start; a cursor is a string, as the store hands it out. |

There is no push: a consumer polls `pull` and acknowledges what it has handled.

## Changing the flow, importing, and housekeeping

| Operation | Keys | Answers |
|---|---|---|
| `publish` | `actor`, `change`; `dry_run`, `expected_version` | `{"verdict": …, "report": {…}}`: a `DeclarationChange`'s dry run and impact report (`"dry_run": true`), or its approval at the version read, which installs the new version. An approver of a kind the installed version's `publishing.taken_by` does not name, the operator excepted, is refused `invariant_violated`, `publish_taken_by`, `delegable`. |
| `import_batch` | `actor`, `batch`; `disposition`, `dry_run` | `{"written": …, "findings": […], "refused_types": […], "refusal": …}` |
| `maintain` | `actor`, `task` | `{"moved": …, "refused": …}`. `task` is `{"name": "prune_attempts" \| "prune_idempotency" \| "archive_events", "before": "<timestamp>"}`. |

A **change** is an object: `{"type": "DeclarationChange", "transition": "draft", "inputs": {"source": {"<module>": "<its YAML>", "<module>.examples": "<its examples>"}, "source_ref": "git:…"}}`, then `submit`, which runs the dry run, then `publish` with `expected_version` to approve it. [`publish-and-import.md`](../design/publish-and-import.md) is the whole procedure.

A **batch** writes objects of `mirror` types, each referencing others by their legacy key:

```json
{"objects": [
   {"type": "PurchaseOrder", "legacy_key": "4300", "state": "OPEN",
    "attributes": {"number": "PO-04300", "supplier": "S-12", "legacy_key": "4300"},
    "created_at": "2025-11-02T08:00:00Z", "created_by_kind": "human",
    "entered": {"state": {"at": "2025-11-03T10:00:00Z", "by": "u20117", "by_kind": "human"}},
    "legacy_intervals": [{"dimension": "state", "value": "DRAFT", "entered_at": "2025-11-02T08:00:00Z",
                          "left_at": "2025-11-03T10:00:00Z", "entered_by": "jo@corp", "entered_by_kind": "human"}],
    "legacy_entries": [{"kind": "note", "occurred_at": "2025-11-02T09:00:00Z", "payload": {"text": "…"},
                        "about": {"customer": "Customer:C-7"}}],
    "silent_spans": [["2025-12-20T00:00:00Z", "2026-01-05T00:00:00Z"]]}],
 "sequences": [{"sequence": "order_number", "last": 4300, "scope": ""}],
 "legacy_kinds": {"note": ["text"]}}
```

`type`, `legacy_key` and `state` are required; the rest is what the old record can say: when the object was created and by what kind of actor, when it took its current state or value and who changed it (`entered`), its earlier values (`legacy_intervals`), its audit rows (`legacy_entries`, with `legacy_kinds` listing, for each kind, the fields an erasure keeps, so list none that may hold personal data; a kind it does not list keeps nothing), the spans the record is silent over, and each sequence's last number. A batch's `sequences` raise only a sequence a module declares, to a `last` from 0 to 2⁶³ − 3; anything else refuses the batch, which then writes nothing. The **disposition** is a dry run's `findings`, each given a `disposition`: `clean_upstream`, `admit` (an invariant), `exclude` or `drop` (a reference or a field), optionally with `decided_by` and `reason`. A reference to a legacy key neither the batch nor the store has is a finding, `"<Type>.<reference>: names nothing"`, so one dry run lists them all.

## Upgrade

| Operation | Keys | Answers |
|---|---|---|
| `upgrade` | `actor`; `dry_run` | the steps from the store's engine release to this one, run, or with `"dry_run": true` run and rolled back |

A service started on a store an earlier release made answers every operation but `upgrade` with `503 engine_mismatch`. The store's operator runs it:

```json
POST /v1/upgrade
{"actor": {"id": "operator"}, "dry_run": true}

{"from": "objectflow-core 0.2.0", "to": "objectflow-core 1.0.0", "dry_run": true, "upgraded": false,
 "steps": [{"release": "objectflow-core 1.0.0", "changes": "the store records each restore of a copy in of_restore, …"}],
 "restored": null}
```

`restored` is the restore the upgrade did, where the store was a backup's marked copy: its margin and each counter's gap, as the start says one (ADR-0207). Another actor is answered `409`. A store already at this release answers `"upgraded": false` with no steps. A store of a release this one does not upgrade from, a later one included, does not start. [Operate](operate.md#upgrading-the-engine) gives the procedure.

## The evaluator endpoint

A deployment posts each evaluator call to the endpoint `--evaluator <name>=<url>` names, with `Authorization: Bearer <token>` where `--evaluator-token` gives one:

```json
{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-0042"]}
```

The arguments come in the order the function declares, in the notation above. Answer `200` with `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}`. A check still pending answers `{"verdict": "unanswered"}`. Anything else, including no answer within `--evaluator-timeout`, makes the guard unknown, refused `temporal`, and the service logs why. An `unsatisfied` answer is refused with the condition's own remedy.

## Sandbox controls

A sandbox serves these beside every operation; a deployment serves none of them.

| Control | Keys | Answers |
|---|---|---|
| `/sandbox/load` | `files` (each `name` and `text`, imported modules first), `attempt_retention`, `idempotency_retention` | `{"loaded": true, "modules": […], "notices": …, "types": […], "evaluators": […], "now": …}`. It installs the flow unchecked and empties the store. |
| `/sandbox/reset` | none | `{"now": …}`: every object gone, the flow kept |
| `/sandbox/clock` | `advance` (a duration) or `set` (a timestamp) | `{"now": …}` |
| `/sandbox/evaluators` | `answers`: each `"<evaluator>.<function>"` to `satisfied`, `unsatisfied` or `unanswered` | the answers held |
