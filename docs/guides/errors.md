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
| `unsatisfied` | A guard failed. | `clause` (the rule), `unknown` (`true` if it read a value nobody has yet, rather than finding it false), `objects` (what the remedy points at, by id), `proposable`, `consulted` (the outside answers and metric values it decided on) |
| `invariant_violated` | The change would break a rule that must always hold. | `failures` (each `object_id`, `type`, `invariant`, `remedy`; `object_id` is `null` for an object the request was creating), `objects` (the objects in conflict, such as another holder of a unique value), `proposable` |
| `invalid_input` | An input is wrong before any rule runs. | `inputs` (the names at fault), `reasons` (each one's reason, below) |
| `unavailable` | The object's state does not allow this transition. | `state` |
| `unknown_transition` | The type has no transition of that name. | `transition` |
| `not_requestable` | The transition runs only as part of another. | `parents` (the transitions that run it) |
| `not_proposable` | A proposal (`"propose": true`) of a transition not marked `proposable`. | none |
| `stale` | The object moved on, or the store stayed locked. | `cause`: `expected_version` with `expected` and `actual` (re-read, then decide again), or `retries_exhausted` (remedy `temporal`: another process held the store's write lock too long) |
| `not_found` | The object does not exist. | none |
| `over_limit` | A loop or cascade would pass its declared bound. Nothing was done. | `loop`, `bound` |
| `call_refused` | A transition this one calls on another object was refused. | `step`, `object_id`, `verdict` (that transition's own verdict, whole) |

### Input reasons

`invalid_input` gives each input's reason in `reasons`:

| Reason | The input… |
|---|---|
| `unknown_input` | is not one the transition takes. |
| `missing` | is required and was not sent, or was `null`. |
| `wrong_type` | is a different JSON type than its declared type takes. |
| `malformed` | has the right JSON type but the wrong form: a bad date, an identity with invisible characters. |
| `wrong_currency` | is money in another currency than the declared one. |
| `no_such_object` | names an object that does not exist, or one of another type. |
| `out_of_range` | is past what its type holds: too many digits or decimal places, a string over 64 KiB. |
| `not_allowed` | is a value the transition does not accept here, such as a repeated element of a set. |

A required input whose attribute is optional elsewhere is refused another way: `unsatisfied` with the clause `<attribute>_provided`. Map both shapes to the same form field.

## Clauses and invariants the engine adds

Besides the rules a flow declares, the engine adds some of its own. A refusal names them like any other rule.

| Name | Remedy | Refuses |
|---|---|---|
| `actor_known` | `dependent` | an actor the store has no object for. Create its actor object first. |
| `actor_active` | `dependent` | an actor, or a `principal`, whose object is in a final state: someone who left, an agent retired. |
| `<attribute>_provided` | `self_serviceable` | a required input left out, where its attribute is optional elsewhere. |
| `occurred_within` | `self_serviceable` | an `occurred_at` further back than the transition's `backdating_limit` allows. |
| `whole_open` | `self_serviceable` | moving a part to a whole that is in a final state. |
| `successor_not_superseded` | `self_serviceable` | naming as successor the object itself, or one already superseded. |
| `corrects_current` | `self_serviceable` | a correction of an observation that was already corrected. |
| `not_erased` | `unreachable_from_here` | writing a personal attribute again on an object whose personal data was erased. |
| `subject_open` | `unreachable_from_here` | recording a label or an observation on an object in a final state. |
| `subject_owned` | `unreachable_from_here` | recording on an object another system owns (a `mirror` type). |
| `value_in_range` | `unreachable_from_here` | an effect whose arithmetic has no value: past its type's bound, a division by zero, a date past 9999. |
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
| `malformed_filter` | `query`, `metric` | The filter cannot be answered. `reason` says why: it does not parse; compares a member with a literal its type cannot hold; is not a boolean; calls an evaluator or a metric; reads the request (`actor`, `inputs`); divides by zero; nests too deep; holds more than 256 terms; or, for a metric, reads something other than the metric's item. |
| `unknown_order_key` | `query` | An `order` key that is not `created_at`, `entered_at(<tracked member>)` or an attribute holding one value. |
| `unknown_field` | `query` | A `fields` entry the type does not have. |
| `malformed_cursor` | `query`, `available` | A cursor that did not come from this read. Start again without one. |
| `unknown_metric` | `metric` | The flow declares no such metric. |
| `unknown_dimension` | `metric` | `keep` or `bind` names a dimension the metric does not have. |
| `filter_on_combined` | `metric` | A filter on a combined metric, which takes none. |
| `no_window` | `metric` | `over` on a metric that has no time window. |
| `personal` | `metric` | The filter reads a value marked `personal`. |
| `unknown_source` | `export` | No such source. Sources are `log`, an observation kind, or `<Type>.intervals`, `.attempts`, `.transitions` or `.labels`. |
| `invalid_cursor` | `export`, `pull`, `acknowledge` | A cursor below 0 or past the end of the log. |
| `unknown` | `pull`, `acknowledge` | No such subscription. |
| `revoked` | `pull`, `acknowledge` | The subscription was revoked. |
| `unknown_task` | `maintain` | No such task. Tasks are `prune_attempts`, `prune_idempotency` and `archive_events`. |
| `inside_retention` | `maintain` | The `before` time is inside the retention the service was started with. |
| `no_retention` | `maintain` | The service was started without a retention for this task, so it prunes nothing. |

`maintain` also refuses an actor the way a request does, with `actor_known` or `actor_active`.

## Faults

A fault's body is always `{"fault": "<reason>", "code": "<code>"}`.

| `code` | Status | Means | What to do |
|---|---|---|---|
| `bad_request` | `400` | The body is not a JSON object, names a key the operation does not take, names a key twice, has a key of the wrong JSON type, lacks a required key, or is over 2 MiB. | Fix the request. Unknown keys are refused so a typo is never read as "absent". |
| `not_found` | `404` | No such operation. | Check the name. |
| `method_not_allowed` | `405` | An operation sent with anything but `POST`, or `/health` or `/metrics` with anything but `GET`. | Use the right method. |
| `slow_body` | `408` | The body did not arrive within the idle timeout. | Send it again. |
| `conflict` | `409` | A sandbox with no flow loaded. | Load one. |
| `key_reused` | `409` | This actor already used the idempotency key for a different request. | Use a new key for a new request. |
| `too_large` | `413` | Reserved for a request too large to read. Today a body over 2 MiB is `400 bad_request`, and an over-long URL is refused `414` by the HTTP layer, with no body. | Send a smaller request. |
| `not_run` | `501` | A construct this engine does not run yet, such as `archive_events`. | Don't use it. |
| `engine_mismatch` | `503` | The store was made by another engine release. Only `upgrade` is served until it is upgraded. | Run the upgrade (see [Operate](operate.md)). |
| `read_timeout` | `503` | A read passed its deadline (30 s by default), waited that long for a turn, or was stopped so the store's log could be cut after an erasure. Nothing was changed. | Narrow the read with an indexed filter or a smaller page, or send it again. |
| `busy` | `503` | Every connection is busy, or this idle one was closed to make room. Nothing was done. | Send it again on a new connection. |
| `disk_full` | `503` | The store's disk is full, so the write was not made. | The operator frees space; then send it again. |
| `store_replaced` | `503` | The store's file was moved, removed or replaced under the running service. | The operator restarts the service on the right file. |
| `unavailable` | `503` | The service cannot answer for another reason. | Retry later. |
| `internal` | `500` | The service's own fault. | Report it, with the log line. |

`GET /health` answers `503` with one of these codes when the deployment needs an operator: `upgrade_due`, `disk_low` (under 64 MiB free), `store_replaced`, `log_blocked` (a log line waited more than 5 s to be written) and `writes_stalled` (another process's lock stopped the last write). See [Operate](operate.md).

## Retrying safely

- Send an `idempotency_key` with every write. A retry with the same key and the same body returns the first answer, `"replayed": true`, and applies nothing twice. The key belongs to the actor, so retry under the same actor.
- Retry a `temporal` refusal, a `busy`, `read_timeout` or `slow_body` fault, and a write that got no answer at all. Don't retry `unreachable_from_here`.
- A `stale` with cause `expected_version` means someone else changed the object. Re-read it and let the person, or the agent, decide again before resending.
