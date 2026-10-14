# Concepts

The model behind ObjectFlow, on one page. Read it after the [quickstart](quickstart.md), or before writing your first flow.

## At a glance

| Term | In one line |
|---|---|
| **Flow** | The rules for a business process, written as one or more YAML **modules**. |
| **Type** | A kind of business object, such as `LeaveRequest`: its attributes, states and transitions. |
| **Object** | One instance of a type, with an id like `leaverequest-3`, a current state and a version number. |
| **State** | Where an object is in its life. An object is in exactly one state at a time. |
| **Transition** | A named change an actor can request, such as `submit`: it may move the object to another state and write its attributes. |
| **Actor** | Whoever makes a request: a person, an agent or a service, each an object of a type the flow declares. |
| **Condition** | A named rule with a remedy. Used as a **guard** on a transition, it refuses, records or warns. |
| **Invariant** | A rule that must hold of every object at all times. A change that breaks one is refused. |
| **Verdict** | The engine's answer to a request: applied (`satisfied`) or refused, with a `remedy`. |
| **Remedy** | What the caller can do about a refusal: fix it, ask someone, wait, work on another object, or stop. |
| **Event** | The record of one applied change: who, what, when, and what the rules read. Events are never edited. |
| **Version** | The flow installed in a store. A new one is published by a reviewed change. |
| **Store** | Where objects and events live: a sandbox in memory, or a deployment's SQLite file. |

## Flows and modules

A flow is written in YAML. Each file is a **module** (`module: leave`) declaring types, and optionally enumerations, sequences, metrics and evaluators. A module may import types from another. The modules installed together are the store's **version**: version 1, 2, and so on. The [flow format card](flow-format-card.md) lists every key.

The engine is the only authority on the flow: every request is checked against the installed version, and nothing else writes the store.

## Types, objects and attributes

A **type** declares its attributes, states and transitions. Each **object** of it has:

- an id, `<type in lower case>-<number>`, such as `leaverequest-3`;
- a `state`, and a `version` that rises by one with every change, so a client can say which version it read;
- its attributes, and **derived attributes** computed from them (`total: sum(l in lines: l.amount)`), never written directly;
- who created it and when.

Attribute types are `string`, `bool`, `int`, `decimal(p,s)`, `money(CCY)`, `timestamp`, `date`, `duration`, `identity`, `file` and the flow's enumerations. A **reference** points at another object, or at a set of them (`"Line[]"`). A reference marked `aggregation: composite` makes the referenced objects **parts** of a **whole**, created through the whole and living no longer than it.

Some markings change how an attribute is treated:

| Marking | Effect |
|---|---|
| `optional: true` | It may have no value. Otherwise it always has one; a set is never optional, only empty. |
| `unique` | No two objects hold the same value. |
| `indexed: true` | Queries may filter and order on it quickly. |
| `personal: true` | It holds personal data: an erasure removes it, and no metric reads it. |
| `identity` with `actor_kind` | Objects of this type are **actors**, named by this attribute. |

## States

Every state belongs to a **category** the module declares, such as `live` or `closed`. Metrics use categories: an object is **open** until it reaches a `closed` or final state. A state marked `final: true` is one no transition leaves.

## Transitions

A transition is the only way an object changes. It has a **kind**:

| Kind | Does |
|---|---|
| `initial` | Creates an object, in its `to` state. |
| `external` | Moves an object `from` one or more states `to` another. |
| `internal` | Changes an object without moving it, in the states listed in `from`. |
| `assertion` | Puts an object into a state the request names, from any state, when the record is wrong: an override, recorded with its reason. |
| `erasure` | Erases the object's `personal` attributes, at any state, given a `reason`. |

A transition declares its **inputs**, its **guards** (the conditions checked before it applies) and its **effect**: the steps that write attributes, create or call other objects, or loop over a set. A transition marked `only_via` runs only as part of another, such as a line created by its order's `add_line`.

Who may take it can be limited by actor kind with `taken_by: [human]`. A transition marked `proposable` lets a caller file it as a **proposal** that someone else approves.

## Rules

Rules are declared once and checked on every request, from whoever sends it.

**Conditions** are named boolean expressions, each with a `remedy` and a `description`. A transition lists the conditions it uses as **guards**, each with an enforcement:

| Enforcement | When the condition fails |
|---|---|
| `deny` | The request is refused, `unsatisfied`, naming the condition. |
| `audit` | The request applies, and the would-be refusal is recorded for metrics. Use it to try a rule before enforcing it. |
| `warn` | The request applies, and the answer's `flags` carry the warning, as the record does. |

