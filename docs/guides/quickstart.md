# Quickstart

Run the engine, load a small flow, and make your first requests in about ten minutes. You'll see a request applied, refused with a remedy, and read back with its history.

**You need:** Linux or macOS, `git`, `curl`, `jq`, Python 3.10 or later, and [rustup](https://rustup.rs) (it installs the pinned Rust toolchain on the first build). Run every command from the repository's root.

## 1. Build and start the engine

```sh
(cd core && cargo build --release -p objectflow-service)
core/target/release/objectflow-service --port 8712
```

The service prints `listening on http://127.0.0.1:8712`, then one line per request. Started without `--store`, it runs a **sandbox**: an in-memory store with a clock that starts at `2026-01-01T00:00:00Z` and moves only when you tell it. Nothing in a sandbox survives a restart.

Leave it running and open a second terminal. Define a helper that posts one operation:

```sh
OF=http://127.0.0.1:8712
of() { curl -s "$OF/v1/$1" -H 'Content-Type: application/json' -d "$2"; echo; }
```

Every operation is `POST /v1/<operation>` with a JSON body. The helper takes the operation's name and the body.

## 2. Write a flow

A flow declares the types of object the engine governs, the states each moves through, and the transitions between them. Save this as `leave.yaml`:

```yaml
# leave.yaml
module: leave
categories: [live, closed]

types:
  Employee:
    description: A person who asks for leave or approves it.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live, description: Works here. }
      GONE: { category: closed, final: true, description: Has left. }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
      depart: { kind: external, from: ACTIVE, to: GONE }

  LeaveRequest:
    description: A request for days off.
    tracking: record
    attributes:
      days: { type: int }
    states:
      DRAFT: { category: live, description: Being written. }
      SUBMITTED: { category: live, description: Waiting for a decision. }
      APPROVED: { category: closed, final: true, description: Granted. }
      REJECTED: { category: closed, final: true, description: Turned down. }
    conditions:
      reasonable:
        description: A request asks for 1 to 20 days.
        expression: inputs.days >= 1 and inputs.days <= 20
        remedy: self_serviceable
    transitions:
      open: { kind: initial, to: DRAFT, required_inputs: [days], guards: { reasonable: deny } }
      submit: { kind: external, from: DRAFT, to: SUBMITTED }
      approve: { kind: external, from: SUBMITTED, to: APPROVED }
      reject: { kind: external, from: SUBMITTED, to: REJECTED }
```

`Employee` is an **actor** type: its `login` is an `identity`, so an employee can make requests. `reasonable` is a **condition**, and `open` uses it as a **guard** that denies a request it fails. Its `remedy` tells the caller what to do about a refusal: `self_serviceable` means "change your request". The guard reads `inputs.days`, the value sent, because a guard runs before the transition writes anything.

Check the flow. A sandbox's load runs the checker too, and refuses a flow it finds wrong with the same findings, but the checker also runs the flow's examples, which a load does not:

```sh
scripts/setup/python-env          # once: the checker's Python environment
scripts/.venv/bin/python scripts/check-flows.py leave.yaml
```

```text
0 fatal · 0 notices
```

## 3. Load it

```sh
jq -Rs '{files: [{name: "leave.yaml", text: .}]}' leave.yaml \
  | curl -s "$OF/sandbox/load" -H 'Content-Type: application/json' -d @-; echo
```

```json
{"loaded": true, "modules": ["leave"], "notices": 0, "now": "2026-01-01T00:00:00Z", "types": ["Employee", "LeaveRequest"], "evaluators": []}
```

## 4. Add the people

A fresh store knows one actor, `operator`. Every other actor is an object in the store, so create the employees as the operator:

```sh
of request '{"actor": {"id": "operator"}, "type": "Employee", "transition": "join", "inputs": {"login": "ana"}}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "employee-1", "version": 1, "events": [2], "created": ["employee-1"], "replayed": false}
```

```sh
of request '{"actor": {"id": "operator"}, "type": "Employee", "transition": "join", "inputs": {"login": "bo"}}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "employee-2", "version": 1, "events": [3], "created": ["employee-2"], "replayed": false}
```

A request names its actor by identity (`ana`) or by object id (`employee-1`). The engine records who acted. It never decides whether they were allowed to: that is your application's job.

## 5. Make requests, and meet refusals

Ana asks for 30 days:

```sh
of request '{"actor": {"id": "ana"}, "type": "LeaveRequest", "transition": "open", "inputs": {"days": 30}}'
```

```json
{"kind": "unsatisfied", "remedy": "self_serviceable", "clause": "reasonable", "unknown": false, "objects": [], "proposable": false, "inputs": ["days"], "consulted": {}}
```

**A refusal is an answer, not an error.** It comes back as HTTP `200`. Its `kind` says what happened, `clause` names the rule, and `remedy` says what to do next. Ana asks for 5 days instead, then submits:

```sh
of request '{"actor": {"id": "ana"}, "type": "LeaveRequest", "transition": "open", "inputs": {"days": 5}}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-3", "version": 1, "events": [4], "created": ["leaverequest-3"], "replayed": false}
```

```sh
of request '{"actor": {"id": "ana"}, "object_id": "leaverequest-3", "transition": "submit"}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-3", "version": 2, "events": [5], "created": [], "replayed": false}
```

A value of the wrong form is refused before any rule runs:

```sh
of request '{"actor": {"id": "ana"}, "type": "LeaveRequest", "transition": "open", "inputs": {"days": "five"}}'
```

```json
{"kind": "invalid_input", "remedy": "self_serviceable", "inputs": ["days"], "reasons": {"days": "malformed"}}
```

## 6. Ask what is possible

`availability` evaluates the real rules against the real data and says what each transition offers an actor right now. Screens render their buttons from it, and agents plan with it:

```sh
of availability '{"actor": {"id": "bo"}, "id": "leaverequest-3"}' | jq -c '.[] | {transition, availability}'
```

```json
{"transition": "submit", "availability": "unavailable"}
{"transition": "approve", "availability": "available"}
{"transition": "reject", "availability": "available"}
{"transition": "record", "availability": "available_with_input"}
```

`record` is built in: every object can carry labels. Someone the store does not know is refused, with the remedy `dependent`, because another object (their `Employee`) must exist first:

```sh
of request '{"actor": {"id": "carl"}, "object_id": "leaverequest-3", "transition": "approve"}'
```

```json
{"kind": "unsatisfied", "remedy": "dependent", "clause": "actor_known", "unknown": false, "objects": [], "proposable": false, "inputs": [], "consulted": {}}
```

## 7. Write safely

Send the version you read as `expected_version`, so a decision made on an old screen cannot overwrite a change its user never saw:

```sh
of request '{"actor": {"id": "bo"}, "object_id": "leaverequest-3", "transition": "approve", "expected_version": 1}'
```

```json
{"kind": "stale", "remedy": "self_serviceable", "cause": "expected_version", "expected": 1, "actual": 2}
```

Send an `idempotency_key` with every write. A retry after a timeout then returns the first answer, marked `replayed`, instead of applying twice:

```sh
of request '{"actor": {"id": "bo"}, "object_id": "leaverequest-3", "transition": "approve", "expected_version": 2, "idempotency_key": "approve-lr3"}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-3", "version": 3, "events": [6], "created": [], "replayed": false}
```

```sh
of request '{"actor": {"id": "bo"}, "object_id": "leaverequest-3", "transition": "approve", "expected_version": 2, "idempotency_key": "approve-lr3"}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-3", "version": 3, "events": [6], "created": [], "replayed": true}
```

A transition the object's state does not allow is `unavailable`, and its remedy, `unreachable_from_here`, says no retry will help:

```sh
of request '{"actor": {"id": "ana"}, "object_id": "leaverequest-3", "transition": "reject"}'
```

```json
{"kind": "unavailable", "remedy": "unreachable_from_here", "state": "APPROVED"}
```

## 8. Read it back

```sh
of get '{"id": "leaverequest-3"}'
```

```json
{"id": "leaverequest-3", "type": "LeaveRequest", "state": "APPROVED", "version": 3, "declaration_version": 1, "created_at": "2026-01-01T00:00:00Z", "created_by_kind": "human", "created_dated": true, "attributes": {"days": 5}, "recorded_from": "2026-01-01T00:00:00Z", "open": false}
```

Every change is an event, with who made it:

```sh
of history '{"id": "leaverequest-3"}' | jq -c '.events[] | {position, transition, from_state, to_state, actor_id}'
```

```json
{"position": 4, "transition": "open", "from_state": null, "to_state": "DRAFT", "actor_id": "ana"}
{"position": 5, "transition": "submit", "from_state": "DRAFT", "to_state": "SUBMITTED", "actor_id": "ana"}
{"position": 6, "transition": "approve", "from_state": "SUBMITTED", "to_state": "APPROVED", "actor_id": "bo"}
```

A worklist is a query with a filter written in the flow's expression language:

```sh
of query '{"actor": {"id": "bo"}, "type": "LeaveRequest", "filter": "state == LeaveRequest.APPROVED"}'
```

```json
{"items": [{"id": "leaverequest-3", "type": "LeaveRequest", "fields": {"state": "APPROVED"}}], "cursor": null, "refused": null}
```

Stop the service with Ctrl-C. It finishes the requests in flight and exits.

## Next

- [Concepts](concepts.md): the model behind what you just did.
- [Write a flow](write-a-flow.md): examples, the checker, and patterns for real flows.
- [Build an application](integrate.md): actors, refusals, reads and subscriptions from your own code.
- [Errors and refusals](errors.md): every `kind`, `remedy`, reason and fault code, and what to do about each.
