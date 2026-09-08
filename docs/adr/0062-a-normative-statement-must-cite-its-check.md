# ADR-0062: A normative statement must cite the check that enforces it

- **Status:** Accepted — repair of D134–D148, 2026-09-08; pending author review
- **Refines:** ADR-0052, ADR-0055, ADR-0060, ADR-0061
- **Date:** 2026-09-08
- **Refined by:** ADR-0063 — the swap test names its normalisations, and check 52's claim is retracted; ADR-0064 — a binder's own creations replace the machine's.

## Context

Three reviewers re-read the declaration syntax against their own earlier findings. Two returned non-blocking. What survived was almost entirely one shape, and the decidability audit named it rather than merely listing instances:

> Four of the five defects above are one shape: a rule was changed in §3 or §8 and its check in §10 was not.

- §8.3 was rewritten to permit `if` in an outcome value; check 21 still called it an error, so following the document produced a declaration that would not publish.
- §3.1 widened a sequence scope to an indexed attribute; check 44 still said "reference".
- §3.4 declared the old self-exclusion idiom an error; no check enforced it.
- Check 8 gained a required singular `part`; nothing reconciled it with check 17, which forbids writing a `part` end, so the construct was unsatisfiable.

This happened in iterations 10, 11 and 13. The document's structure invites it: rules are stated where a reader meets them, and what is verified lives in one list at the end, and nothing connects the two. Every occurrence was found by a human reviewer, one iteration late, and each time the fix was scoped to the instance. `docs/LESSONS.md` already records what scoping a fix to the instance costs, from the `stored` gap, which recurred eleven times for the same reason.

## Decision

**Every normative statement in §1 to §9 cites the check that enforces it.** A sentence saying something is rejected, is an error, is a publish error, or that publishing rejects it, carries a `check <n>` reference. Check 52 fails a publish when one does not.

This is the first check on the **document** rather than on a declaration, which is why it needed a separate fixture mechanism: it runs over the whole text rather than over a fenced example. `DOC_FIXTURES` in `scripts/check-syntax-doc.py` holds its minimal failing example, so it is demonstrated on the same terms as every other claimed check.

Twelve statements needed citations. The check was confirmed by removing one and watching it fire at the right line.

## Alternatives rejected

- **Move the rules into §10 so there is one place.** Rejected: a reader meets a rule where the construct is, and a language reference that states its rules only in a validation appendix is unreadable. The two locations are right; the missing thing was the link.
- **Require the reverse link as well, every check citing the section it enforces.** Deferred rather than rejected, and for a better reason than the first draft of this ADR gave. It said no defect needed it; iteration 15 then produced one — check 33 was widened to satisfy a citation and came out forbidding more than §9.2 does. But a reverse link would not have caught it either: check 33 cites §9.2 and would pass any presence test. Neither direction catches a **contradiction**; both catch absence. What caught it was a reviewer reading the two texts side by side. The reverse link is still worth adding eventually, because it puts the two texts where one person can compare them, which is the thing that actually works.
- **Review discipline instead of a check.** Rejected on evidence: this was the third iteration in which the same discipline was intended and did not hold. A rule enforced by attention is a rule that fails when attention is elsewhere, which is the argument the whole project rests on.

## Consequences

- `scripts/check-syntax-doc.py` gains `doc_checks()` and `DOC_FIXTURES`, and enforces 24 of 53 checks with 26 fixtures.
- Fourteen further defects from the same round are repaired in the sections that define them, the substantive ones being the swap test for type-scan symmetry (superseding ADR-0052's shape whitelist, which excluded "at most one open X per Y"), the counting rule a self-excluding scan implies, and rounding for decimal products.
- `docs/design/case-study-approvals-and-bookings.md` is corrected. It carried the `id` exclusion that ADR-0061 made an error, and it is the example ADR-0052's symmetry rule was derived from. That is the failure this ADR is about, in a sibling document.
- Cascade from-state coverage is defined as a check and not implemented. *(ADR-0063 then demoted it to a publish report, its condition having turned out undecidable as written, so the check list ends at 52 and this item has no number.)*
