# ADR-0063: The swap test names its normalisations, and check 52's claim is retracted

- **Status:** Accepted — repair of D149–D158, 2026-09-08; pending author review
- **Refines:** ADR-0052, ADR-0061, ADR-0062
- **Date:** 2026-09-08

## Context

ADR-0062 introduced check 52, the first check on the document rather than on a declaration, and ADR-0061 replaced ADR-0052's whitelist of symmetric invariant shapes with a swap test. Both were the right change and both were made slightly too fast.

Two reviewers went at them directly, which is what they should have done: a new mechanism is the least-checked thing in a document, and one that makes a claim about its own effectiveness invites the test.

**The swap test rejected the shapes it blessed.** It defined symmetry as the swapped predicate being "the same up to the order of its conjuncts", and asserted that overlap of two ranges and equality on a shared key pass. They do not: swapping `b.start < end` gives `start < b.end`, the same comparison only once direction is discounted, and swapping `b.resource == resource` reverses its operands. Under the stated relation the section's own worked example fails. So the rule was either too weak, rejecting what it claimed to accept, or it meant semantic equivalence, which is unbounded and hands an implementer nothing.

**Check 52 claimed more than it does.** Its row said it was "the one that would have caught" the four rule-and-check divergences that motivated it. A reviewer ran the implemented predicate over those four: it catches one. Two it misses for want of a trigger phrase, and the fourth it cannot see at all, that one being a contradiction between two checks rather than between a rule and a check. The two it misses are the two that produced blocking defects.

## Decisions

### 1. The swap test names exactly four normalisations and nothing else

Conjuncts compare as a set; `==` and `!=` modulo the order of their operands; `<`, `>`, `<=` and `>=` modulo direction; a member of `this` modulo its spelling, so `subject` and `this.subject` are one reference. Two predicates a human can see are equivalent but that differ in any other way are **not** symmetric for this purpose.

That last sentence is the decision. A test used by a publish check has to be a procedure an implementer builds identically twice, not an equivalence a reader can argue for. Drawing the line visibly, and low, is better than drawing it where a reasonable person would and leaving each implementer to find their own edge.

### 2. Symmetry is defined by what it protects, not by pairs

"Its verdict does not depend on which of the objects it relates is the one being written." The earlier wording, "true or false of a pair of objects", excluded a cardinality scan, which relates a set rather than a pair and is symmetric in exactly the sense that matters.

### 3. Check 52's claim is retracted in the document

Not softened. It was a testable claim, someone tested it, and it failed. The row now says what the check does — enforce the discipline going forward for the phrasings it recognises — and names its two mechanical bounds: one citation shields every normative statement within 260 characters of it, and the phrasing list is fixed. The detector itself was widened from seven phrasings to twenty, which found seventeen uncited statements in this document, and runtime refusals were excluded because they are not publish-time rules.

### 4. The cascade-coverage check becomes a report

It had been numbered 53. Its condition, "every non-terminal state the part can be in when the trigger fires", could not be decided: the cheap reading rejects correct models, and the expensive one requires propagating a guard on a different transition two states earlier. It is now the strict form, reported without failing, naming the uncovered states. A risk that is undecidable in general and cheap to compute conservatively is what a report is for.

### 5. The syntax document owns the outcome grammar

The design document restated it and had gone stale in three of six lines, still showing forms the syntax document had replaced. One owner per rule is the structural answer to cross-document drift, which is the half of the problem no citation rule reaches.

## Alternatives rejected

- **Define swap-test equivalence semantically.** Rejected: unbounded, and it makes check 6 unbuildable, which is the state the whitelist was replaced to escape.
- **Keep the whitelist and add "status filter on both sides" to it.** Rejected again. The whitelist's failure was that it excluded shapes nobody had modelled yet; a longer list has the same failure later.
- **Soften check 52's claim rather than retract it.** Rejected on the project's own rule: a quiet softening lets the claim be re-derived from an older draft. The disproof is recorded next to the check.
- **Make check 52 catch the two it misses by matching any imperative sentence.** Rejected for now: the false-positive rate over prose that discusses rules rather than stating them would make the check noise, and a check people learn to ignore is worse than no check.

## Consequences

- Symmetry is decidable, and both shapes ADR-0052 named now demonstrably pass, checked by hand against the four normalisations.
- The syntax document is at iteration 15 with 52 checks, 24 implemented. Every normative statement in §1 to §9 cites its check; two of them needed a check to be written first, which is check 52's most useful result so far.
- `docs/DESIGN.md` loses its copy of the outcome grammar and gains the distinction between a conditional step and a conditional value.
- Check 52 remains unable to detect a citation that is present and wrong. Two such were found in the first twelve, both by reading. The mechanism's real payoff so far has been forcing someone to read a check against the sentence citing it, which widened two checks; the automated half has caught none of the four defects it was named for.
