# ADR-0131: How flows are authored and checked

- **Status:** Accepted by the author, 2026-09-28: "accept all your recommendations", on the questions of `authoring-flows.md` §7 put to the author the same day. Question 2 had no recommendation and was answered the same day, below.
- **Date:** 2026-09-28
- **Refined by:** ADR-0135 — the source reference is a required opaque string, and the engine records a digest of the source beside it, which the repository's side checks. ADR-0136 — examples are a file beside the description, set up by requests from an empty store, shape-checked now and run by the engine at publish. ADR-0137 — the contradiction checker: three definite shapes within one type, an analysis sound for refusal, and the expression parser it rests on. ADR-0140 — the design's own examples all run, while a description being written is only told of one that does not.
- **Refines:** ADR-0116 (the flow description format), ADR-0077 (declaration and type versions), ADR-0097 (a flow change's lifecycle)
- **Relates to:** PRD F1, F5, F6, F7; `authoring-flows.md`; `flow-format.md` §7, §9; `declaration-syntax.md` check 22

## Context

On 2026-09-25 the author asked for three things, in their words: that the way flows are specified be "clearly described and version controlled … structured such that even manual editing is possible but … clearly enough that an AI agent can be the default drafter"; that "it should be possible to vadlidate [validate] the description automatically"; and that there be "a way to validate if the specs provided by the user are semantically correct without logical errors (such as conflicting conditions)". `authoring-flows.md` set out options and a recommendation for each, and eleven questions. The written form was answered on 2026-09-26 by the flow description format (ADR-0116). On 2026-09-28 the author accepted the recommendations on the rest.

## Decision

1. **A flow is a set of files in a version-controlled repository.** The repository and its checks are an upper layer, as ADR-0104 and ADR-0110 place applications, and the engine keeps governance: a `DeclarationChange`, and the version it installs, record the source reference, such as the commit, the change was drafted from. A description can be checked without drafting a change, which the flow checker does.
2. **A description carries no version numbers.** Publishing computes them. Objects and events record only the store-wide declaration version (ADR-0077), so nothing reads a number an author wrote, and check 22's clause on a version that did not advance applies only to the internal text form the checks read.
3. **Validation is deterministic.** The flow checker's steps refuse a publish. A model's review of a description's prose is advisory, run outside the engine, and never blocks.
4. **A claim about behaviour is an example the engine runs.** An example is a setup, a request and the expected verdict, with its clause and remedy class, or the state reached. The engine runs every example at publish against an empty in-memory store, with the clock and the id source injected, so it tests the rules and not the live data. Names in a description are resolved like names in a rule, and prose is rationale, printed as unverified.
5. **Example coverage is reported, not required.** The publish report gives, for each transition, the examples that exercise it, so drafting is never blocked by a transition not yet covered.
6. **A change that breaks an example is refused** until the example is updated in the same change, as a failing test would be.
7. **A definite contradiction refuses a publish**, such as two guards of one transition that no object can satisfy together. What the analysis cannot decide is reported. The analysis runs when a description is checked or published, never on the request path.
8. **The first logical-error checker works within one type.** Waits across objects come later.
9. **No new dependency.** The checker is written for the fragment one type's rules use: comparisons, null tests, enumeration membership and counts. An off-the-shelf solver such as Z3 is reconsidered when waits across objects are checked, and needs the author's approval then.

## Alternatives rejected

- **The store's own history as the only version control.** A repository gives review, diffs and branches to people and agents alike, and the store still records what was published and on whose approval.
- **Hand-written version numbers.** They are the bookkeeping most likely to be wrong, whether a person or an agent writes them, and no reader needs them.
- **A model's review that can block a publish.** Its verdict depends on the model and can differ between runs, and the format promises that whatever it checks is decidable from the text.
- **Claims about behaviour in prose.** Prose cannot be checked, and the risk `DESIGN.md` §13 names is rules that say the wrong thing.
- **Coverage as a refusal.** It would stop an agent's first draft from publishing, which V1's minimal flow must be able to do.
- **A broken example reported like an impact.** The rules and what they were meant to do would drift apart without anyone deciding it.
- **Reporting contradictions only.** A flow with a transition no object can take is wrong, and a report is read after the damage.
- **Z3 now.** A contradiction within one type needs far less, and every dependency is a cost the author asked to keep low.

## Answered after

**Question 2, the default drafter.** *Answered by the author 2026-09-28* ("accept the rest", of the recommendation put with ADR-0132): an agent normally drafts a flow, and a person reviews and approves it. F1's authorship clause is the author's words, and F6 is a Must, still in the third release (PRD revision 14).

## Consequences

- `authoring-flows.md` records the answers, and `flow-format.md` §9's first question is closed.
- `TODO.md` holds the design work: recording the source reference on a `DeclarationChange`; the examples, as part of the format, with their publish check; and the checker for contradictions within one type. The PRD gains each requirement, in the author's words of 2026-09-25, together with the design that covers it, so the traceability map covers every row as it is added.
