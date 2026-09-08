# ADR-0059: Both ends of a relationship are declared, and cardinality decides which one stores the value

- **Status:** Accepted — repair of the iteration-9 syntax review, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0060 — twelve decisions from the iteration-11 syntax review.
- **Refines:** ADR-0045, ADR-0058

## Context

ADR-0044 settled that a relationship is physically held on one end and that the other end is a derived view resolved as a query, and gave the reason: making the runtime write both ends created a second write path whose events carried no transition name, could not be filtered by a subscription, left `changes_state` undefined, and stamped the far object's last-written index so that binding a unit invalidated every approval on its delivery.

What it never settled is **which** end holds it, and the declaration syntax inherited the gap. Every example declared both ends — `Robot.binding : Delivery? inverse units` in one type and `Delivery.units : Robot[] inverse binding` in the other — with nothing in the text distinguishing the stored end from the view. Check 17 already rejected "a write whose target is not an attribute or **stored relationship end** of `this`", so the checks depended on a term the grammar never defined. A declaration author reading section 3 could not tell which of the two ends they were allowed to write, and the checker had no pair-level rule for `ref` at all, so a pair that could not be stored anywhere published cleanly.

The question is not academic. It decides what `set` is legal, which end a migration has to carry data on, and which end a storage backend indexes.

## Decision

Both ends stay declared, each in its own type, so a type reads completely on its own without the reader having to find the far declaration. Exactly one end stores the value, and which one is **derived from the declaration** rather than marked:

| The pair | The stored end |
|---|---|
| `part` / `owner` | the `owner`. The child holds the parent, which is what makes re-parenting one write (ADR-0058) |
| `ref` with one singular end and one set end | the singular end |
| `ref` with two singular ends | ambiguous; exactly one must be marked `stored` |
| `ref` with two set ends | neither end can hold it; publishing rejects the pair, and the author declares a type for the association |

Only the stored end is writable. Check 17 rejects a write naming a derived end. Check 41 rejects a pair with no stored end or two, and rejects an `inverse` the far type does not declare or whose far end names a different near end.

The rule is the relational one — the value sits on the side that can hold exactly one — so it matches what an author who has ever normalised a schema already expects, and it needs no marking in the common case. The explicit `stored` marking exists only where cardinality genuinely cannot decide.

## Alternatives rejected

- **Declare only the stored end and let `inverse` create the far view implicitly.** This is the smallest grammar and removes the possibility of a disagreeing pair entirely. Rejected because a reader of `type Delivery` would see `for u in units` in a transition body with no declaration of `units` anywhere in the type; the name would arrive from a different type's text. The document's own principle is that a declaration is read by people who did not write it, and a name appearing from nowhere is the opposite of that.
- **Require an explicit `stored` marking on every pair.** Rejected because it is a mandatory annotation whose value is deducible from the two lines it sits between, which makes it something to forget and something to get wrong. It survives only for the one-to-one case, where nothing else can decide.
- **Let either end be written and have the store choose a representation.** Rejected for the reason ADR-0044 already gave: it reintroduces the far-side write, and the choice would not be visible in the declaration, so two backends could disagree about which writes are legal.
- **Allow set-to-set pairs and materialise a join table.** Rejected because the association is nearly always a thing with its own state — an assignment, a membership, a booking, all of which acquire a start date, a status or an approver within a release or two — and an implicit join table has no place to put them. Forcing the author to declare the type makes the eventual attribute an edit rather than a migration.

## Consequences

- Section 3.3 of `docs/design/declaration-syntax.md` states the rule and the syntax document's grammar gains a `stored` marking on `ref`.
- Check 41 is new. It is implemented in `scripts/check-syntax-doc.py`, and all three of its clauses were confirmed by injecting each into the document's own examples and observing the finding at the right line.
- A one-to-one relationship is now slightly more expensive to declare than any other shape. That is deliberate: it is the shape where an author most often means a composition instead.
- Storage will index the stored end. Nothing in the design yet says whether the derived end gets a covering index, which is a storage-schema question and is recorded as such in `TODO.md`.
