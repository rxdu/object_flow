# ADR-0239: A file attribute or input says which media types it takes, and an expression reads a file's media type and size

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #44; built the same day.
- **Date:** 2026-11-01
- **Relates to:** PRD F2 and F11; ADR-0173 decision 11 (a file's wire form); ADR-0205 decision 1 (a value written is fitted to its attribute); ADR-0146 (a publish's severities); `DESIGN.md` §5.2 and §5.7; `docs/research/file-media-types.md`

## Context

A `file` attribute or input holds a hash, a size and the media type its caller states (`DESIGN.md` §5.2). Nothing in a flow can say which media types it takes, and no expression reads a file's members: a guard reading `f.media_type` is not run (`eval.rs`, `member`). Issue #44, filed 2026-10-31 at 0.8.0, has the case: an attachment that is to be a photo or a PDF, never a video. The rule then lives in a consumer's own API, which binds no import, no service actor and no other application writing the record, and which no form can read from the declaration.

HTML's `accept` and Camunda's file picker hint such a list to a form; OpenAPI describes one, with ranges such as `image/*`; Microsoft Dataverse enforces one on the server, naming the refused type; OWASP warns that the type a client states is its claim, a quick check and no security control (`docs/research/file-media-types.md`, each quotation found in its source's raw page). The format already narrows a type in its declaration where the type has a parameter: `money(USD)` refuses another currency, `wrong_currency`.

## Decision

1. **`media_types` on a `file` attribute or input.** An attribute of type `file` or `file[]`, a type's or an observation kind's, and a declared input of either type, may declare `media_types`: a list of at least one media type, each `type/subtype` or a range of one type, `type/*`, written in lower case, with no parameters, and none twice (step 2, `schema`). On any other type it is refused (step 3, `names`). A file is taken where its media type is one listed, or has a listed range's type. A set takes each element by the list. Without the key, a file of any media type is taken, as before.
2. **Every write is held to it.**
   - An input of a media type its list does not take is refused `invalid_input`, the input's reason `wrong_media_type`, remedy `self_serviceable`. An attribute input is held to its attribute's list, a declared input to its own. A proposal's inputs are held as a request's are, and so are those of a transition an effect causes, by a `call`, a `create` or a cascade.
   - An effect's write, `assign` or `add`, of a file its attribute does not take is refused `value_in_range`, as a value its attribute cannot hold (ADR-0205 decision 1). `remove` is not held to the list, since a set may hold a file from before its list narrowed.
   - An import's value of a media type its attribute does not take is a value not of its type, the class the import already gives one.
   - A migration's backfill, or a value a move's entry gives, of such a file is fatal, as a value its attribute cannot hold. A value a move carries from the old attribute is held, not written, so decision 5 keeps it.
   - A pending proposal holding such a file is one the version invalidates, `proposals_invalidated`, as one holding a value its type no longer holds is (`DESIGN.md` §9).
3. **`declaration` answers each attribute's and declared input's `media_types`**, so a form gives HTML's `accept` the same list.
4. **An expression reads a file's `media_type`, a string, and `size`, an integer of bytes**, so a guard can bound a size or take a media type in some states only. Both checkers type them, and refuse any other member of a file, as a member of a string is refused (step 3, `names`).
5. **A version narrowing a list keeps every value held.** The list binds writes, so a transition that writes nothing to the attribute leaves its value as it is. The version's report notices `media_types_narrowed:<Type>.<attribute>`, naming each object holding a file the new list does not take, where the list is added or no longer takes a media type the installed one took.
6. **What it checks is the type the caller states.** The engine never holds the bytes, so it cannot tell a file's content; the application's blob store, which hashes them, checks that the content is of the type stated (`integrate.md`, `SECURITY.md`).

## Alternatives rejected

- **The list as a parameter of the type, `file(image/*)`, as `money(USD)` is.** A change to the list would then be a change of type, and a version changing a type sets its held values aside (`type_changed`).
- **A guard alone.** The language has no string operation beyond equality, membership and `length` (`DESIGN.md` §5.7), so a guard can list exact media types but cannot state `image/*`, and a form cannot read a guard as a list.
- **A version narrowing a list refused, or decided, over each value held.** The files an application already keeps would become a migration's, and the list binds what is written, as HTML's and Dataverse's do.
- **A declared size bound**, which Dataverse sets per column. A guard reading `size` bounds it; a declared bound waits for a case where a form needs to read it.
- **The engine reading the bytes.** It never holds them (`DESIGN.md` §5.2).

## Consequences

- Built in the core (`model.rs`, `request.rs`, `eval.rs`, `import.rs`, `publish.rs`, `check/names.rs`, `verdict.rs`) and in the reference, the checker `scripts/check-flows.py`, the runner `scripts/flowrun.py`, the publish `scripts/flowpublish.py` and the Studio's sandbox, with `flow.schema.json`. `flow-format.md` §4.3, §4.8, §5 and §10, the card, `errors.md`, `wire.md`, `integrate.md`, `DESIGN.md` §5.2, `SECURITY.md` and `CHANGELOG.md` say so.
- **The two checkers agree** on all 682 of the oracle's cases, the new plants among them (`checker_oracle.rs`). The new schema plants found the core's `repr` writing a pattern's backslash once where Python's writes it twice, so a pattern holding one was worded otherwise; it escapes a backslash and the control characters as Python does now.
- **Tested in the core:** `a_file_input_of_a_media_type_its_list_does_not_take_is_refused`, a request's, a set's and a proposal's inputs, a range and the wire's lower case; `an_effect_writes_no_file_its_attribute_does_not_take`, `assign`, `add` and `remove`; `a_caused_transitions_file_inputs_are_held_to_their_lists`, a `create` and a `call`; `an_expression_reads_a_files_media_type_and_size`; `an_imported_file_of_a_media_type_its_attribute_does_not_take_is_not_of_its_type`; `a_narrowed_list_of_media_types_keeps_what_is_held_and_names_it`; `a_backfill_writing_a_file_its_list_does_not_take_is_fatal`; `a_proposal_holding_a_file_its_input_no_longer_takes_is_invalidated`; and through the service, `a_files_media_types_are_declared_and_held_through_the_wire`, the declaration's lists and the wire's `wrong_media_type`. In the reference, the checker's `media_type_plants` and `expression_type_plants`, the runner's `media_self_test`, examples run through step 6, and the Studio's `test_media_types.py`.
- **Planted defects, each caught:** thirteen in the core, one at each check of decisions 1, 2, 4 and 5, and twenty-two in the reference.
- **Not done, and why.** An agent's generated tools describe a file without its list, which `declaration` answers. A version that renames an attribute and narrows its list notices nothing under the new name, as `type_changed` compares attributes by name. Step 5 does not reason over a file's members, which it leaves undecided. The text language, retired by ADR-0116, has no form for the list, so converting a module to it drops the list.
- Building this found a defect that predates it, recorded in `TODO.md` and fixed on its own: a backfill that fits its first object hid a later object's value it cannot hold.
