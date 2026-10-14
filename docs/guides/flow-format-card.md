# The flow format on one page

Every key of a flow description and of its examples file, what it takes, and where [`flow-format.md`](../design/flow-format.md) defines it; that document is normative, and this card is checked against its schema and its checker (`scripts/check-flow-format-doc.py` rule 8): it names every key either schema defines, every finding code the checker reports, and no key they do not, and its module is checked and its examples run. [`write-a-flow.md`](write-a-flow.md) walks a first flow through; the [samples](../design/flow-format/examples/) show each construct in use.

## A module and its examples

```yaml
module: loans
categories: [live, closed]

enumerations:
  Condition: [GOOD, WORN, DAMAGED]

sequences:
  loan_numbers:
    description: Numbers each loan.

types:
  Member:
    description: A person who borrows equipment, and who acts.
    tracking: record
    attributes:
      user_id: { type: identity, actor_kind: human, unique: true, indexed: true }
      name: { type: string, personal: true, optional: true }
    states:
      ACTIVE: { category: live }
      LEFT: { category: closed, final: true }
    transitions:
      join:
        kind: initial
        to: ACTIVE
        required_inputs: [user_id, name]
      leave: { kind: external, from: ACTIVE, to: LEFT }
      forget: { kind: erasure, inputs: { reason: { type: string } } }

  Item:
    description: A piece of equipment that is lent out.
    tracking: record
    attributes:
      tag: { type: string, unique: true, indexed: true }
      condition: { type: Condition, default: Condition.GOOD }
      loans: { reference: "Loan[]", opposite: item }
    states:
      AVAILABLE: { category: live }
      RETIRED: { category: closed, final: true }
    derived_attributes:
      on_loan:
        description: Whether a loan of the item is open.
        expression: any(l in loans where l.open)
    conditions:
      not_on_loan:
        description: No loan of the item is open.
        expression: not on_loan
        remedy: dependent
    transitions:
      register:
        kind: initial
        to: AVAILABLE
        required_inputs: [tag]
        optional_inputs: [condition]
      retire:
        kind: external
        from: AVAILABLE
        to: RETIRED
        taken_by: [human]
        guards: { not_on_loan: deny }

  Loan:
    description: One member's loan of one item, from lending to return.
    tracking: record
    attributes:
      number: { type: string, identifier: { sequence: loan_numbers, format: "L-{n:5}" }, unique: true }
      item: { reference: Item, opposite: loans }
      borrower: { reference: Member, assignee: true }
      due: { type: date }
      returned_condition: { type: Condition, optional: true }
      lent_by: { type: identity }
    states:
      OUT: { category: live }
      RETURNED: { category: closed, final: true, required_attributes: [returned_condition] }
    invariants:
      one_open_loan_an_item:
        description: An item is on at most one open loan.
        expression: not open or count(l in item.loans where l.open) <= 1
    conditions:
      item_available:
        description: The item is in service and on no other loan.
        expression: inputs.item.state == Item.AVAILABLE and not inputs.item.on_loan
        remedy: dependent
      due_within_a_month:
        description: The loan is due within 30 days of today.
        expression: inputs.due <= date(now) + 30 days
        remedy: self_serviceable
    transitions:
      lend:
        kind: initial
        to: OUT
        required_inputs: [item, borrower, due]
        guards: { item_available: deny, due_within_a_month: warn }
        effect:
          - assign: { location: lent_by, expr: actor.id }
      give_back:
        kind: external
        from: OUT
        to: RETURNED
        required_inputs: [returned_condition]
    metrics:
      loans_by_month:
        description: Loans made, by month.
        measure: transition_count
        transition: lend
        group_by: [month]
```

```yaml
examples_for: loans

setups:
  an_item_on_loan:
    description: A member has borrowed a drill.
    steps:
      - request: { type: Member, transition: join, as: ana, inputs: { user_id: '"ana@lab"', name: '"Ana"' } }
      - request: { type: Item, transition: register, as: drill, inputs: { tag: '"D-1"' } }
      - request: { type: Loan, transition: lend, as: loan, actor: ana, inputs: { item: drill, borrower: ana, due: 2026-01-08 } }

examples:
  an_item_on_loan_cannot_be_lent_again:
    description: A second loan of a drill already out is refused until it comes back.
    given: an_item_on_loan
    request: { type: Loan, transition: lend, actor: ana, inputs: { item: drill, borrower: ana, due: 2026-01-08 } }
    expect: { verdict: unsatisfied, clause: item_available, remedy: dependent }
  a_returned_item_is_lent_again:
    description: Once returned, the drill can be lent again.
    given: an_item_on_loan
    steps:
      - request: { object: loan, transition: give_back, actor: ana, inputs: { returned_condition: Condition.WORN } }
    request: { type: Loan, transition: lend, actor: ana, inputs: { item: drill, borrower: ana, due: 2026-01-08 } }
    expect: { verdict: applied, state: OUT }
  an_item_on_loan_is_not_retired:
    description: Retiring a drill that is out is refused until it is returned.
    given: an_item_on_loan
    request: { object: drill, transition: retire, actor: ana }
    expect: { verdict: unsatisfied, clause: not_on_loan, remedy: dependent }
```

