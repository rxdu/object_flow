# ADR-0224: An unsatisfied clause names the inputs it reads

- **Status:** **Decided** 2026-10-20 under the author's direction ("go with your recommendation", on taking issue #28's proposed design into 0.5.0, a field named `inputs` in the transition's declared order).
- **Date:** 2026-10-20
- **Relates to:** issue #28; ADR-0173 decision 9 (an `invalid_input` names each input and its reason); `library-api.md` §4, `Unsatisfied`; `errors.md`; `integrate.md`, Refusals

## Context

Issue #28 reported that a guard's refusal names its `clause` but not the inputs that clause read, while an `invalid_input` names each input at fault, so a form marks a malformed day on its field and a day out of order nowhere. To mark it, a client either parses the condition's expression, re-implementing the flow language outside the engine, or keeps its own table from clause to field, a copy of the flow's rules held beside the flow. The one mapping the guides offered was a naming convention, `<attribute>_provided` read back to its input. The engine already collects each `inputs.<name>` an expression reads (`Expr::inputs_read`), for the taint of what an input writes.

## Decision

1. **An `unsatisfied` verdict answers `inputs`**: each input its clause reads, in the order its transition declares them, required inputs first, then optional ones, then those declared with a type alone. It is computed from the declaration, so it is the same for every request, on `request` and `check` alike.
2. **What names inputs**: a guard of the requested transition names each declared input its condition's expression reads; a generated `<attribute>_provided` names its attribute's input. Anything else names none: a clause of the engine's own over the actor, the object or the store (`actor_known`, `whole_open`), a condition that reads no input, and a guard of an object the request reached beyond the one it names (a creation's or a call's), whose inputs are no field of the caller's form. An empty list is information: what refused is not on the form.
3. **An offer's verdict answers `inputs` empty**: `available` and `availability` decide only what reads no input, what reads one waiting for it and making the offer `available_with_input` (`DESIGN.md`, the `availability` row), so no clause they refuse on reads one.
4. **It is an added answer field**, which `/v1` allows within a release (`wire.md`, What `/v1` promises), and lands in 0.5.0. The `<attribute>_provided` clause keeps its name; a client no longer needs to read it.

## Alternatives rejected

- **`declaration` answering each condition's inputs**, the verdict unchanged: it would also let a form hint before any request, but every client joins clause to inputs itself, and the refusal still says nothing. It can be added beside this if a client asks for the hint.
- **An author-declared `marks: [last_day]` on a condition**, for a rule that reads two inputs but is about one: the most precise for a form, but one more declaration to keep in step with the expression, for a need no consumer has stated. A later step could let it override the computed set where given.
- **The order the expression reads them in, or their names' order**: neither is anything a form shows, and the expression's changes with a rewording of the rule; the transition's declared order is the one its author chose.
- **Naming an undeclared input a condition reads**: no form has a field for it.

## Consequences

- `verdict.rs`: `Told::inputs`. `request.rs`: `clause_inputs`, and `told` computing it for an unsatisfied verdict. `reads.rs`: an offer's is empty. The service writes `inputs` beside `proposable`, and lists it in `not_computed` where what a refusal tells is not computed. The Flow Studio's runner backend (`sandbox.py`, `clause_inputs`) answers the same from the reference runner's `inputs_read`, as the studio's tests comparing it with the core require.
- Docs: `errors.md` (the `unsatisfied` row and the `<attribute>_provided` paragraph), `integrate.md` (Refusals), `quickstart.md`'s two refusals, `library-api.md` §4 and `renderers.md`'s offer.
- Tests: the core's `an_unsatisfied_clause_names_the_inputs_it_reads` (two inputs in declared order, declared unlike both their names' order and the expression's; one input beside a stored attribute; a generated `_provided`; a rule over the stored object; a clause the transition does not guard with), and the service's `an_unsatisfied_verdict_names_the_inputs_its_clause_reads` on `request` and `check`. Each fails a mutant: the names' order, any condition named, no `_provided`, the service answering none.
- Not covered: a `warn` guard's flag on a satisfied request names no inputs; added when a client asks for it.
