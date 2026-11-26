# ADR-0294: A creation refused for an entry names the fields its rule reads

- **Status:** **Decided** 2026-11-26, accepted in advance under the author's direction of 2026-09-28 as a confident recommendation, reviewing issue #115 at the author's word ("review open issues and address them properly"). Built the same day.
- **Date:** 2026-11-26
- **Refines:** ADR-0224 decision 2 (a guard of an object the request reached names no input); ADR-0290 decision 6 (a creation refused for an entry is `call_refused` with `entry`)
- **Relates to:** issue #115; ADR-0291 decision 5; ADR-0214 (a release before 1.0.0 may change `/v1`); `docs/guides/errors.md`, `call_refused`; `docs/guides/integrate.md`, Refusals

## Context

Issue #115, filed 2026-11-26 at 0.32.2, reported that a creation a `create`'s `each` makes, refused by a guard of the transition it creates by, names no input. Its `call_refused` carries `entry`, and the call's own verdict answers `inputs: []`. The same guard refusing the same transition requested directly names the inputs it reads (ADR-0224). So a form marks the entry at fault, but not which of its fields.

ADR-0224 decision 2 names no input for a guard of an object the request reached beyond the one it names, since that object's inputs "are no field of the caller's form". An entry's fields are the caller's: the request gave them, inside its list (ADR-0290 decision 2). ADR-0290 came after ADR-0224 and left the case open.

An entry wrong before any rule runs is already marked field by field: `invalid_input`'s `entries` gives each fault as `{entry, input, reason}` (ADR-0290 decision 6). Rails' nested attributes and Django's formsets also address an error in a nested record by the record's place and the field.

## Decision

1. **A `call_refused` naming an `entry`, refused by a guard of the transition the entry's object is created by, names in its `verdict`'s `inputs` each field of the entry that guard reads.** They come in the order that transition declares them, as ADR-0224 decision 1 orders a direct request's. A generated `<attribute>_provided` names its attribute's field. The call's verdict is the target transition's own, its clause qualified by the target's type, so its `inputs` names that transition's inputs, and `entry` says where the caller gave them.
2. **An input the step names is never named**, since no entry holds it: an `item: this` the step passes alike to every entry. Nor is an observation kind's `subject` or `corrects`.
3. **Every other refusal of a call or a creation names none, as ADR-0224 decision 2 says.** That covers a guard of an object the entry's object reaches in turn, a creation no `each` made, and a call.
4. **It is computed from the declaration**, the same for every entry, on a request, a `check` and a batch's refused request alike.
5. **It changes what a field answers in one case**, which a release before 1.0.0 may (ADR-0214), and the changelog says so. A client marking the call's `inputs` as top-level fields, ignoring `entry`, would mark the wrong field. Before, that field there was always empty, so such a client marked nothing.

## Alternatives rejected

- **A field of its own beside `entry` naming the fields**, the issue's second form. The verdict already says which of its transition's inputs its clause reads. A second field would say it again elsewhere, and the call's `inputs` would stay empty where it has an answer.
- **A path in the outer refusal, such as `lines[1].quantity`.** ADR-0290 rejected a path in `invalid_input`'s `inputs`, since it changes what a name there means.
- **Naming the step's inputs too.** They are no field of the entry, and the caller gave none of them.
- **Naming the entry's fields a deeper guard reads**, of an object the entry's object creates or calls. That guard reads its own transition's inputs, which the entry's object's effect computes, not fields the caller gave.

## Consequences

- `request.rs`: `Refusal::entry_inputs`, set by `entry_inputs_read` where an `each`'s creation is refused, and told by `told`. `model.rs`: `entry_holds`, the one rule for which inputs an entry holds, which the load's expansion of `each` reads too.
- The reference runner records the transition an entry's object is created by and the fields the entry holds (`flowrun.py`, `call_refused`). The Flow Studio's sandbox names the same fields from them.
- Docs: `errors.md`'s `call_refused` row, `integrate.md`'s Refusals, `library-api.md` §4 and the changelog.
- Tests: the core's `a_creation_refused_for_an_entry_names_the_fields_its_rule_reads` covers two fields in declared order, a field beside a step's input, the step's input alone, a deeper guard, a creation no `each` made, `check` and a batch. The Studio's `test_a_refused_entry_names_the_fields_its_rule_reads_alike` holds both backends alike. Two planted defects in the core and one in the sandbox were each caught.
- **Not covered**, each recorded in `TODO.md`. A creation taking `rest: inputs` passes the caller's own top-level inputs on by name, and a guard of it reading one still names none (ADR-0224 decision 2). Two `each` steps of one transition creating by the same target are told apart by no field of the refusal, since `step` names the target and not the list.