A condition reading a value nobody has yet is **unknown**, and an unknown guard fails as a false one does. The refusal says `"unknown": true`, so a caller can tell "not yet" from "no".

**Invariants** must hold of every object at all times. The engine checks every invariant a write could affect and refuses a change that would break one, `invariant_violated`, naming each object it fails on.

**Evaluators** let a guard ask an outside system, such as "is this bill paid?". A deployment posts each call to an endpoint you run. An evaluator that does not answer in time makes its guard unknown, refused `temporal`.

The expression language reads attributes, follows references (`category.name`), aggregates sets (`count(l in lines)`, `sum(…)`, `any(…)`), compares money and times (`now - 30 days`), and reads metrics. [`declaration-syntax.md`](../design/declaration-syntax.md) §8 defines it.

## Actors

Every request names its **actor**. An actor is an object of a type whose `identity` attribute has an `actor_kind`: `human`, `agent` or `service`. The kind comes from the actor's type, never from the request, so every event and metric says truthfully which kind acted. A fresh store has one actor, `operator`, which creates the others.

**The engine records who acted. It never decides whether they may.** Authenticating people and deciding their permissions is your application's job. Rules about who acts belong in the flow only where they are part of the process, such as `taken_by: [human]` on an approval, or "nobody approves their own request" written as an invariant over a recorded approver.

An actor in a final state, such as someone who left, is refused every request. A request made on someone's behalf, such as an agent acting for a person, names that person as its `principal`.

## Requests and verdicts

A request asks for one transition. The engine checks it in order: the transition exists and the object's state allows it; the inputs are valid; the guards pass; the effect runs; every affected invariant still holds. Then it commits, all at once, as one event, or not at all. **Nothing is ever half-applied.**

The answer is a **verdict**. `satisfied` means applied; anything else is a refusal with a `remedy`, the caller's next move. [Errors and refusals](errors.md) lists them all.

`check` answers the verdict a request would get, writing nothing. `availability` answers, for one object and one actor, what every transition would get right now. Screens render their buttons from it, and agents plan with it.

## Events and history

Every applied request is an **event** with a position in one store-wide order. An event records the actor and its kind, the `principal`, when it was recorded and when it `occurred`, the inputs sent, what the rules read and the outside answers they used. Nothing is edited: a mistake is fixed by a new event, and an observation by a correction that names the one it corrects.

Refused requests are kept too, in the **attempt log**, which metrics read to show which rules refuse most.

## Metrics

Every type gets standard metrics with nothing declared: time in each state, throughput, work in progress, age of the oldest open objects, how often each transition is taken, refusals and flags per rule, and override counts. Each splits by version and by actor kind, so you can see where agents stall and people do not. A flow declares its own metrics too, and a guard can read one.

A metric that names a person, such as open work per assignee, says so in its type's declaration (`person_dimensions`), so your application can keep it behind its own permissions.

## Changing a flow

A new version is published as a `DeclarationChange`, a built-in object with its own transitions: `draft` it with the new modules, `submit` it to run a **dry run** over the store's real objects, read the impact report, and approve it with `publish`. The dry run runs the modules' examples and reports which objects would move or be newly refused. Approval installs the version in one step; nothing changes until then.

Existing data from another system comes in through `import_batch`, into types marked `mirror`, before a version makes them the flow's own. [`publish-and-import.md`](../design/publish-and-import.md) works through a go-live.

## Personal data

An attribute or input marked `personal` holds personal data. An `erasure` transition removes every personal value of its object: from the object, from its history, from exports and from the store's file and its `-wal` once the log is cut back, the free space of an index it touched aside, which [Operate](operate.md) says how to clear. Free text a flow does not mark `personal`, and a request's `context`, are kept as written, so mark every input that may name a person.

## Sandbox and deployment

The same service runs two kinds of store:

| | Sandbox | Deployment |
|---|---|---|
| Started with | no `--store` | `--store <file>` (`--create` the first time) |
| Data | in memory, gone on exit | one SQLite file |
| Clock | starts at `2026-01-01T00:00:00Z`, moves only when told | the wall clock |
| Outside systems | fixed answers you set | your HTTP endpoints |
| Flow | loaded directly, unchecked | published through a reviewed change |
| Use | building, testing, demos | production |

Next: [write a flow](write-a-flow.md), or [build an application](integrate.md).
