# Writing a flow

A flow is a YAML module: the types of object it governs, the states each moves through, the transitions between them and the rules on each. Beside it, an examples file says what the rules should do, and the engine runs those examples every time the flow is published. This page takes a first flow from an empty file to examples passing on both of the engine's implementations. The format's specification is [`flow-format.md`](../design/flow-format.md), and its expression language is [`declaration-syntax.md`](../design/declaration-syntax.md) §8; [`integrate.md`](integrate.md) is how an application uses a flow once it exists.

## 1. Set up

```sh
scripts/setup/python-env                    # the checker's pinned Python environment, once
```

To run examples on the Rust core as well, the core's toolchain (rustup, which installs the pinned version on first use) and the Flow Studio's server environment (`apps/flow-studio/README.md`).

## 2. A first module, and its examples

`documents.yaml`, a document that is drafted and then published:

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

`documents.examples.yaml`, what it should do:

```yaml
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

## 3. Check it, and run its examples

```sh
scripts/.venv/bin/python scripts/check-flows.py documents.yaml documents.examples.yaml
apps/flow-studio/scripts/examples-on-core documents.yaml documents.examples.yaml
```

The checker runs the publish checks, then every example on the reference runner: `0 fatal · 0 notices`, `examples: 3 run: 3 passed`. The second command runs the same examples on the core. Pass a module's imports before it, in import order, as `people.yaml service.yaml service.examples.yaml`; check a new version against the last with `--previous=old.yaml`.

A **fatal** finding stops a publish, and names its line and how to fix it. A **notice** does not. `coverage` lists, first, the transitions no example takes at all, by its request, a step, a setup or a cascade it expects, and second, those only a setup, a step or an expected cascade takes, which no example tests as its request; `unknown` a guard or filter that an absent optional value would make unknown, which is usually a bug.

The [Flow Studio](../../apps/flow-studio/README.md) is the same loop with a diagram, a simulator and a live offer.

## 4. Learn from the samples

[`docs/design/flow-format/examples/`](../design/flow-format/examples/), each with its examples file, all checked and passing on both implementations:

| Sample | What it shows |
|---|---|
| `people.yaml` | actors: types whose objects make requests, with no roles |
| `approvals.yaml` | a state machine shared by two types, one replacing its creation |
| `reimbursement.yaml` | a claim with lines, policy flags that warn, an agent suggesting and a person confirming, an external system's check |
| `delivery.yaml`, `inventory.yaml` | an item handed over with a checklist, cascades, and a proposal a person approves |
| `service.yaml`, `service-v2.yaml` | observations recorded about a job, and a second version migrating live jobs |
| `customers.yaml` | personal data corrected and erased, an override, a rule across objects |
| `servicedesk.yaml` | mirror types another system owns, and importing them |
| `issues.yaml` | Jira's work items: subtasks, links, sprints |

## 5. Patterns for common needs

- **Lines of a document**, such as a requisition's: a `Line[]` attribute with `aggregation: composite` and `opposite`, the line created by the parent's transition (`create:` in its effect, the line's creation `only_via` it), and a total as a `derived_attributes` sum (`reimbursement.yaml`).
- **Approval by amount**: guards over the derived total on transitions to each level, and the decision recorded in a part. A transition has one target, so a level that either completes or escalates is two transitions with complementary guards, `approve` guarded `total <= SGD 5000.00` and `approve_and_escalate` guarded `total > SGD 5000.00`; `availability` tells a screen which of the two to show, the other refused (ADR-0176 decision 6).
- **A transition a person must take**: mark it `taken_by: [human]` (or the kinds that may, `[human, service]`). It records the kind that took it as `<transition>_by_kind` and holds an invariant `<transition>_taken_by` over the record, so an agent is refused `delegable`, its offer is marked `actor_dependent`, and its tools leave the transition out, or, where it is also `proposable`, hand it over for proposing only: "an agent proposes, a person decides" is `taken_by: [human]` with `proposable: true`. State it on every such transition, approvals, overrides and payments among them: nothing else stops an agent taking one.
- **Other rules about who acts**, "nobody approves their own", "the line's nature confirmed by a person": a condition may not read the actor, so record it in the outcome (`assign: { location: approved_by, expr: actor.id }`, or `actor.kind`) and constrain the record with an invariant, as `taken_by` does for you. An agent refused by one is told `delegable`, and its offer is marked `actor_dependent`.
- **Check who acted against a fact as it stood**: an invariant holds at all times, so one reading through references, "approved by the requester's manager" written `approver == requisition.requester.manager.user_id`, is checked again whenever the requester's manager changes, and then refuses the change. Copy the fact onto the record when the decision is made, `create: { … inputs: { manager_id: requisition.requester.manager.user_id } }`, and write the invariant over the record's own attributes.
- **Several currencies**: an amount holds one currency, so a currency reference with a rate, the transaction's amounts as decimals, and the home-currency amount computed and kept on entry (§6).
- **A value a state requires**: set it from an input, a literal, `now`, who acts (`actor.id`, `actor.kind`), or a path of references that are never empty ending in an attribute that is never empty (`cost_centre.fiscal_year`); anything else needs a required input or an invariant (`flow-format.md` §4.5).
- **A metric's regression test**: an example's `expect` may say what a metric reads after its request, `metrics: { ServiceJob.transition_counts: 3 }` for its whole value, or its rows by every dimension, `ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }]` (`flow-format.md` §11.4), as `service.examples.yaml`'s `a_reassignment_is_measured_as_a_handoff` does.
- **An effect sees the new state**: the state is written first, then the effect runs (`DESIGN.md` §6 step 5), so a derived value read in an effect already counts the object in its new state.
- **An external system's check**: an `evaluators:` entry, read by a guard; an example or a setup gives its answer with `evaluators:`, as `xero.bill_matches: satisfied`, and a sandbox with `/sandbox/evaluators`.
- **The agent proposes, a person decides**: mark the transition `proposable`; the agent files it with `propose: true`, and approving the proposal takes it as the approver.
- **Existing records at go-live**: a first version declaring the families brought in as `mirror`, which `scripts/mirror-version.py <module.yaml> <Type>...` writes from the module, an import, then a publish of the module itself; `publish-and-import.md` §7 works one through, for §6's orders. Plan the model for it: a mirror may not reference a type the store owns, so a reference from a family brought in to one born here, an order to its requisition, makes that type a mirror too; keep the link on the new side, a requisition naming its order. And the import checks invariants, not guards: a rule legacy data may already break, such as never receiving more than ordered, is checked at go-live only if it is also an invariant.

## 6. Several currencies

An amount holds one currency, which its type names, `money(USD)`, so a sum, a comparison or a threshold never mixes two (ADR-0068). A flow whose orders are placed in several currencies and whose limits are in one keeps three things (ADR-0173 decision 5):

1. **A currency reference with a rate**, an object finance keeps, here `Currency` with `rate_to_sgd`;
2. **the transaction's amounts as decimals**, in the currency its reference names, here a line's `unit_price`, since no `money` type can hold whichever currency an order is placed in;
3. **the home-currency amount computed and kept when the line is entered**, here `amount_sgd`, assigned from the rate that day, so a rate changed later does not change what was entered; limits, budgets and metrics are written against it.

Nothing stops a flow adding two decimals in different currencies, since a decimal carries none: keep each total in one currency, the home currency's computed amounts or one order's own.

```yaml
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

Its examples show the line valued on entry, a rate changed afterwards leaving the order as valued, and the limit held in Singapore dollars:

```yaml
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
