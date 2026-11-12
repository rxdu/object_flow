# ADR-0257: A text declared `not_blank` is never written blank

- **Status:** **Decided** 2026-11-04 under the author's direction ("check the latest issues and address all of them properly"), on the recommendation formed the same day for issue #69, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-04
- **Refined by:** ADR-0274 — step 5 leaves out the invariant `not_blank` generates where no other invariant reads its text.
- **Refines:** ADR-0249 (`trim`), which refuses a blank text only through a guard on each act
- **Relates to:** PRD F2; ADR-0176 decision 1 (a marking generating an invariant); ADR-0146 (a publish leaves no violation nobody admitted); ADR-0161 (a label's name empty once normalised refused); [`declaring-a-text-never-blank.md`](../research/declaring-a-text-never-blank.md); issue #69

## Context

Issue #69, filed 2026-11-04 at 0.16.0, found that a required text is refused only when it is absent. Reproduced 2026-11-04 on the published 0.16.0 (`TODO.md`, #69), on a fresh SQLite store: `Note.write`, requiring `text`, an attribute `{ type: string }`, took `""` and `"   "`, and `Note.close`, declaring an input `reason: { type: string }`, took both too; `write` without `text` was refused `invalid_input`.

Since ADR-0249 a guard `trim(inputs.reason) != ""` refuses such a text, but only on the act that names it, and each new act taking the text must repeat it. The engine already refuses its own blank text: a label's name empty once normalised is refused `invalid_input`, `malformed` (ADR-0161).

Proven platforms declare this rule once, on the field, and refuse the value rather than change it ([`declaring-a-text-never-blank.md`](../research/declaring-a-text-never-blank.md)). Jakarta Bean Validation's `@NotBlank` refuses a value that is null or holds no character that is not white space, and Rails' `presence` a string *"that is either empty or consists of whitespace"*. Django's forms and Pydantic strip the value first, which ADR-0249 decision 2 rejected: a request records what was sent.

## Decision

1. **An attribute or a transition's declared input of type `string` may be marked `not_blank: true`.** A text empty once trimmed, as `trim` reads white space (ADR-0249), is never written.
2. **An input so marked, or one taking the name of an attribute so marked, given a blank text is refused `invalid_input`, the input named, its reason `malformed`**, as a label's name empty once normalised is (ADR-0161). An optional input left out is not blank.
3. **An attribute so marked holds the invariant `<attribute>_not_blank`**, `<attribute> is null or trim(<attribute>) != ""`, which the engine generates after the type's own. So every write is checked as any invariant is, an effect's, a call's, a move's, a backfill's and an import's among them. A publish over objects already holding a blank value resolves them as it resolves any new invariant's violations: fixed by a transition, given a value by a backfill, or admitted (ADR-0146). `may_admit` and `admit` name it as any invariant of the type. A type MUST NOT declare an invariant of that name (step 3, `names`).
4. **What is written stays as it was sent**, as with a `trim` guard (ADR-0249 decision 2).
5. **In this release it marks a single `string` alone.** A set, a value's field, an observation's field and an evaluator's argument are refused, as is any other type (step 3, `names`).

## Alternatives rejected

- **`blank: false`, as the issue wrote it.** Every marking of an attribute or an input is a flag set `true` where it applies, its absence the default, as `optional`, `unique` and `personal` are. `blank` would be the one key meaningful only as `false`.
- **A condition and a guard on each act**, as now. It is correct, but the rule is the text's own, and a new act taking the text need not repeat it.
- **`min_length: 1`.** A length counts white space, as JSON Schema's `minLength` does, so `"   "` passes it.
- **Trimming the text as it is written.** It changes what a writer wrote (ADR-0249).
- **A reason of its own, `blank`.** The engine refuses its own blank text as `malformed` (ADR-0161), and a reason per rule would grow the wire's set with each rule.

## Consequences

- Built in the core's `model.rs` (the marking, and the library generating the invariant), `request.rs` (a request's inputs, after their defaults, at every depth, and a proposal's when it is filed) and its checker's `raw.rs` (`not_blank_errors`) and `view.rs` (`with_not_blank`), and in `scripts/check-flows.py` and `scripts/flowrun.py`; the schema, `flow-format.md` §3, §4.3, §4.8 and §6 and `declaration-syntax.md` §3.1 and §5.1 say so, the text language writing the marking `not blank`.
- **It refuses nothing 0.16.0 accepted** of a module that does not mark it.
- **Tested** on both backends (`a_text_marked_not_blank_is_never_written_blank`): `""`, `"   "` and a tab, a newline and U+3000 together refused `malformed`, as a required attribute's input, an optional one's and a transition's own; `" a "` and U+001C written as sent; an optional input left out taken; an effect writing a blank text refused `invariant_violated`, `memo_not_blank`, nothing written; a call passing a blank text and a default giving one refused as a request sending it is; and a proposal with a blank text refused when it is filed. A version marking a held text `not_blank` decides the object holding a blank one, and its change is not submitted (`a_version_marking_a_text_not_blank_decides_each_blank_one_held`). Both checkers agree on six plants: a string attribute and input marked, clean, and the marking refused on an `int`, a set of strings, an `int` input and an observation's field, and a type's own invariant of the generated name refused. The Flow Studio's test passes through both runners alike. Nine planted defects were each caught, five in the core and four in the reference runner: the request's refusal, the same refusal skipped in a call (the core's), the proposal's, the generated invariant and the checker's refusal.
- **The agents' tool schemas do not say it.** A string input marked `not_blank` is offered as any string is, and a blank one is refused when sent.
