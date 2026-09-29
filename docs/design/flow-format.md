# The flow description format

Status: **draft 2, 2026-09-26, adopted.** This is the one written form of an ObjectFlow flow (ADR-0116), with its terms taken from UML, SCXML and Kubernetes (ADR-0115, Appendix A). It is the normative definition of how a flow is written. Its machine-readable half is the schema [`flow-format/flow.schema.json`](flow-format/flow.schema.json) (§2), and [`scripts/check-flows.py`](../../scripts/check-flows.py) implements every rule stated here; each rule names the step and the finding code that enforce it. Where this document and the implementation disagree, both are wrong until one is corrected, and `scripts/check-flow-format-doc.py` holds them to each other. The meaning of every form is the meaning of the declaration it converts to, defined in [`declaration-syntax.md`](declaration-syntax.md), which remains the definition of the declaration model; this document does not restate the engine's semantics. The format expresses every construct of the model since 2026-09-26; Appendix B says where each is written.

## 1. Scope and conformance

A **flow description** is one YAML file declaring one **module**: its state categories, its enumerations and its types, and for each type its attributes, states, conditions, transitions and metrics. A description is **valid** when all six steps of §7 accept it with no fatal finding. Only a valid description can be proposed for publication. A module MAY also carry an **examples file**, the claims about its behaviour that the engine runs (§11).

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119 and RFC 8174, when written in capitals.

Every word in a description is one of two kinds, and the examples in this document and in the walkthrough colour them apart:
- **Reserved**: a key or value this format defines, such as `transitions`, `kind`, `external`, `guards` or `deny`, and the words of the expression language. The full list is §10. A reserved word means the same thing in every description.
- **Named by the author**: a name the description itself declares, such as a module, a type, a state, an attribute, a condition, a transition, a metric, an enumeration or one of its values, or a category, and every later use of that name. Descriptions and expressions are the author's text.

## 2. The file

1. A file MUST be UTF-8 YAML holding exactly one document, which is one module. (step 1, `yaml`)
2. Only `true` and `false` are booleans. `yes`, `no`, `on` and `off` are ordinary words, so a value named `NO` stays a name. (step 1)
3. A mapping MUST NOT contain the same key twice. A YAML loader would otherwise keep the last one silently. (step 1, `yaml`)
4. Inside an inline list `[…]` or an inline mapping `{…}`, a value containing a comma, a question mark, a colon followed by a space, a bracket, a brace or `#` MUST be quoted, as YAML requires; for example `{ type: "decimal(10,3)" }`. Unquoted, such a value either fails to parse (step 1, `yaml`) or parses into the wrong structure (step 2, `schema`). Anywhere, a value containing a colon followed by a space MUST be quoted or written as a folded block after `>-`; an aggregate's body does contain one, as in `sum(i in this.intervals: i.duration)` (step 1, `yaml`).
5. Comments begin with `#` and are ignored.
6. Nothing in a description has a default: every required key is written, including `tracking` and every condition's `remedy`. (step 2, `schema`)
7. A description SHOULD name its schema on its first line, as `# yaml-language-server: $schema=<path to flow.schema.json>`. Editors built on the YAML language server read this line to validate the description and describe each key as it is typed; a relative path is resolved from the description's own location. (not checked)

**The schema.** [`flow-format/flow.schema.json`](flow-format/flow.schema.json) is a JSON Schema (draft-07) of a description's structure, versioned with this document's draft and carrying a description of each key for editors. It is normative for structure only, which is step 2 of §7: what it cannot express, the order of declarations, the names a description declares and uses, and the attributes a state requires, is stated in this document and checked in steps 1, 3 and 4. Its `$id` is relative until the schema is published at a stable address.

## 3. Names

| What is named | Form | Example |
|---|---|---|
| module | lower case with underscores, starting with a letter | `inventory` |
| type, enumeration, observation kind | upper camel case | `InventoryItem`, `Condition` |
| state, enumeration value | upper case with underscores | `RESERVED`, `DAMAGED` |
| category, attribute, observation, invariant, condition, transition, metric, flag | lower case with underscores | `reserve`, `not_damaged` |

(step 2, `schema`)

