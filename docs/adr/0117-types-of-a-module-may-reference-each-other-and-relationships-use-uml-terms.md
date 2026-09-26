# ADR-0117: Types of a module may reference each other; relationships use UML's terms

- **Status:** Accepted by the author, 2026-09-26: "go with (a) and the UML terms", choosing between two ways to write relationships under the rule that every name is defined before it is used.
- **Date:** 2026-09-26
- **Refines:** the author's rule that "names used at one place should have been defined before" (`flow-format.md` §3), ADR-0115 (established terms), ADR-0116 (the format replaces the text language)
- **Relates to:** ADR-0058 (every final transition of a whole disposes of its parts), `declaration-syntax.md` §3.2 and §3.3

## Context

The author asked that every name in a description be defined before it is used, and the format enforced it strictly, including for types: a type referenced by another type's attributes had to be declared first. Relationships, the third construct of ADR-0116's plan, made the rule impossible to keep. The model declares both ends of a relationship, each in its own type, so that a type reads completely on its own (`declaration-syntax.md` §3.3): a delivery names its checklist items, and each item names its delivery. Two related types therefore name each other, and whichever is declared first names one declared after it.

Two ways were put to the author: (a) the types of a module may reference each other in any order, and define-before-use holds for everything else; (b) relationships move to a section of their own after all types, as UML treats an association as an element of its own, which keeps the rule for types but breaks it one level down, since a type's conditions would read relationship ends declared further on.

## Decision

1. **The types of one module form a single group that may reference each other in any order**, as the types of a Protocol Buffers file, a GraphQL schema or an OpenAPI document do. A reference MUST name a type the module declares or imports. Every other name is still defined before it is used: the module's sections and a type's sections are in their order, and a name within a type, a state, an attribute, a condition, is declared before a transition uses it.
2. **A relationship is written in UML's terms**, each end on its own type:
   - an end names the other type's end with `opposite` (UML `Property::opposite`), in place of the model's `inverse`;
   - of two single ends, `stored: true` marks the one that holds the value; otherwise the single end of a pair stores it, as `declaration-syntax.md` §3.3 fixes;
   - a whole's end is marked `aggregation: composite` (UML `AggregationKind::composite`), in place of the model's `part`; the part's end is a single, required reference whose opposite is that end, which converts to the model's `owner`;
   - on a composite end, `cascade` lists the transitions each part takes when the whole takes one of the listed transitions, with its inputs and a limit, and `survives` lists the transitions the parts outlive, or is `true`. Cascade is the term of SQL's `ON DELETE CASCADE` and of object-relational mappers, since UML has none.
3. **A transition that only other transitions may take is marked `only_via`**, the model's own term, since no state-machine standard names it: creating a part is only via its whole (the model's check 11).
4. **The format checks what the model requires of these forms**: an opposite names this end back, a part's end is single and required, a cascade names a transition of the part and passes its inputs, and every transition of a whole into a final state is either cascaded on or survived, never both, which is the model's check 37 and which the internal checker does not implement.

## Alternatives rejected

- **(b) Relationships in a section after all types.** It keeps define-before-use for types, but a type's conditions then read ends declared later, and a reader of a type no longer sees its relationships with it.
- **Separate modules for types that reference each other.** Two modules that import each other are the same cycle one level up.
- **Keep the model's `inverse`, `part` and `owner`.** ADR-0115 adopts established terms where they exist, and UML's `opposite` and `aggregation: composite` name exactly these.

## Consequences

- `flow-format.md` §3 states the rule for types, §4.3 the relationship keys, and §4.8 `only_via`; open question 4 of §9 is answered.
- The ordering check between types is removed from `scripts/check-flows.py`, and a reference to an undeclared type is refused by name instead.
- The `delivery.yaml` example has a composition with a cascade and a survived transition, and a relationship with a stored single end and a derived set end.
