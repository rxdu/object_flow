# ADR-0154: The core's import

- **Status:** Accepted, 2026-10-05, at the author's direction ("go with your recommendation, importing first"), given the recommendation that importing be the core's next slice, since the two examples that wait against both the core and the reference runner wait for it. Decisions 2 to 4 settle three gaps the slice found, D524 to D526; decision 5 answers the third cold reading's open questions the import reaches, as ADR-0148 decision 3 assigns them to the builder that reaches them. Each is open to the author's revision.
- **Refined by:** ADR-0163 — a legacy entry's shape in a batch, the kept fields per kind, and a label's subject as `<Type>:<key>`. ADR-0176 decision 3 — a reference to a key the batch and the store lack is a class, decidable by `EXCLUDE`.
- **Date:** 2026-10-05
- **Refines:** ADR-0150 (the wire), ADR-0153 (the core's slices), ADR-0146 decision 7 and ADR-0148 decision 3 (the import, and its open questions), ADR-0101 (the import batch)
- **Relates to:** `design/library-api.md` §5, §6; `design/publish-and-import.md` §4, §6; `design/storage-schema.md` §2, §6; `design/flow-format.md` §4.15, §11.3; `design/cold-reading-3-review.md` §3

## Context

Every example that runs against the reference runner passes against the core (ADR-0153), and the two that run against neither wait on an `import` step: an employee the HR system owns, ported into the `Employee` mirror. The design's import is `import_batch`, one transaction per batch, which writes each object of a `mirror` type through the built-in assertion and applies a disposition file, decided by a person, to every class of violation a dry run found (`publish-and-import.md` §4, §6, ADR-0146).

Reading the interface to build it found three gaps no record holds:
- `import_batch` takes a `DispositionFile`, and no document defines one: `publish-and-import.md` §6 defines its classes, `ViolationClass` and `Disposition`, and no file (D524).
- A refused batch is "refused whole, naming the first row it could not place" (`publish-and-import.md` §4), and `ImportReport` has no field to name it in (D525).
- An example's `import` step takes no disposition, and the reference runner admits whatever the object it imports violates, so an example could import a violation nobody decided, with no reason recorded (D526).

And it reaches thirteen of the third cold reading's open questions, which ADR-0148 left to the builder: C14, C22 to C24, C28, C29 and C31 to C37 (`cold-reading-3-review.md` §3, D516). Decision 5 answers twelve, and decision 6 the thirteenth, C34.

## Decision

1. **The wire is `POST /v1/import_batch`**, as ADR-0150 decision 1 makes every operation: its body is `actor`, `batch`, `disposition` and `dry_run`, and its result is the `ImportReport`. An object's attribute values are in the wire's notation (ADR-0150 decision 2), and a reference is the legacy key of the object it names, which is always a mirror's, since a mirror stores references only to other mirrors (step 3, `names`). The report names no object, so a caller finds what it imported by the library's `lookup`, `POST /v1/lookup`, with the source `legacy` and the key as decision 5 qualifies it, or by an external identifier its type declares.
2. **A disposition file is the list of `publish-and-import.md` §6's violation classes** (D524). The import finds the classes over the batch and looks each up in the file by its type, kind, rule and shape, which are a class's identity; an `id` is the dry run's label and matches nothing.
3. **A refused batch says so in its report** (D525). `ImportReport` gains `refusal`: the first object the import could not place, by type and legacy key, and why; `written` is then zero and nothing is written.
4. **An example's `import` admits nothing** (D526). Its object is a one-object batch with an empty disposition, its legacy key the step's alias, so an object that violates an invariant or leaves a required attribute out refuses the step, in the reference runner as in the core.
5. **The open questions the import reaches, answered:**
   - **C32, two channels for an admission.** `ImportBatch.admitted` is removed. The file is the one way an import admits: an object in an `ADMIT` class is admitted with that class's reason.
   - **C31, a class's identity.** A class's `shape` is a fixed text for its kind of failure, never a value, so re-extracting finds the same class: an invariant `does not hold`; a required attribute `is absent`; a value `is not declared` or `is not of its type`; an external identifier `is shared`; a reference `names an excluded object`. A violation the file has no class for refuses the batch, as an `UNDECIDED` one does.
   - **C33, an owned type in a batch.** It refuses the whole batch, naming its first such object, and `refused_types` lists every owned type the batch named, on a dry run and on a refused batch alike.
   - **C35, a value outside the declaration.** A state, an enumeration member or a value of the wrong type is a `GUARD` class, so its objects can be excluded or cleaned upstream, and never admitted. An attribute the type does not declare refuses the batch, since it is the mapping's error and no decision about the data.
   - **C22, a mirror's uniqueness.** A mirror's `unique` is an invariant like its others, an `INVARIANT` class the file may admit, since `storage-schema.md` §8 compiles none of a mirror's invariants. An external identifier's uniqueness is held by `of_external_id`'s key, so a value two objects share is a `GUARD` class.
   - **C23, the legacy key's namespace.** A legacy key is unique within its family, its type alone where it extends nothing. It is kept in `of_external_id` under source `legacy`, its value qualified by the family's root type, `<Type>:<key>`, and its `attribute` empty, since no attribute holds it. A reference names its target by legacy key, qualified the same way.
   - **C29, `REFERENCE` and `DROP`.** An excluded object is one this batch's file excludes and the store does not hold; one the store holds is left as last imported, and a reference to it resolves. A legacy key naming nothing in the batch or the store refuses the batch, whatever an earlier file excluded, since the store keeps no record of an exclusion. `DROP` on a required reference refuses the batch, and the dry run reports it.
   - **C24, `recorded_from`.** It is computed, as `publish-and-import.md` §4 states and ADR-0148's F25 confirmed, and `ImportedObject.recorded_from` is removed. The state's intervals are whole when they run unbroken from the legacy creation. It is set when an object is first imported, and a refresh leaves it, since a refresh's later intervals describe nothing earlier.
   - **C28, a port's changer.** `EnteredAt.by_kind` is recorded as the mapping gives it, the kind `unknown` where it gives none. The store's own actors are not consulted, since the change was made in the legacy system, which the mapping describes.
   - **C37, a kind with no time.** A missing creation time and a missing creation kind are separate: the time missing, the object is undated and takes the earliest time the import knows of it; the kind missing, it is `unknown`.
   - **C36, sequences.** A sequence is raised past the last value the legacy system minted, so the next number minted is the one after it, and never lowered. The batch keys each by sequence and scope, the scope empty for an unscoped sequence.
   - **C14, a refresh and an admission.** A refresh that leaves an admitted invariant holding discharges the admission, as any write that finds it holding does (ADR-0054). Its first two cases, a migration's, wait for publishing.
6. **What the core's import runs:**
   - new objects of a mirror type, and refreshes of one already held, matched by legacy key, its declared external identifiers kept beside it;
   - every legacy key resolved before a row is written;
   - the classes of kinds `INVARIANT`, `GUARD` and `REFERENCE`, found once, after every object of the batch is placed, over the objects it writes (C34), each applied as the file decides, an exclusion's references becoming classes of their own;
   - each written object's event, `import`, with provenance `asserted`, marked `imported`, and its `state_source` `imported`;
   - its intervals opened at the times `entered` gives, by the changer it names, and at the import otherwise, by a changer of kind `unknown`;
   - its creation time and kind, its being dated, and its `recorded_from`;
   - a refresh that changes nothing writing nothing, one that changes a value writing one event, and one whose entry time precedes the latest interval entry of the same dimension refusing the batch;
   - no personal value written to an object whose erasure is recorded;
   - each sequence raised;
   - a dry run, which writes nothing and reports every class with its count, a sample of legacy keys and the file's decision for it.

   A batch that reaches anything else is not run, naming it: legacy entries, legacy intervals, silent spans, a refresh's intermediate values, observations and labels on a mirror's objects, and files. *Narrowed 2026-10-05:* legacy intervals and silent spans on a first import are imported since (`TODO.md`, the scenarios' first slice); a refresh's legacy intervals, its intermediate values, are still not run.
7. **A mirror's objects are written only by the import, their erasure and a publish's migration** (`flow-format.md` §4.15). A mirror declares no transition but its erasure (`declaration-syntax.md` §2, check 53), which the core runs as any erasure, and recording an observation on a mirror's object is refused as `subject_owned`.
8. **Both of the studio's backends serve the route**, so the replay plays an `import` step against either. The reference runner's backend imports new objects with an empty disposition, which is all an example's step is, and answers anything else as not run.

## Alternatives rejected

- **For decision 1:** an import step written through `/v1/request` with a flag, which would give the import a second shape on the wire beside the library's operation; a sandbox control, which a deployment's service would then lack, though importing is a deployment's operation.
- **For decision 2:** matching a class by its `id`, which the dry run assigns, so re-extracting after a clean would need the old ids carried; a disposition per object, the decision §6 exists to make by class.
- **For decision 3:** a refused batch raising an error, against `library-api.md` §2, for which a refusal is a value; a refused batch writing what it could place, the failure the disposition file exists to prevent.
- **For decision 4:** letting an example's import admit what it violates, as the runner did, which admits with no decision and no reason; a disposition key on an import step, which no example needs while every imported object satisfies its type.
- **For decision 5:**
  - C32: keeping `admitted` as an override of the file, which an import decided by class would then not be;
  - C31: a shape carrying the value, so every new value makes a new class and the file never settles;
  - C33: writing the rest of a batch past an owned type, which `publish-and-import.md` §4's one transaction forbids;
  - C35: refusing the whole batch on one undeclared value, so a handful of malformed rows block every other, with no way to decide them in the file;
  - C22: a mirror's `unique` as a constraint, which would make a legacy duplicate unadmittable, against §8's rule that a mirror's invariants compile to nothing;
  - C23: one store-wide namespace of legacy keys, in which a customer and a job keyed `17` collide; a source per type, which a family, ported together, would split;
  - C24: `recorded_from` supplied by the mapping, a second answer beside the computed one; recomputing it on a refresh, which would move it to the refresh's port;
  - C28: the kind of the store's actor of the same identity, which may be another person, the legacy system's identities being its own.
- **For decision 6:** the whole of `import_batch` in one slice, legacy history and silent spans included, when no example and no caller needs them yet and each is answered as not run where reached.
