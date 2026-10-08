# ADR-0169: A mirror may declare a minted identifier

- **Status:** Decided by the author, 2026-10-08 ("go with your recommendation on check 44"). The question had been put without a recommendation; the recommendation below was formed when the author asked for it, and stated with the decision.
- **Date:** 2026-10-08
- **Refines:** none
- **Relates to:** ADR-0168 (which recorded the question); `design/declaration-syntax.md` §10 check 44; `design/flow-format.md` §4.12, §4.15; `design/publish-and-import.md` §4 (sequences), §7 (cutover)

## Context

Building ADR-0168, which declares every type of the scenarios' port a mirror, found that check 44 refuses a minted identifier on a mirror. The check asks of a scope that it name "a reference or attribute that any one of the type's creations does not write", and of a format placeholder that it not reach "through a reference or attribute any creation does not write". Both checkers compute what every creation writes as the intersection of what each creation writes, and over a type with no creation answer nothing rather than everything, so every scope and every placeholder fails on a mirror, which has no creation. An abstract type, which has none either, is exempted by name; a mirror was not.

A minted identifier is minted when an object is created (`flow-format.md` §4.12), and a mirror's objects are never created here: the import writes them, with the values the legacy record gives. While a type is a mirror its identifier is inert. The type keeps it across the cutover, `publish-and-import.md` §7 having the cutover remove the marking and declare the transitions, and the import's sequences raise each sequence past the last value the legacy system minted, so the first identifier minted after the cutover follows the last one ported (§4).

## Decision

1. **A mirror may declare a minted identifier.** While the type is a mirror the import gives its value and nothing mints it; the identifier is minted once the cutover gives the type its creations. `flow-format.md` §4.12 says so.

2. **Check 44's clauses on what every creation writes ask nothing of a type with no creation**, as its text reads: no creation of a mirror fails to write what a scope or a placeholder names. Both checkers decide the two clauses only for a type with a creation, and the rest of check 44, a declared sequence, a format's number, the attribute's type, a scope's indexing and a placeholder's form, still applies to a mirror. `declaration-syntax.md` §10 check 44 says so. The defect is D544.

## Alternatives rejected

- **A mirror may not declare a minted identifier**, a new refusal. The cutover would then change the attribute's declaration as well as removing the marking, a version advance §7 keeps to the marking and the transitions, and the refusal would forbid a declaration that does nothing wrong.
- **What every creation writes made everything over no creation**, in `creation_writes` itself. Check 8 reads the same set, and what it asks of a type with no creation is a separate question; the change is kept to check 44, whose text decides it.

## Consequences

- The flow checker and the core's checker accept a mirror whose minted identifier is scoped and formatted by what it holds, held by a case of the flow checker's self-test the core's oracle compares, and still refuse a type with creations that does not write its identifier's scope.
- `TODO.md`'s question of check 44 is decided and done. The scenarios' version 1 keeps declaring no minted identifier, as ADR-0168 decided, nothing in the scenarios needing one.
