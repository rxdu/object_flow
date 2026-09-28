# ADR-0116: The flow description format replaces the text language

- **Status:** Accepted by the author, 2026-09-26: "yes, replace the text language with yaml", in answer to whether the YAML form should become the written form of every flow (`authoring-flows.md` §7, question 3), asked with the review of ADR-0115.
- **Date:** 2026-09-26
- **Refined by:** ADR-0117 — the types of a module may reference each other in any order, and relationships are written in UML's terms (construct 3 of §4's plan). ADR-0118 — metrics are written under their type, and `combine` waits (construct 7). ADR-0119 — assertions, erasures and corrections are written (construct 8). ADR-0120 — a migration is a section of the module it publishes, checked against the previous version (construct 9c). ADR-0121 — the design's YAML is checked where it stands, for decision 5: a module may span blocks, an excerpt is part of a checked module, and imports resolve. ADR-0122 — a flow is recoverable from its description: what a loop's limit counts, `from: any` without its target, defaults at creation, and the other gaps the recoverability review found. ADR-0129 — the flow checker's step 3 gains the `metric` notice. ADR-0131 — flows live in a repository, carry no version numbers, and state behavioural claims as examples run at publish.
- **Refines:** ADR-0010 (the declaration is data), ADR-0115 (the text language kept its keywords until this decision)
- **Relates to:** `docs/design/declaration-syntax.md`, `docs/design/flow-format.md`, `docs/design/authoring-flows.md` §3

## Context

`authoring-flows.md` §3 recommended keeping the text language as the only written form; the author overturned that on 2026-09-25, asking for a structure a reader can scan, and a YAML form was put on trial. Three shapes later, with its terms aligned to UML, SCXML and Kubernetes (ADR-0115), the author decided that it replaces the text language.

Two facts shape how. The YAML form expresses only the constructs the trial's modules needed: `flow-format.md` Appendix B lists the rest, among them shared machines, relationships and cascades, derivations, counters, sequences, evaluators, assertions, erasure, supersession, migrations and labels. And every implemented publish check, thirty-six of them, runs over the text language, which the YAML form is converted to.

## Decision

1. **A flow is written in the flow description format**, `flow-format.md`, and in nothing else. People and agents write it, review it and keep it under version control; it is what a change proposes for publication.
2. **The text language is retired as a written form.** Until the publish checks run on the declaration model directly, it survives only as the internal form the conversion produces and the checker reads. No one writes it, reviews it or is shown it; where the walkthrough shows the conversion, it is labelled as what the engine checks.
3. **`declaration-syntax.md` remains the definition of the declaration model**: what each construct means, how requests against it execute, and what each publish check refuses. Its notation is the internal form. For a construct the YAML form expresses, `flow-format.md` states how it is written; for one it does not yet, the syntax document is the reference until it does.
4. **The format grows construct by construct**, in the order `TODO.md` sets, each with its YAML form, its section of `flow-format.md`, its schema, its conversion, an example and planted mistakes that prove its checks. A flow that needs a construct the format cannot yet express cannot be written until the construct has a form.
5. **The design's examples are rewritten in YAML** as their constructs gain forms, the first consumer's module in `unit-journey.md` first. Architecture decision records keep the notation they were written in, being records of their time.
6. **The schema is published with the format**: `docs/design/flow-format/flow.schema.json`, JSON Schema draft-07, versioned with the specification's draft, with a description on every key an editor shows. It is normative for structure only; the rules it cannot express are the specification's and the checker's. A description names it with the modeline `# yaml-language-server: $schema=<path>`, which editors built on the YAML language server read.
7. **The trial's files become the format's**: `docs/design/yaml-trial/` moves to `docs/design/flow-format/`, the schema at its root and the example modules under `examples/`, and `scripts/check-yaml-trial.py` becomes `scripts/check-flows.py`.

## Alternatives rejected

- **Keep both written forms**, converted each way. `authoring-flows.md` §3 set it aside: a round trip must preserve comments and layout, and two sources raise which one the repository holds.
- **Retire the text language outright now**, with no internal form. The thirty-six implemented checks would stop running until each is rebuilt over the model; the conversion keeps them running while that happens.
- **Convert every construct and example before deciding.** It would delay a decision the author has made, and the order of the work is better set by which constructs the first consumer needs.
- **Keep the YAML form a trial.** The author decided.

## Consequences

- `DESIGN.md` names the flow description format as the written form, and the syntax document as the definition of the model; `declaration-syntax.md` carries a status note saying the same.
- `flow-format.md` leaves trial status, gains a section on the schema, and gains Appendix B, the coverage of the model by the format.
- `authoring-flows.md` §3 and §7 record the answer; `TODO.md` holds the plan: coverage in order, the examples, and the checks over the model.
- `publish-and-import.md`'s draft of a change takes the description files as its source; its text is revised when the draft's interface is.
- Until the checks run on the model, a finding from step 4 names a check of the syntax document, reported at the line of the description it came from.
