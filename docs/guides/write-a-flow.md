# Write a flow

How to write a flow, test it with examples, and grow it with common patterns. [Concepts](concepts.md) explains the model; the [flow format card](flow-format-card.md) lists every key on one page; [`flow-format.md`](../design/flow-format.md) is the full specification, and [`declaration-syntax.md`](../design/declaration-syntax.md) §8 the expression language.

A flow is a YAML **module**. Beside it, an **examples file** says what its rules should do. The checker runs the examples on every check, and the engine runs them again on every publish, so a rule that stops doing what you meant fails before it reaches a store.

## Set up

Run everything from the repository's root. Build the checker's Python environment once:

```sh
scripts/setup/python-env
```

To run the examples on the engine's core as well, you need its Rust toolchain (rustup installs the pinned one on the first build) and the Flow Studio's server environment ([`apps/flow-studio/README.md`](../../apps/flow-studio/README.md)).

## A first module

A document that is drafted, then published:

```yaml
# documents.yaml
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
      DRAFT:     { description: Being written., category: live }
      PUBLISHED: { description: Released to its readers., category: closed, final: true, required_attributes: [published_at] }
    transitions:
      start:   { kind: initial, to: DRAFT, required_inputs: [title] }
      publish:
        kind: external
        from: DRAFT
        to: PUBLISHED
        effect:
          - assign: { location: published_at, expr: now }
```

`required_attributes` makes `published_at` required once a document is `PUBLISHED`, so `publish` must set it, as its effect does.

## Its examples

Each example states a starting point, a request and what should happen:

```yaml
# documents.examples.yaml
examples_for: documents

setups:
  a_draft:
    description: A document being written.
    steps:
      - request: { type: Document, transition: start, as: doc, inputs: { title: Release notes } }

examples:
  a_document_is_started_as_a_draft:
    description: Starting a document makes a draft with its title.
    request: { type: Document, transition: start, inputs: { title: Release notes } }
    expect: { verdict: applied, state: DRAFT, values: { title: Release notes } }

  publishing_records_when:
    description: Publishing a draft records when it was published.
    given: a_draft
    request: { object: doc, transition: publish, after: 1 h }
    expect: { verdict: applied, state: PUBLISHED, values: { published_at: now } }

  a_published_document_is_not_published_again:
    description: A published document is not published a second time.
    given: a_draft
    steps:
      - request: { object: doc, transition: publish }
    request: { object: doc, transition: publish }
    expect: { verdict: unavailable }
```

- A **setup** is a named starting state that examples build on with `given`. `as: doc` names the object a step creates, so later requests say `object: doc`.
- `after: 1 h` moves the clock before the request. `now` in `values` means the time it ran.
- `expect` gives the `verdict` (`applied`, or a refusal kind such as `unavailable` or `unsatisfied` with its `clause` and `remedy`), and the `state` and `values` it leaves.

## Check it

```sh
scripts/.venv/bin/python scripts/check-flows.py documents.yaml documents.examples.yaml
```

```text
0 fatal · 0 notices
examples: 3 run: 3 passed, 0 failed, 0 not run
```

The checker runs every publish check, then every example on the reference runner. Pass a module's imports before it, in import order: `people.yaml service.yaml service.examples.yaml`. Check a new version against the last one with `--previous=old.yaml`.

- A **fatal** finding stops a publish. It names its line and how to fix it.
- A **notice** does not stop a publish, but read each one. `coverage` lists the transitions no example tests. `unknown` flags a guard or filter that an absent optional value would make unknown, usually a bug. `agent_open` says what an agent could take, and `publishing_open` that the default decides who approves a version ([Agents](agents.md)). `taken_by_dropped` flags a version that widens who takes a transition or approves a version, and `personal_dropped` one that drops `personal` from an attribute, whose values an erasure then no longer reaches. `unbounded_reach` names a rule that reads an unbounded set, and the bound to declare.

To run the same examples on the engine's core:

