# ADR-0148: One normative home per rule, and what the third cold reading left open

- **Status:** Accepted, 2026-10-04, by the author ("go ahead with 1 and 2, and do 3 too"): fix the contradictions and regressions the third cold reading found (decision 2), record the rest as open questions answered when a builder reaches them (decision 3), and give every rule one normative home (decision 1). The individual fixes were decided at the author's earlier direction ("continue with your recommends"). Decision 4 by the author the same day ("yes, stop it at the close"), given the case.
- **Date:** 2026-10-04
- **Refined by:** ADR-0154 — thirteen of decision 3's open questions answered by the core's import, C14's third case, C22 to C24, C28, C29 and C31 to C37. ADR-0203 — no metric reads a personal value through a reference or a derived attribute either.
- **Refines:** ADR-0147 (its triage items 17, 54, 94 and 123 are corrected by decision 2), ADR-0105 (decision 2's upgrade over version 0), ADR-0116 (what the flow format and the model are each the home of)
- **Relates to:** `cold-reading-3-review.md`; every document named in decision 1; D514 to D516

## Context

Three cold readings found 97, 124 and 139 findings. The third, of the text written in one day to state ADR-0147, found about thirty that were contradictions between documents or regressions of that day's edits. The cause was structural: one rule was stated in full in up to five documents (`DESIGN.md`, `flow-format.md`, `declaration-syntax.md`, `publish-and-import.md`, `library-api.md`), each decision had to land in all of them, and the copies drifted. The rest were edges a builder would reach, many in older text, and the counts did not fall from round to round.

## Decision

1. **Every rule has one normative home**, where it is stated in full. Any other document may name the rule in a clause and cite its home by file and section; a citing sentence adds no condition of its own, and where it seems to, the home decides and the citation is the defect. The homes:

   | Topic | Home |
   |---|---|
   | What a deployment writes: keys, values, forms, the MUST rules on a description, the checker's steps and codes, the examples format and what an example runs | `flow-format.md` |
   | What each construct means: types, attributes, states, transitions and their markings, guards and conditions, invariants, effects, the expression language, observations, recording and corrections, the `erase` construct and which types declare one, supersession, the meaning of migration mappings, metrics and the standard metrics, and the publish checks | `declaration-syntax.md` |
   | How a request executes: its steps, verdicts and remedies, the order of refusals and the objects they name, the generated guards' order, idempotency and the attempt log; history and delivery; what erasure reaches and records; the built-in types and their lifecycles; the read surface | `DESIGN.md` §5.5, §6, §7, §8, §9, §10 |
   | Publishing, from the dry run to the approval, the report and its severities, the application of mappings; and the import | `publish-and-import.md` |
   | The shapes of operations, requests, verdicts and pages | `library-api.md` |
   | Storage, the DDL each change emits, and what is compiled | `storage-schema.md` |
   | The acceptance conditions | `adversarial-harness.md` |

   `DESIGN.md` §5 and §11 summarise and cite. Worked documents, `metric-scenarios.md` and `unit-journey.md`, illustrate; where one disagrees with a home, it is a finding against one of them. The corpus checker now resolves every citation of a document's section to a heading of that document.

2. **The third reading's contradictions and regressions are fixed** (`cold-reading-3-review.md` §5, F1 to F27), each in its home, the other statements replaced by citations:
   - F1: a publish evaluates every invariant over every object as the mappings leave it, an invariant it did not change needing no new evaluation over an object nothing it wrote affects; a type the change touches is one whose declaration it alters, or whose rules read such a declaration (correcting the narrowing of 2026-10-04).
   - F2: a migration event goes to each object whose row the mappings change, and an admission-only event to one only an `admit` reaches.
   - F3, F4: one list of what invalidates a pending proposal; an approval's events are the proposal's `approve` first, then the target's and what that causes.
   - F5, F6: a function without `fresh` does not read the stamp; an evaluator that does not answer, or answers too old, refuses with `temporal`, an exception to the declared remedy that examples follow.
   - F7, F8, F9: an attribute correction marks the rows whose value or `.held` it changes and relabels from the current interval's start, an untracked attribute's correcting only its value and event; a correction supplies `subject`; an expectation's `values` may name an observation's members.
   - F10: only a kind with a personal field has a generated `forget` (correcting ADR-0147's triage item 54's reach); a label's note is redacted on the subject's erasure event.
   - F11 to F18: scalars a description or examples file writes stay strings but for booleans and numbers; a creation never fails the interval check; a set may be personal, emptied by an erasure; `migrate`, `import` and `redact` are reserved transition names; an abstract base may be a mirror; an examples file resolves against every module the check is given; the built-in `DeclarationChange` guards' remedy is `unreachable_from_here`; the library's comment on named objects cites §5.5.
   - F19 to F22: the DDL for a removed or newly optional required attribute, a changed type and a removed or changed compiled invariant; a publish leaving an object in a state its new version does not reach is fatal; an upgrade over version 0 records version 1 with no source; the model's lists follow the publishing document.
   - F23 to F27: check 56's row states both rules attributed to it; `MetricPage` carries a refusal; `recorded_from` is read from the state's intervals and those the mapping supplies, a member opened at the port not moving it (correcting ADR-0147's triage item 94); an abstract base declares no metrics, a family's figure being combined from its subtypes' (correcting item 123); the restatements of the closed-stretch rule cite it, and its summary is narrowed to a span still current.

3. **The other findings are open questions**, recorded in `cold-reading-3-review.md` §5 and answered when a builder reaches each, the Rust core's first slice and its comparison with the Python reference being the instrument (TODO 215), not a further round of reading. One of them went to the author as it stood, a member's span that ends after its object closed counting the closed days retroactively (§4 finding 21), and is decided by decision 4.

4. **A member's span accrues only while its object is open** (decided by the author on 2026-10-04, answering decision 3's question: "yes, stop it at the close"). A span of a tracked member, an assignee's, counts only the time its object spent in states neither `closed` nor terminal: one ending after the object closed stops at the close, one outlasting a closed stretch the object reopened from leaves that stretch out, and a split piece counts only its own open time. A state's span keeps its exit less its entry, a closed state's included. *Rejected:* stopping only a span still current, which credited the closed days retroactively when a reassignment ended the span, and made a figure change after the fact; leaving out a closed state's own span, which would lose the time objects spend done.

## Alternatives rejected

- **For decision 1:** keeping every document complete on its own, which is what produced the drift; making `DESIGN.md` the home of everything, which would make it the format and the model at once, against ADR-0116's division.
- **For decision 3:** a full triage of every finding, as the first two rounds had, whose counts did not fall, and which a builder answers more exactly by meeting each edge with a failing test.
- **For the fixes:** the other reading each finding lists.

## Consequences

The documents are converted topic by topic, each rule's copies merged into its home first so that nothing a copy alone said is lost; the conversion is recorded as D515, the fixes as D514, and the open questions as D516, which stays open until the core answers them.
