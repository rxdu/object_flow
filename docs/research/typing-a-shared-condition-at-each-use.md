# Typing a shared condition at each use

How a type checker types a body declared once whose names are bound only where it is used, as C++'s two-phase lookup binds a template's dependent names at each instantiation, and how it checks a built-in function's arguments before it runs, as CEL's checker resolves a function's overloads ahead of runtime. Asked 2026-11-16 at the author's word ("go with your recommendations", on taking the five findings where a flow passes the checks and then misbehaves when it runs, and putting how to address each).

## The question

A condition is declared once on a type and guards any number of its transitions. What it reads of a transition's `inputs` has a type only once the transition is known. Both checkers type a condition once, with no transition, so an input's type is unknown there and check 24 refuses nothing the condition reads of it (`TODO.md`, the findings of 2026-11-01 and 2026-11-15). Separately, neither checker types the argument of the built-in `date`, so `date` of a text passes the checks and fails its run (`TODO.md`, the finding of 2026-11-14).

## Findings

### C++ templates: two-phase name lookup

A template's body is declared once and used with many arguments. cppreference's *Dependent names* page splits the names a body reads by whether they depend on the template's parameters, and checks them at different points:

- "Non-dependent names are looked up and bound at the point of template definition."
- "Binding of dependent names is postponed until lookup takes place." Members of an unknown specialization "are always dependent, and are looked up and bound at the point of instantiation as all dependent names".

So a name that does not depend on the use is checked once, where the body is written. A name that does is checked at each use, against what that use supplies.

### CEL: overloads resolved ahead of runtime

CEL's language definition specifies each function as "a set of overloads. Each overload defines the number and type of arguments and the type of the result". At runtime, "If no overload matches, the runtime error `no_matching_overload` is raised". Its type checker "attempts to identify possible runtime errors …, particularly `no_matching_overload` and `no_such_field`, ahead of runtime". An expression without CEL's dynamic features "can be fully statically type-checked and all overloads can be resolved ahead of runtime".

## ObjectFlow beside it

Observed 2026-11-16 at `87a6ae8`, on both checkers (the core's through `/sandbox/load`) and the core's release build:

- **A condition's inputs.** A condition `inputs.count == "three"`, `count` an `int` input of the one transition guarding with it, loads clean, and the core refuses every request through it. `inputs.count <= this.quantity`, a `decimal(9,2)`, loads clean and is compared by value, where `this.delivered <= this.quantity`, an `int` attribute, is refused, "it compares an int with a decimal of scale 2". `inputs.snap.hash`, `snap` a `file` input, loads clean and the core answers `501 not_run`, "a member `hash` of a file", where `photo.hash`, a `file` attribute, is refused, "it reads hash of a file, which has only media_type and size".
- **Where each checker reads a condition.** Each already reads a condition with each transition guarding with it for the input's name: a condition reading `inputs.nothing` is refused, "deliver takes no input 'nothing'" (`check-flows.py:2882`, `check/names.rs:353`). Its typing alone is done once, with no transition (`check-flows.py:3974`), where an effect is typed with its transition.
- **`date(…)`'s argument.** `date(title)`, `date(this.title)`, `date(5)` and `date(true)` each load clean on both checkers, where `trim(5)` is refused, "trim takes a string, and is given a number". Neither checker types `date`'s argument (`check-flows.py:3590`, `check/names.rs:4056`). Run, the core answers `date` of a text `501 not_run`, "the date of a text". The reference runner raises `TypeError` on it, and reads `date(5)` as five seconds past the epoch (`flowrun.py:1512`).

## What it suggests

This is a recommendation for the author, not a decision.

- **Type a condition that reads `inputs` against each transition guarding with it**, and one that reads none once, as now. A condition is a body declared once whose `inputs` are the names that depend on its use, as a template's dependent names do (inferred). This follows two-phase lookup, and needs no declaration of the condition's inputs beside the transition's own.
- **Give `date` two overloads, a timestamp and a date**, both runners' only cases, and refuse any other argument at step 3. That is what CEL's checker does for `no_matching_overload`, and what both checkers already do for `trim` and `add_months`.

## Sources

| Source | URL | Retrieved | Version |
|---|---|---|---|
| cppreference, *Dependent names* | https://en.cppreference.com/w/cpp/language/dependent_name | 2026-11-16 | the page as served; it gives no date |
| CEL language definition, `google/cel-spec` `doc/langdef.md` | https://raw.githubusercontent.com/google/cel-spec/master/doc/langdef.md | 2026-11-16 | branch `master`, last changed by commit `21ec6f4e52e8` (2026-10-27) |
