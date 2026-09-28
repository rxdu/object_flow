# ADR-0133: Three gaps the triage of the pending decisions found

- **Status:** Accepted by the author, 2026-09-28: "accept all, fix the defects, all four themes as recommended", after the triage of the decisions awaiting review; this records the third theme.
- **Date:** 2026-09-28
- **Refined by:** ADR-0134 — check 60 counts a personal input as it counts a personal attribute, and holds a machine's binders rather than the machine.
- **Refines:** ADR-0031 (erasure), ADR-0049 (external evaluators), ADR-0060 §7 (a part declared on an abstract base), ADR-0087 (erasure along a supersession chain), ADR-0116 (the flow description format)
- **Relates to:** PRD D8; `DESIGN.md` §5.5, §8, §10; `flow-format.md` §4.13, §4.14, §4.18; `declaration-syntax.md` §3.2, §5.1, §10 checks 37, 45 and 60

## Context

On 2026-09-28 the author had the decisions still awaiting review triaged against DESIGN.md and the later decisions. Three held a gap that needed a choice (D455, D456, D461):
1. **Erasure was optional.** ADR-0031 lets a type declare `erase`, and check 60 required it only of a type taking part in supersession (ADR-0087). A type with a personal attribute and no `erase` had values nothing could erase, against PRD D8, a Must: personal data is "erasable, like personal data anywhere else in the store". The CRM case study's `Activity`, whose `summary` is personal, was one.
2. **`eager` behaved as `deferred`.** ADR-0049 marks a guard that calls an external evaluator `eager`, "consulted whenever the transition's availability is computed", or `deferred`, only when the transition is requested. Since ADR-0054 and ADR-0077 no read calls an evaluator: `DESIGN.md` §10 says "Neither `available`, `check` nor `availability` calls an external evaluator". So the marking chose nothing.
3. **A part declared on an abstract base went unchecked in the format.** ADR-0060 §7 has each concrete subtype give the cascade clauses of a part its abstract base declares, since the base has no transitions, and holds the subtype to the coverage rule. The internal form writes it as a top-level `cascade <part>` (`declaration-syntax.md` §3.2). The flow format had no way to write it, and its checker let a subtype's final transition leave such a part uncovered, as three probes showed.

## Decision

1. **Every concrete type holding a personal attribute, its own or inherited, declares an erasure.** Check 60 refuses one that does not, whether or not it takes part in supersession. An abstract base need not, and its concrete subtypes must. The CRM case study's `Activity` gains `forget`.
2. **The `eager` and `deferred` markings are withdrawn.** A guard that calls an external evaluator is consulted when its transition is requested, before the write transaction, and by no read, each of which names the guards it did not evaluate. The format's `evaluation` key and the syntax's two words go, and so does check 45, which refused either word on a guard that calls no evaluator.
3. **The format writes a part on an abstract base with `inherited_parts`.** A concrete subtype gives, for each composite end it inherits from an abstract base, its `cascade` and `survives`, as an end of its own takes them, and is held to the coverage rule of check 37. An abstract type gives none, and an entry names a part the type inherits. The conversion writes the internal form's top-level `cascade <part>` and `survives <part>`.

## Alternatives rejected

- **Leaving erasure optional where no supersession is involved.** Personal data nothing can erase is what D8 exists to prevent, and a publish is the one place the gap can be found before the data exists.
- **Reviving `eager`: letting `availability` consult such an evaluator.** Every availability read of an object would then make a call to a third party, which ADR-0048 and ADR-0054 kept reads free of, and a read would depend on another system being up.
- **Refusing a composite end on an abstract base.** It would withdraw what ADR-0060 §7 decided and the internal form writes: one part type serving the members of a family, which ADR-0056 recommends.
- **Letting a subtype redeclare the inherited end with its clauses.** A member has one declaration (check 33), and a redeclared end would have to repeat its reference, opposite and aggregation, which could then disagree with the base.

## Consequences

- `flow-format.md` §4.13, §4.14 and §4.18, its key lists and `flow.schema.json`, and `declaration-syntax.md` §5.1 and checks 45 and 60 say so. `scripts/check-flows.py` and `scripts/check-syntax-doc.py` enforce decisions 1 and 3, and the flow checker's self-test plants an inherited part left uncovered.
- The customers example and the payments case study lose their `evaluation` lines.
