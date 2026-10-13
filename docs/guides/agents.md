# Agents

How an AI agent acts through ObjectFlow, and how you keep it within bounds. Read the [quickstart](quickstart.md) first: this page extends its leave flow.

## How an agent acts

An agent is an actor like any other: an object of a type whose identity has `actor_kind: agent`. It makes the same requests a person makes and is refused by the same rules. There is no side door. Because its kind comes from its type, every event and metric records truthfully that an agent acted.

An agent's loop:

1. **Get its tools**, generated from the flow: `POST /v1/tools {"actor_kind": "agent"}`.
2. **Plan** with `object_availability`: what each transition offers it now, and why not.
3. **Act**: each write tool call becomes a request in the agent's name.
4. **Read the verdict.** On a refusal, the `remedy` is its next move:

| `remedy` | The agent… |
|---|---|
| `self_serviceable` | fixes its request and retries. |
| `delegable` | hands the task to a person, or files a proposal where `proposable` is `true`. |
| `temporal` | retries later. |
| `dependent` | works on the objects the refusal names first. |
| `unreachable_from_here` | stops. |

[Errors and refusals](errors.md) lists every refusal it can meet.

## Keep it in bounds: in the flow

**On the wire, an agent may take any transition the flow does not restrict.** A flow naming no `taken_by` leaves every transition open to it: given the wire and such a flow, a reviewer's agent recorded payments, erased personal data and made employees. State in the flow what a person must do. The leave flow made agent-ready:

- `taken_by: [human]` on each type makes a person the default for its transitions;
- `taken_by: [human, agent]` opens the transitions an agent may take: here, opening and submitting a request;
- `proposable: true` on `approve` lets an agent propose an approval that a person then decides;
- `publishing: { taken_by: [human] }` keeps the approval of a new flow version with people.

<details>
<summary>The agent-ready <code>leave.yaml</code></summary>

```yaml
# leave.yaml
module: leave
publishing: { taken_by: [human] }
categories: [live, closed]

types:
  Employee:
    description: A person who asks for leave or approves it.
    tracking: record
    taken_by: [human]
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live, description: Works here. }
      GONE: { category: closed, final: true, description: Has left. }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
      depart: { kind: external, from: ACTIVE, to: GONE }

  Assistant:
    description: An AI agent that drafts requests for people.
    tracking: record
    taken_by: [human]
    attributes:
      name: { type: identity, actor_kind: agent, unique: true }
    states:
      ON: { category: live, description: In service. }
      OFF: { category: closed, final: true, description: Retired. }
    transitions:
      enable: { kind: initial, to: ON, required_inputs: [name] }
      retire: { kind: external, from: ON, to: OFF }

  LeaveRequest:
    description: A request for days off.
    tracking: record
    taken_by: [human]
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
      open: { kind: initial, to: DRAFT, required_inputs: [days], guards: { reasonable: deny }, taken_by: [human, agent] }
      submit: { kind: external, from: DRAFT, to: SUBMITTED, taken_by: [human, agent] }
      approve: { kind: external, from: SUBMITTED, to: APPROVED, proposable: true }
      reject: { kind: external, from: SUBMITTED, to: REJECTED }
```

</details>

A rule about who acted that is not a whole transition, such as "a line's nature confirmed by a person", records the actor's kind when it acts (`assign: { location: nature_confirmed_by, expr: actor.kind }`) and constrains the record with an invariant, as the `reimbursement.yaml` sample's `confirmed_by_a_person` does. An agent it refuses is told `delegable`.

The checker points at what is left open. Without the `taken_by` and `publishing` lines, it gives the notice `agent_open`, naming by type the transitions an agent may take on the wire, and `publishing_open`, saying the default decides who approves the next version: an agent is refused and any other kind may. The notice `taken_by_dropped` flags a new version that loosens a `taken_by` or `publishing` rule the previous one had.

## Try it

Load the agent-ready flow into a fresh sandbox, then add two people and an agent:

```sh
jq -Rs '{files: [{name: "leave.yaml", text: .}]}' leave.yaml \
  | curl -s "$OF/sandbox/load" -H 'Content-Type: application/json' -d @-; echo
```

```json
{"loaded": true, "modules": ["leave"], "notices": 0, "now": "2026-01-01T00:00:00Z", "types": ["Employee", "Assistant", "LeaveRequest"], "evaluators": []}
```

```sh
of request '{"actor": {"id": "operator"}, "type": "Employee", "transition": "join", "inputs": {"login": "ana"}}'
of request '{"actor": {"id": "operator"}, "type": "Employee", "transition": "join", "inputs": {"login": "bo"}}'
of request '{"actor": {"id": "operator"}, "type": "Assistant", "transition": "enable", "inputs": {"name": "helper"}}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "employee-1", "version": 1, "events": [2], "replayed": false}
{"kind": "satisfied", "flags": [], "object_id": "employee-2", "version": 1, "events": [3], "replayed": false}
{"kind": "satisfied", "flags": [], "object_id": "assistant-3", "version": 1, "events": [4], "replayed": false}
```

The agent's tools: one per transition it may take, `approve` for proposing only, and four reads.

```sh
of tools '{"actor_kind": "agent"}' | jq -c '[.tools[].name]'
```

```json
["get_object", "leave_request_approve", "leave_request_open", "leave_request_submit", "object_availability", "query_objects", "read_metric"]
```

Each tool is `{name, description, request, input_schema}`, the schema in JSON Schema ([`renderers.md`](../design/renderers.md) §3 defines them). Hand them to your model; send each write it calls as a request in the agent's name, its `type` and `transition` the tool's `request` and the rest the call's arguments, and each read as the operation it names. Always send the `type` beside the `object_id`: the store then refuses, `not_found`, an object of any other type the id names, so `customer_revoke` cannot revoke a subscription, nor a label's `forget` reach another type's object of a same-named transition.

