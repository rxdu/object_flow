# Errors and refusals

Every way the engine says no, and what to do about each. Use this page to write a client's error handling, or to tell an agent how to recover.

## How to read an answer

The engine answers in one of three shapes:

| You get | It means | Branch on |
|---|---|---|
| `200` with a verdict, `{"kind": …}` | The request was understood and decided. `satisfied` means it was applied. Any other `kind` is a **refusal**: nothing was written. | `kind`, then `remedy` |
| `200` with `"refused": "<code>"` | A read could not be answered as asked. It carries no items. | `refused` |
| `4xx` or `5xx` with `{"fault": …, "code": …}` | The request was malformed, or the service could not answer it. | `code` |

**A refusal is not an error.** It comes back as `200` because the engine did its job: it checked the request and said why it cannot apply. Treat `fault` text as a message for people. It may change between releases, so branch only on `code`, `kind`, `remedy` and `refused`.

A client's whole decision fits in a few lines:

```python
if status != 200:
    handle_fault(answer["code"])           # see "Faults" below
elif answer["kind"] == "satisfied":
    done(answer["object_id"], answer["version"])
else:
    act_on(answer["remedy"], answer)       # see "Remedies" below
```

A later release may add verdict kinds, reasons and codes, but never a sixth remedy. **Handle a `kind` you don't know by its `remedy`.**

## Remedies

Every refusal carries a `remedy`: what the caller can do about it.

| `remedy` | Meaning | A person's screen | An agent |
|---|---|---|---|
| `self_serviceable` | Change the request: fix an input, or re-read and send again. | Mark the field, or refresh. | Fix it and retry. |
| `delegable` | Someone else must act: another person, or whoever approves a proposal. | Route it to them. | Hand it to a person, or file a proposal where `proposable` is `true`. |
| `temporal` | Wait: an outside system has not answered, or a time has not come. | Say "try again later". | Retry later. |
| `dependent` | Another object must change first. `objects` names it. | Link to those objects. | Work on those objects first. |
| `unreachable_from_here` | Nothing the caller can do will help. | Hide the action. | Stop. Don't retry. |

## Verdict kinds

