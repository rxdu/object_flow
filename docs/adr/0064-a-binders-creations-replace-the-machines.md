# ADR-0064: A binder's own creations replace the machine's

- **Status:** Accepted — repair of D159–D164, 2026-09-08; pending author review
- **Refines:** ADR-0003, ADR-0026, ADR-0060, ADR-0062
- **Date:** 2026-09-08

## Context

A type that binds a shared machine may declare its own transitions, and ADR-0026 made them additional: the machine's transitions are the shared lifecycle, the type's are what only that type does.

That is right for every transition except a creation, and the syntax document had been quietly relying on it not being. §2.1 offered three remedies for a binder that needs a required attribute, non-optional reference or singular part which a machine-supplied creation cannot provide — make it optional, give it a `default`, or declare its own `create` — and check 8 required every creation of the type to write or fill it. The third remedy cannot satisfy the check: declaring a creation does not remove the machine's, so a card payment obliged to hold its card details from birth could still be created without them, by exactly the transition the binder was trying to replace.

A reviewer modelling a payments ledger found it by probing rather than reading, on the fourth consecutive iteration in which that clause moved. The document's own named example had no legal spelling.

## Decision

**A binder's own `create` transitions replace the machine's creations for that binder. Every other transition it declares is additional.**

Birth is where a type's obligations are established: its required attributes written, its required parts filled, its identifiers minted. A type that says how it is born says so completely, and a generic creation inherited alongside a specific one is a hole in whatever the specific one was added to guarantee.

Check 34, which requires a non-abstract type to have a creation, counts a machine-supplied one only where the binder declares none of its own. Check 8 checks the creations that remain.

## Alternatives rejected

- **Strike the third remedy**, leaving "optional or `default`". This was the other coherent reading, and it means a required part cannot be declared on a binder of a creating machine at all. Rejected because that is a real modelling need — two payment types sharing a lifecycle where one must hold its card details from birth — and because it would have required deleting the example §2.1 offers for the case rather than making the example work.
- **A `suppresses` marking** naming machine transitions the binder does not offer. Rejected as more general than the evidence supports: creation is the only transition where the additive rule actually bites, because it is the only one that establishes obligations rather than changing state. A general suppression mechanism can be added later if a second case appears.
- **Let the machine's creation fill the part.** Not possible: the part's type is the binder's, and a machine that knew it would not be shared.

## Consequences

- `scripts/check-syntax-doc.py` drops machine-supplied creations for a binder that declares its own, and check 8's part clause is implemented per creation. It was specified in iteration 14, tightened in 15, and unimplemented until now — the reviewer's probe passed for four rounds while the text said it should fail.
- Check 52's citation is now scoped to the **sentence** rather than a 260-character window. The window gave a demonstrated false pass, and the citation shielding it named a different check than the enforcing one, so a reader following it was sent to the wrong place. Sentence scope immediately found four more uncited statements.
- Check 33, widened in iteration 15 to satisfy a citation, came out forbidding more than §9.2 does. That is the rule-and-check divergence class occurring **inside the repair built for it**, and it is invisible to check 52 in both directions, since an overreaching check and a wrong citation both pass a presence test. ADR-0062's alternatives are updated: the reverse link is still worth adding, but not because it would catch this — nothing mechanical catches a contradiction. It is worth adding because it puts the two texts where one person can compare them, which is what has caught all five instances.
