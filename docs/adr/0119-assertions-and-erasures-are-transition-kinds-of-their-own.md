# ADR-0119: Assertions and erasures are transition kinds of their own

- **Status:** Decided at the author's direction, 2026-09-26 ("go ahead with construct 8"); the author's own acceptance pending.
- **Date:** 2026-09-26
- **Refines:** ADR-0115 (the transition kinds are UML's), ADR-0116 (the format replaces the text language)
- **Relates to:** ADR-0015 (provenance), ADR-0087 (erasure follows supersession), ADR-0105 (erasure records an event and stays erased), ADR-0106 (an assertion's reason is an enumeration), `declaration-syntax.md` §4.2, §6.3 to §6.5, `flow-format.md` §4.13

## Context

ADR-0115 made the format's transition kinds UML's three, `initial`, `external` and `internal`, for the three the format then wrote. The model has two more, `assert` and `erase` (`declaration-syntax.md` §4.2), which construct 8 of ADR-0116's plan gives a written form, with the correction marking `corrects` and the admissions an assertion may make.

Neither fits a UML kind as the format uses them. An assertion runs from any state into one of several states the request chooses, while an external transition here names its source states and one target. An erasure runs at any state, a final one included, while an internal transition names the states it is taken in, and a final state takes none (the model's check 40, which step 4 applies). No state-machine standard reviewed for ADR-0115 has either.

## Decision

1. **Two further kinds, keeping the model's words**: `assertion`, whose `to` lists the states it may put an object in, and `erasure`, which has neither `from` nor `to`. "Erasure" is also the word of the GDPR's Article 17, "Right to erasure ('right to be forgotten')" ([gdpr-info.eu](https://gdpr-info.eu/art-17-gdpr/), which reproduces the regulation; EUR-Lex's page did not render to the fetch).
2. **The inputs a kind fixes are generated; the ones an author chooses are declared.** An assertion's target and admissions, the model's `input to : state` and `input admits : invariant[]?`, follow from `to` and `may_admit` and are written by the conversion. Its `reason`, whose enumeration is the author's, is a declared input like any other, and step 3 requires it, as it requires an erasure's and a correction's (the model's checks 28 and 29).
3. **`corrects` and `may_admit` keep the model's words**, as keys of a transition, since no standard names either.
4. **Step 3 checks what the internal checker does not**: an admission names an invariant of the type or of one related to it (check 27), a correction writes exactly what it lists (check 28), an erasure reaches every part type holding personal data (check 39), and a personal attribute is optional (check 9).

## Alternatives rejected

- **Markings on UML's kinds**, an external transition marked `asserted` and an internal one marked `erasure`. Each marking would carry exceptions to the rules its kind otherwise obeys: several targets and no source, and a transition at a final state.
- **Generating `reason` as well.** The reason's type is the author's choice, and a declared input is read by name as `inputs.reason` like any other, with nothing read that is not written down.
- **Declaring `to` and `admits` as inputs**, as the model's text does. Their types, `state` and `invariant[]`, are not types an input may have in the format, and both are already fixed by the transition's own keys.

## Consequences

- `flow-format.md` §4.8 lists five kinds, §4.13 describes assertions, admissions, corrections, erasure and deletion guards, and §5 lets an expression read a category by its name.
- `docs/design/flow-format/examples/customers.yaml` has one of each, and a metric counting the overrides by reason.
- A `transition_count` cannot count an assertion or an erasure, which run from any state; a formula over `transitions` counts them by `t.transition`.