| `kind` | What happened | Fields besides `kind` and `remedy` |
|---|---|---|
| `satisfied` | Applied. | `object_id`, `version`, `events` (positions, the request's own first), `replayed` (`true` for an idempotent retry), `flags` (warnings raised) |
| `unsatisfied` | A guard failed. | `clause` (the rule), `unknown` (`true` if it read a value nobody has yet, rather than finding it false), `objects` (what the remedy points at, by id), `proposable`, `inputs` (each input the rule reads, in the order the transition declares them, so a form marks their fields; none for a rule over no input), `consulted` (the outside answers and metric values it decided on) |
| `invariant_violated` | The change would break a rule that must always hold. | `failures` (each `object_id`, `type`, `invariant`, `remedy`; `object_id` is `null` for an object the request was creating), `objects` (the objects in conflict, such as another holder of a unique value), `proposable` |
| `invalid_input` | An input is wrong before any rule runs. | `inputs` (the names at fault), `reasons` (each one's reason, below) |
| `unavailable` | The object's state does not allow this transition. | `state` |
| `unknown_transition` | The type has no transition of that name. | `transition` |
| `unknown_type` | The installed version has no type of that name, the request's creation naming it; its remedy is `unreachable_from_here` where no version is installed, since only the operator's publish helps, and `self_serviceable` otherwise (ADR-0210 decision 6). | `type` |
| `not_requestable` | The transition runs only as part of another, or through its own operation, as a change's `publish` runs through `publish`. | `parents` (the transitions or operation that run it) |
| `not_proposable` | A proposal (`"propose": true`) of a transition not marked `proposable`, or of a change's or a proposal's own transitions. | none |
| `stale` | The object moved on, or the store stayed locked. | `cause`: `expected_version` with `expected` and `actual` (re-read, then decide again; an approval's `target_version` is refused the same way), or `retries_exhausted` (remedy `temporal`: another process held the store's write lock too long) |
| `not_found` | The object does not exist, or is not of the `type` named beside its id, a proposal's or a change's included. | none |
| `over_limit` | A loop or cascade would pass its declared bound. Nothing was done. | `loop`, `bound` |
| `call_refused` | A transition this one calls on another object was refused. | `step`, `object_id`, `verdict` (that transition's own verdict, whole) |

### Input reasons

`invalid_input` gives each input's reason in `reasons`:

| Reason | The input… |
|---|---|
| `unknown_input` | is not one the transition takes. |
| `missing` | is required and was not sent, or was `null`. |
| `wrong_type` | is a different JSON type than its declared type takes, or a string naming no member of the enumeration it takes. |
| `malformed` | has the right JSON type but the wrong form: a bad date, an identity with invisible characters, a decimal with more places than its scale, which is never rounded, a set repeating an element. |
| `wrong_currency` | is money in another currency than the declared one. |
| `no_such_object` | names an object that does not exist, or one of another type. |
| `out_of_range` | is past what its type holds: more digits than a decimal or an amount holds, an integer past 64 bits, a string over 64 KiB. |
| `not_allowed` | is a value of its type the transition does not take here: an assertion's `to` or `admits` it does not list, an observation's `corrects` naming another subject's, a subscription selecting nothing. |

A required input whose attribute is optional elsewhere is refused another way: `unsatisfied` with the clause `<attribute>_provided`, whose `inputs` names that input, as an `invalid_input`'s does.

## Clauses and invariants the engine adds

Besides the rules a flow declares, the engine adds some of its own. A refusal names them like any other rule.

| Name | Remedy | Refuses |
|---|---|---|
| `actor_known` | `dependent` | an actor the store has no object for. Create its actor object first. |
| `actor_active` | `dependent` | an actor, or a `principal`, whose object is in a final state: someone who left, an agent retired. |
| `<attribute>_provided` | `self_serviceable` | a required input left out, where its attribute is optional elsewhere. |
| `occurred_within` | `self_serviceable` | an `occurred_at` further back than the transition's `backdating_limit` allows. |
| `whole_open` | `self_serviceable` | moving a part to a whole that is in a final state, by an input, an effect or a creation from another whole's transition. |
| `identity_unchanged` | `unreachable_from_here` | changing an actor's identity after its creation: the record names actors by it. |
| `successor_not_superseded` | `self_serviceable` | naming as successor the object itself, or one already superseded. |
| `corrects_current` | `self_serviceable` | a correction of an observation that was already corrected. |
| `proposal_pending` | `dependent` | filing a proposal of a transition on an object where the same proposer has one pending. `objects` names it: withdraw it, or wait for its decision. Another proposer's is taken. |
| `not_erased` | `unreachable_from_here` | writing a personal attribute again on an object whose personal data was erased, by an input or a step at any depth, a loop's included. |
| `subject_open` | `unreachable_from_here` | recording a label or an observation on an object in a final state. |
| `subject_owned` | `unreachable_from_here` | recording on an object another system owns (a `mirror` type). |
| `value_in_range` | `unreachable_from_here` | an arithmetic past its type's bound, wherever the request evaluates it, a guard, an invariant, a loop's filter or an effect: an integer past 64 bits, a decimal past its digits, a date past 9999, a date moved by part of a day. An effect's division by zero, or an operand absent (`count + 1` where `count` is unset: give it a `default`, or guard the transition with `count is not null`); a plain copy of an absent value, `held := peg`, clears an optional attribute instead, and is refused only where the attribute is required. A value written that its attribute's type does not hold: text to a number, a decimal with a fraction to an integer. |
| `<attribute>_unique` | (invariant) | a second object holding a value declared `unique`. `objects` names the holder. |
| `identity_taken` | (invariant) | an actor identity another actor already holds, compared ignoring the case of ASCII letters. |
| `<state>_invariant` | (invariant) | entering a state without its `required_attributes`. |
| `<transition>_taken_by` | `delegable` | an actor of a kind the transition's `taken_by` does not name, such as an agent approving where only a person may. |
| `publish_taken_by` | `delegable` | approving a new flow version by a kind the installed version's `publishing.taken_by` does not name. |

### Publishing a new version

A `DeclarationChange` is refused with these clauses while it is submitted and approved:

| Clause | Remedy | Means |
|---|---|---|
| `dry_run_clean` | `unreachable_from_here` | Its dry run found something fatal, or an example failed. Read why with a `publish` preview (`"dry_run": true`), then `revise` it. |
| `current` | `unreachable_from_here` | It was drafted against a version that is no longer installed. Draft it again. |
| `fatal` | `unreachable_from_here` | Re-run at approval, the dry run now finds something fatal. |
| `impact_unchanged` | `unreachable_from_here` | Its impact now reaches objects the approver was not shown. `refresh` it, read the report again, and approve the version you read. |
| `decision_unresolved` | `unreachable_from_here` | Its report still holds a decision nobody resolved. |

## Read refusals

A read that cannot be answered says why in `refused`, with no items. Some carry a `reason`, a sentence for people.

| `refused` | From | Means |
|---|---|---|
| `unknown_type` | `query`, `available`, `exceptions`, `diagnostics` | The flow has no such type. |
| `unknown_transition` | `available` | The type has no such transition. |
| `not_requestable` | `available` | The transition runs only as part of another. |
| `value_in_range` | `get`, `query`, `metric` | A value the read derives or filters by is past what its type holds: an integer past 64 bits, a time past the calendar. `object_id` names the object, where one is known; the answer names no value. `availability` and `check` say it as a transition's verdict, as a request would be refused. |
| `malformed_filter` | `query`, `metric` | The filter cannot be answered. `reason` says why: it does not parse; compares a member with a literal its type cannot hold; is not a boolean; calls an evaluator or a metric; reads the request (`actor`, `inputs`); divides by zero; nests too deep; holds more than 256 terms (about 64 comparisons chained by `or`); or, for a metric, reads something other than the metric's item. |
| `unknown_order_key` | `query` | An `order` key that is not `created_at`, `entered_at(<tracked member>)` or an attribute holding one value. |
| `unknown_field` | `query` | A `fields` entry the type does not have. |
| `malformed_cursor` | `query`, `available` | A cursor that did not come from this read. Start again without one. |
| `unknown_metric` | `metric` | The flow declares no such metric. |
| `unknown_dimension` | `metric` | `keep` or `bind` names a dimension the metric does not have. |
| `filter_on_combined` | `metric` | A filter on a combined metric, which takes none. |
| `no_window` | `metric` | `over` on a metric that has no time window. |
| `personal` | `query`, `lookup`, `available`, `availability`, `check`, `metric` | The read selects by a personal value, and its body does not set `"reads_personal": true`: a query's filter or order, a lookup by a personal identifier, a transition decided by a personal member, a metric's filter or its binding of a dimension that names a person. A personal value is one a flow marks `personal`, and, where an actor type marks its identity personal, any value of type `identity`. The gateway sets the opt-in where the caller may read by such a value (`wire.md`). |
| `unknown_source` | `export` | No such source. Sources are `log`, an observation kind, or `<Type>.intervals`, `.attempts`, `.transitions` or `.labels`. |
| `invalid_cursor` | `export`, `pull`, `acknowledge` | A cursor below 0, past the end of the log, or in the gap a restore skipped, which the restored copy never saw (ADR-0207); for `<Type>.attempt_counts`, a day not over yet or one a restore recounts. A consumer holding a cursor from before a restore reconciles what it read after the copy's last record, which the service's start names, then reads again: `pull` without a cursor, and `export` from the position the restore's record names (`operate.md`, Backups and restore). |
| `unknown` | `pull`, `acknowledge` | No such subscription. |
| `revoked` | `pull`, `acknowledge` | The subscription was revoked. |
| `stale` | `acknowledge` | Another process held the store's write lock past the busy timeout and its retries, as a request is refused `stale` with cause `retries_exhausted`; nothing was written. Send it again. |
| `unknown_task` | `maintain` | No such task. Tasks are `prune_attempts`, `prune_idempotency` and `archive_events`. |
| `inside_retention` | `maintain` | The `before` time is inside the retention the service was started with. |
| `no_retention` | `maintain` | The service was started without a retention for this task, so it prunes nothing. |

`maintain` also refuses an actor the way a request does, with `actor_known` or `actor_active`.

## Faults

A fault's body is always `{"fault": "<reason>", "code": "<code>"}`.

| `code` | Status | Means | What to do |
|---|---|---|---|
| `bad_request` | `400` | The body is not a JSON object, names a key the operation does not take, names a key twice, has a key of the wrong JSON type, lacks a required key, or is over 2 MiB; or its text holds U+0000 outside an input's or an attribute's value, or a name it gives is over 1 KiB (an actor's id or principal, a type, a transition, an object's id, a legacy or an idempotency key). | Fix the request. Unknown keys are refused so a typo is never read as "absent". |
| `not_found` | `404` | No such operation, or, for `declaration`, no such type. | Check the name. |
| `method_not_allowed` | `405` | An operation sent with anything but `POST`, or `/health` or `/metrics` with anything but `GET`. | Use the right method. |
| `slow_body` | `408` | The body did not arrive within the idle timeout. | Send it again. |
| `conflict` | `409` | A sandbox with no flow loaded, or an `upgrade` asked by an actor that is not the store's operator. On a store on PostgreSQL also: a version whose generated names would pass the 63 bytes PostgreSQL keeps of a name, or a value of a unique, indexed or identity attribute past the 2,704 bytes an index's entry holds. | Load one, or ask as the operator; shorten the type's or attribute's name, or the value. |
| `key_reused` | `409` | This actor already used the idempotency key for a different request. | Use a new key for a new request. |
| `too_large` | `413` | Reserved for a request too large to read. Today a body over 2 MiB is `400 bad_request`, and an over-long URL is refused `414` by the HTTP layer, with no body. | Send a smaller request. |
| `not_run` | `501` | A construct this engine does not run yet, such as `archive_events`, or one it stopped: derived attributes read more than 64 deep, each reading the next, which a version installed before the checkers bounded that depth may hold. | Don't use it; publish a version the checkers accept. |
| `engine_mismatch` | `503` | The store was made by another engine release. Only `upgrade` is served until it is upgraded. | Run the upgrade (see [Operate](operate.md)). |
| `read_timeout` | `503` | A read passed its deadline (30 s by default), waited that long for a turn, or was stopped so the store's log could be cut after an erasure. Nothing was changed. | Narrow the read with an indexed filter or a smaller page, or send it again. |
| `busy` | `503` | Every connection is busy, or this idle one was closed to make room. Nothing was done. | Send it again on a new connection. |
| `disk_full` | `503` | The store's disk is full, so the write was not made. | The operator frees space; then send it again. |
| `store_replaced` | `503` | The store's file, or its `-wal`, was moved, removed or replaced under the running service. Every later read and write is refused, and the stop skips its checkpoint, until a restart (ADR-0205 decision 8). | The operator stops the service, keeps its files, and restarts it on the right file, or restores the store (see [Operate](operate.md#backups-and-restore)). |
| `store_unreadable` | `503` | `/health` could not read the store's file as a store, the file no longer begins as a SQLite database, or a read or a write met SQLite's words for a corrupt file, as after one written over in place under the running service. Every later read and write is refused, and the stop skips its checkpoint, until a restart (ADR-0205 decision 8). | The operator stops the service and restores the store (see [Operate](operate.md#backups-and-restore)). |
| `store_connection_discarded` | `503` | A rollback on a SQLite store failed, so the service gave up the store's connection with whatever it held uncommitted, rather than run another statement on it, where a write could join that transaction or a read see what it never committed. Every later read and write is refused until a restart (ADR-0228 decision 3). On PostgreSQL the next write opens the store again instead, as for a lost connection. | The operator stops the service, checks the store's disk, and starts it again. |
| `unavailable` | `503` | The service cannot answer for another reason. On a store on PostgreSQL also: the connection to the database was lost, the next write opening it again; another session held a table past the busy timeout, a publish beside a backup's dump; or the database is not the one the store was in, a standby a failover promoted or a restore in its place, every later read and write refused until a restart. | Retry later; for the last, the operator marks the database a copy and restarts the service ([Operate](operate.md)). |
| `internal` | `500` | The service's own fault. | Report it, with the log line. |

`GET /health` answers `503` with one of these codes when the deployment needs an operator: `upgrade_due`, `disk_low` (under 64 MiB free), `store_replaced`, `store_unreadable`, `store_connection_discarded` (a rollback failed and the store's connection was given up), `disk_full` (the last write failed for a full volume, and none has committed since), `log_blocked` (a log line waited more than 5 s to be written), `writes_stalled` (a write waits for another process's lock, or the last one was refused after its retries), `store_stalled` (the store did not answer `/health`'s read within 5 s, a database or volume that stopped answering) and `writes_failing` (three writes or more in a row answered `500`). See [Operate](operate.md).

## Retrying safely

- Send an `idempotency_key` with every write. A retry with the same key and the same body returns the first answer, `"replayed": true`, and applies nothing twice. The key belongs to the actor, so retry under the same actor.
- Retry a `temporal` refusal, a `busy`, `read_timeout` or `slow_body` fault, and a write that got no answer at all. Don't retry `unreachable_from_here`.
- A read you give up on stops at its next step and gives back its place in its pool, so sending it again does not stack reads nobody waits for; still give a read a timeout above the service's `--read-timeout`, so one that would answer is not abandoned ([Operate](operate.md)). A write you give up on runs to its end: retry it under its key.
- A `stale` with cause `expected_version` means someone else changed the object. Re-read it and let the person, or the agent, decide again before resending.
