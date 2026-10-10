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

A **fatal** finding stops a publish, and names its line and how to fix it. A **notice** does not. `coverage` lists transitions no example requests as the request under test, even when a setup or a cascade takes them, and `unknown` a guard or filter that an absent optional value would make unknown, which is usually a bug.

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
- **Approval by amount**: guards over the derived total on transitions to each level, and the decision recorded in a part.
- **Rules about who acts**, "a person confirms", "nobody approves their own": a condition may not read the actor, so record it in the outcome (`assign: { location: confirmed_by, expr: actor.kind }`, or `actor.id`) and constrain the record with an invariant. An agent refused by one is told `delegable`, and its offer is marked `actor_dependent`.
- **Several currencies**: an amount holds one currency (ADR-0068). Keep a currency reference with a rate, write amounts in the transaction's currency as decimals, and compute and keep the home-currency amount when the line is entered (`SGD 1.00 * quantity * unit_price * currency.rate_to_sgd`), against which thresholds and budgets are written. Nothing stops adding two decimals of different currencies, so keep each total in one.
- **A value a state requires**: set it from an input, a literal, `now`, or a path of references that are never empty ending in an attribute that is never empty (`cost_centre.fiscal_year`); anything else needs a required input or an invariant (`flow-format.md` §4.5).
- **An effect sees the new state**: the state is written first, then the effect runs (`DESIGN.md` §6 step 5), so a derived value read in an effect already counts the object in its new state.
- **An external system's check**: an `evaluators:` entry, read by a guard; an example or a setup gives its answer with `evaluators:`, as `xero.bill_matches: satisfied`, and a sandbox with `/sandbox/evaluators`.
- **The agent proposes, a person decides**: mark the transition `proposable`; the agent files it with `propose: true`, and approving the proposal takes it as the approver.
- **Existing records at go-live**: a first version declaring the families brought in as `mirror`, an import, then a publish of the flow proper (`publish-and-import.md` §4 to §7).
