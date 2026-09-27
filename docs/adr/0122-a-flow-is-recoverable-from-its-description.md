# ADR-0122: A flow is recoverable from its description

- **Status:** Accepted by the author, 2026-09-27: "go ahead with your recommendations, ensure there is no ambiguity after the revision". Each decision below is the recommendation `flow-recoverability-review.md` made for a finding, and D397's is the one that review left to the author.
- **Date:** 2026-09-27
- **Refines:** ADR-0116 (the format replaces the text language), ADR-0071 (bounded loops), ADR-0098 (metrics and `combine`), ADR-0029 (sequences)
- **Relates to:** `flow-format.md` §4.1, §4.3, §4.8, §4.9, §4.12, §4.17, §5; `declaration-syntax.md` §3.3, §4.2, §5.1, §5.2, §6.9; `DESIGN.md` §3, §5.5, §6; `flow-recoverability-review.md`

## Context

The author asked whether a flow builder, given the specification and the YAML, could recover the robot inventory and the Jira flows without ambiguity. Two readers with no context reconstructed them from the specification alone. Most of each flow was recovered exactly; ten questions the specification left open (D395 to D404) would let two careful builders produce engines that accept or refuse different requests, or compute different values, and the checker let two forms through that the text forbids (D405, D406). This record decides each question, so the specification settles it.

## Decision

1. **A loop's `limit` counts what the loop acts on** (D395). A `foreach` runs once for each element of its collection that satisfies `where`, and its limit bounds those runs; a cascade's limit bounds the parts it drives, those passed over in a final state not counted. A request whose loop or cascade would run more times than its limit is refused with `over-limit`; nothing is truncated.
2. **A loop's collection is read once, when the loop starts** (D396). The collection and `where` are evaluated before the first run, and the loop runs over that snapshot in ascending id order, even where its own steps change the collection; each step still reads the state the steps before it produced.
3. **`from: any` is every other state that is not final** (D397). An external transition always changes the state, so no transition leaves a state for itself, and no self-transition needs a rule of re-entry: an interval starts only when the state changes. A change that keeps the state is an internal transition, whose `from: any` is every state that is not final.
4. **A creation may omit a defaulted attribute** (D398). An attribute with a default may be an optional input of any transition: omitted at a creation it takes its default, omitted elsewhere it keeps its value, as the model's §5.1 says of `accepts`. Listed in `required_inputs`, it must be supplied. The format's §4.17 said the opposite and is corrected.
5. **A relationship of two single ends is one-to-one** (D399). The end marked `stored` holds a value no other object of its type holds, a uniqueness the store enforces as it does `unique: true`.
6. **A combined metric aggregates its inputs to its own dimensions** (D400). Each input is aggregated over every dimension the combined metric's `group_by` does not name, before the expression combines them; `version` and `actor_kind` are kept only when named.
7. **An absent dimension value is a group of its own** (D401), reported as absent; a row is not dropped for it.
8. **A sequence starts at 1 and rises by 1** (D402), monotonic and not gapless: a number drawn by a request that is then refused is not reused, as the glossary says.
9. **Every refusal has a verdict** (D403). A request, or a call it makes, on an object not in a state its transition leaves is refused `unavailable`, remedy `unreachable_from_here`; a required input missing, an input not of its declared type, a reference input naming no object or one of another type, and a set input with a repeated element are refused `invalid input`, remedy `self_serviceable`; a call or creation a request makes that is refused refuses the request, with its verdict and the step that made it.
10. **A null is absence** (D404). An optional input given as null is not supplied, and its write is skipped; only `clear` removes a value.
11. **A step is never conditional on an input** (D405). An optional input is read in a step only as the whole of an expression, which is skipped when it is absent; step 3 refuses it inside a larger expression, as the model's check 48 does. Behaviour that differs by whether an input is given is two transitions.
12. **A set of references names its opposite** (D406), and step 3 refuses one that does not; a machine's required set end is the exception, its opposite being on the binder.
13. **A module does not re-export its imports.** A name is imported from the module that declares it.

## Alternatives rejected

- **A limit over the whole collection.** It would refuse retiring a unit that had ever been on two engagements, since ended lines stay in the collection: the bound would measure history, not the work a request does.
- **Truncating a loop at its limit.** The model refuses rather than doing part of the work silently (ADR-0071); the schema's "at most limit times" is corrected to say so.
- **A live collection, re-read as the loop advances.** A loop whose steps remove its elements would skip every other one under an engine that pages by position.
- **A self-transition that re-enters its state.** UML's external self-transition exits and re-enters, and the interval rule would then need an exception for a state that did not change; excluding the target from `from: any` keeps both the rule and Jira's meaning of a global transition, which moves a work item *to* a status from the others.
- **The format's own reading of defaults**, that a creation must supply a defaulted input it lists. The model is the definition (ADR-0116), and its reading lets a caller omit what has a sensible default, which is what a default is for.
- **Many-to-one as the default for two single ends.** A derived single end that resolves to two objects is not single; the pair would be a set end in disguise.
- **Keeping every input dimension in a combined metric.** Its expression aggregates nothing, so it could not reduce a pair of inputs split by month to one value per model.
- **Dropping a row with an absent dimension.** A total over all groups would then disagree with the same metric computed without that dimension.
- **Gapless sequences.** They serialise every creation of a type behind one counter; `edge-cases.md` records gaplessness as out of scope.
- **Verdicts left to the engine.** The attempt log's verdicts are what the metrics over attempts count, so two engines would disagree on them.

## Consequences

- `flow-format.md` §4.1, §4.3, §4.8, §4.9, §4.12, §4.17 and §7 state the decisions, §4.17 and §4.8 with correction notes; `declaration-syntax.md` §3.3, §4.2, §5.2 and §6.9 and `DESIGN.md` §3, §5.5 and §6 say the same.
- `scripts/check-flows.py` reads `from: any` without the target, accepts a defaulted attribute as an optional input, and refuses a set reference with no opposite and a step conditional on an optional input, each proven by a plant; `scripts/check-syntax-doc.py` reads `any` the same way in the creation report.
- The robot inventory, the flow review's appendix and the Jira catalogue are corrected for D408 and D409.