The agent drafts and submits a request for Ana. Naming her as `principal` records whom it acted for:

```sh
of request '{"actor": {"id": "helper", "principal": "ana"}, "type": "LeaveRequest", "transition": "open", "inputs": {"days": 4}}'
of request '{"actor": {"id": "helper", "principal": "ana"}, "object_id": "leaverequest-4", "transition": "submit"}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-4", "version": 1, "events": [5], "replayed": false}
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-4", "version": 2, "events": [6], "replayed": false}
```

It may not approve. The refusal is `delegable`, and says a proposal would be accepted:

```sh
of request '{"actor": {"id": "helper"}, "object_id": "leaverequest-4", "transition": "approve"}'
```

```json
{"kind": "invariant_violated", "remedy": "delegable", "failures": [{"object_id": "leaverequest-4", "type": "LeaveRequest", "invariant": "approve_taken_by", "remedy": "delegable"}], "objects": [], "proposable": true}
```

So it proposes, and a person decides. Approving the proposal takes the transition as the approver:

```sh
of request '{"actor": {"id": "helper"}, "object_id": "leaverequest-4", "transition": "approve", "propose": true}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "proposal-5", "version": 1, "events": [7], "replayed": false}
```

```sh
of request '{"actor": {"id": "bo"}, "object_id": "proposal-5", "transition": "approve"}'
```

```json
{"kind": "satisfied", "flags": [], "object_id": "leaverequest-4", "version": 3, "events": [8, 9], "replayed": false}
```

The record says who did what, and of which kind:

```sh
of history '{"id": "leaverequest-4"}' | jq -c '.events[] | {transition, actor_id, actor_kind, principal}'
```

```json
{"transition": "open", "actor_id": "helper", "actor_kind": "agent", "principal": "ana"}
{"transition": "submit", "actor_id": "helper", "actor_kind": "agent", "principal": "ana"}
{"transition": "approve", "actor_id": "bo", "actor_kind": "human", "principal": null}
```

## Keep it in bounds: in your gateway

The flow governs objects. Some operations govern the store itself, and the engine does not read an actor's kind to refuse them. The generated tools offer none of them, so an agent held to its tools, each call sent with its tool's `type`, cannot ask. If an agent can reach the wire directly, your gateway must refuse it:

- `maintain`, `upgrade` and `import_batch`;
- `pull`, `acknowledge`, and the `revoke` transition, on a subscription that is not the agent's own;
- recording a label (`"type": "label"`, `record`), which anyone may put on any object, unless your agent should, and a label's `forget`, or a label recorded with `corrects`, on a label another recorded;
- every operation on a `DeclarationChange`, its `check` among them, unless your agent drafts flow changes: the engine already refuses an agent's approval, and its moving on a change another drafted, but drafting is for whoever authors your flows;
- the identity `operator`, in any case of its letters, which is the store's operator: never let a user or an agent name an actor so;
- a `principal` the agent chose: the engine records whom an agent names as acting for, the operator or any person included, so set it from your own session, never from the model's arguments;
- an `idempotency_key` the agent chose: each write tool's schema offers one, and a key the model makes up and repeats replays an earlier verdict, so set keys in your gateway, one for each call you mean to make once;
- an `occurred_at` the agent chose: each write tool's schema offers one for a transition that may be backdated, so the model may backdate within the transition's limit; set it in your gateway, or leave it out;
- `check`, which the tools do not offer: it answers what a request would meet and records nothing, so an agent sending it in a loop could learn, without a trace, which person holds a unique value or, by halving a guess, a personal value an invariant compares;
- requests past a budget of bytes as well as of calls: a refused request is kept with what it sent, as every refusal is, so twenty refusals of a 1.9 MB body grew a store from 4.5 MB to 24.9 MB.

`publish` needs no rule of yours. An agent may draft and submit a `DeclarationChange`, but its approval is refused unless the flow's `publishing` names `agent`. Where no module declares `publishing`, any other kind may approve; `publishing: { taken_by: [human] }` keeps it with people.

**Reads are not filtered.** A read authorizes nothing, so an agent's `get_object` and `query_objects` return what a person's would, personal attributes included. Filter what you hand the model by your own rules on who may see what. A metric that names a person is listed in its type's `person_dimensions`. **What you filter out stays hidden**: an agent's `query` whose `filter` or `order` reads a member marked personal, in any type, is refused `personal`, as a metric's filter is, where an agent ranging over `name >= "…"` recovered the names its answers left out. So is the query of an actor the store does not know, an agent not yet registered among them: register an agent before it reads. A person's, a service's and the operator's queries are answered. **The values a guard consulted travel with the events**: `history`, `events`, `pull` and `export` carry, in each event's `consulted`, what each guard was told, a per-person metric's value among them, such as `claimant_amount_claimed(claimant: employee-222)`; filter `consulted` as you filter the metric itself.

**`availability` offers an agent a transition kept for people** where it takes inputs: one `taken_by: [human]` with inputs is answered `available_with_input`, as for anyone, and the agent's request is then refused. Read `taken_by` from the tools, which never offer it.

**Name the agent as the actor of its reads.** Reads that grow with the store run in a pool of the agents' own, so an agent's sweeping queries never hold up a person's worklist.

**The tools are what the flow allows an agent, not what you permit it.** Filter them by your own rules before handing them over.

## Retiring an agent

Move its actor object to a final state (`retire` above). From then on every request naming it is refused `actor_active`, and it is offered nothing. Revoke its key at your gateway too.