```sh
apps/flow-studio/scripts/examples-on-core documents.yaml documents.examples.yaml
```

The [Flow Studio](../../apps/flow-studio/README.md) is the same loop with a diagram, a simulator and a live view of what each object offers.

## Learn from the samples

[`docs/design/flow-format/examples/`](../design/flow-format/examples/) holds checked samples, each with its examples file, passing on both of the engine's implementations:

| Sample | Shows | Checked with |
|---|---|---|
| `people.yaml` | Actors: types whose objects make requests. | `people.yaml` |
| `approvals.yaml` | A state machine two types share, one replacing its creation. | `approvals.yaml approvals.examples.yaml` |
| `reimbursement.yaml` | A claim with lines, policy flags that warn, an agent suggesting and a person confirming, an outside system's check. | `reimbursement.yaml reimbursement.examples.yaml` |
| `delivery.yaml`, `inventory.yaml` | An item handed over with a checklist, cascades, and a proposal a person approves. | `inventory.yaml inventory.examples.yaml`; `people.yaml inventory.yaml delivery.yaml delivery.examples.yaml` |
| `service.yaml`, `service-v2.yaml` | Observations recorded about a job, and a second version migrating live jobs. | `people.yaml service.yaml service.examples.yaml`; `--previous=service.yaml people.yaml service-v2.yaml service-v2.examples.yaml`; `people.yaml service.yaml service-migration.examples.yaml`, whose examples publish version 2 over version 1 and so are never carried into a change |
| `customers.yaml` | Personal data corrected and erased, an override, a rule across objects. | `people.yaml customers.yaml customers.examples.yaml` |
| `servicedesk.yaml` | Mirror types another system owns, and importing them. | `people.yaml servicedesk.yaml servicedesk.examples.yaml` |
| `issues.yaml` | Jira's work items: subtasks, links, sprints. | `people.yaml issues.yaml issues.examples.yaml` |

The "Checked with" column gives each sample's files in the order the checker takes them: an examples file creating actors needs `people.yaml` first, which declares them, even where its module does not import it.

## Patterns

**Lines of a document**, such as a requisition's. Declare a `Line[]` attribute with `aggregation: composite` and an `opposite`. The parent's transition creates each line (`create:` in its effect), the line's creation is `only_via` that transition, and a total is a derived `sum` (`reimbursement.yaml`).

**Approval by amount.** Put guards over the derived total on the transitions to each level, and record the decision in a part. A transition has one target, so a level that either completes or escalates is two transitions with complementary guards: `approve` guarded `total <= SGD 5000.00`, `approve_and_escalate` guarded `total > SGD 5000.00`. `availability` tells a screen which one applies.

**A transition a person must take.** Mark it `taken_by: [human]`, or the kinds that may (`[human, service]`). It records the kind that took it as `<transition>_by_kind`, refuses any other kind `delegable`, and is left out of an agent's tools. With `proposable: true` as well, an agent may propose it for a person to decide. Nothing else stops an agent, so state it on every such transition: approvals, payments, assertions and erasures, a person's erasure first. Or state it once on the type, `taken_by: [human]`, and open what agents may do with `taken_by: [human, agent]` on those transitions.

**Other rules about who acts**, such as "nobody approves their own request". A condition may not read the actor, so record the actor in the outcome (`assign: { location: approved_by, expr: actor.id }`, or `actor.kind`) and constrain the record with an invariant. An agent refused by one is told `delegable`.

**A rule about a fact as it stood.** An invariant holds at all times, so one reading through references, such as `approver == requisition.requester.manager.user_id`, is checked again whenever the requester's manager changes, and then refuses that change. Copy the fact onto the record when the decision is made (`create: { … inputs: { manager_id: requisition.requester.manager.user_id } }`) and write the invariant over the record's own attributes. The checker's `veto` notice finds the first form.

**A value a state requires.** Set it from an input, a literal, `now`, the actor (`actor.id`, `actor.kind`), or a path of references that are never empty ending in an attribute that is never empty (`cost_centre.fiscal_year`). Anything else needs a required input or an invariant.