`scripts/check-flows.py loans.yaml loans.examples.yaml` checks it and runs the examples: `0 fatal · 3 notices`, `3 passed`. Three things the checker taught in writing it: a guard on an initial transition reads `inputs.item`, since the attribute is not yet written (notice `before`); a `personal` attribute is optional and its type declares an `erasure` taking a `reason`; and sections come in one order.

## Sections, in their order

| Where | Keys, in the order they are written |
|---|---|
| module (§4.1) | `module`, `imports`, `publishing`, `categories`, `enumerations`, `sequences`, `evaluators`, `machines`, `types`, `metrics`, `migration` |
| type (§4.2) | `description`, `abstract`, `extends`, `mirror`, `tracking`, `state_machine`, `taken_by`, `attributes`, `inherited_parts`, `observations`, `states`, `derived_attributes`, `summary`, `invariants`, `conditions`, `transitions`, `metrics` |
| machine (§4.10) | `description`, `requires`, `states`, `conditions`, `transitions` |
| metric (§4.9) | `description`, `measure`, `state`, `transition`, `source`, `item`, `split_by`, `filter`, `dimensions`, `group_by`, `time_dimension`, `expression`, `flag_when` |

Required: a module's `module`, `categories` and `types`; a type's `description` and `tracking` (`record`, `serial` or `quantity`), and its `states` unless it binds a `state_machine`. Nothing has a default: every required key is written, every condition's `remedy` too.

## The module

| Key | Takes | § |
|---|---|---|
| `module` | the module's name, lower case | 4.1 |
| `imports` | by module, the `types`, `machines`, `enumerations`, `sequences` and `evaluators` used from it | 4.1 |
| `publishing` | `taken_by`: the kinds that approve the store's next version, read from the installed one; the operator always may | 4.1 |
| `categories` | the state categories its states use, such as `[live, closed]` | 4.1 |
| `enumerations` | each a list of upper-case values, read qualified, `Condition.WORN` | 4.1 |
| `sequences` | counters minting identifiers, each with a `description` | 4.12 |
| `evaluators` | external systems a guard asks: a `description` and `functions`, each with a `description`, `arguments` (`type` or `reference`, `description`) and `fresh`, the oldest answer used | 4.18 |
| `machines` | state machines several types share | 4.10 |
| `types` | the types, by upper-camel-case name | 4.2 |
| `metrics` | combined metrics across types: `input_metrics`, `group_by`, `expression`, `flag_when` | 4.9 |
| `migration` | how a publish of this version moves the objects of the one before: `description`, `removed_states`, `removed_members`, `renamed_attributes`, `backfill`, and `admit`, each an `invariant` with a `reason` | 4.16 |

## A type

| Key | Takes | § |
|---|---|---|
| `abstract`, `extends` | `abstract: true` on a base with no objects; `extends: <Base>` inherits its attributes, derived attributes, invariants and tracking, never its transitions | 4.14 |
| `mirror` | `true` for a type another system owns: only an import writes it | 4.15 |
| `state_machine` | the shared machine the type binds, declaring no states of its own; the machine's `requires` lists what a binder declares | 4.10 |
| `taken_by` | the kinds that may take every transition of the type declaring none, `[human]`; a subtype does not inherit it | 4.2 |
| `inherited_parts` | on a subtype, how the parts of a composite end it inherits go with its final transitions | 4.14 |
| `observations` | datapoints recorded about an object, never changed: each a `kind`, `description`, `attributes`, `max_recording_delay` and `invariants` | 4.4 |
| `states` | each a `category`, and `description`, `final`, `required_attributes`, `superseding` | 4.5 |
| `derived_attributes` | each a `description` and an `expression`, never written; `indexed` makes it a query filter | 4.11 |
| `summary` | the members a listing returns by default | 4.2 |
| `invariants` | each a `description` and an `expression` that holds of every object at all times | 4.6 |
| `conditions` | each a `description`, an `expression` and a `remedy`, used as guards | 4.7 |
| `transitions` | by name, the name being its trigger | 4.8 |
| `metrics` | figures over many objects: a measure or a formula | 4.9 |