1. A name MUST be unique among the names of its section, and a type, machine, enumeration, sequence or evaluator MUST NOT share its name with one a module in its closure declares, the modules it imports and theirs (step 3, `names`; the model's check 33). A repeated key is refused by step 1.
2. **Every name MUST be defined before it is used.** A module declares, in this order, `module`, `imports`, `categories`, `enumerations`, `sequences`, `evaluators`, `machines`, `types`, `metrics` and `migration`; a machine declares, in this order, `description`, `requires`, `states`, `conditions` and `transitions`; a metric declares, in this order, `description`, `measure`, `state`, `transition`, `source`, `item`, `filter`, `dimensions`, `group_by`, `time_dimension`, `expression` and `flag_when`, and a combined metric `description`, `input_metrics`, `group_by`, `expression` and `flag_when`; a type declares, in this order, `description`, `abstract`, `extends`, `mirror`, `tracking`, `state_machine`, `attributes`, `inherited_parts`, `observations`, `states`, `derived_attributes`, `summary`, `invariants`, `conditions`, `transitions` and `metrics`. The machines and types of a module form one group and MAY reference each other in any order, as related types must (ADR-0117); a reference MUST name a type the module declares or imports (step 3, `names`). Every other name is defined before it is used, except that a cascade's `on` and an attribute's `survives`, and those of an entry in `inherited_parts`, name transitions of the type, which follow its attributes (step 3, `order`).
3. Reserved words of the text language MAY be used as names, as `declaration-syntax.md` §9.2 allows, with these exceptions (step 3, `names`):
   - a category MUST NOT be named `any`, `terminal` or `superseding`, and a transition MUST NOT be named `any` (the text language's check 33);
   - an attribute, including an observation's, MUST NOT be named `state`, `inputs`, `actor`, `this`, `now`, `referrers` or `this_event`, since an expression resolves those words before any attribute;
   - a member MUST NOT be named `id`, `open`, `created_at`, `created_by_kind`, `recorded_from` or `declaration_version`, which every object has, or `imported_at`, which every mirror has, since it would shadow them (§5; the model's check 33);
   - a condition MUST NOT be named `<attribute>_provided`, and an invariant MUST NOT be named `<state>_invariant` with the state in lower case or `<attribute>_unique`, since those are the names of generated guards and invariants (§6): `<attribute>_unique` is the invariant a `unique` attribute, or the stored end of a one-to-one relationship, generates, and the name a refusal of a duplicate carries.

## 4. Structure

### 4.1 The module

| Key | Required | Value |
|---|---|---|
| `module` | yes | the module's name |
| `imports` | no | a mapping from another module's name to the list of its types, enumerations, sequences, evaluators and machines this module uses; where that module is among the files checked, before this one, each name MUST be one it declares (step 3, `names`), and where it is not, step 3 reports an `imports` notice, since steps 2 and 3 cannot check what is imported from it; a module does not re-export the names it imports, so each name is imported from the module that declares it (ADR-0122) |
| `categories` | yes | the list of state categories the module's states use. `closed` is in every vocabulary whether or not it is listed, and has the meaning `declaration-syntax.md` §1 gives it: work is open until it enters a `closed` or final state, which the standard metrics read |
| `enumerations` | no | a mapping from an enumeration's name to its list of values |
| `sequences` | no | a mapping from a sequence's name to its `description` (§4.12) |
| `evaluators` | no | the systems outside the store a guard may ask (§4.18) |
| `machines` | no | a mapping from a shared state machine's name to its declaration (§4.10) |
| `types` | yes | a mapping from a type's name to its declaration (§4.2) |
| `metrics` | no | metrics that combine other metrics, across the module's types (§4.9) |
| `migration` | no | how a publish of this version moves the live objects of the version before it (§4.16) |

### 4.2 A type

| Key | Required | Value |
|---|---|---|
| `description` | yes | one or more sentences saying what an object of the type is |
| `abstract` | no | `true` for a base with no objects of its own (§4.14) |
| `extends` | no | the abstract base the type inherits from (§4.14) |
| `mirror` | no | `true` for a type another system owns, written only by the import (§4.15) |
| `tracking` | unless inherited, or the type is abstract | `record`, `serial` or `quantity` (`declaration-syntax.md` §2) |
| `state_machine` | no | the shared state machine the type binds (§4.10), in place of its own `states` |
| `attributes` | no | §4.3 |
| `inherited_parts` | no | on a subtype, how each part it inherits from an abstract base goes with its final transitions (§4.14) |
| `observations` | no | §4.4 |
| `states` | unless the type binds a `state_machine` or is abstract | §4.5 |
| `derived_attributes` | no | §4.11 |
| `summary` | no | the members a listing returns by default, `state` among them (`DESIGN.md` §5.2); each MUST be `state` or a member of the type (step 3, `names`) |
| `invariants` | no | §4.6 |
| `conditions` | no | §4.7 |
| `transitions` | unless the type binds a `state_machine`, is abstract, or is a mirror | §4.8 |
| `metrics` | no | §4.9 |

### 4.3 Attributes

Each attribute is a mapping with exactly one of `type` and `reference`. (step 2, `schema`)

| Key | Value |
|---|---|
| `type` | a built-in type (`string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `duration`, `identity`, `file`, `event`) or an enumeration's name, `[]` making the attribute a set of values; or `counter`, on a type tracked by quantity (§4.17) |
| `reference` | the name of a type the module declares or imports; `[]` makes the attribute a set of references |
| `opposite` | the attribute of the referenced type that is the other end of this relationship (UML `Property::opposite`) |
| `stored` | `true` on the one of two single ends that holds the value |
| `aggregation` | `composite` on a whole's end: the referenced objects are its parts (UML `AggregationKind::composite`) |
| `cascade`, `survives` | on a composite end: what its parts do when the whole takes a transition (below) |
| `optional` | `true` if the attribute may be without a value; otherwise it always has one. A set, of values or of references, is never optional and never absent: a creation that does not take it as an input creates it empty, and `clear` empties it (ADR-0122) |
| `default` | the value a creation that does not take the attribute as an input gives it (§4.17) |
| `identifier` | the value is minted from a sequence when the object is created (§4.12) |
| `external` | the system that owns the value, such as `xero`: its values are unique for each source, and it MAY be optional, as an object not yet matched to that system has none (§4.15) |
| `unique` | `true` if no two objects of the type may hold the same value; `in_scope`, `with` and `where` restrict it (§4.12) |
| `indexed`, `personal` | as `declaration-syntax.md` §3.1 defines them |
| `actor_kind` | `human`, `agent` or `service`, on the `identity` attribute of a type whose objects make requests |
| `assignee` | `true` on a reference that names who is responsible for the object |
| `unit` | the unit of a measured value |
| `description` | one sentence, where the name alone does not say what the attribute holds |

`unit` applies only to an observation's attribute, and `unique`, `indexed`, `identifier`, `external`, `default`, `actor_kind`, `assignee`, `opposite`, `stored`, `aggregation`, `cascade` and `survives` only to a type's attribute, since the model has no form for them elsewhere. (step 3, `names`)

**Relationships.** Both ends of a relationship are declared, each on its own type, so that a type reads completely on its own, and each names the other with `opposite`; the two ends MUST name each other and be in one module (step 3, `names`). Exactly one end stores the value, as `declaration-syntax.md` §3.3 fixes: of a single end and a set end, the single one; of two single ends, the one marked `stored`; two set ends cannot store a pair, which is then a type of its own with a reference to each side (step 4, `check 41`). An end with no `opposite` is a reference with no named way back, and MUST be single (step 3, `names`), except a set a machine requires, whose opposite its binder's end names (§4.10). Of two single ends, the value the stored end holds is held by no other object of its type: the relationship is one-to-one, and the store refuses a second holder as it refuses a duplicate of a `unique` attribute (ADR-0122).

**Composition.** An end marked `aggregation: composite` makes the referenced objects **parts** of this one, which is their **whole**: a part belongs to exactly one whole, so its end back MUST be single and required (step 3, `names`), and a part lives no longer than its whole. Creating a part MUST be `only_via` a transition of the whole (§4.8; step 4, `check 11`). On the composite end:
- `cascade` is a list of clauses `{ on: [<transition of the whole>, …], transition: <transition of the part>, inputs: { … }, limit: <n> }`: when the whole takes one of the transitions in `on`, each part takes `transition` with the inputs given, which are expressions over the whole and the triggering transition's inputs; `limit` bounds the parts it drives in one request, those passed over in a final state not counted, and a request that would drive more is refused with `over-limit` (ADR-0122). The part transition MUST exist, MUST NOT be initial, and MUST receive the inputs it requires (step 3, `names`). A part already in a final state is passed over, and a part whose guards refuse aborts the whole request.
- `survives` is `true` if the parts outlive every final transition of the whole, or the list of transitions they outlive.

Every transition of the whole into a final state MUST be covered by a cascade or by `survives`, and none by both (step 3, `names`; the model's check 37, ADR-0058).

### 4.4 Observations

An observation is a datapoint recorded about an object and never changed afterwards (`declaration-syntax.md` §6.8). Each has a `kind` (a type name), a `description`, its `attributes` (§4.3), and MAY have a `max_recording_delay`, a duration such as `7 days`, and `invariants` (§4.6). Conditions MAY read a type's observations by the observation's name.

### 4.5 States

| Key | Required | Value |
|---|---|---|
| `category` | yes | a category the module declares (step 3, `names`) |
| `description` | no | what it means for an object to be in the state |
| `final` | no | `true` if no transition leaves the state: a final state (UML `FinalState`) |
| `superseding` | no | `true` on a final state an object ends in by being superseded by another (§4.19) |
| `required_attributes` | no | attributes that have a value whenever an object is in the state; this is the state's invariant (UML `State::stateInvariant`) |

Every transition into a state MUST set each attribute the state requires: by requiring it as an input, by assigning it a value that is provably present, or by leaving it in place from source states that all require it. A value is provably present when it is `now`, a literal, a qualified enumeration value, an input that is not optional or has a default (`inputs.<name>`), or an attribute that every source state requires or that is not optional; any other expression is not counted, even when it would have a value. A transition MUST NOT clear an attribute its target requires. (step 3, `required`)

### 4.6 Invariants

An invariant is a condition that holds for every object of the type at all times. Each has a `description` and an `expression` (§5). A state's `required_attributes` generate invariants of their own (§6); `invariants` is for any other.

### 4.7 Conditions

A condition is a named test that transitions use as guards.

| Key | Required | Value |
|---|---|---|
| `description` | yes | the condition, stated as what holds when it is satisfied |
| `expression` | yes | §5 |
| `remedy` | yes | what a caller can do when it fails: `self_serviceable` (supply or correct an input of the transition requested), `delegable` (another actor must act), `temporal` (only time will satisfy it), `dependent` (another object must change first) or `unreachable_from_here` (nothing the caller supplies, waits for, or asks of another actor or object will satisfy it from here: another transition must come first, such as one that writes the value the condition reads, or none can); `DESIGN.md` §5.5; a condition whose transition takes no input is never `self_serviceable` (step 3 notice `self_serviceable`, ADR-0122). The declared class is carried as written by every refusal the condition makes, whatever state it fails from; an engine neither infers nor corrects it (`declaration-syntax.md` §5.1), and a condition whose remedy differs by the state it fails from is written as two conditions where the verdict must say which |

Every condition MUST be used by at least one transition. (step 3, `names`)

### 4.8 Transitions

A transition changes an object, and every change to an object is one. A transition's name is its **trigger** (UML `Transition::trigger`): a request names the transition it asks for. Each transition has a `kind`: one of UML's three, or one of the two the model adds, which UML has no term for (§4.13, ADR-0119):

| `kind` | Means | `from` | `to` |
|---|---|---|---|
| `initial` | a new object comes into being in a state: the transition from UML's initial pseudostate | MUST be absent | the state |
| `external` | the object moves from a state to another (UML `TransitionKind::external`) | one state, a list of states, or `any` | the state it moves to |
| `internal` | the object changes without changing state (UML `TransitionKind::internal`) | the states it may be taken in, or `any` | MUST be absent |
| `assertion` | the request puts the object into a state it names, from any state, overriding the flow | MUST be absent | the states it may put the object in |
| `erasure` | the object's personal attributes are erased, at any state, a final one included | MUST be absent | MUST be absent |

(step 2, `schema`; every state named MUST be declared, step 3, `names`)

`from: any` is every state that is not final other than the transition's target, so a transition that any other state may take, such as Jira's "all statuses can transition to this one", is written once (`declaration-syntax.md` §4.2). No transition leaves a state for itself: an external transition always changes the state, and a change that keeps it is an internal transition, whose `from: any` is every state that is not final (ADR-0122). *(Corrected 2026-09-27: this said the target state was included, which allowed a transition from a state to itself and left open whether it re-entered the state.)* It stands alone, never in a list (step 2, `schema`), and the checks read it as the states it stands for, so a state added later is covered without an edit.

| Key | Required | Value |
|---|---|---|
| `kind`, `from`, `to` | as above | |
| `description` | no | what the transition does, in one sentence |
| `only_via` | no | the transitions, written `Type.transition`, that alone may take this one, by a `call`, a `create` or a cascade; no caller may request it (step 3, `names`) |
| `proposable` | no | `true` if a request for the transition may be filed as a proposal instead, executed as its approver when approved (ADR-0036); never with `only_via`, since no one requests such a transition (step 3, `names`; the model's check 14). When to propose, and who may approve, are the application's (ADR-0114) |
| `corrects` | no | on an internal or external transition: the attributes it corrects, which it writes exactly (§4.13) |
| `may_admit` | no | on an assertion: the invariants a request may admit breaking (§4.13) |
| `required_inputs` | no | attributes the caller MUST supply; each is written to the attribute of the same name |
| `optional_inputs` | no | attributes the caller MAY supply; each is written if supplied, and one given as null is not supplied. An attribute that is neither optional nor defaulted is always required; a defaulted one left out of a creation takes its default (§4.17) |
| `inputs` | no | declared inputs, which write no attribute of their own name (below) |
| `guards` | no | a mapping from a condition's name to its enforcement, in the order the guards are evaluated |
| `effect` | no | the list of writes the transition makes, in order (UML `Transition::effect`) |
| `backdating_limit` | no | how far in the past a caller may say the change happened, a duration such as `2 days`. Every transition the request takes, the requested one and each it calls or cascades to, records the same occurred time and checks, when it runs, that the time does not precede the current interval of a state or tracked value it changes on its own object, so an internal transition that changes neither checks none; only the requested transition checks the bound. A failure in a call or cascade refuses the call, and so the request, as `unsatisfied` by `occurred_within` (ADR-0122) |

A **declared input** is a value the caller supplies that is not simply written to the attribute of its name: a reason recorded with the event, a value a guard tests, or one an effect writes elsewhere. Each is a parameter of the transition's trigger, in UML's terms:

| Key | Value |
|---|---|
| `type` or `reference` | exactly one: a built-in type or an enumeration, or the type of object it names; `[]` makes it a set |
| `optional` | `true` if the caller MAY leave it out; a set is never optional, since an empty set is supplied instead (step 3, `names`) |
| `personal` | `true` if erasure redacts it from every event that recorded it |
| `default` | an expression used when the caller leaves it out; not allowed on an optional input (step 4, `check 20`) |
| `description` | what the input is for |

Every input is recorded with the event, and every expression reads it as `inputs.<name>`, as it reads attribute inputs. A declared input MUST NOT share its name with an attribute the transition takes as an input, and every `inputs.<name>` a transition's guards and effect read MUST be an input of that transition; a condition several transitions use is checked against each of them (step 3, `names`). An input is written to an attribute of another name by an effect, `assign: { location: <attribute>, expr: inputs.<name> }`, and counts as a value provably present when it is not optional or has a default.

A guard's **enforcement** is one of the three validation actions of Kubernetes admission policies:
- `deny`: the transition is refused when the condition fails;
- `audit`: the condition is evaluated and its failures are recorded, and it refuses nothing (step 3 notice `audit`);
- `warn`: a failure is reported to the caller with the result, and the transition goes through (step 3 notice `warn`).

An **effect step** is one of:
- `assign: { location: <attribute>, expr: <expression> }`, which writes the value of the expression (§5) to the attribute, as SCXML's `<assign>` does;
- `clear: [<attribute>, …]`, which removes the values of the listed attributes, which MUST be optional (step 4, `check 17`);
- `add: { location: <attribute>, expr: <expression> }` and `remove: { location, expr }`, which add a value to or remove one from a set-valued attribute, one whose type ends in `[]` (step 3, `names`); adding a value the set holds, or removing one it does not, leaves the set as it is, and the transition still applies (ADR-0122);
- `call: { target: <path>, transition: <name>, inputs: { <input>: <expression>, … } }`, which takes a transition of another object: its guards are evaluated and it records its own event, in the same request (UML's `CallOperationAction`, whose object is its `target`). Where the target is an attribute or an input whose type is known, including a type imported from a module checked earlier, the transition MUST be one of that type's, MUST NOT be an initial one, and MUST receive every input it requires and none it does not take (step 3, `names`); a target reached any other way is checked by the publish checks;
- `create: { type: <Type>, transition: <name>, inputs: { … }, result: <name> }`, which creates an object by an initial transition of its type, declared in the module or imported, and MUST pass the inputs that transition requires (step 3, `names`); `result` names the new object for the steps after it (UML's `CreateObjectAction`);
- `foreach: { item: <name>, array: <expression>, where: <expression>, limit: <n>, steps: [ … ] }`, which runs its steps for each element of a collection that satisfies `where`, or, with `range: <expression>` in place of `array`, for each number from 1 to that value (SCXML's `<foreach>`). `limit` is REQUIRED and bounds the elements that satisfy `where`, one run each; a request that would run more is refused with `over-limit`, never truncated (step 2, `schema`; `declaration-syntax.md` §5.2). The collection and `where` are evaluated once, when the loop starts, and the loop runs over that snapshot in ascending id order even where its steps change the collection (ADR-0122).
- `supersede: <successor>`, which names the object that supersedes this one, in a transition into a superseding state (§4.19).

Every write stays on the object the transition acts on; another object is changed only by calling one of its transitions or creating it. An effect MAY name a type declared later in the module, since a type often creates or calls objects that refer back to it, which no order of declaration could otherwise allow.

A transition MUST NOT both write and clear one attribute. (step 3, `names`)

An optional input is read in a step only as the whole of an expression, which is skipped when the input is absent; it MUST NOT appear inside a larger expression of a step, since the step would then be conditional on whether it was given, which the model forbids: behaviour that differs by that is two transitions, each taking what it needs (step 3, `names`; the model's check 48; ADR-0122). An optional input given as null is not supplied.

**Refusals.** A request is refused with the first of these that applies, in the order a request is executed (`DESIGN.md` §6), and each carries its remedy (`DESIGN.md` §5.5): the generated guard `actor_known` (`unsatisfied`, remedy `dependent`) when the request's actor, or its principal, names no object of a type that marks an actor identity; `not found` when the object does not exist; `unknown transition` when the object's type, or for a creation the type named, declares no transition of that name the request can take, an initial one for a creation and any other for an object; `stale` when its expected version is out of date; `not requestable` when the transition is `only_via` others; `unavailable` when the object is not in a state the transition may be taken in; `invalid input` when an input is one the transition does not take, is not of its declared type, names no object or one of another type, or repeats an element of a set, or when a declared input that is not optional, or a required input of an attribute neither optional nor defaulted, is left out or given as null; `unsatisfied` when a guard fails, the generated guards first, `occurred_within`, `whole_open` and `not_erased` and, for a recording, `subject_open`, `corrects_current` and `subject_owned`, each with the remedy `DESIGN.md` §5.5 states for it (ADR-0138), then the `<attribute>_provided` guards in the order of `required_inputs` (§6), which refuse a required input of an optional or defaulted attribute left out or given as null, and then the transition's own guards in their order; then, as the effect runs, in the order its steps run, a call or creation that is refused, which refuses the request with its verdict, the step that made it and the object it targeted, a loop's element among them, and `over-limit` when a loop or cascade would exceed its limit, which is checked when the loop starts, before its first run; and last, invariant violated, naming each object on which an invariant fails, in ascending id order, and on each every invariant that fails, in the order its type declares them, each with the remedy `DESIGN.md` §5.5 infers for it, the first of which is the refusal's (ADR-0122).

### 4.9 Metrics

A metric is a figure computed from the record of many objects (`declaration-syntax.md` §6.9). It is declared under the type whose rows it reads, since every source of rows belongs to one type (ADR-0118), and takes one of two forms: a fixed measure, or a formula in the model's metric language.

**A fixed measure** names one of two figures the model provides:

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the figure is |
| `measure` | yes | `median_time_in_state`, the median time objects spend in `state`; or `transition_count`, the number of times `transition` is taken |
| `state` or `transition` | as `measure` requires | a declared state or transition (step 2, `schema`; step 3, `names`) |
| `group_by` | no | the dimensions to group by: `month`, `week`, `actor` (the one who took the transition, only for `transition_count`), or an attribute of the type |
| `flag_when` | no | a mapping from a flag's name to an expression over `value`, such as `"value > 60 days"` |

A `transition_count` counts the transitions along its transition's source and target states, so it MUST NOT name a transition that shares a source and target with another. (step 3, `names`)

A fixed measure groups and windows as service's `time_working` does in the model (`declaration-syntax.md` §6.9, ADR-0139). A `median_time_in_state` is grouped by the `month` or `week` in which a span began, and by an attribute as it was held when the span began if the attribute is tracked, or as it is now if not; it is windowed on the span's entry. A `transition_count` is grouped by the `month` or `week` in which the transition occurred, by `actor`, the id of the actor who requested it, and by an attribute as it was held before the transition's writes if tracked, or as it is now if not; it is windowed on when the transition occurred.

**A formula** aggregates an expression over the rows of a source, grouped by its dimensions:

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the figure is |
| `source` | yes | the rows: `objects`, the type's objects; `intervals`, each span an object spent in one state; `intervals(<member>)`, each span a tracked member held one value; `transitions`, each transition taken; `attempts`, each request that did not apply; `attempt_counts`, their daily counts; `labels`, each label applied to an object of the type; or the name of one of the type's observations as its `observations` section keys it, such as `inspections`, each observation of it. The format names the collection where the model's metric names the kind (step 2, `schema`) |
| `item` | yes | the name each row is read by, as a `foreach` step names its element |
| `filter` | no | an expression over the row; only the rows it holds for are counted |
| `dimensions` | no | a mapping from a dimension's name to an expression over the row; the figure is computed for each combination of their values |
| `time_dimension` | no | a timestamp over the row, by which a guard's `over last <duration>` selects rows |
| `expression` | yes | the value, combining aggregates over the rows of one group: `count()`, `count(where …)`, `count(distinct …)`, `sum`, `min`, `max`, `avg`, `median` and `percentile(<p>, …)` |
| `flag_when` | no | a mapping from a flag's name to an expression over `value` and the dimensions |

```yaml
inspection_pass_rate:
  description: The share of applicable inspections that pass, per engineer.
  source: inspections
  item: r
  filter: r.outcome != CheckOutcome.NOT_APPLICABLE
  dimensions:
    engineer: r.subject.engineer
  time_dimension: r.occurred_at
  expression: count(where r.outcome == CheckOutcome.PASS) * 1.000 / count()
  flag_when: { low: "value < 0.900" }
```

The source MUST be one the type has, and `intervals(<member>)` MUST name a member the type declares (step 3, `names`) that is tracked: the state, an enumeration attribute or a single stored reference (step 4, `check 58`). The filter, the dimensions, the time dimension and the expression read the row through `item` and read no other name but `now`, and each member of the row they read MUST be one the source's rows have, as `declaration-syntax.md` §6.9 lists them for each source (step 3, `names`). So `item` MUST NOT be a name an expression reads as something else, such as `state`, `now` or `value` (step 3, `names`). A flag reads `value`, the metric's dimensions, and `version` and `actor_kind`, which every metric has unless it declares them, and so no dimension is named `value` (step 3, `names`).

The rest of the model's rules for a metric are its publish checks (step 4, `check 56`): a dimension reaches at most two hops from the row, no personal member is read anywhere in a metric, `avg`, `median`, `percentile` and the time buckets `day` to `year` are used in a metric only, and a guard's `metric(…)` binds only the metric's dimensions and windows only a metric with a `time_dimension`, or a combined one whose every input has one (ADR-0128). A filter, value or flag may range over a collection of the object its row belongs to, within two hops; a metric's value is rounded half to even to its expression's scale; and a dimension path reads each object as it is now, a set end included (ADR-0128). A row whose dimension has no value is counted in a group of its own, the absent value, never dropped; a row whose dimension reaches a set is counted once under each distinct value it reaches; a group whose rows all lack a body is reported, with the value its expression has over no rows (ADR-0123); and an expression names a state of the metric's type bare, as `time_in(MISSING)`, or qualified, as `Robot.MISSING` (ADR-0122).

**Labels.** Every type carries labels without declaring any: a label is a built-in observation with a `name`, a personal `note` and the `subject_state` the object was in, applied by a request, never declared (`declaration-syntax.md` §6.8). A label is read only as a metric's source, `source: labels`, whose rows have `subject`, `name`, `subject_state`, `occurred_at`, `recorded_at`, `recorded_by_kind` and `declaration_version`, and not the note, which is personal; an expression anywhere else that reads labels is refused (step 3, `names`; the model's check 55). A label becomes a rule only by promotion to a state, an attribute or an observation kind, which is a published version.

**A combined metric** combines other metrics' values, joined by their dimensions, as a rate divides one count by another: the model's `combine`, dbt's derived metric (ADR-0098, ADR-0118). It may cross types, so it is declared in the module's own `metrics` section, after the types:

```yaml
metrics:
  problems_per_incident:
    description: Problems opened for each incident opened, by month, which says how much of the incident load is being traced to a cause.
    input_metrics:
      problems: Problem.problems_opened
      incidents: Incident.incidents_opened
    group_by: [month]
    expression: problems * 1.000 / incidents
```

`input_metrics` maps the name the expression reads to a metric, `<Type>.<metric>` or a combined metric declared above, so a combined metric never reaches itself (step 3, `names` and `order`; the model's check 56). `group_by` names dimensions its inputs have, or `version` and `actor_kind`, which every metric has; an input without one of them contributes its value to every group that differs only in that dimension. The expression reads only the input metrics' names and aggregates nothing, since a combined metric has no rows of its own; its flags read `value` and its dimensions (step 3, `names`). A module's metrics, its types' and its own, share one namespace, as the model's metric declarations do, so no two have one name (step 3, `names`; the model's check 33). Each input is aggregated over every dimension the combined metric's `group_by` does not name before the expression combines them, so a combined metric has exactly the dimensions it names, and `version` and `actor_kind` only when it names them (ADR-0122). A read that keeps fewer of them, or binds some, aggregates each input the same way over the rows it selects and combines after (ADR-0124). The inputs are joined over every group any of them has: an input with no rows in a group contributes the value its expression has over no rows, as §5 says, so a model never lent has a loan time of zero rather than none.

The keys are the ones the semantic layers use for the same things (ADR-0118, Appendix A.4): dbt, Cube and Looker call the groups dimensions and narrow the rows with a filter, dbt aggregates a metric against its time dimension, and dbt's derived metric names what it combines `input_metrics`.

### 4.10 Shared state machines

A **state machine** that several types share is declared once, under `machines`, and each type that uses it **binds** it with `state_machine: <name>` (UML's `StateMachine`, the behaviour a class's `classifierBehavior` names; `declaration-syntax.md` §2.1, §4). A machine declares:

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the lifecycle is |
| `requires` | no | what a binding type must declare: `attributes`, each as §4.3 declares one, and `invariants`, by name |
| `states`, `conditions`, `transitions` | as a type's | §4.5, §4.7, §4.8, over the attributes it requires |

The attributes a machine requires are its attributes for every rule of §4.5 to §4.8, so an effect that names any other is refused (step 3, `names`). A machine's states MUST NOT declare `required_attributes`, since the invariant that generates belongs to each binder, which declares it instead and names it under `requires` (step 3, `names`).

A type that binds a machine declares no `states` (step 2, `schema`), and MUST declare every attribute and invariant the machine requires, each attribute with the same `type` or `reference` and the same optionality (step 3, `names`; the model's check 16). It MAY declare transitions of its own, which only it has; an initial transition of its own **replaces** the machine's initial transitions for that type. Any other transition, and any condition, MUST NOT share a name with one of the machine's (step 3, `names`). A binder whose objects are created by the machine's initial transition MUST NOT declare a required attribute the machine does not write (step 4, `check 8`), and so declares an initial transition of its own when its objects need one from the start.

### 4.11 Derived attributes

A **derived attribute** is a value computed from the object and never written: UML's derived property (`Property::isDerived`, shown `/name` in a class), whose value OCL states with `derive:`, and the model's `derive` (`declaration-syntax.md` §2, §8.3). A type declares its derived attributes under `derived_attributes`, after its states, whose names an expression reads, and before its invariants and conditions, which read derived attributes:

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the value is |
| `expression` | yes | the value, in the expression language (§5); its type is the expression's |
| `indexed` | no | `true` lets a query filter on it (`declaration-syntax.md` §7) |

A derived attribute is read by its name wherever an attribute is: in a condition, an invariant, an effect's expression and another derived attribute. Of its own type, it reads only the derived attributes declared above it (step 3, `order`); one of another type, read through a relationship or a type scan, may be declared anywhere in the module, since types reference each other in any order (§3), and step 3 refuses such reads that form a cycle, so derived attributes never form one (step 3, `order`; the model's check 3; ADR-0122). It reads neither `inputs` nor `this_event`, since it is evaluated outside any request (step 3, `names`). Its name MUST differ from every attribute and observation kind of the type, which share one scope of names (step 3, `names`; the model's check 33). Nothing writes it: a transition that takes it as an input, or assigns, adds to, removes from or clears it, is refused, and so is a state that requires it, since `required_attributes` names stored attributes; an invariant may read it instead (step 3, `names`). `if … then … else` is allowed in its expression (`declaration-syntax.md` §8.3), and like every expression it MUST NOT read `actor` (step 4, `check 64`) or call `metric(…)` (step 4, `check 56`).

An **indexed** derived attribute reads only what the store holds on the object itself: attributes marked `indexed`, single references that store their value (§4.3, Relationships), and `state`. It MUST NOT read `now`, another derived attribute, a set, a path through a reference into another object, or the object's flow data, `entered_at`, `time_in`, `this.intervals` or `this.transitions` (step 3, `names`; the model's check 46).

A derived attribute is declared in a section of its own rather than among the attributes, where UML lists it, because its expression reads the type's states, which are declared after the attributes; a `derive:` key on an attribute would read a name defined further down (§3).

### 4.12 Sequences and identifiers

A **sequence** is a named counter that mints identifiers, SQL's `CREATE SEQUENCE` and the model's `sequence` (`declaration-syntax.md` §1). Its numbers start at 1 and rise by 1: monotonic, and not gapless, since a number drawn by a request that is then refused is not reused (`DESIGN.md` §14, ADR-0122). A module declares its sequences under `sequences`, each with a `description`, and a type's attribute takes its value from one with `identifier`, when the object is created (`declaration-syntax.md` §3.1):

| Key | Required | Value |
|---|---|---|
| `sequence` | yes | a sequence the module declares or imports (step 3, `names`) |
| `scope` | no | a reference, or an `indexed` attribute, of the type; the sequence numbers each of its values separately |
| `format` | yes | the text minted, with placeholders |

```yaml
number:
  type: string
  identifier: { sequence: delivery_number, format: "DLV-{n:6}" }
  unique: true
```

An identifier is minted during the creation, after the creation's writes and before its effect's other steps, so a creation need not write it, and a state may require it (the model's check 8 counts the mint as a write). Its value is text, so its attribute's `type` MUST be `string` (step 3, `names`). A `scope` MUST be a single reference, or an attribute marked `indexed`, that every creation of the type writes: a required input of every initial transition, or an attribute each assigns a value that is provably present (step 3, `names`; the model's check 44).

A `format` holds literal text and these placeholders, and MUST contain `{n}` or `{n:<width>}` (step 3, `names`):

- `{n}`, the number; `{n:<width>}`, the number padded with zeros to at least that many digits;
- `{<attribute>}`, a `string` or enumeration attribute of the object, an enumeration giving its value's name; `{<reference>.<attribute>}`, one of the object a single reference names, and never further (step 3, `names`);
- `[ … ]`, a segment left out whole when a value inside it is absent.

Each attribute or reference a placeholder reads MUST be written by every creation, as a scope must, and an optional attribute MAY be read only inside `[ … ]` (step 3, `names`; the model's check 44). So `format: "{delivery.number}-{n:2}"` numbers a delivery's checklist items `DLV-000123-01`, `DLV-000123-02` and so on, one series for each delivery.

**Uniqueness.** `unique` takes one of four forms, each the model's sugar for an invariant (`declaration-syntax.md` §3.1). Each is enforced by a unique index on every backend (`storage-schema.md` §8), so a `unique` attribute is indexed without the `indexed` marking, which it may still carry, and a type scan over it meets the model's check 7 (ADR-0122):

| Form | No two objects hold the same value |
|---|---|
| `unique: true` | across the type |
| `unique: in_scope` | within one value of the identifier's `scope`, which the attribute MUST have (step 3, `names`) |
| `unique: { with: [a, …] }` | together with the attributes listed, which the type MUST declare (step 3, `names`) |
| `unique: { where: <expression> }` | among the objects for which the expression holds; it reads what an invariant may, and neither `inputs` nor `this_event` (step 3, `names`) |

An identifier implies none of them, since a scoped sequence repeats its numbers across scopes.

### 4.13 Assertions, corrections, erasure and deletion guards

These are the ways the model changes a record other than by the flow's own steps (`declaration-syntax.md` §4.2, §6.3 to §6.5). UML has no term for an assertion or an erasure, so both keep the model's words (ADR-0119).

**An assertion** (`kind: assertion`) puts an object into a state the request names, from any state, when the record is wrong: an override. Its `to` lists the states it may put the object in, and the request names one of them as its input `to`. It MUST declare an input `reason` whose type is an enumeration, so that overrides are counted by reason; an explanation in words is a further input, optional and `personal`, since free text may name someone (step 3, `names`; the model's check 29). It MUST NOT declare an input named `to` or `admits`, which it has already (step 3, `names`). A request whose `to` is not one of the listed states, or is the object's current state, is refused with `invalid input`, since no transition leaves a state for itself; an assertion is never taken at a final state. Its `effect` MAY write the object, as any transition's does, for what the state it puts the object in cannot hold (ADR-0122).

```yaml
correct_state:
  kind: assertion
  to: [PROSPECT, ACTIVE, DORMANT]
  description: Puts the customer into the state they are really in, when the record is wrong.
  inputs:
    reason: { type: OverrideReason }
    detail: { type: string, optional: true, personal: true }
  may_admit: [active_invariant]
```

`may_admit` lists the invariants a request of the assertion is permitted to break; the request names the ones it does break as its input `admits`, and each admission is recorded on the event. An `admits` naming an invariant `may_admit` does not list is refused with `invalid input`; one the request does not in fact break records no admission. An assertion evaluates the generated guards every request does, `occurred_within` among them, so an `occurred_at` it is given is refused, since it has no `backdating_limit`, and then its own guards, and none of the flow's (ADR-0122). An admitted invariant is the type's own, one generated for a state's `required_attributes` included, or `<Type>.<invariant>`, an invariant of a type reachable from it by relationships whose ends name each other (step 3, `names`; the model's check 27). Only an assertion may admit (step 3, `names`). Step 3 does not require an assertion to set what its target state requires: the invariant is checked when the assertion applies, and the assertion may admit it.

**A correction** is an internal or external transition marked `corrects: [<attribute>, …]`, whose events carry the `corrected` provenance. It MUST write exactly the attributes it lists, by its attribute inputs and its effect, and MUST declare an input `reason` (step 3, `names`; the model's check 28).

**An erasure** (`kind: erasure`) erases every `personal` attribute of its object, at any state, a final one included, since requests to erase arrive for closed records (`declaration-syntax.md` §6.4). It declares neither `from` nor `to` (step 2, `schema`) and MUST declare an input `reason` (step 3, `names`; the model's check 29). It admits every invariant that reads what it erases, and once an object's own erasure is recorded, a request that writes one of its personal attributes is refused by the generated guard `not_erased`. A type holding a personal attribute, its own or inherited, or taking a personal input in a transition, its own or its machine's, MUST declare an erasure, its own or its machine's, since the erasure of an object is what reaches both (step 3, `names`; the model's check 60; PRD D8, ADR-0133, ADR-0134); an abstract base and a machine need not, a type that binds the machine must, and a type with neither need not. An erasure MUST reach every part type holding a personal attribute, by calling that type's erasure in its effect, on the part's attribute or on a `foreach` item over it, or the person's data would stay in the parts (step 3, `names`; the model's check 39). An observation kind's fields are erased with their subject and need no call. Because an erasure writes absence, a `personal` attribute or field MUST be optional (step 3, `names`; the model's check 9).

An assertion's and an erasure's guards MUST be `deny`, and neither has a `backdating_limit`: an assertion's time is when the store was told, and an erasure is not a step of the flow (step 4, `check 57`, `check 58`).

**A deletion guard** is an ordinary condition over `referrers`, every object holding a live reference to this one, parts excepted, since they cascade (`declaration-syntax.md` §6.5). A type's `close` guarded by `none(r in referrers where r.state.category != closed)` is refused while an order of the customer's is open.

### 4.14 Families: abstract types and `extends`

A **family** is an abstract base together with every type that extends it (`declaration-syntax.md` §2): UML's abstract class and generalization, and Jira Service Management's asset object types, which "can have a single parent object type" and may be abstract. A type marked `abstract: true` has no objects of its own. It declares `description`, and may declare `tracking`, `attributes`, `derived_attributes` and `invariants`, and nothing of a lifecycle: no states, transitions, `state_machine`, conditions, observations or metrics (step 2, `schema`).

A type that declares `extends: <Base>` inherits the base's attributes, derived attributes and invariants, and its `tracking` if it declares none, and nothing else: it declares or binds its own lifecycle, and inherits no transition. A reference may name a base, so one relationship serves every type of the family, as `Incident.asset` does in the service desk example.

```yaml
Asset:
  description: A configuration item in the asset schema, the base of every kind of asset; it has no objects of its own.
  abstract: true
  tracking: serial
  attributes:
    name: { type: string }

Laptop:
  description: A laptop, assigned to the person who uses it.
  extends: Asset
  state_machine: AssetLifecycle
```

A base MUST be declared in the module or imported and MUST be abstract, and following a type's bases MUST NOT come back to it (step 3, `names`; the model's check 43). A type MUST NOT declare a member it inherits (step 3, `names`; the model's check 33). A type with objects MUST have a tracking mode, its own or inherited (step 2, `schema`; step 3, `names`; the model's check 42). A base's own rules are checked once, at the base, over its own members, so a base's invariant reads no member only a subtype has, and names states by their category, since a base has no states; what depends on each type's creations, such as an inherited identifier's scope, is checked for each type that extends it.

**A part declared on a base.** A composite end declared on an abstract base carries no `cascade` or `survives`, since the base has no transitions to trigger them. Each concrete subtype gives them in `inherited_parts`, one entry for each part it inherits, with the same `cascade` and `survives` an end of its own takes, and it is held to the coverage rule of §4.3 for them: every transition of the subtype into a final state is cascaded on or survived, never both (step 3, `names`; the model's check 37). An entry MUST name a composite end the type inherits, and an abstract type gives none, having no transitions (step 3, `names`). This is the internal form's top-level `cascade <part>` and `survives <part>` (`declaration-syntax.md` §3.2, ADR-0060 §7); until 2026-09-28 the format could not write it, and a subtype's final transition went unchecked against the base's parts (ADR-0133).

```yaml
module: notes
categories: [live, closed]

types:
  Carrier:
    description: The base of anything that carries notes; it has no objects of its own.
    abstract: true
    tracking: record
    attributes:
      notes:
        reference: "Note[]"
        aggregation: composite
        opposite: subject

  Ticket:
    description: A ticket, whose notes are filed when it is closed.
    extends: Carrier
    inherited_parts:
      notes:
        cascade:
          - { on: [close], transition: file, limit: 100 }
    states:
      OPEN:   { category: live }
      CLOSED: { category: closed, final: true }
    transitions:
      raise: { kind: initial, to: OPEN }
      annotate:
        kind: internal
        from: OPEN
        inputs:
          text: { type: string }
        effect:
          - create: { type: Note, transition: add, inputs: { for_subject: this, body: inputs.text } }
      close: { kind: external, from: OPEN, to: CLOSED }

  Note:
    description: A note on a ticket, or on anything else that carries notes.
    tracking: record
    attributes:
      subject: { reference: Carrier, opposite: notes }
      body:    { type: string }
    states:
      ACTIVE: { category: live }
      FILED:  { category: closed, final: true }
    transitions:
      add:
        kind: initial
        to: ACTIVE
        only_via: [Ticket.annotate]
        required_inputs: [body]
        inputs:
          for_subject: { reference: Carrier }
        effect:
          - assign: { location: subject, expr: inputs.for_subject }
      file: { kind: external, from: ACTIVE, to: FILED }
```

### 4.15 Mirror types and external identifiers

**An external identifier** is an attribute whose value another system gives, marked `external: <source>`, such as a customer's contact in Xero: its values are unique for each source and it MAY be optional, as an object not yet matched to that system has none (`declaration-syntax.md` §3.1). It is a value, never a reference (step 3, `names`). A type that holds one is still this store's, and its transitions, such as `link_xero` in the customers example, are requested by the sync with that system.

**A mirror** is a type another system owns: this store holds it, and only the import writes it (`declaration-syntax.md` §2, ADR-0075). It is marked `mirror: true`, declares its attributes, its states and its external identifier, which the import matches objects by (step 3, `names`), and binds no machine (step 2, `schema`).

```yaml
Employee:
  description: A person employed by the organization, as the HR system records them; the HR system owns the record, and only the import writes it.
  mirror: true
  tracking: record
  attributes:
    employee_id: { type: string, external: hr, indexed: true }
    name:        { type: string, optional: true, personal: true }
  states:
    EMPLOYED: { category: active }
    LEFT:     { category: closed, final: true }
  transitions:
    forget:
      kind: erasure
      description: Erases this store's copy of the person's name; the HR system erases its own.
      inputs:
        reason: { type: string }
```

A mirror takes no transition but an erasure, since erasure is this store's obligation over its own copy (ADR-0101); nothing of this store may write it, by a `call` or a `create`, or join it to a type this store owns by `extends` or a composition, while mirrors may extend and compose with each other (step 4, `check 53`). A type this store owns may reference a mirror, and read it in a guard, as `Laptop.assigned_to` names an `Employee`. The rules about a lifecycle, such as a state needing a way out, do not apply to a mirror, whose states another system moves.

### 4.16 Migrations

A publish changes a live object only by a recorded migration (`declaration-syntax.md` §6.6, ADR-0099). A module's `migration` section says how a publish of this version moves the live objects of the version before it; it is written with the version it publishes, and replaced or removed in the next (ADR-0120):

| Key | Required | Value |
|---|---|---|
| `description` | yes | what this version changes, and how its live objects are moved |
| `removed_states` | no | for each type, each state the previous version had and this one does not, and the state its objects move to |
| `removed_members` | no | for each enumeration, each member the previous version had and this one does not, and the member live values become |
| `renamed_attributes` | no | for each type, each attribute renamed, from its old name to its new |
| `backfill` | no | for each type, each attribute a live object must be given, and the expression that gives it |
| `admit` | no | invariants the migrated objects may still break, each `{ invariant: <Type>.<invariant>, reason: <Enumeration>.<MEMBER> }`, recorded as an admission with its reason |

```yaml
migration:
  description: >-
    Version 2 renames the photo to the completion photo, records the site of
    every job, merges the skills reason for a reassignment into workload, and
    admits the finished jobs from before the photo was required.
  removed_members:
    ReassignmentReason: { SKILLS: WORKLOAD }
  renamed_attributes:
    ServiceJob: { photo: completion_photo }
  backfill:
    ServiceJob: { site: '"HQ"' }
  admit:
    - { invariant: ServiceJob.photo_when_done, reason: MigrationReason.LEGACY_DATA }
```

Against this version alone, a mapping MUST name a state, member or attribute this version no longer has and move it to one it has; a backfill MUST write a stored attribute and read only the object's own members and literals, not `now` or an input; and an admission MUST name an invariant of the type and a member of a declared enumeration (step 3, `names`).

Against the version before it, which the checker is given as `--previous=<file>` and which the examples' version pairs are checked with, every state a type loses MUST be mapped, every mapping MUST name what the previous version had, and a field added to an observation kind MUST be optional, since an observation is born final (step 3, `names`; the model's checks 22 and 23). What depends on the live objects, which only a publish can count, is reported as a notice, `migration`: a new required attribute with no backfill, a member gone with no mapping, and an attribute gone where one of its type is new, which may be a rename. A removed attribute needs nothing: its values stay in history (`declaration-syntax.md` §6.6).

### 4.17 Quantity tracking, counters and defaults

A type tracked by `quantity` counts things rather than tracking each: stock, a batch, a balance (`declaration-syntax.md` §7). It declares at least one **counter**, an attribute of `type: counter`, which is an `int`, never absent, and zero when the object is created without being written; a type tracked by `record` or `serial` declares none (step 4, `check 31`). A counter is never optional and has no default (step 3, `names`), is changed by assigning it, as `reserved := reserved + inputs.quantity`, and is never cleared (step 4, `check 17`). An observation has no counter, being born final (step 3, `names`). Whether a counter may go below zero is a fact about what it counts, and an invariant says it: `reserved >= 0 and reserved <= on_hand`.

```yaml
PartStock:
  description: The stock of one spare part, counted rather than tracked one by one.
  tracking: quantity
  attributes:
    on_hand:  { type: counter, indexed: true }
    reserved: { type: counter, indexed: true }
  derived_attributes:
    available:
      description: The parts on hand and not reserved; a query may filter on it.
      expression: on_hand - reserved
      indexed: true
```

A **default** is the value every creation that does not take the attribute as an input gives it, written first, so that it is recorded on the event as any write is; it never writes an object that already exists, and a publish that adds an attribute gives live objects a value by a backfill (§4.16). Being written before anything else, a default reads no member and no input: it is a literal, a qualified value or `now` (step 3, `names`). A required attribute with a default is written by every creation, and so satisfies a state that requires it (step 4, `check 8`). An attribute with a default MAY be an optional input of any transition: left out of a creation it takes its default, and left out of any other transition it keeps its value, as `accepts` does in the model (`declaration-syntax.md` §5.1); listed in `required_inputs`, it MUST be supplied. *(Corrected 2026-09-27: this said a creation takes such an input as the attribute's optionality says, so that it must be supplied, which contradicted the model the form converts to (D398, ADR-0122).)*

A default, like every expression, MAY be written as YAML's own boolean or number, `default: false` or `default: 0` (§5).

### 4.18 External evaluators

An **evaluator** is a system outside the store that a guard may ask, such as the accounting system that owns invoices (`declaration-syntax.md` §6.2). A module declares its evaluators under `evaluators`, each with a `description` and its `functions`; a function declares a `description`, its `arguments` in the order a call passes them, each as a declared input is (§4.8), and, optionally, `fresh`, how old a verdict may be and still be used, a duration such as `30 min`.

```yaml
evaluators:
  xero:
    description: Xero, which owns the business's contacts and invoices.
    functions:
      invoice_valid:
        description: Whether an invoice with this number exists in Xero and is valid.
        arguments:
          invoice_number: { type: string }
        fresh: 30 min
```

A condition asks one as `<evaluator>.<function>(<argument>, …)`. An evaluator returns a **verdict and never a value**, so the call is the whole condition or its negation, `not xero.invoice_valid(invoice_number)`, never part of a larger expression, where a verdict would hide which part failed (step 3, `names`; the model's check 24). The call names a declared function and passes as many arguments as it takes (step 3, `names`; the model's check 19). Only a guard asks an evaluator: an invariant, a derived attribute or an effect that calls one is refused (step 3, `names`). A verdict too old for its `fresh` bound is refused as `temporal`, and an argument that reads a value the outcome reaching the guard writes, which could never agree before the transaction, is refused at publish (step 4, `check 61`).

An evaluator that does not answer refuses the guard as unsatisfied and unknown, with the remedy `temporal`, whatever the condition declares, since it may answer a moment later (ADR-0138). A condition that asks an evaluator is consulted when its transition is requested, before the transaction, and by no read: `availability`, `available` and `check` each name the guards they did not evaluate (`DESIGN.md` §10). It carries no marking of when. *(Changed 2026-09-28, ADR-0133: the marking `evaluation: eager` or `deferred` is withdrawn. `eager` promised a consultation whenever availability was computed, which no read performs, so it behaved as `deferred`.)*

### 4.19 Supersession

An object is **superseded** when another carries on in its place: a duplicate merged into the customer it duplicated, a work item moved to another space, where a new one takes its work (`declaration-syntax.md` §6.1, ADR-0028). The object's type never changes, and its history stays readable under the rules that wrote it; the new object is linked from the old, and a reader follows the chain on request.

A state marked `superseding: true` is final (step 2, `schema`), and a transition into it names the successor with a `supersede` step; a `supersede` step belongs only to such a transition (step 3, `names`; the model's check 26):

```yaml
merged_into:
  kind: external
  from: [PROSPECT, ACTIVE, DORMANT]
  to: MERGED
  only_via: [Customer.merge_in]
  inputs:
    successor: { reference: Customer }
  effect:
    - supersede: inputs.successor
```

The successor is an input that names an object, or a name an earlier `create` step binds, as a move creates the work item that supersedes the one it moves; never `this` (step 3, `names`; the model's checks 26 and 49). The model's check 26 also refuses "a cycle among the declared supersession targets", which no step checks yet, and whose meaning is open: the model's own example merges a contact into a contact, a cycle among types that must be allowed (`TODO.md`). *Corrected 2026-09-26: this sentence first said step 4 refuses such a cycle; the internal checker's check 26 does not, and the claim was not checked before it was written.* Erasure follows the chain, through each member's own erasure, which §4.13 already requires of every type holding personal data (step 3, `names`; the model's check 60). A superseded whole's parts are cascaded or survive its superseding transition as for any final transition (§4.3).

## 5. Expressions

An `expression` is written in the expression language of `declaration-syntax.md` §8, which this format does not change. Within it:
- an attribute of the object is read by its name, and a value the caller supplies is read as `inputs.<attribute>`;
- every name an expression reads at the start of a path MUST be declared where the expression is evaluated: for a type, one of its attributes, observation kinds or derived attributes (§4.11); for a machine, an attribute it requires (§4.10); for an observation kind's invariant, one of that kind's fields; and in an effect, also the name a `foreach` or `create` step binds. A derived attribute and a uniqueness condition are evaluated outside any request, and read neither `inputs` nor `this_event`. The names an aggregate binds, `count(l in lines where …)`, are declared by it (step 3, `names`);
- `state` is the object's current state, and `now` is the time of the request. Every object also has `id`, `open` (true while its state is neither `closed` nor final), `created_at`, `created_by_kind`, `recorded_from` and `declaration_version`, and every mirror `imported_at`, each read by its name as `state` is (`declaration-syntax.md` §8, ADR-0096, ADR-0106). Step 3 refused them until 2026-09-26, when rewriting `returns-module.md` read `created_at`, which the model admits wherever `state` is;
- `this_event` is the event the current transition will record. An attribute or input of type `event` holds one, and an event-typed input accepts only `this_event`, so a sign-off cannot be recorded against an old event (`declaration-syntax.md` §6.7; step 3, `names`; the model's check 25). `changed_since([<attribute>, …], <event>)` is true when any of the attributes, or any part of a composite end named, has changed since that event, as `signed_off` in the delivery example requires of an approval: `any(a in approvals where not changed_since([price, checklist_items], a.at_event))`;
- `referrers` is every object holding a live reference to this one, parts excepted, and a category is read by its name, as in `r.state.category != closed` (§4.13);
- an enumeration value and a state of another type are written qualified, `<Enumeration>.<VALUE>` and `<Type>.<STATE>`; a value of an enumeration or of a type the module declares MUST exist, and a bare upper-case name MUST be a state of the type (step 3, `names`);
- an expression that is a boolean or a number MAY be written as YAML's own, `is_debit: true` or `default: 0`, and stands for that literal, where the schema says an expression goes and no key there takes the value as YAML's own, as `final: true` and `limit: 50` do; any other expression is a YAML string, and a text literal is quoted inside it, `site: '"HQ"'` (step 2, `schema`). A YAML author writes `true` bare, and YAML reads it as a boolean before the format sees it, so refusing it as not a string would be a trap; rewriting the design's examples found the refusal on 2026-09-26, first in a default and then in a creation's inputs;
- a money literal is a currency and an amount, `USD 0.00`, as in the model (`declaration-syntax.md` §8);
- over no elements, `count` and `sum` are 0, `all` and `none` are true, `any` is false, and `min`, `max`, `avg`, `median` and `percentile` are absent (ADR-0122);
- absence is tested with `is null` and `is not null`; a comparison with an absent value is unknown, and a guard that is unknown refuses (`declaration-syntax.md` §8.2);
- only an effect's value MAY read `actor`, as `assign: { location: handled_by, expr: actor.id }` records who acted; a condition, an invariant, a derived attribute, a default, a metric and every other expression MUST NOT, since who may act is decided outside the flow (ADR-0114; step 4, `check 64`). *(Corrected 2026-09-26: this said that no expression may read `actor`, which overstated the model's check 64 and contradicted the delivery example, whose `approve` writes `actor.id`.)*
- `actor.id` is the identity the request named its actor by, the value the acting object holds in its attribute marked `actor_kind`, and not that object's own `id`; `actor.kind` is that type's actor kind (`declaration-syntax.md` §3.1, ADR-0122);

## 6. What a description declares

Each form converts to the text language as follows, and means what that declaration means.

| Description | Text language |
|---|---|
| `kind: initial`, `to: S` | `create <name> -> S` |
| `kind: external`, `from: [A, B]`, `to: C` | `do <name> { A, B } -> C` |
| `kind: internal`, `from: [A, B]` | `act <name> at { A, B }` |
| `required_inputs: [a]`, `optional_inputs: [b]` | `accepts a, b` |
| `inputs: { r: { type: T, optional: true, personal: true } }` | `input r : T? personal` |
| `required_inputs: [a]` where `a` is optional or has a default | also the guard `require a_provided: inputs.a is not null because self_serviceable`, evaluated after the other generated guards and before the transition's own, in the order of `required_inputs` (§4.8) |
| `guards: { g: deny }` | `require g: <expression> because <remedy>` |
| `guards: { g: audit }`, `{ g: warn }` | the same, marked `observe` or `flag` |
| `assign: { location: b, expr: e }` | `set b := e` |
| `add: { location: b, expr: e }`, `remove: …` | `add b := e`, `remove b := e` |
| `call: { target: p, transition: t, inputs: { i: e } }` | `call p.t(i := e)` |
| `create: { type: T, transition: t, inputs: { i: e }, result: r }` | `create r = T.t(i := e)` |
| `foreach: { item: x, array: c, where: w, limit: n, steps: […] }` | `for x in c where w limit n { … }`; with `range: e`, `for x in 1..e limit n { … }` |
| `clear: [a]` | `clear a` |
| `final: true` on state `S` | `state S category … terminal` |
| `required_attributes: [a, b]` on state `S` | `invariant s_invariant: state != S or (a is not null and b is not null)` |
| `backdating_limit: 2 days` | `backdatable within 2 days` |
| `machines: { M: { requires: …, states: …, transitions: … } }` | `machine M version 1 { requires attr …; state …; … }` |
| `state_machine: M` on a type | `machine M` in the type, with only the type's own transitions |
| `summary: [a, b]` | `summary  a, b` |
| `abstract: true`, `extends: B` | `type T extends B version 1 abstract { … }`, with only the type's own members |
| `migration: { removed_states: { T: { A: B } }, removed_members: { E: { X: Y } }, renamed_attributes: { T: { a: b } }, backfill: { T: { a: e } }, admit: […] }` | `removed state A -> B`, `removed member E.X -> Y`, `renamed attr a -> b`, `backfill a := e`, `admit T.i because E.M`, at the end of the module |
| `type: counter`; `default: e` | `counter c`; `default e` |
| `metrics: { m: { input_metrics: { a: T.x }, group_by: [d], expression: e } }` | `metric m version 1 { combine a = x; by d; value e }` |
| `superseding: true`; `supersede: s` | `state S category … terminal superseding`; `supersede s` |
| `evaluators: { x: { functions: { f: { arguments: { a: { type: t } }, fresh: d } } } }` | `evaluator x version 1 { fn f(a : t) fresh d }` |
| `mirror: true`; `external: s` | `type T version 1 mirror { … }`; `external "s"` |
| `derived_attributes: { d: { expression: e, indexed: true } }` | `derive d = e indexed` |
| `sequences: { s: … }` | `sequence s version 1` |
| `identifier: { sequence: s, scope: r, format: f }` | `identifier from s scoped by r format "f"` |
| `unique: in_scope`, `unique: { with: [a, b] }`, `unique: { where: e }` | `unique in scope`, `unique with a, b`, `unique where e` |
| `only_via: [W.t]` | `only via W.t` in the transition's head |
| `reference: T, opposite: e` (and `stored: true`) | `ref <name> : T inverse e` (`stored`) |
| `reference: "T[]", aggregation: composite, opposite: e` | `part <name> : T[] inverse e` |
| the part's end back, `reference: W, opposite: <composite end>` | `owner <name> : W inverse <composite end>` |
| `cascade: [{ on: [a], transition: t, limit: n }]`, `survives: [b]` | `cascade on a to T.t limit n`, `survives on { b }` |
| `inherited_parts: { p: { cascade: [{ on: [a], transition: t, limit: n }], survives: [b] } }` | `cascade p on a to T.t limit n`, `survives p on { b }`, at the top level of the type (`declaration-syntax.md` §3.2) |
| `measure: median_time_in_state` | a metric over the type's intervals in the state, valued `median(i.duration)`, by `month(i.entered_at)`, `week(i.entered_at)`, `i.held(<a>)` for a tracked attribute and `i.object.<a>` for another, windowed on `i.entered_at` (ADR-0139) |
| `measure: transition_count` | a metric over the type's transitions along the transition's states, valued `count()`, by `month(t.occurred_at)`, `week(t.occurred_at)`, `t.actor_id` for `actor`, `t.held(<a>)` for a tracked attribute and `t.object.<a>` for another, windowed on `t.occurred_at` (ADR-0139) |
| `kind: assertion`, `to: [A, B]`, `may_admit: [i]` | `assert x -> { A, B } { input to : state; input admits : invariant[]?; may admit i }` |
| `from: any` | `do x any -> S`; `act x at any` |
| `proposable: true` | `proposable` in the transition's head |
| `kind: erasure` | `erase x { … }` |
| `corrects: [a]` | `corrects a` |
| `source: s`, `item: i`, `filter: f` | `from i in <T>.s where f`; `from i in <T>` for `objects`, and `from i in <Kind>` for an observation kind |
| `dimensions: { d: e }`, `time_dimension: t` | `by d = e`, `window on t` |
| `expression: v`, `flag_when: { n: c }` | `value v`, `flag n when c` |

The generated guards and invariants take the names shown, which §3 reserves. The internal form keeps its own keywords for these forms (`create`, `do`, `act`, `terminal`, `observe`, `flag`); since ADR-0116 no one writes it.

## 7. Validity

A description is checked in six steps, and each stops the check if it finds anything fatal. Every finding names the file, the line and the construct.

| Step | Code | Refuses |
|---|---|---|
| 1. strict loading | `yaml` | a file YAML cannot parse, including a `?` unquoted inside an inline collection, or a key repeated in a mapping |
| 2. structure | `schema` | a missing or unknown key, a value of the wrong form, a name in the wrong case, a transition kind without the `from` and `to` it requires, a metric without the `state` or `transition` its measure requires |
| | `syntax` | an expression the grammar of `declaration-syntax.md` §8 does not accept, such as a comparison that chains or a list in brackets, which `scripts/flowexpr.py` parses (ADR-0137) |
| 3. names and order | `order` | sections out of order, an examples file's keys out of order or a setup given one declared after it (§11.1), a derived attribute that reads itself or one of its type declared after it, derived attributes of several types that read each other in a cycle |
| | `names` | a name that is not declared (a state, condition, attribute, category, transition or value), a reserved name (§3), an unused condition, an input a transition reads and does not take, an optional set input, a `create` or `call` that names no such transition or passes the wrong inputs, an `add` or `remove` on an attribute that is not a set, a relationship whose ends do not name each other, a part whose end back is optional or a set, a cascade to a transition the part does not have, a final transition of a whole its parts neither cascade on nor survive, an `only_via` naming no transition, a reference to a type neither declared nor imported, an imported name its module, checked before, does not declare, a binder that lacks what its machine requires, declares it with another type or optionality, or redeclares one of the machine's transitions or conditions, a machine whose condition or effect names an attribute it does not require, a derived attribute that is written, required by a state, reads what only a transition has, or shares a name with another member, an indexed derived attribute that reads what the store does not hold indexed on the object, an identifier from no declared sequence or on an attribute that is not a string, a scope that is not a single reference or indexed attribute every creation writes, a format without a number or with a placeholder §4.12 does not allow, a uniqueness in scope without a scope or with an attribute the type does not declare, a metric over a source its type does not have or reading anything but its item and the members its rows have, a flag reading neither the value nor a dimension, an assertion, erasure or correction without a `reason` input, an assertion's reason that is not an enumeration, an admission of an invariant neither the type's nor a related type's, a correction that does not write exactly what it lists, an erasure that does not reach a part type holding personal data, a personal attribute that is required, a type that extends one that is undeclared or not abstract or comes back to itself, redeclares what it inherits, or has no tracking, a mirror without an external identifier, an external identifier on a reference, a counter that is optional, has a default or belongs to an observation, a default that reads a member or an input, a call to an evaluator's undeclared function or with the wrong number of arguments, a verdict inside a larger expression or outside a condition, a proposable transition that is only via others, an event input given anything but `this_event`, a summary naming what is not a member, a metric name two metrics of the module share, a combined metric over an undeclared metric or one not above it, grouped by a dimension its inputs lack, reading beyond its inputs or aggregating, a supersede outside a transition into a superseding state or such a transition without one, a successor that is `this` or neither an object input nor a created name, a type holding personal data or taking a personal input with no erasure, a cascade or survives on an abstract base's part, an entry in `inherited_parts` on an abstract type or naming no part the type inherits, an inherited part a final transition neither cascades on nor survives, a migration mapping that names what this version still has or lacks the target of, a backfill of no stored attribute or one reading beyond the object, an admission of no invariant or reason, and, against the previous version, a state removed with no mapping, a mapping from what it did not have, or a required field added to an observation kind, a transition that both writes and clears an attribute, an optional input on an attribute neither optional nor defaulted, a set of references with no opposite outside a machine's requirements, an optional input read inside a larger expression of a step, an attribute key the attribute's kind has no form for (§4.3), a `transition_count` that cannot be told apart, and in an examples file a name of §11 that does not resolve, an alias named before a step gives it, an input of the wrong form, a setup step that cannot apply, or an expectation the request cannot give (§11.5) |
| | `required` | a transition into a state that does not set, or that clears, an attribute the state requires |
| 4. publish checks | `check N` | anything the text language's implemented publish checks refuse, reported at the line of the description it came from |
| 5. logic | `contradiction` | within one type, a transition no request can take, since from its states and with its required inputs its blocking guards cannot all hold, or what it writes makes an invariant of the state it enters false whatever the request; and a state no object can be in, since its invariants cannot hold together while a transition enters it (§7.1) |
| 6. examples | `example` | an example of the module's examples file whose request does not do what its `expect` says, or whose setup does not apply, run by the reference runner, `scripts/flowrun.py` (§11.5) |

Step 3 also reports ten notices, which are not fatal: `audit`, for each guard that audits; `warn`, for each guard that warns; `self_serviceable`, for each guard whose condition is `self_serviceable` on a transition that takes no input; `imports`, for a module imported from that is not among the files checked; `metric`, for a declared metric over transitions whose filter compares a transition's `to_state` to a state the version changes the transitions into, or its `from_state` to one it changes the transitions out of, since what it counts changes from that version on (ADR-0129); `coverage`, for the transitions of a module that no example in its examples file requests (§11); `neverpasses`, for a guard that audits or warns and can never pass when its transition is taken; `undecided`, for conditions that expand to more cases than step 5 decides; `notrun`, for an example that reaches a construct the reference runner does not yet execute, so it is reported rather than passed; and `migration`, for what a publish over live objects of the previous version would need (§4.16).

**A change's source digest.** `scripts/check-flows.py --digest <files>` computes the digest a flow change records beside its source reference: SHA-256 over the files' modules in ascending order of their names, each framed by its name and its length in bytes (ADR-0135). A repository's checks compute it over the files at a commit and compare it with the one a published version records, so the version is traced to the commit and checked against it.


### 7.1 What step 5 decides

Step 5 refuses only what it proves (ADR-0131 decisions 7 to 9, ADR-0137). Each condition is expanded into cases under the three-valued logic of `declaration-syntax.md` §8.2: a guard passes only when true, an invariant holds unless false, and a comparison is unknown when a value it reads is absent. A case is decided when every part of it is one of: a value present or absent; a state, an enumeration value or a boolean within a set; text or an identity equal or not to a literal; or a comparison of numbers, amounts, durations, timestamps or counts over the object's own collections, one side a value of the type or an input and the other a constant or another such value plus a constant. A derived attribute is read as its expression, and `open` as the states it names. Any other part, such as a path to another object, a metric, an evaluator, `all` or arithmetic beyond adding a constant, is left open: it may be true, false or unknown, so it can only make conditions look possible, and a flow is never refused over a clause the step cannot read. The same part written twice is the same value, since a request reads each once.

Nothing is assumed of what an earlier request wrote beyond what the declaration makes it: a required attribute is present and an optional one may be absent, and an invariant, which may stand violated under an admission (§4.13), counts against a transition only where it reads nothing but what the transition writes and the state it enters. A creation reads no earlier value of its object. A condition that expands to more than 4096 cases is reported as `undecided` rather than decided. The checker prints, for each module, how many of its guard conditions step 5 decided in full, and a publish report carries the count for each type (`publish-and-import.md` §2).

`scripts/flowlogic.py` implements the step, and its self-test decides twenty-two small types, each contradiction found and each set that can hold left alone.
## 8. A minimal valid description

```yaml
module: documents
categories: [live, closed]

types:
  Document:
    description: A document that is drafted and then published.
    tracking: record

    attributes:
      title:        { type: string }
      published_at: { type: timestamp, optional: true }

    states:
      DRAFT:
        description: Being written.
        category: live
      PUBLISHED:
        description: Released to its readers.
        category: closed
        final: true
        required_attributes: [published_at]

    transitions:
      start:
        kind: initial
        to: DRAFT
        required_inputs: [title]
      publish:
        kind: external
        from: DRAFT
        to: PUBLISHED
        effect:
          - assign: { location: published_at, expr: now }
```

A full description is [`flow-format/examples/inventory.yaml`](flow-format/examples/inventory.yaml); [`flow-format/examples/service.yaml`](flow-format/examples/service.yaml) adds an import, an assignee and observations.

## 9. Open questions

These are open in `authoring-flows.md` §3 and §7, and an answer would change this document:
1. ~~Whether version numbers are computed at publication~~: *decided 2026-09-28*, they are, and a description carries none (ADR-0131).
2. Whether the generated guards and invariants should carry written descriptions.
3. Whether a transition whose source state does not require an attribute its target requires should be refused, as now, even when the attribute happens to be kept.
4. ~~How two types that reference each other are ordered~~: *decided 2026-09-26*, the types of a module may reference each other in any order (ADR-0117).
5. ~~The order in which the constructs of Appendix B gain a form~~: *decided 2026-09-26*, the author's order and the placement of the parts outside it, which `TODO.md` records.

## 10. Reserved vocabulary

These words are reserved by the format. `scripts/check-flow-format-doc.py` holds this list equal to the keys and values `flow.schema.json` and the checker define.

**Keys:** `module`, `imports`, `categories`, `enumerations`, `sequences`, `evaluators`, `functions`, `arguments`, `fresh`, `machines`, `requires`, `state_machine`, `types`, `description`, `abstract`, `extends`, `mirror`, `tracking`, `attributes`, `inherited_parts`, `observations`, `states`, `derived_attributes`, `summary`, `invariants`, `conditions`, `transitions`, `metrics`, `category`, `final`, `superseding`, `required_attributes`, `type`, `reference`, `opposite`, `stored`, `aggregation`, `cascade`, `on`, `survives`, `only_via`, `proposable`, `corrects`, `may_admit`, `optional`, `identifier`, `external`, `sequence`, `scope`, `format`, `unique`, `with`, `indexed`, `personal`, `actor_kind`, `assignee`, `unit`, `kind`, `max_recording_delay`, `expression`, `remedy`, `from`, `to`, `required_inputs`, `optional_inputs`, `inputs`, `default`, `guards`, `effect`, `assign`, `location`, `expr`, `clear`, `add`, `remove`, `supersede`, `call`, `target`, `create`, `result`, `foreach`, `item`, `array`, `range`, `where`, `limit`, `steps`, `backdating_limit`, `migration`, `removed_states`, `removed_members`, `renamed_attributes`, `backfill`, `admit`, `invariant`, `reason`, `measure`, `source`, `filter`, `dimensions`, `time_dimension`, `input_metrics`, `state`, `transition`, `group_by`, `flag_when`.

**Values:** `true`, `false`, `closed`, `record`, `serial`, `quantity`, `human`, `agent`, `service`, `initial`, `external`, `internal`, `assertion`, `erasure`, `any`, `deny`, `audit`, `warn`, `self_serviceable`, `delegable`, `temporal`, `dependent`, `unreachable_from_here`, `median_time_in_state`, `transition_count`, `objects`, `intervals`, `transitions`, `attempts`, `attempt_counts`, `labels`, `composite`, `in_scope`, `month`, `week`, `actor`.

**In expressions:** the reserved words of the text language, `declaration-syntax.md` §9.5.

**Example keys:** `examples_for`, `setups`, `examples`, `description`, `given`, `evaluators`, `steps`, `request`, `import`, `type`, `object`, `transition`, `as`, `actor`, `after`, `inputs`, `state`, `values`, `expect`, `verdict`, `clause`, `remedy`, `cascaded`.

**Example values:** `operator`, `now`, `satisfied`, `unsatisfied`, `unanswered`, `applied`, `unavailable`, `invalid_input`, `not_requestable`, `over_limit`, `invariant_violated`, `call_refused`, `self_serviceable`, `delegable`, `temporal`, `dependent`, `unreachable_from_here`.

## 11. Examples

A claim about what a flow does is an **example** the engine runs, not a sentence (ADR-0131 decision 4, ADR-0136). A module's examples are written in an **examples file** beside its description, named after it with `.examples.yaml` in place of `.yaml`: `service.yaml`'s are `service.examples.yaml`. The file belongs to a flow change's source as the description does, so a change that alters the rules and the examples that describe them is one change, and the source digest covers it, named `<module>.examples` (ADR-0135). An examples file SHOULD name `examples.schema.json` on its first line, as a description names its schema (§2). [`flow-format/examples/service.examples.yaml`](flow-format/examples/service.examples.yaml) is a complete one, and `service-v2.examples.yaml` the same examples as version 2 updates them.

### 11.1 The file

| Key | Required | Value |
|---|---|---|
| `examples_for` | yes | the module whose rules the examples exercise, which opens the file, so an examples file is never mistaken for a description |
| `setups` | no | named starting states, each built by requests (§11.2) |
| `examples` | yes | named examples, at least one (§11.4) |

The keys are written in that order, a setup is given only a setup declared before it, and names are unique within `setups` and within `examples` (step 3, `order` and `names`). A setup's and an example's `description` is the author's prose: it is printed beside the example as its rationale, unverified, and nothing checks it.

### 11.2 Setups

A **setup** is a starting state, built by requests from an empty store: one that holds declaration version 0, with its operator, and the version under check (ADR-0126). It is never written directly. A state built by requests is one the flow can reach, so an example proves nothing about an object the rules could never produce, and no second write path into governed state exists, even in a test.

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the state is |
| `given` | no | a setup declared before this one, whose steps come first |
| `evaluators` | no | the stubbed answers of §11.3, for this setup's steps |
| `steps` | yes | the requests and imports that build it, in order (§11.3) |

Every step of a setup MUST apply. A step the rules refuse makes each example given the setup fail, reported as the setup's failure, not as the example's verdict. Step 3 refuses what it can see without running: a step that leaves out a required input, or requests a transition that is only via others (§4.8).

### 11.3 Steps

A step is a `request` or an `import`, each as one key.

| `request` key | Required | Value |
|---|---|---|
| `type` | for a creation | the type created, whose transition MUST be its initial one |
| `object` | otherwise | the alias an earlier step gave the object acted on |
| `transition` | yes | a transition the type declares, its own, its machine's or its base's |
| `as` | no | on a creation, the alias later steps name the new object by |
| `actor` | no | who makes the request: `operator`, the default, or the alias of an object whose type marks an actor identity (§4.3) |
| `after` | no | a duration the clock advances before the step, such as `1 h` |
| `inputs` | no | the request's inputs, each one the transition takes (§4.8) |

An `import` writes one object of a `mirror` type, which only the import writes (§4.15), so an example can start from what a legacy system held: its `type`, the `as` alias, the `state` it arrives in, its attribute `values` and an optional `after`.

**Values.** An input or value is written as the attribute or input it fills declares:
- a reference, by the alias of an object of that type or a subtype, and a set of references as a list of aliases;
- an enumeration value, qualified, as `ReassignmentReason.WORKLOAD`;
- a timestamp, in UTC, as `2026-01-01T09:00Z`, or as `now` or `now + <duration>`, the clock at that step;
- a file, by a file name, for which the example supplies a file whose content is that name;
- anything else as its literal.

**Recording.** A datapoint is recorded by requesting its kind's generated creation, `record`, with `subject` and the kind's fields as inputs, as any caller records one (§4.4, `declaration-syntax.md` §6.8).

**The clock and ids.** Every example's clock starts at `2026-01-01T00:00:00Z` and moves only by `after`, and object ids come from an injected source, so an example gives the same result on every run and every machine (ADR-0077).

**Evaluators.** An example never calls another system. `evaluators` maps each evaluator function a step or the request calls, as `<evaluator>.<function>`, to the answer it gives, `satisfied`, `unsatisfied` or `unanswered`, the last refusing the guard as unknown with the remedy `temporal` (§4.18); an example's map extends its setup's, and overrides it where both name a function. A call with no stubbed answer fails the example.

### 11.4 Examples and their expectations

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the example shows, in the author's words |
| `given` | no | the setup it starts from; none starts from the empty store |
| `evaluators` | no | stubbed answers, §11.3 |
| `steps` | no | further steps, after the setup's, that MUST apply |
| `request` | yes | the request under test, written as a step's |
| `expect` | yes | what the request MUST do |

| `expect` key | Value |
|---|---|
| `verdict` | `applied`, or the refusal: `unsatisfied`, `unavailable`, `invalid_input`, `not_requestable`, `over_limit`, `invariant_violated` or `call_refused` (`DESIGN.md` §5.5) |
| `clause` | for `unsatisfied`, the clause that refuses first: a guard of the transition, a generated guard, or `<Type>.<condition>` of a part a cascade drives; for `invariant_violated`, the invariant, qualified where it is another type's; for `call_refused`, optional, the refused call's or creation's clause, always qualified |
| `remedy` | for `unsatisfied`, the refusing clause's remedy class; for `call_refused`, optional, the refused call's |
| `state` | for `applied`, the state the object is in afterwards |
| `values` | for `applied`, optional: attributes and the values they hold afterwards, written as §11.3's values |
| `cascaded` | for `applied`, optional: the transitions the request's cascades took, each as `<Type>.<transition>` |

An `unsatisfied` expectation MUST name its clause and remedy, and the remedy MUST be the one the clause declares; an `applied` one MUST name the state, which MUST be one the transition leaves its object in; a refusal names no state or values, and only `unsatisfied`, `invariant_violated` and `call_refused` name a clause (step 3, `names`). A `call` or `create` of the effect that is refused gives `call_refused`, and a part a cascade drives that refuses gives its own verdict, its clause qualified by its type (`DESIGN.md` §5.5). A request that leaves out a required input is refused as `invalid_input`, so an example expecting any other verdict for one is refused.

### 11.5 What is checked, and when

**Shape, whenever a description is checked.** The flow checker checks an examples file after the modules it is given, at step 2 against `examples.schema.json` and at step 3 against those modules: every name of §11.1 to §11.4 resolves, each alias is given before it is named and only once, each input is one the transition takes and has the form its type asks, and each expectation is one the request can give. It then reports the notice `coverage`, naming the examined module's transitions that no example requests (§7). Coverage is reported and never required, so a first draft publishes (ADR-0131 decision 5).

**Running, at every publish.** The engine runs every example of the change's source against an empty in-memory store with the clock and the id source injected, after the checks of `publish-and-import.md` §1 step 1, so it tests the rules and not the live data. An example whose request does not do what `expect` says, or whose setup does not apply, refuses the publish; since the examples are part of the change, the change that breaks one is refused until it updates it (ADR-0131 decision 6). The publish report lists, for each transition, the examples that request it, applied and refused.

**Running, whenever a description is checked.** The flow checker's step 6 runs every example with the reference runner, `scripts/flowrun.py`, an executable model of a request's execution (`DESIGN.md` §6) and of the expression language's meaning, which shares nothing with the engine but the parser, so each is the other's oracle (ADR-0132 decision 2). An example that does not do what it expects is fatal, `example`. The runner is built in slices (`TODO.md`), and an example that reaches a construct it does not yet execute, such as a metric a guard reads, is reported as the notice `notrun`, naming the construct, never counted as passed.

## Appendix A. Terminology against established conventions

A review of 2026-09-26, at the author's request that this format use existing conventions where they exist rather than invent terms. Every source below was read in its primary form: UML 2.5.1 (OMG formal/2017-12-05, https://www.omg.org/spec/UML/2.5.1/PDF), W3C SCXML (Recommendation, 1 September 2015, https://www.w3.org/TR/scxml/), XState v5 (https://stately.ai/docs), the Symfony Workflow component (https://symfony.com/doc/current/workflow.html), AASM (https://github.com/aasm/aasm), django-fsm 2.8.2 (https://github.com/viewflow/django-fsm/blob/2.8.2/README.rst), Kubernetes ValidatingAdmissionPolicy (https://kubernetes.io/docs/reference/access-authn-authz/validating-admission-policy/), OPA Gatekeeper (https://open-policy-agent.github.io/gatekeeper/website/docs/violations/) and Atlassian Jira (https://support.atlassian.com/jira-cloud-administration/docs/what-is-a-workflow-status/). The author adopted the established terms the same day (ADR-0115); §A.3 records what changed.

### A.1 The terms

| This format | Established term | Source | Assessment |
|---|---|---|---|
| `states`, `transitions` | state, transition | UML §14; SCXML §3.3, §3.5; XState | standard |
| `from`, `to` | `source`, `target` | UML §14.5.11.6; SCXML `target` | both are established: UML's model says source and target, the configuration languages of Symfony and AASM say `from` and `to` |
| `guards` | guard | UML `Transition::guard`; XState `guard`; AASM `guard:`; Symfony `guard` | standard |
| `conditions` | `cond`; conditions; named guards | SCXML `<transition cond>`; django-fsm 2.x `conditions=[…]`; XState `setup({ guards })` | established |
| a transition's name, which a request names | trigger or event | UML `Transition::trigger`; AASM `event`; Symfony applies a transition by name | standard in substance; §4.8 says so |
| `kind: action` (draft 1) | internal transition; targetless transition | UML `TransitionKind::internal`, §14.5.12.3: it "occurs without exiting or entering the source State (i.e., it does not cause a state change)"; SCXML Appendix D; XState "targetless self-transition" | **conflicted**: in UML, SCXML and XState an action is behaviour a transition executes, not a kind of transition; now `internal` |
| `kind: state_change` (draft 1) | external transition | UML `TransitionKind::external`, the default | invented where UML has a term; now `external` |
| `kind: creation` (draft 1) | initial transition, from the initial pseudostate | UML §14.2.4.6; SCXML `<initial>`; XState `initial` | invented; now `initial`, although an initial transition here is named, takes inputs and may be one of several |
| `terminal` (draft 1) | final state | UML `FinalState`; SCXML `<final>`; XState `type: 'final'` | the design's own word; now `final` |
| `required_attributes` | state invariant | UML `State::stateInvariant`, §14.5.9: "conditions that are always true when this State is the current State" | a special case of a standard concept, which §4.5 now names; the generated invariant is `<state>_invariant` |
| `invariants` | invariant | UML and OCL class invariant | standard |
| `outcome` (draft 1) | effect; actions | UML `Transition::effect`, §14.5.11.6; XState `actions`; SCXML executable content | ADR-0046's term; now `effect` |
| `copy` (draft 1) | assign | SCXML `<assign location expr>`, §5.4; XState `assign` | invented; now `assign`, taking an expression |
| `clear` | none | | no standard term; an assignment of no value |
| `enforced`, `observed`, `flagged` (draft 1) | `Deny`, `Audit`, `Warn`; `deny`, `dryrun`, `warn` | Kubernetes `ValidatingAdmissionPolicyBinding.validationActions`; OPA Gatekeeper `enforcementAction` | no state-machine standard has these; Kubernetes' three match the three meanings, Deny refusing, Audit recording the failure, Warn reporting it to the client; now `deny`, `audit`, `warn` |
| `category` | status category | Jira: statuses "must belong to one of three status categories – To do, In progress, or Done" | established outside the state-machine standards |
| `required_inputs`, `optional_inputs` | parameters; event data | UML operation parameters; SCXML `_event.data` | the design's `inputs`, which expressions read as `inputs.<attribute>`; consistent with the expression language |

**The model as a whole** is closest to a UML protocol state machine, which "specifies which BehavioralFeatures of that Classifier can be invoked in a given protocol state and under what conditions" (§14.4.3.1). It departs in one respect: a protocol transition has no effect (§14.4.3.2), and a transition here writes attributes. It is therefore a behavioural state machine whose triggers are named requests.

### A.2 The diagram

UML's notation (§14.2.4) against the walkthrough's diagram as it was drawn for draft 1; §A.3 says what the diagram now does:
- the initial pseudostate is "a small solid filled circle" (§14.2.4.6), as drawn;
- a final state is "a circle surrounding a small solid filled circle" (§14.2.4.5); the double border drawn on `RETIRED` is the automata-theory convention for an accepting state, "denoted graphically by a double circle" (Wikipedia, Deterministic finite automaton);
- an internal transition is listed in the state's internal-transitions compartment (§14.2.4.4) and is "not shown explicitly" as an arrow (§14.2.4.9); a loop drawn outside a state is an external self-transition, which exits and re-enters the state, so the loops drawn for `regrade` read as the wrong kind;
- a transition's label is `trigger [guard] / behavior-expression`, every part optional (§14.2.4.8), so a label of the name alone conforms;
- a frame drawn around states is, in UML, a composite state, which implies a hierarchy this format does not have; the `DELIVERED` frame is also drawn from nothing the description declares, which departs from the rule that the diagram maps onto the description.

### A.3 Adopted

On 2026-09-26 the author answered "adopt standards and update our specs" (ADR-0115):
1. The transition kinds are UML's: `initial`, `external` and `internal`. ADR-0016 carries a note that its "self-transition" is UML's internal transition, and `DESIGN.md` says internal transition where it said action.
2. The walkthrough's diagram lists internal transitions inside the states they occur in, draws a final state as UML does, and replaces the frame around the delivered states with a UML comment anchored to them.
3. `terminal` is `final`, `outcome` is `effect`, and `copy` is `assign`, which takes an expression.
4. The guard enforcements are Kubernetes' `deny`, `audit` and `warn`.
5. §4.5 names `required_attributes` as the state's invariant, generated as `<state>_invariant`, and §4.8 states that a transition's name is its trigger.
6. The delivered grouping is drawn as a comment, which declares nothing, rather than as a frame, which in UML declares a composite state.

The text language kept its own keywords until the author decided that this format replaces it (ADR-0116); it survives only as the internal form the checker reads.

### A.4 Metric terms

On 2026-09-26 the author asked that the metric language's terms be verified before any was adopted, and then accepted them (ADR-0118). Each was checked against the product's current documentation that day:

| This format | The model (§6.9) | dbt (MetricFlow) | Cube | Looker (LookML) |
|---|---|---|---|---|
| a metric under the type it reads | `metric`, at the top level | a simple metric under its semantic model; a ratio or derived one at the top level | a measure in its cube | a measure in its view |
| `dimensions` | `by` | dimensions: "the non-aggregatable columns … that describe or categorize data" | dimensions: "attributes related to measures"; a query's `dimensions`, "An array of dimensions" | `dimension` |
| `filter` | `where`, in `from` | `filter`, "WHERE clause equivalent" | a measure's `filters` | a measure's `filters` |
| `time_dimension` | `window on` | `agg_time_dimension` | `timeDimensions`, "grouping and filtering by a time dimension" | a `dimension_group` of `type: time` |
| `day` … `year`, a week from Monday | the same (ADR-0106) | granularities `day` to `year` | the same; "week (starting on Monday)" | timeframes `date` to `year`; "weeks in Looker start on Monday" |
| `avg`, `median`, `percentile` in an expression | the same | `agg: average`, `median`, `percentile` | `type: avg`; no median or percentile type | `type: average`, `median`, `percentile` |
| `flag_when` | `flag` | none | none | none |
| `input_metrics`, in a module's `metrics` | `combine` | a derived metric: `expr` over `input_metrics` | none | none |

Sources: dbt's [latest spec](https://docs.getdbt.com/docs/build/latest-metrics-spec), [metrics overview](https://docs.getdbt.com/docs/build/metrics-overview), [dimensions](https://docs.getdbt.com/docs/build/dimensions) and [simple metrics](https://docs.getdbt.com/docs/build/simple); Cube's [dimensions](https://docs.cube.dev/reference/data-modeling/dimensions), [measures](https://docs.cube.dev/reference/data-modeling/measures) and [query format](https://docs.cube.dev/reference/core-data-apis/rest-api/query-format); Looker's [measure types](https://docs.cloud.google.com/looker/docs/reference/param-measure-types) and [dimension groups](https://docs.cloud.google.com/looker/docs/reference/param-field-dimension-group). dbt's week start day was not checked. *Retracted 2026-09-26: the Cube cell first quoted a query's "dimensions to group by", wording a summarising fetch produced and the page does not contain; it was invented. Every other quote in this table was then checked against the pages' raw text.*

## Appendix B. Coverage of the declaration model

The declaration model is defined in `declaration-syntax.md`; this table says, for each of its parts, whether this format can write it yet (ADR-0116). No construct is marked *not yet* since 2026-09-26; were one added to the model, a flow that needs it could not be written until the construct had a form, each added with its section here, its schema, its conversion, an example and planted mistakes that prove its checks.

| Part of the model | `declaration-syntax.md` | This format |
|---|---|---|
| module, imports, categories, enumerations | §1 | written (§4.1) |
| sequences and identifiers minted from them | §1, §3.1 | written (§4.12) |
| external evaluators | §1, §6.2 | written (§4.18) |
| a type, its tracking mode and description | §2 | written (§4.2) |
| a type's summary, the members a listing returns | §2 | written (§4.2) |
| abstract types, families and `extends` | §2 | written (§4.14) |
| named machines shared by several types, binders | §2, §4 | written (§4.10) |
| mirror types and external identifiers | §2 | written (§4.15) |
| states, categories and final states | §2.1 | written (§4.5) |
| attributes: type, optional, unique, indexed, personal | §3.1 | written (§4.3) |
| attributes: scoped, compound and partial uniqueness | §3.1 | written (§4.12) |
| attributes: counters and defaults | §3.1, §7 | written (§4.17) |
| references, including an assignee | §3.2, §6.10 | written (§4.3) |
| parts, composition, inverse ends and cascades | §3.2, §3.3 | written (§4.3) |
| invariants over one object | §3.4 | written (§4.6, §4.5) |
| invariants over relationships and type-scans | §3.4 | written, in the expression language (§4.6, §5) |
| transitions: initial, external and internal | §4, §4.2 | written (§4.8) |
| transition markings: `only via` | §4.2 | written (§4.8) |
| transitions of kind `assert` and `erase` | §4.2 | written (§4.13) |
| transition markings: proposable | §4.2 | written (§4.8), with backdating |
| inputs that write an attribute of the same name | §5.1 | written (§4.8) |
| inputs that write nothing, or another name | §5.1 | written (§4.8) |
| guards, and their enforcement | §5.1 | written (§4.8) |
| eager and deferred evaluation of guards | §5.1 | withdrawn (ADR-0133): a guard that asks an evaluator is consulted when its transition is requested |
| effect steps: assign, clear | §5.2 | written (§4.8) |
| effect steps: create, call, for, add, remove | §5.2 | written (§4.8) |
| effect step: supersede | §5.2, §6.1 | written (§4.19) |
| derivations | §2 | written (§4.11) |
| assertions and admissions | §6.3 | written (§4.13) |
| erasure and correction | §6.4 | written (§4.13) |
| deletion guards | §6.5 | written (§4.13) |
| migrations: removed and renamed members, backfill | §6.6 | written (§4.16) |
| `this_event` and extension | §6.7 | written (§5; extension is `extends`, §4.14) |
| observations | §6.8 | written (§4.4) |
| labels | §6.8 | written (§4.9): applied by request, read as a metric's source |
| metrics: median time in a state, transition counts | §6.9 | written (§4.9) |
| metrics: the metric language, sources, dimensions, filters and flags | §6.9 | written (§4.9) |
| metrics: combined metrics, `combine` | §6.9 | written (§4.9) |
| standard metrics | §6.11 | provided for every type, with nothing to write |
| quantity tracking and its counters | §7 | written (§4.17) |
| expressions | §8 | written: the same language (§5) |
