# ADR-0170: A cascaded part is of a concrete type

- **Status:** Decided by the author, 2026-10-08 ("go with your recommendation on check 30"). The question had been put without a recommendation; the recommendation below was formed when the author asked for it, and stated with the decision.
- **Date:** 2026-10-08
- **Refines:** none; it states what `design/flow-format.md` §4.14 left to follow from an abstract type having no transitions.
- **Relates to:** `design/declaration-syntax.md` §2, §3.2, §10 check 30; `design/flow-format.md` §4.3, §4.14, §7; ADR-0133 (a part declared on a base)

## Context

Check 30 refuses "a `cascade` naming `<Type>.<transition>` where that type is `abstract` and some member of its family does not have that transition, so the cascade would reach a part it cannot drive" (`declaration-syntax.md` §10). The inventory of 2026-10-05 listed it among the checks neither checker enforces.

Running a module whose whole cascades to parts of an abstract type, every member of whose family declares the transition, showed the flow checker refusing it in step 3: "Rack.slots cascades to Slot.drop, which is not a transition of Slot that acts on a part" (observed 2026-10-08, `check-flows.py` on a scratch module). An abstract type declares no transitions (`flow-format.md` §4.14), so a cascade naming one names none, whatever its family declares, and the core's checker words the same finding. The format therefore refuses every cascade to an abstract part type, more than check 30 does, and nothing in the format said so: it followed from a base having no transitions.

ADR-0133 settled the other half of a family and its parts, a part declared on an abstract whole, which each concrete subtype cascades in `inherited_parts`. No module of the design, and nothing of the first customer, declares a composite end whose part type is abstract.

## Decision

1. **A cascade's part type is concrete.** A composite end may name an abstract type, so a whole holds parts of several kinds, but such an end carries no `cascade`: a cascade names a transition of the part's type, and an abstract type has none. The end may `survive` its whole's final transitions, as any end may. `flow-format.md` §4.14 says so, and §7's step 3 refuses it under `names`, as it already did.

2. **Check 30 is subsumed in the flow format.** What check 30 refuses, a cascade to an abstract type some member of whose family cannot take, is a case of decision 1, which refuses every cascade to an abstract type. A case of the flow checker's self-test holds it, a whole cascading to an abstract part type every member of whose family declares the transition, refused with that finding, and the core's oracle replays it.

## Alternatives rejected

- **A cascade to an abstract part type, where every member of its family declares the transition**, check 30's own rule. It is sound, but it needs a meaning per member that nothing asks for yet: the inputs a cascade passes, which each member's transition must take, coverage of each member's final transitions, and the runtime's choice of transition by each part's type, in both checkers and both runtimes. Nothing in the design needs a family as a cascaded part, and allowing it later relaxes a refusal, so every module accepted now stays accepted.

## Consequences

- `flow-format.md` §4.14 states decision 1; `declaration-syntax.md` §10 check 30 notes it is subsumed in the flow format.
- `TODO.md`'s inventory of checks neither checker enforces drops check 30.
- Should a module need a family as a cascaded part, check 30's rule is the alternative to revisit, with its per-member meaning stated first.