## An attribute

Exactly one of `type` and `reference`; `[]` after either makes a set, never optional and never absent.

| Key | Takes | § |
|---|---|---|
| `type` | `string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `date`, `duration`, `identity`, `actor_kind`, `file`, `event`, an enumeration, or `counter` on a type tracked by `quantity` | 4.3 |
| `reference` | the type it names | 4.3 |
| `opposite` | the other end of the relationship, on the referenced type; both ends declare it | 4.3 |
| `stored` | on one of two single ends: the end that holds the value | 4.3 |
| `aggregation` | `composite`: the referenced objects are parts, created `only_via` the whole | 4.3 |
| `cascade` | on a composite end, clauses `{ on: [<transition>], transition: <part's>, inputs: { … }, limit: <n> }` | 4.3 |
| `survives` | on a composite end, `true` or the final transitions the parts outlive; each final transition of the whole is covered by a cascade or this | 4.3 |
| `optional` | `true` if it may be without a value | 4.3 |
| `default` | a literal, a qualified value or `now`, given at creation when not an input | 4.17 |
| `identifier` | minted at creation: `sequence`, `format` (`{n}`, `{n:5}`, `{<attribute>}`, `[ … ]` around an optional segment) and `scope` | 4.12 |
| `external` | the system owning the value, such as `xero`; unique per source, may be absent | 4.15 |
| `unique` | one of four forms: `true`, `in_scope`, `{ with: [<attribute>] }` or `{ where: <expression> }`; `with` and `where` do not combine, and "one live per" is an invariant | 4.12 |
| `indexed` | queries may filter and order on it | 4.3 |
| `personal` | erased by the type's `erasure`; so `optional` | 4.13 |
| `actor_kind` | `human`, `agent` or `service`, on the `identity` attribute of a type whose objects act | 4.3 |
| `assignee` | on a reference: who is responsible for the object | 4.3 |
| `unit` | on an observation's attribute: the unit measured | 4.4 |
| `description` | what it holds, where the name does not say | 4.3 |

## A transition

| `kind` | `from` | `to` |
|---|---|---|
| `initial` | absent | the state it creates the object in |
| `external` | a state, a list, or `any` | the state it moves to |
| `internal` | the states it may be taken in, or `any` | absent |
| `assertion` | absent | the states a request may put the object in, as an override |
| `erasure` | absent | absent; erases the personal attributes, taking a `reason` input |

| Key | Takes | § |
|---|---|---|
| `only_via` | the `Type.transition`s that alone take it, by `call`, `create` or cascade; no caller requests it | 4.8 |
| `proposable` | `true`: a request may be filed as a proposal, executed as its approver | 4.8 |
| `taken_by` | the kinds that may take it, `[human]`, on any transition, an erasure and an assertion included; records `<transition>_by_kind` and holds `<transition>_taken_by`, which no `may_admit` admits | 4.8 |
| `corrects` | the attributes a correction writes, exactly | 4.13 |
| `may_admit` | on an assertion, the invariants a request may admit breaking | 4.13 |
| `required_inputs`, `optional_inputs` | attributes the caller supplies, each written to its name | 4.8 |
| `inputs` | declared inputs, read as `inputs.<name>`: `type` or `reference`, `optional`, `personal`, `default`, `description` | 4.8 |
| `guards` | condition names, in order, each `deny`, `audit` or `warn` | 4.8 |
| `effect` | the steps below, in order | 4.8 |
| `backdating_limit` | how far back a caller may say it happened, `2 days` | 4.8 |

| Effect step | Does |
|---|---|
| `assign: { location: <attribute>, expr: <expression> }` | writes a value; `expr: actor.id` records who acted |
| `clear: [<attribute>]` | removes optional values |
| `add`, `remove: { location, expr }` | adds a value to a set, or removes one |
| `call: { target: <path>, transition: <name>, inputs: { … } }` | takes another object's transition, its guards evaluated, its own event recorded |
| `create: { type: <Type>, transition: <initial>, inputs: { … }, result: <name> }` | creates an object, `result` naming it for the steps after |
| `foreach: { item: <name>, array: <expression>, where: <expression>, limit: <n>, steps: [ … ] }` | runs steps per element, or per number with `range` in place of `array`; more than `limit` refuses `over-limit` |
| `supersede: <successor>` | names the object superseding this one, into a `superseding` state (§4.19) |

## A metric

| Form | Keys | § |
|---|---|---|
| measure | `measure`: `median_time_in_state` with `state`, or `transition_count` with `transition`; `group_by`: `month`, `week`, `actor` or an attribute | 4.9 |
| formula | `source` (`objects`, `intervals`, `intervals(<member>)`, `transitions`, `attempts`, `attempt_counts`, `labels`, or an observation kind), `item`, `split_by`, `filter`, `dimensions`, `time_dimension`, `expression` | 4.9 |
| either | `description`, and `flag_when`, named flags over `value` and the dimensions | 4.9 |

## Expressions (§5, `declaration-syntax.md` §8)

- **Names**: an attribute by its name; an input as `inputs.<name>`; `state`, `now`, `id`, `open`, `created_at`, `created_by_kind`, `recorded_from`, `declaration_version`; a mirror's `imported_at`; `this_event`, `referrers`; a value qualified, `Condition.WORN`, a state of another type `Item.AVAILABLE`, a category bare, `closed`.
- **Logic**: `and`, `or`, `not`, `implies`; `is null`, `is not null`. A comparison with an absent value is **unknown**, and an unknown guard fails as a false one does.
- **Aggregates**: `count`, `sum`, `any`, `all`, `none`, `min`, `max`, `avg`, `median`, `percentile`, written `count(l in loans where l.open)`; over nothing, `count` and `sum` are 0, `all` and `none` true, `any` false, the others absent.
- **Values**: money `USD 12.50`, a date `date(now) + 30 days`, a duration `2 h`, text quoted inside the YAML string, `'"HQ"'`; `true`, `false` and numbers bare.
- **Who acted**: only an effect reads `actor`, `actor.id` and `actor.kind`; a rule reads what an effect recorded.

## The examples file (§11)

| Key | Takes |
|---|---|
| `examples_for` | the module |
| `setups` | named starting states: `description`, `given` (a setup it starts from), `evaluators` (each answer, `xero.bill_matches: satisfied`), `steps` |
| `examples` | each a `description`, `given`, `evaluators`, `steps`, a `request` or an `approve`, and its `expect` |
| a step | one of `request`, `propose`, `approve`, `reject`, `withdraw`, `import`, `publish` |
| `request`, `propose` | `type` (to create) or `object` (an alias), `transition`, `as` (the new object's alias), `creates`, `actor`, `after`, `occurred_at`, `inputs`; a proposal's `proposal` alias |
| `approve`, `reject`, `withdraw` | `proposal`, `actor`, `after` |
| `import` | a mirror's object as a legacy system held it: `type`, `as`, `state`, `values`, `after`, and `sequences`, each `{ sequence: <name>, last: <n>, scope: <alias> }`, the last number the legacy system minted |
| `publish` | `source`, a file beside the examples, `actor`, `after` |
| `expect` | `verdict` (`applied` or a refusal), `clause`, `remedy`, `state`, `values`, `cascaded`, `warned`, `occurred_at`, `metrics`, `proposal` |

The clock starts at `2026-01-01T00:00:00Z` and `after` moves it; the actor `operator` acts where none is named.

## Refusals and their remedies

A refusal names its clause and one of five remedies, the one a caller acts on: `self_serviceable`, the caller corrects the request; `delegable`, someone else may act; `temporal`, it may pass later; `dependent`, another object must change first, the refusal naming it; `unreachable_from_here`, nothing the caller can do.

## What the checker reports (§7)

| Step | Fatal codes |
|---|---|
| 1. loading | `yaml` |
| 2. structure | `schema`, `syntax` |
| 3. names and order | `order`, `names`, `required` |
| 4. publish checks | `check N` |
| 5. logic | `contradiction` |
| 6. examples | `example` |

Notices, which refuse nothing: `audit`, `warn`, `self_serviceable`, `reason_kept`, `imports`, `metric`, `coverage`, `neverpasses`, `before`, `unknown`, `undecided`, `notrun`, `migration`, `veto`, `agent_open`, `taken_by_dropped`, `unbounded_reach`, `publishing_open`, `personal_dropped`, `rule_changed`.
