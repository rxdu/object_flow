# ADR-0010: The declaration is data, inspectable at runtime

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

Mediated access alone describes something that already exists. A repository layer, a service layer or a disciplined ORM boundary all abstract away direct database access and permit only designed operations. If restriction were the whole claim, this would be a code convention.

## Decision

The model is declared as data and is inspectable at runtime. A caller can discover what the designed operations are, which of them apply to a given object now, and precisely why the others do not — not merely be prevented from doing anything else.

## Alternatives rejected

### Code-defined models with a conventional service layer

Rejected because it delivers restriction without discovery. An agent cannot be handed such a system without the rules also being written into its prompt, where they immediately begin to drift from the implementation. It also leaves the UI, the API and the agent interface as separate work.

## Rationale

This is the property that makes the access surfaces interchangeable. Today a single business rule is expressed five times — as a storage constraint, as backend validation, as frontend form state, as error message text, and now as an agent tool description — in four languages, each free to drift. The rule itself is not hard; keeping five copies honest is. Setup cost therefore scales with the number of surfaces rather than the number of rules.

With one inspectable declaration, those become projections: the form knows which fields to require because the transition's guards say so, the button is disabled with the real reason attached, and the agent's tool schema is generated from the same guard list.

## Consequences

- The same structured unsatisfied-guard output serves three consumers: the agent's next action, the UI's tooltip on a disabled control, and the API's error response.
- The rule set for an object type should be printable as a readable artifact, so that someone who knows the business process can verify it. Since the residual risk is specification gaps rather than implementation bugs (see DESIGN.md, Known limits), legibility is as load-bearing for trust as enforcement is.
