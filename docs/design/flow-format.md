# The flow description format

Status: **draft 2, 2026-09-26, adopted.** This is the one written form of an ObjectFlow flow (ADR-0116), with its terms taken from UML, SCXML and Kubernetes (ADR-0115, Appendix A). It is the normative definition of how a flow is written. Its machine-readable half is the schema [`flow-format/flow.schema.json`](flow-format/flow.schema.json) (§2), and [`scripts/check-flows.py`](../../scripts/check-flows.py) implements every rule stated here; each rule names the step and the finding code that enforce it. Where this document and the implementation disagree, both are wrong until one is corrected, and `scripts/check-flow-format-doc.py` holds them to each other. The meaning of every form is the meaning of the declaration it converts to, defined in [`declaration-syntax.md`](declaration-syntax.md), which remains the definition of the declaration model; this document does not restate the engine's semantics. The format does not yet express every construct of the model; Appendix B says which it does.

## 1. Scope and conformance

A **flow description** is one YAML file declaring one **module**: its state categories, its enumerations and its types, and for each type its attributes, states, conditions, transitions and metrics. A description is **valid** when all four steps of §7 accept it with no fatal finding. Only a valid description can be proposed for publication.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119 and RFC 8174, when written in capitals.

Every word in a description is one of two kinds, and the examples in this document and in the walkthrough colour them apart:
- **Reserved**: a key or value this format defines, such as `transitions`, `kind`, `external`, `guards` or `deny`, and the words of the expression language. The full list is §10. A reserved word means the same thing in every description.
- **Named by the author**: a name the description itself declares, such as a module, a type, a state, an attribute, a condition, a transition, a metric, an enumeration or one of its values, or a category, and every later use of that name. Descriptions and expressions are the author's text.

## 2. The file

1. A file MUST be UTF-8 YAML holding exactly one document, which is one module. (step 1, `yaml`)
2. Only `true` and `false` are booleans. `yes`, `no`, `on` and `off` are ordinary words, so a value named `NO` stays a name. (step 1)
3. A mapping MUST NOT contain the same key twice. A YAML loader would otherwise keep the last one silently. (step 1, `yaml`)
4. Inside an inline list `[…]` or an inline mapping `{…}`, a value containing a comma, a question mark, a colon followed by a space, a bracket, a brace or `#` MUST be quoted, as YAML requires; for example `{ type: "decimal(10,3)" }`. Unquoted, such a value either fails to parse (step 1, `yaml`) or parses into the wrong structure (step 2, `schema`).
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