**A deadline.** Keep it as an attribute the transition that starts the clock writes, `assign: { location: expires_at, expr: now + 7 days }`, and guard the transition that ends it with `now >= expires_at`. Your application's scheduler queries `expires_at <= now` and requests it: the engine starts nothing by itself. A version that changes the period changes one expression. A retention period works the same way: `erase_after` written when an application is rejected, and the scheduler erasing those due.

**Back to the stage it came from.** An effect cannot read the state being left, because the state is written first. So a return to one of several stages is one transition per stage, each guarded on a recorded stage: `resume_to_interviews` guarded `held_from == Stage.INTERVIEWS`. `availability` offers only the one that applies.

**An effect sees the new state.** The state is written first, then the effect runs, so a derived value read in an effect already counts the object in its new state.

**A guard's input.** A guard runs before the transition writes anything, so on a creation it reads the value sent, `inputs.days`, not the attribute `days`, which has no value yet. The checker's `before` notice catches this.

**An outside system's check.** Declare it under `evaluators:` and read it from a guard. An example gives its answer with `evaluators: { xero.bill_matches: satisfied }`; a sandbox with `/sandbox/evaluators`.

**A metric's regression test.** An example's `expect` may state what a metric reads after its request: `metrics: { ServiceJob.transition_counts: 3 }` for its whole value, or its rows by every dimension, `ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }]` (`service.examples.yaml`).

**Existing records at go-live.** A first version declares the types brought in as `mirror`, which `scripts/mirror-version.py <module.yaml> <Type>...` writes from your module; an import fills them; then a publish of the module itself makes them the flow's own. Plan the model for it: a mirror may not reference a type the store owns, so keep a link on the new side (a requisition naming its order, not the order its requisition). The import checks invariants, not guards: a rule old data may already break is checked at go-live only if it is also an invariant. [`publish-and-import.md`](../design/publish-and-import.md) §7 works a go-live through.

## Several currencies

An amount holds one currency, which its type names (`money(USD)`), so no sum, comparison or threshold mixes two. A flow whose orders come in several currencies, with limits in one, keeps three things:

1. **a currency object with a rate**, which finance keeps, here `Currency` with `rate_to_sgd`;
2. **the transaction's amounts as decimals**, in the currency its reference names, here a line's `unit_price`, since no `money` type can hold whichever currency an order uses;
3. **the home-currency amount, computed and kept when the line is entered**, here `amount_sgd`, so a rate changed later does not change what was entered. Write limits, budgets and metrics against it.

Nothing stops a flow adding two decimals in different currencies, since a decimal carries none: keep each total in one currency.

