# ADR-0155: The core's publishing, and its first slice

- **Status:** Accepted, 2026-10-05, by the author ("go with your recommendations on all four"), given four recommendations on how the Rust core takes on publishing: decisions 1 to 4. Decision 5, the first slice's scope, follows from them.
- **Date:** 2026-10-05
- **Refines:** ADR-0153 decision 5 (the core does not check a module), ADR-0135 (the source's digest), ADR-0085, ADR-0097 and ADR-0147 (the `DeclarationChange` and its publish)
- **Relates to:** `design/publish-and-import.md` §1 to §3; `DESIGN.md` §9; `design/library-api.md` §6; `design/storage-schema.md` §5, §10; PRD F7, F10, F13

## Context

Every example passes against the core (ADR-0154), so what comes next is the design's, and publishing is the largest part of it that the core lacks. A publish checks the new source and runs its examples, compares it with the installed version, reads every object the change touches into an impact report, and, once approved, installs the version in one transaction around one migration event per object (`publish-and-import.md` §1).

Three facts shaped the recommendations:
- **Step 1 is the checks.** The sixty-four checks of `declaration-syntax.md` §10, the flow checker's contradiction analysis and the examples, about 5,900 lines of Python in `scripts/check-flows.py` and `scripts/check-syntax-doc.py`. ADR-0153 decision 5 kept the core from checking, deferring the checks to publishing.
- **The source's digest is SHA-256** (ADR-0135), which Rust's standard library lacks.
- **Nothing implements a publish's live part**, in Rust or Python, so no oracle compares it as the replay compared requests. The flow checker does compare a module with its previous version (`--previous`), statically.

## Decision

1. **A sandbox's publish trusts its caller's checks, and a deployment's waits for the core's.** In a sandbox, step 1 is the flow checker's, which the studio runs over a source before it reaches the core, as it does before `/sandbox/load` (ADR-0153 decision 5): the core loads the source with its strict loader, refusing one it cannot read as a fatal finding, and takes the checks and the examples as passed. Outside a sandbox, a dry run and a publish are not run, naming the publish checks, until the core has its own checker. Nothing unchecked is installed outside a sandbox.
2. **The digest is computed with the `sha2` crate**, pinned in the workspace, over the source as ADR-0135 frames it, and held to `scripts/check-flows.py --digest` byte for byte.
3. **Publishing comes in slices.** The first is decision 5's. The second is the approval: the DDL around the mappings, one migration event per object, the admissions an `admit` records, the intervals of a newly tracked member, the redaction of a newly personal attribute, superseding the other open changes and invalidating the proposals the change breaks, and every element's own version (ADR-0131). The core's own checker follows as slices of its own, held to the flow checker over every module of the design and every plant of its self-test, as the replay held requests to the reference runner.
4. **A sandbox's `load` installs version 1 directly.** A sandbox is created at version 0, holding the built-in types, and its `load` installs the modules given as version 1, without a `DeclarationChange`, its source reference `sandbox:load` and its digest computed. Publishing then moves it on. This is the sandbox's shortcut PRD F13 allows; a deployment's store installs its first version by a published change (ADR-0126).
5. **What the first slice runs:**
   - `of_declaration` with versions 0 and 1, and every event and object carrying the version installed when it was written;
   - the built-in `DeclarationChange`, its states and transitions as `DESIGN.md` §9 gives them: `draft` and `revise`, taking the source as a map from each module's name to its description and from `<module>.examples` to its examples file, and `source_ref`, refused as `invalid_input` where it is empty, longer than 512 characters or holds a control character; `reopen`, `reject` and `withdraw`; `supersede`, only via a publish; and `submit` and `refresh`, which run the dry run, attach its report and are refused while it finds anything fatal, as `dry_run_clean`;
   - `publish` with `dry_run`, which produces the same report and writes nothing, and without it, not run until the approval's slice;
   - the dry run: the source loaded; compared with the installed version; every object of each type the change touches read into the impact report of `publish-and-import.md` §1 step 3, with each impact's severity from §2, the mappings of the source's `migration` applied in simulation and every invariant evaluated once after them, a violation no standing admission covers reported as a decision, covered or not by an `admit`;
   - the report as `PublishReport`, its `affected` ids per category; the fields the slice does not compute, such as compilation and fan-out, named as not computed on the wire.

## Alternatives rejected

- **For decision 1:** porting the checker before any publish, which leaves the impact report, the part a person approves, unbuilt behind the longest piece of work; publishing without checks anywhere, which breaks "a publish either installs a version or installs nothing" (`publish-and-import.md` §1); a deployment's core calling the Python checker, which makes the engine depend on a second runtime, against ADR-0132.
- **For decision 2:** a hand-written SHA-256, about a hundred lines of hashing the project would own and test, to avoid a small dependency that does only this.
- **For decision 3:** the whole publish in one slice, which would be committed untested in parts no example reaches.
- **For decision 4:** every sandbox load going through a draft and a publish, which would run the dry run over an empty store on every load for nothing a person reads.
