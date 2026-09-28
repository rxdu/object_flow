# ADR-0137: Contradictions within one type are refused by an analysis that refuses only what it proves

- **Status:** Accepted by the author, 2026-09-28: "accept all your recommendations", on the five questions put the same day about the checker ADR-0131 decisions 7 to 9 left to design.
- **Date:** 2026-09-28
- **Refines:** ADR-0131 (how flows are authored and checked)
- **Relates to:** PRD F12, UC-15; `flow-format.md` §7, §7.1; `publish-and-import.md` §1; `DESIGN.md` §5.9; ADR-0132 decision 2; ADR-0136 decision 5

## Context

ADR-0131 decided that a definite contradiction refuses a publish, such as two guards of one transition that no object can satisfy together; that what the analysis cannot decide is reported; that the first checker works within one type; and that it needs no new dependency, written for the fragment one type's rules use: comparisons, null tests, enumeration membership and counts. Five questions were put to the author on 2026-09-28: which contradictions are definite, how the analysis stays sound, what it reports rather than refuses, how it treats counts, and how it is built.

Neither checker parsed an expression into a tree: both matched text with regular expressions. So the analysis, and the reference runner of ADR-0136, needed a parser first.

## Decision

1. **Three shapes are definite contradictions, each within one type:** a transition no request can take, since from its states and with its required inputs its blocking guards cannot all be true; a state no object can be in, since its invariants cannot hold together, while a transition enters it; and a transition whose own writes make an invariant of the state it enters false whatever the request. Each refuses the publish, with finding code `contradiction`.
2. **The analysis is sound in the direction a refusal needs.** Under the three-valued logic of `declaration-syntax.md` §8.2, a guard must be true and an invariant not false. What it decides is presence, a state, an enumeration value or a boolean within a set, text or an identity against a literal, and difference bounds between ordered values of one type or its inputs; anything else, a path to another object, a metric, an evaluator, `all`, arithmetic beyond adding a constant, is an opaque part that may be true, false or unknown, so it only ever makes conditions look possible. A derived attribute is read as its expression. Nothing is assumed of what an earlier request wrote beyond what the declaration makes it, and an invariant counts against a transition only where it reads nothing but what the transition writes and the state it enters, since an invariant may stand violated under an admission.
3. **What is reported, not refused:** a condition too large to decide, beyond 4096 cases, as the notice `undecided`; an audit or warn guard that can never pass when its transition is taken, as the notice `neverpasses`; and, for each type, how many guard conditions were decided in full, a figure the checker prints and the publish report carries, not a notice, since nothing about it asks the author to act.
4. **A count over the object's own collection is a whole number of at least zero**, one per distinct collection and filter, with `none` and `any` its being zero or at least one; the filter itself stays opaque.
5. **One expression parser, then the analysis.** `scripts/flowexpr.py` parses the grammar of `declaration-syntax.md` §8, proven on every expression in the design's descriptions and on its own shapes and malformed cases, and step 2 refuses an expression it does not accept, as `syntax`. `scripts/flowlogic.py` is step 5 of the flow checker, proven on twenty-two small types. The Rust core ports both, with these as its oracle (ADR-0132 decision 2), and the reference runner of examples builds on the same parser.

## Alternatives rejected

- **Reporting contradictions only.** ADR-0131 rejected it: a flow with a transition no object can take is wrong, and a report is read after the damage.
- **Treating an unreadable clause as false, or as true, when deciding.** Either would let the analysis refuse a flow over something it cannot read; an opaque part that may take any value never does.
- **Assuming earlier values satisfy the invariants.** An admission lets an object stand in violation, so a refusal built on that assumption could refuse a flow that works.
- **Deciding counts by what their filters say.** Relating two filtered counts needs reasoning over the elements, which the fragment of ADR-0131 decision 9 leaves out; keeping each filter opaque never claims more than is seen.
- **An off-the-shelf solver.** ADR-0131 decision 9 defers Z3 to the waits across objects, and needs the author's approval then; differences, finite sets and presence need no solver.
- **Parsing with regular expressions, as the checkers did.** A grammar that is not parsed cannot be analysed, and `issues.yaml` carried a membership in brackets, which the grammar does not have, past both checkers until the parser met it.

## Consequences

- `flow-format.md` §1 and §7 state the five steps, the codes `syntax` and `contradiction`, and the two notices, and §7.1 what step 5 decides; `scripts/check-flow-format-doc.py` reads step 5's codes from `flowlogic.py`.
- `scripts/check-flows.py` runs the parser's proof and the analysis's self-test on every run, and plants a malformed expression, a pair of guards that cannot both hold and a warning that can never pass.
- `issues.yaml`'s resolution guard writes its set in braces.
- `publish-and-import.md` §1 runs step 5 at publish, `DESIGN.md` §5.9 says so, and PRD F12 states the requirement in the author's words of 2026-09-25.