```yaml
# purchasing.yaml
module: purchasing
categories: [live, closed]

sequences:
  order_number:
    description: Numbers the orders, as PO-04301.

types:
  Currency:
    description: A currency an order may be placed in, with its rate to Singapore dollars, kept by finance.
    tracking: record
    attributes:
      code:        { type: string, unique: true }
      rate_to_sgd: { type: "decimal(12,6)" }
    states:
      ACTIVE:  { description: Orders may be placed in it., category: live }
      RETIRED: { description: No longer offered., category: closed, final: true }
    transitions:
      add:      { kind: initial, to: ACTIVE, required_inputs: [code, rate_to_sgd] }
      set_rate: { kind: internal, from: ACTIVE, required_inputs: [rate_to_sgd] }
      retire:   { kind: external, from: ACTIVE, to: RETIRED }

  Order:
    description: An order placed in one currency, its lines' amounts kept in that currency and in Singapore dollars.
    tracking: record
    attributes:
      number:   { type: string, identifier: { sequence: order_number, format: "PO-{n:5}" }, unique: true }
      currency: { reference: Currency }
      lines:    { reference: "OrderLine[]", aggregation: composite, opposite: order, survives: true }
    states:
      OPEN:   { description: Lines are being added., category: live }
      PLACED: { description: Sent to the supplier., category: closed, final: true }
    derived_attributes:
      total_sgd:
        description: The order's value in Singapore dollars, as its lines were valued when entered.
        expression: "sum(l in lines: l.amount_sgd)"
    conditions:
      within_limit:
        description: An order of SGD 10,000.00 or more is not placed without finance's review.
        expression: total_sgd < SGD 10000.00
        remedy: delegable
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [currency] }
      add_line:
        kind: internal
        from: OPEN
        description: Adds a line, priced in the order's currency.
        inputs:
          quantity:   { type: int }
          unit_price: { type: "decimal(12,2)" }
        effect:
          - create:
              type: OrderLine
              transition: add
              inputs: { order: this, quantity: inputs.quantity, unit_price: inputs.unit_price }
      place:
        kind: external
        from: OPEN
        to: PLACED
        guards:
          within_limit: deny

  OrderLine:
    description: A line of an order, its price in the order's currency and its amount in Singapore dollars at the rate on entry.
    tracking: record
    attributes:
      order:      { reference: Order, opposite: lines }
      quantity:   { type: int }
      unit_price: { type: "decimal(12,2)" }
      amount_sgd: { type: money(SGD) }
    states:
      LISTED: { description: On its order., category: closed, final: true }
    transitions:
      add:
        kind: initial
        to: LISTED
        only_via: [Order.add_line]
        required_inputs: [order, quantity, unit_price]
        effect:
          - assign: { location: amount_sgd, expr: SGD 1.00 * quantity * unit_price * order.currency.rate_to_sgd }
```

Its examples show a line valued on entry, a rate changed afterwards leaving the order as valued, and the limit held in Singapore dollars:

```yaml
# purchasing.examples.yaml
examples_for: purchasing

setups:
  an_order_in_dollars:
    description: An order placed in US dollars, at 1.35 Singapore dollars to the dollar.
    steps:
      - request: { type: Currency, transition: add, as: usd, inputs: { code: USD, rate_to_sgd: 1.35 } }
      - request: { type: Order, transition: open, as: order, inputs: { currency: usd } }

examples:
  a_line_is_valued_in_singapore_dollars_on_entry:
    description: A line of four at USD 100.00 is valued SGD 540.00 at the rate when it is entered.
    given: an_order_in_dollars
    request: { object: order, transition: add_line, inputs: { quantity: 4, unit_price: 100.00 } }
    expect: { verdict: applied, state: OPEN, values: { number: PO-00001, total_sgd: SGD 540.00 }, cascaded: [OrderLine.add] }

  a_rate_changed_later_leaves_the_order_as_valued:
    description: A rate set after a line was entered leaves the order's value as its line was valued, SGD 540.00, and the order is placed at it.
    given: an_order_in_dollars
    steps:
      - request: { object: order, transition: add_line, inputs: { quantity: 4, unit_price: 100.00 } }
      - request: { object: usd, transition: set_rate, inputs: { rate_to_sgd: 1.40 } }
    request: { object: order, transition: place }
    expect: { verdict: applied, state: PLACED, values: { total_sgd: SGD 540.00 } }

  the_limit_is_in_singapore_dollars:
    description: An order of USD 8,000.00 is SGD 10,800.00, past the limit, so it is not placed without finance's review.
    given: an_order_in_dollars
    steps:
      - request: { object: order, transition: add_line, inputs: { quantity: 80, unit_price: 100.00 } }
    request: { object: order, transition: place }
    expect: { verdict: unsatisfied, clause: within_limit, remedy: delegable }
```

```sh
scripts/.venv/bin/python scripts/check-flows.py purchasing.yaml purchasing.examples.yaml
```

```text
examples: 3 run: 3 passed, 0 failed, 0 not run
```

It also gives two `coverage` notices, since no example tests `Currency.retire` and three transitions are taken only by setups.

## What a later release promises

A flow that is valid, and whose examples pass, under one release stays valid, means the same and passes under every later release of the same major version. A check that would refuse it arrives first as a notice.
