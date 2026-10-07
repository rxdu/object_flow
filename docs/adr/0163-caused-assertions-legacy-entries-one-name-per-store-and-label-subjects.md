# ADR-0163: No caused assertion, legacy entries in a batch, one name per store, and a label's subject

- **Status:** Decided by the author, 2026-10-07 ("go with your recommendations on 1 to 5"), on recommendations stated with the questions (`TODO.md`, D529).
- **Date:** 2026-10-07
- **Refines:** ADR-0040 (assertion), ADR-0078 (erasure reaches legacy history), ADR-0146 and ADR-0147 (the import's classes), ADR-0154 (the import's interface)
- **Relates to:** `design/flow-format.md` §4.8, §5; `design/publish-and-import.md` §4, §6; `design/library-api.md` §5; `design/storage-schema.md` §6, §9; `design/defects.md` D529

## Context

Four questions building the core raised, each searched for in the record first.

- **A caused assertion.** `flow-format.md` §4.8 lets a `call` take any of its target's transitions but an initial one, so it may name an assertion; the flow checker refuses `to` among the call's inputs, which an assertion's state is, and accepts the call without it, so every caused assertion is refused at run time on both backends.
- **Legacy entries.** `library-api.md` §5 gives an imported object's `legacy_entries` only as mappings of "only this object's fields". ADR-0078 puts in the import mapping the fields kept per entry kind through an erasure, though neither `ImportBatch` nor `of_legacy_entry` carries a kind or a list; and the dry run's `FIELD` class (ADR-0146, ADR-0147) presumes that an entry marks a field about another object, which no shape says how.
- **Two modules naming one type** (D529). `people.yaml` and `servicedesk.yaml` each declare `Service`, the format forbidding only a name shared with a module's closure; a store holding both keeps one, silently, in the core and the reference runner alike.
- **A label's subject in a batch.** A label's subject may be of any type, so its legacy key cannot be qualified by its reference's type as every other reference's is; building the core's import settled `<Type>:<key>`, the form the store keeps a legacy key in.

## Decision

1. **A `call` or a `create` MUST NOT name an assertion**, as it MUST NOT name an initial transition; the flow checker reports it under `names`, and the core's checker with it. An assertion is a declared override whose requester the upper layer decides (ADR-0040, ADR-0114), and one an effect caused would let a person override state who was never allowed to. The core's stop on a caused assertion stays, guarding what the checker refuses.
2. **A legacy entry in a batch is `{kind, occurred_at, payload, about}`**, `about` optional: `kind` names its entry kind, `occurred_at` when it happened, and `payload` its fields. The import gives it its ordinal, after those the store holds for the object.
3. **A batch carries the fields each entry kind keeps through an erasure**, `ImportBatch.legacy_kinds`, a mapping from kind to field names, and each entry is written with its kind and the list it was ported under, `of_legacy_entry.kind` and `kept`, so a later batch listing a kind differently does not change what an earlier entry keeps. A kind the batch does not list keeps nothing (ADR-0078).
4. **An entry's `about` maps each field describing another object to that object's `<Type>:<key>`.** The import moves such a field to that object's history, as an entry of the same kind, time and ordinal rule, where the store or the batch holds the object, and reports it otherwise as a class of kind `FIELD`, which `DROP` decides by leaving the field behind and `EXCLUDE` by excluding the object whose entry carries it (ADR-0146, ADR-0147).
5. **A store holds one type, enumeration or machine of a name.** A sandbox's load and a publish refuse a name two of their modules declare, naming both, in the core and in the flow checker's check of files together; `servicedesk.yaml`'s `Service` becomes `RegisteredService`.
6. **A reference that may name any type names its object `<Type>:<key>`** in a batch, as settled building the core's import and now confirmed: a label's subject, today the only such reference.

## Alternatives rejected

- **Letting a call pass an assertion's `to`.** The checker's refusal would become a rule of its own, and an effect could override state its requester had no right to.
- **An entry flat, its fields beside `kind` and `occurred_at`.** A payload field named `kind` or `occurred_at` could not be ported, and erasure's kept list would have to exclude the entry's own keys by convention.
- **The kept list in a table of its own, by kind.** A later batch listing a kind differently would change what entries already ported keep, after erasures may have relied on it.
- **A field about another object dropped silently.** The dry run's `FIELD` class exists so a person decides it; dropping it unasked is the silent loss the import is built to avoid.
- **Refusing the clash in the flow checker alone.** A sandbox loads files the studio's tooling checked, but a publish's source reaches the core directly; both must refuse it.
- **Renaming `people.yaml`'s `Service`.** It is the actor type every example module meets through `people.yaml`; the registry entry is the one particular to its module.
- **A label's subject named by a separate type field.** One string in the store's own qualified form needs no second field and reads as the store's external identifiers do.

## Consequences

- `flow-format.md` §4.8 states rule 1, `check-flows.py` and the core's checker report it, and their plants hold it. *(Applied 2026-10-07 building it to a part's cascade as well, the third way a transition is caused, by decision 1's own reasoning: a cascade naming an assertion is refused as a call naming one is.)*
- `library-api.md` §5's `ImportedObject` and `ImportBatch`, `publish-and-import.md` §4's Attach step and §6's `FIELD` class, and `storage-schema.md` §6's `of_legacy_entry` and §9's step 6 state rules 2 to 4; the core imports, reads in `history` and erases legacy entries by them.
- `servicedesk.yaml` and its examples name `RegisteredService`; the core's library load and its publish, and the flow checker's check of files together, refuse a name two modules declare; D529 is resolved.
- `publish-and-import.md` §4 and `library-api.md` §5 keep rule 6 as written, now the author's.