1. A name MUST be unique among the names of its section. A repeated key is refused by step 1.
2. **Every name MUST be defined before it is used.** A module declares, in this order, `module`, `imports`, `categories`, `enumerations`, `sequences`, `machines` and `types`; a machine declares, in this order, `description`, `requires`, `states`, `conditions` and `transitions`; a metric declares, in this order, `description`, `measure`, `state`, `transition`, `source`, `item`, `filter`, `dimensions`, `group_by`, `time_dimension`, `expression` and `flag_when`; a type declares, in this order, `description`, `tracking`, `state_machine`, `attributes`, `observations`, `states`, `derived_attributes`, `invariants`, `conditions`, `transitions` and `metrics`. The machines and types of a module form one group and MAY reference each other in any order, as related types must (ADR-0117); a reference MUST name a type the module declares or imports (step 3, `names`). Every other name is defined before it is used. (step 3, `order`)
3. Reserved words of the text language MAY be used as names, as `declaration-syntax.md` §9.2 allows, with these exceptions (step 3, `names`):
   - a category MUST NOT be named `any`, `terminal` or `superseding`, and a transition MUST NOT be named `any` (the text language's check 33);
   - an attribute, including an observation's, MUST NOT be named `state`, `inputs`, `actor`, `this`, `now`, `referrers` or `this_event`, since an expression resolves those words before any attribute;
   - a condition MUST NOT be named `<attribute>_provided`, and an invariant MUST NOT be named `<state>_invariant` with the state in lower case, since those are the names of generated guards and invariants (§6).

## 4. Structure

### 4.1 The module

| Key | Required | Value |
|---|---|---|
| `module` | yes | the module's name |
| `imports` | no | a mapping from another module's name to the list of its types this module uses |
| `categories` | yes | the list of state categories the module's states use; `closed` has the meaning `declaration-syntax.md` §1 gives it |
| `enumerations` | no | a mapping from an enumeration's name to its list of values |
| `sequences` | no | a mapping from a sequence's name to its `description` (§4.12) |
| `machines` | no | a mapping from a shared state machine's name to its declaration (§4.10) |
| `types` | yes | a mapping from a type's name to its declaration (§4.2) |

### 4.2 A type

| Key | Required | Value |
|---|---|---|
| `description` | yes | one or more sentences saying what an object of the type is |
| `tracking` | yes | `record`, `serial` or `quantity` (`declaration-syntax.md` §2) |
| `state_machine` | no | the shared state machine the type binds (§4.10), in place of its own `states` |
| `attributes` | no | §4.3 |
| `observations` | no | §4.4 |
| `states` | unless the type binds a `state_machine` | §4.5 |
| `derived_attributes` | no | §4.11 |
| `invariants` | no | §4.6 |
| `conditions` | no | §4.7 |
| `transitions` | unless the type binds a `state_machine` | §4.8 |
| `metrics` | no | §4.9 |

### 4.3 Attributes

Each attribute is a mapping with exactly one of `type` and `reference`. (step 2, `schema`)

| Key | Value |
|---|---|
| `type` | a built-in type (`string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `duration`, `identity`, `file`) or an enumeration's name |
| `reference` | the name of a type the module declares or imports; `[]` makes the attribute a set of references |
| `opposite` | the attribute of the referenced type that is the other end of this relationship (UML `Property::opposite`) |
| `stored` | `true` on the one of two single ends that holds the value |
| `aggregation` | `composite` on a whole's end: the referenced objects are its parts (UML `AggregationKind::composite`) |
| `cascade`, `survives` | on a composite end: what its parts do when the whole takes a transition (below) |
| `optional` | `true` if the attribute may be without a value; otherwise it always has one |
| `identifier` | the value is minted from a sequence when the object is created (§4.12) |
| `unique` | `true` if no two objects of the type may hold the same value; `in_scope`, `with` and `where` restrict it (§4.12) |
| `indexed`, `personal` | as `declaration-syntax.md` §3.1 defines them |
| `actor_kind` | `human`, `agent` or `service`, on the `identity` attribute of a type whose objects make requests |
| `assignee` | `true` on a reference that names who is responsible for the object |
| `unit` | the unit of a measured value |
| `description` | one sentence, where the name alone does not say what the attribute holds |

`unit` applies only to an observation's attribute, and `unique`, `indexed`, `identifier`, `actor_kind`, `assignee`, `opposite`, `stored`, `aggregation`, `cascade` and `survives` only to a type's attribute, since the model has no form for them elsewhere. (step 3, `names`)

**Relationships.** Both ends of a relationship are declared, each on its own type, so that a type reads completely on its own, and each names the other with `opposite`; the two ends MUST name each other and be in one module (step 3, `names`). Exactly one end stores the value, as `declaration-syntax.md` §3.3 fixes: of a single end and a set end, the single one; of two single ends, the one marked `stored`; two set ends cannot store a pair, which is then a type of its own with a reference to each side (step 4, `check 41`). An end with no `opposite` is a reference with no named way back, and MUST be single.

**Composition.** An end marked `aggregation: composite` makes the referenced objects **parts** of this one, which is their **whole**: a part belongs to exactly one whole, so its end back MUST be single and required (step 3, `names`), and a part lives no longer than its whole. Creating a part MUST be `only_via` a transition of the whole (§4.8; step 4, `check 11`). On the composite end:
- `cascade` is a list of clauses `{ on: [<transition of the whole>, …], transition: <transition of the part>, inputs: { … }, limit: <n> }`: when the whole takes one of the transitions in `on`, each part takes `transition` with the inputs given, which are expressions over the whole and the triggering transition's inputs; `limit` bounds the parts reached in one request. The part transition MUST exist, MUST NOT be initial, and MUST receive the inputs it requires (step 3, `names`). A part already in a final state is passed over, and a part whose guards refuse aborts the whole request.
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
| `remedy` | yes | what a caller can do when it fails: `self_serviceable` (supply an input), `delegable` (another actor must act), `temporal` (only time will satisfy it), `dependent` (another object must change first) or `unreachable_from_here` (a different transition must come first); `DESIGN.md` §5.5 |

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

`from: any` is every state that is not final, the target state included, so a transition that any state may take, such as Jira's "all statuses can transition to this one", is written once (`declaration-syntax.md` §4.2). It stands alone, never in a list (step 2, `schema`), and the checks read it as the states it stands for, so a state added later is covered without an edit.

| Key | Required | Value |
|---|---|---|
| `kind`, `from`, `to` | as above | |
| `description` | no | what the transition does, in one sentence |
| `only_via` | no | the transitions, written `Type.transition`, that alone may take this one, by a `call`, a `create` or a cascade; no caller may request it (step 3, `names`) |
| `corrects` | no | on an internal or external transition: the attributes it corrects, which it writes exactly (§4.13) |
| `may_admit` | no | on an assertion: the invariants a request may admit breaking (§4.13) |
| `required_inputs` | no | attributes the caller MUST supply; each is written to the attribute of the same name |
| `optional_inputs` | no | attributes the caller MAY supply; each is written if supplied. An attribute that is not optional is always required |
| `inputs` | no | declared inputs, which write no attribute of their own name (below) |
| `guards` | no | a mapping from a condition's name to its enforcement, in the order the guards are evaluated |
| `effect` | no | the list of writes the transition makes, in order (UML `Transition::effect`) |
| `backdating_limit` | no | how far in the past a caller may say the change happened, a duration such as `2 days` |

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
- `add: { location: <attribute>, expr: <expression> }` and `remove: { location, expr }`, which add a value to or remove one from a set-valued attribute, one whose type ends in `[]` (step 3, `names`);
- `call: { target: <path>, transition: <name>, inputs: { <input>: <expression>, … } }`, which takes a transition of another object: its guards are evaluated and it records its own event, in the same request (UML's `CallOperationAction`, whose object is its `target`). Where the target is an attribute or an input whose type is known, including a type imported from a module checked earlier, the transition MUST be one of that type's, MUST NOT be an initial one, and MUST receive every input it requires and none it does not take (step 3, `names`); a target reached any other way is checked by the publish checks;
- `create: { type: <Type>, transition: <name>, inputs: { … }, result: <name> }`, which creates an object by an initial transition of its type, declared in the module or imported, and MUST pass the inputs that transition requires (step 3, `names`); `result` names the new object for the steps after it (UML's `CreateObjectAction`);
- `foreach: { item: <name>, array: <expression>, where: <expression>, limit: <n>, steps: [ … ] }`, which runs its steps for each element of a collection that satisfies `where`, or, with `range: <expression>` in place of `array`, for each number from 1 to that value (SCXML's `<foreach>`). `limit` is REQUIRED and bounds that loop; a request that exceeds it is refused (step 2, `schema`; `declaration-syntax.md` §5.2).

Every write stays on the object the transition acts on; another object is changed only by calling one of its transitions or creating it. An effect MAY name a type declared later in the module, since a type often creates or calls objects that refer back to it, which no order of declaration could otherwise allow.

A transition MUST NOT both write and clear one attribute. (step 3, `names`)

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

**A formula** aggregates an expression over the rows of a source, grouped by its dimensions:

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the figure is |
| `source` | yes | the rows: `objects`, the type's objects; `intervals`, each span an object spent in one state; `intervals(<member>)`, each span a tracked member held one value; `transitions`, each transition taken; `attempts`, each request that did not apply; `attempt_counts`, their daily counts; or the name of one of the type's observation kinds, each observation of it |
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

The rest of the model's rules for a metric are its publish checks (step 4, `check 56`): a dimension reaches at most two hops from the row, no personal member is read anywhere in a metric, `avg`, `median`, `percentile` and the time buckets `day` to `year` are used in a metric only, and a guard's `metric(…)` binds only the metric's dimensions and windows only a metric with a `time_dimension`.

The keys are the ones the semantic layers use for the same things (ADR-0118, Appendix A.4): dbt, Cube and Looker call the groups dimensions and narrow the rows with a filter, and dbt aggregates a metric against its time dimension. A metric over other metrics, the model's `combine`, is not written yet.

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

A derived attribute is read by its name wherever an attribute is: in a condition, an invariant, an effect's expression and another derived attribute. It reads only the derived attributes declared above it, so derived attributes never form a cycle (step 3, `order`; the model's check 3), and it reads neither `inputs` nor `this_event`, since it is evaluated outside any request (step 3, `names`). Its name MUST differ from every attribute and observation kind of the type, which share one scope of names (step 3, `names`; the model's check 33). Nothing writes it: a transition that takes it as an input, or assigns, adds to, removes from or clears it, is refused, and so is a state that requires it, since `required_attributes` names stored attributes; an invariant may read it instead (step 3, `names`). `if … then … else` is allowed in its expression (`declaration-syntax.md` §8.3), and like every expression it MUST NOT read `actor` (step 4, `check 64`) or call `metric(…)` (step 4, `check 56`).

An **indexed** derived attribute reads only what the store holds on the object itself: attributes marked `indexed`, single references that store their value (§4.3, Relationships), and `state`. It MUST NOT read `now`, another derived attribute, a set, a path through a reference into another object, or the object's flow data, `entered_at`, `time_in`, `this.intervals` or `this.transitions` (step 3, `names`; the model's check 46).

A derived attribute is declared in a section of its own rather than among the attributes, where UML lists it, because its expression reads the type's states, which are declared after the attributes; a `derive:` key on an attribute would read a name defined further down (§3).

### 4.12 Sequences and identifiers

A **sequence** is a named counter that mints identifiers, SQL's `CREATE SEQUENCE` and the model's `sequence` (`declaration-syntax.md` §1). A module declares its sequences under `sequences`, each with a `description`, and a type's attribute takes its value from one with `identifier`, when the object is created (`declaration-syntax.md` §3.1):

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

**Uniqueness.** `unique` takes one of four forms, each the model's sugar for an invariant (`declaration-syntax.md` §3.1):

| Form | No two objects hold the same value |
|---|---|
| `unique: true` | across the type |
| `unique: in_scope` | within one value of the identifier's `scope`, which the attribute MUST have (step 3, `names`) |
| `unique: { with: [a, …] }` | together with the attributes listed, which the type MUST declare (step 3, `names`) |
| `unique: { where: <expression> }` | among the objects for which the expression holds; it reads what an invariant may, and neither `inputs` nor `this_event` (step 3, `names`) |

An identifier implies none of them, since a scoped sequence repeats its numbers across scopes.

### 4.13 Assertions, corrections, erasure and deletion guards

These are the ways the model changes a record other than by the flow's own steps (`declaration-syntax.md` §4.2, §6.3 to §6.5). UML has no term for an assertion or an erasure, so both keep the model's words (ADR-0119).

**An assertion** (`kind: assertion`) puts an object into a state the request names, from any state, when the record is wrong: an override. Its `to` lists the states it may put the object in, and the request names one of them as its input `to`. It MUST declare an input `reason` whose type is an enumeration, so that overrides are counted by reason; an explanation in words is a further input, optional and `personal`, since free text may name someone (step 3, `names`; the model's check 29). It MUST NOT declare an input named `to` or `admits`, which it has already (step 3, `names`).

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

`may_admit` lists the invariants a request of the assertion is permitted to break; the request names the ones it does break as its input `admits`, and each admission is recorded on the event. An admitted invariant is the type's own, one generated for a state's `required_attributes` included, or `<Type>.<invariant>`, an invariant of a type reachable from it by relationships whose ends name each other (step 3, `names`; the model's check 27). Only an assertion may admit (step 3, `names`). Step 3 does not require an assertion to set what its target state requires: the invariant is checked when the assertion applies, and the assertion may admit it.

**A correction** is an internal or external transition marked `corrects: [<attribute>, …]`, whose events carry the `corrected` provenance. It MUST write exactly the attributes it lists, by its attribute inputs and its effect, and MUST declare an input `reason` (step 3, `names`; the model's check 28).

**An erasure** (`kind: erasure`) erases every `personal` attribute of its object, at any state, a final one included, since requests to erase arrive for closed records (`declaration-syntax.md` §6.4). It declares neither `from` nor `to` (step 2, `schema`) and MUST declare an input `reason` (step 3, `names`; the model's check 29). It admits every invariant that reads what it erases, and once an object's own erasure is recorded, a request that writes one of its personal attributes is refused by the generated guard `not_erased`. A type need not declare an erasure; one that does MUST reach every part type holding a personal attribute, by calling that type's erasure in its effect, on the part's attribute or on a `foreach` item over it, or the person's data would stay in the parts (step 3, `names`; the model's check 39). An observation kind's fields are erased with their subject and need no call. Because an erasure writes absence, a `personal` attribute or field MUST be optional (step 3, `names`; the model's check 9).

An assertion's and an erasure's guards MUST be `deny`, and neither has a `backdating_limit`: an assertion's time is when the store was told, and an erasure is not a step of the flow (step 4, `check 57`, `check 58`).

**A deletion guard** is an ordinary condition over `referrers`, every object holding a live reference to this one, parts excepted, since they cascade (`declaration-syntax.md` §6.5). A type's `close` guarded by `none(r in referrers where r.state.category != closed)` is refused while an order of the customer's is open.

## 5. Expressions

An `expression` is written in the expression language of `declaration-syntax.md` §8, which this format does not change. Within it:
- an attribute of the object is read by its name, and a value the caller supplies is read as `inputs.<attribute>`;
- every name an expression reads at the start of a path MUST be declared where the expression is evaluated: for a type, one of its attributes, observation kinds or derived attributes (§4.11); for a machine, an attribute it requires (§4.10); for an observation kind's invariant, one of that kind's fields; and in an effect, also the name a `foreach` or `create` step binds. A derived attribute and a uniqueness condition are evaluated outside any request, and read neither `inputs` nor `this_event`. The names an aggregate binds, `count(l in lines where …)`, are declared by it (step 3, `names`);
- `state` is the object's current state, and `now` is the time of the request;
- `referrers` is every object holding a live reference to this one, parts excepted, and a category is read by its name, as in `r.state.category != closed` (§4.13);
- an enumeration value and a state of another type are written qualified, `<Enumeration>.<VALUE>` and `<Type>.<STATE>`; a value of an enumeration or of a type the module declares MUST exist, and a bare upper-case name MUST be a state of the type (step 3, `names`);
- absence is tested with `is null` and `is not null`; a comparison with an absent value is unknown, and a guard that is unknown refuses (`declaration-syntax.md` §8.2);
- an expression MUST NOT read `actor`: who may act is decided outside the flow (ADR-0114; step 4, `check 64`).

## 6. What a description declares

Each form converts to the text language as follows, and means what that declaration means.

| Description | Text language |
|---|---|
| `kind: initial`, `to: S` | `create <name> -> S` |
| `kind: external`, `from: [A, B]`, `to: C` | `do <name> { A, B } -> C` |
| `kind: internal`, `from: [A, B]` | `act <name> at { A, B }` |
| `required_inputs: [a]`, `optional_inputs: [b]` | `accepts a, b` |
| `inputs: { r: { type: T, optional: true, personal: true } }` | `input r : T? personal` |
| `required_inputs: [a]` where `a` is optional | also the guard `require a_provided: inputs.a is not null because self_serviceable`, evaluated before the transition's other guards |
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
| `derived_attributes: { d: { expression: e, indexed: true } }` | `derive d = e indexed` |
| `sequences: { s: … }` | `sequence s version 1` |
| `identifier: { sequence: s, scope: r, format: f }` | `identifier from s scoped by r format "f"` |
| `unique: in_scope`, `unique: { with: [a, b] }`, `unique: { where: e }` | `unique in scope`, `unique with a, b`, `unique where e` |
| `only_via: [W.t]` | `only via W.t` in the transition's head |
| `reference: T, opposite: e` (and `stored: true`) | `ref <name> : T inverse e` (`stored`) |
| `reference: "T[]", aggregation: composite, opposite: e` | `part <name> : T[] inverse e` |
| the part's end back, `reference: W, opposite: <composite end>` | `owner <name> : W inverse <composite end>` |
| `cascade: [{ on: [a], transition: t, limit: n }]`, `survives: [b]` | `cascade on a to T.t limit n`, `survives on { b }` |
| `measure: median_time_in_state` | a metric over the type's intervals in the state, valued `median(i.duration)` |
| `measure: transition_count` | a metric over the type's transitions along the transition's states, valued `count()` |
| `kind: assertion`, `to: [A, B]`, `may_admit: [i]` | `assert x -> { A, B } { input to : state; input admits : invariant[]?; may admit i }` |
| `from: any` | `do x any -> S`; `act x at any` |
| `kind: erasure` | `erase x { … }` |
| `corrects: [a]` | `corrects a` |
| `source: s`, `item: i`, `filter: f` | `from i in <T>.s where f`; `from i in <T>` for `objects`, and `from i in <Kind>` for an observation kind |
| `dimensions: { d: e }`, `time_dimension: t` | `by d = e`, `window on t` |
| `expression: v`, `flag_when: { n: c }` | `value v`, `flag n when c` |

The generated guards and invariants take the names shown, which §3 reserves. The text language keeps its own keywords for these forms (`create`, `do`, `act`, `terminal`, `observe`, `flag`) until the author decides whether this format replaces it (ADR-0115).

## 7. Validity

A description is checked in four steps, and each stops the check if it finds anything fatal. Every finding names the file, the line and the construct.

| Step | Code | Refuses |
|---|---|---|
| 1. strict loading | `yaml` | a file YAML cannot parse, including a `?` unquoted inside an inline collection, or a key repeated in a mapping |
| 2. structure | `schema` | a missing or unknown key, a value of the wrong form, a name in the wrong case, a transition kind without the `from` and `to` it requires, a metric without the `state` or `transition` its measure requires |
| 3. names and order | `order` | sections out of order, a derived attribute that reads itself or one declared after it |
| | `names` | a name that is not declared (a state, condition, attribute, category, transition or value), a reserved name (§3), an unused condition, an input a transition reads and does not take, an optional set input, a `create` or `call` that names no such transition or passes the wrong inputs, an `add` or `remove` on an attribute that is not a set, a relationship whose ends do not name each other, a part whose end back is optional or a set, a cascade to a transition the part does not have, a final transition of a whole its parts neither cascade on nor survive, an `only_via` naming no transition, a reference to a type neither declared nor imported, a binder that lacks what its machine requires, declares it with another type or optionality, or redeclares one of the machine's transitions or conditions, a machine whose condition or effect names an attribute it does not require, a derived attribute that is written, required by a state, reads what only a transition has, or shares a name with another member, an indexed derived attribute that reads what the store does not hold indexed on the object, an identifier from no declared sequence or on an attribute that is not a string, a scope that is not a single reference or indexed attribute every creation writes, a format without a number or with a placeholder §4.12 does not allow, a uniqueness in scope without a scope or with an attribute the type does not declare, a metric over a source its type does not have or reading anything but its item and the members its rows have, a flag reading neither the value nor a dimension, an assertion, erasure or correction without a `reason` input, an assertion's reason that is not an enumeration, an admission of an invariant neither the type's nor a related type's, a correction that does not write exactly what it lists, an erasure that does not reach a part type holding personal data, a personal attribute that is required, a transition that both writes and clears an attribute, an optional input on an attribute that is not optional, an attribute key the attribute's kind has no form for (§4.3), a `transition_count` that cannot be told apart |
| | `required` | a transition into a state that does not set, or that clears, an attribute the state requires |
| 4. publish checks | `check N` | anything the text language's implemented publish checks refuse, reported at the line of the description it came from |

Step 3 also reports two notices, which are not fatal: `audit`, for each guard that audits, and `warn`, for each guard that warns.

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
1. Whether version numbers are computed at publication, which this format assumes by having none.
2. Whether the generated guards and invariants should carry written descriptions.
3. Whether a transition whose source state does not require an attribute its target requires should be refused, as now, even when the attribute happens to be kept.
4. ~~How two types that reference each other are ordered~~: *decided 2026-09-26*, the types of a module may reference each other in any order (ADR-0117).
5. ~~The order in which the constructs of Appendix B gain a form~~: *decided 2026-09-26*, the author's order and the placement of the parts outside it, which `TODO.md` records.

## 10. Reserved vocabulary

These words are reserved by the format. `scripts/check-flow-format-doc.py` holds this list equal to the keys and values `flow.schema.json` and the checker define.

**Keys:** `module`, `imports`, `categories`, `enumerations`, `sequences`, `machines`, `requires`, `state_machine`, `types`, `description`, `tracking`, `attributes`, `observations`, `states`, `derived_attributes`, `invariants`, `conditions`, `transitions`, `metrics`, `category`, `final`, `required_attributes`, `type`, `reference`, `opposite`, `stored`, `aggregation`, `cascade`, `on`, `survives`, `only_via`, `corrects`, `may_admit`, `optional`, `identifier`, `sequence`, `scope`, `format`, `unique`, `with`, `indexed`, `personal`, `actor_kind`, `assignee`, `unit`, `kind`, `max_recording_delay`, `expression`, `remedy`, `from`, `to`, `required_inputs`, `optional_inputs`, `inputs`, `default`, `guards`, `effect`, `assign`, `location`, `expr`, `clear`, `add`, `remove`, `call`, `target`, `create`, `result`, `foreach`, `item`, `array`, `range`, `where`, `limit`, `steps`, `backdating_limit`, `measure`, `source`, `filter`, `dimensions`, `time_dimension`, `state`, `transition`, `group_by`, `flag_when`.

**Values:** `true`, `false`, `record`, `serial`, `quantity`, `human`, `agent`, `service`, `initial`, `external`, `internal`, `assertion`, `erasure`, `any`, `deny`, `audit`, `warn`, `self_serviceable`, `delegable`, `temporal`, `dependent`, `unreachable_from_here`, `median_time_in_state`, `transition_count`, `objects`, `intervals`, `transitions`, `attempts`, `attempt_counts`, `composite`, `in_scope`, `month`, `week`, `actor`.

**In expressions:** the reserved words of the text language, `declaration-syntax.md` §9.5.

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
| not yet | `combine` | a derived metric: `expr` over `input_metrics` | none | none |

Sources: dbt's [latest spec](https://docs.getdbt.com/docs/build/latest-metrics-spec), [metrics overview](https://docs.getdbt.com/docs/build/metrics-overview), [dimensions](https://docs.getdbt.com/docs/build/dimensions) and [simple metrics](https://docs.getdbt.com/docs/build/simple); Cube's [dimensions](https://docs.cube.dev/reference/data-modeling/dimensions), [measures](https://docs.cube.dev/reference/data-modeling/measures) and [query format](https://docs.cube.dev/reference/core-data-apis/rest-api/query-format); Looker's [measure types](https://docs.cloud.google.com/looker/docs/reference/param-measure-types) and [dimension groups](https://docs.cloud.google.com/looker/docs/reference/param-field-dimension-group). dbt's week start day was not checked. *Retracted 2026-09-26: the Cube cell first quoted a query's "dimensions to group by", wording a summarising fetch produced and the page does not contain; it was invented. Every other quote in this table was then checked against the pages' raw text.*

## Appendix B. Coverage of the declaration model

The declaration model is defined in `declaration-syntax.md`; this table says, for each of its parts, whether this format can write it yet (ADR-0116). A flow that needs a construct marked *not yet* cannot be written until the construct has a form, each added with its section here, its schema, its conversion, an example and planted mistakes that prove its checks.

| Part of the model | `declaration-syntax.md` | This format |
|---|---|---|
| module, imports, categories, enumerations | §1 | written (§4.1) |
| sequences and identifiers minted from them | §1, §3.1 | written (§4.12) |
| external evaluators | §1, §6.2 | not yet |
| a type, its tracking mode and description | §2 | written (§4.2) |
| abstract types, families and `extends` | §2 | not yet |
| named machines shared by several types, binders | §2, §4 | written (§4.10) |
| mirror types and external identifiers | §2 | not yet |
| states, categories and final states | §2.1 | written (§4.5) |
| attributes: type, optional, unique, indexed, personal | §3.1 | written (§4.3) |
| attributes: scoped, compound and partial uniqueness | §3.1 | written (§4.12) |
| attributes: counters and defaults | §3.1, §7 | not yet |
| references, including an assignee | §3.2, §6.10 | written (§4.3) |
| parts, composition, inverse ends and cascades | §3.2, §3.3 | written (§4.3) |
| invariants over one object | §3.4 | written (§4.6, §4.5) |
| invariants over relationships and type-scans | §3.4 | written, in the expression language (§4.6, §5) |
| transitions: initial, external and internal | §4, §4.2 | written (§4.8) |
| transition markings: `only via` | §4.2 | written (§4.8) |
| transitions of kind `assert` and `erase` | §4.2 | written (§4.13) |
| transition markings: proposable | §4.2 | not yet; backdating is written |
| inputs that write an attribute of the same name | §5.1 | written (§4.8) |
| inputs that write nothing, or another name | §5.1 | written (§4.8) |
| guards, and their enforcement | §5.1 | written (§4.8) |
| eager and deferred evaluation of guards | §5.1 | not yet |
| effect steps: assign, clear | §5.2 | written (§4.8) |
| effect steps: create, call, for, add, remove | §5.2 | written (§4.8) |
| effect step: supersede | §5.2, §6.1 | not yet |
| derivations | §2 | written (§4.11) |
| assertions and admissions | §6.3 | written (§4.13) |
| erasure and correction | §6.4 | written (§4.13) |
| deletion guards | §6.5 | written (§4.13) |
| migrations: removed and renamed members, backfill | §6.6 | not yet |
| `this_event` and extension | §6.7 | not yet |
| observations | §6.8 | written (§4.4) |
| labels | §6.8 | not yet |
| metrics: median time in a state, transition counts | §6.9 | written (§4.9) |
| metrics: the metric language, sources, dimensions, filters and flags | §6.9 | written (§4.9) |
| metrics: combined metrics, `combine` | §6.9 | not yet (ADR-0118) |
| standard metrics | §6.11 | provided for every type, with nothing to write |
| quantity tracking and its counters | §7 | not yet |
| expressions | §8 | written: the same language (§5) |
