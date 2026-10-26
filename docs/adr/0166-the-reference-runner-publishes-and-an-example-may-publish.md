# ADR-0166: The reference runner publishes, and an example may publish

- **Status:** Decided by the author, 2026-10-07 ("go with your recommendation, option 3"), on three options stated with a recommendation; decisions 2 to 5 are the recommendation's parts, settled as it framed them.
- **Date:** 2026-10-07
- **Refines:** ADR-0155 (the core's publishing), ADR-0156 (the core's checker), ADR-0136 (examples), ADR-0153 (the Python reference as the core's oracle)
- **Refined by:** ADR-0167 — the runner's replay of the metric scenarios stops at the port, which carries legacy history ADR-0154 decision 8 leaves to the core; decision 2 holds the runner to the core by the comparisons, not by the scenarios. ADR-0232 — a `publish` step may name the version under publish, `version: change`, in an example starting at the version before, and a change may carry the files a step names.
- **Relates to:** `design/publish-and-import.md` §1 to §3; `design/flow-format.md` §11.3; `design/flow-studio.md` §4, §6; PRD F7, F13, UC-25

## Context

`flow-studio.md` §6 left one later slice: publishing a second version in the studio and watching its migration and impact report. Only the core publishes. The reference runner, the oracle the core is held to (ADR-0132 decision 2, ADR-0153), has no `DeclarationChange`, no dry run and no migration, and the examples format has no step that publishes, so a session holding a publish would neither replay the same on both backends, as UC-25's acceptance asks of a session, nor be saved as an example the flow checker runs.

Publishing is also where the core has been wrong most lately: of the seven defects found from 2026-10-05 to 2026-10-07, D527 to D533, six were in publishing or in what a publish installs, D529 being the exception, each found while building or reading the core, with no second implementation to disagree. The core's publish is 881 lines before its tests, with the store's install beside it.

Three options were put to the author: a publish pane on the core alone; publishing in the reference runner first, then the pane on both backends; or that and a `publish` step in the examples format, so a session holding a publish is saved and checked. The author chose the third.

## Decision

1. **The reference runner publishes**, as `publish-and-import.md` §1 and §2 state, through the same built-in `DeclarationChange` and its lifecycle as the core's, `draft`, `revise`, `reopen`, `submit`, `refresh`, `reject`, `withdraw` and `supersede` requested like any transition, and `publish`, with or without `dry_run`, as the library's operation. Its step 1 is the Python flow checker and its examples, which the core's checker is already held to; steps 2 to 5 are written for the runner from the design, not from the core's code, so that each is the other's oracle (the author's `rules/rust.md`, "Where an independent implementation exists"). Its report computes what the core's does and names the rest as not computed, as the core's does.

2. **Both backends are held to each other and to the record.** The studio's backend comparison plays every example through both, publishes included; the metric scenarios' replay, whose history holds five publishes, runs against the runner's sandbox as well as the core's, so both are held to the values `metric-scenarios.md` derives by hand; and the runner's self-test holds what neither of those reaches. A difference is a finding against one backend, recorded where the comparison's ratchet requires.

3. **An example may publish**, by a `publish` step (`flow-format.md` §11.3): `publish: { source: [<file>, …], actor, after }`, the files named relative to the examples file's own directory and read as a change's source is, each module and its examples file keyed by the name it declares (ADR-0155 decision 6). The step drafts a change with that source and the source reference `example:<examples file>`, submits it and approves it as `actor`, the operator by default, and applies when the version is installed; a step whose publish is refused, at submission or at approval, fails the example, as any step that does not apply does. A `publish` is a step and never the request under test, so an example claims what a version does through the requests after it, on the objects it migrated.

4. **The work comes in slices, each committed with its tests**, in this order: the runner's versions and `DeclarationChange`; its dry run and report; its approval, the install of `publish-and-import.md` §1 step 5; the runner sandbox's wire and both comparisons; the format's `publish` step in the flow checker, the core's examples run, the studio's player and its drafter; and last the studio's pane, publishing a module or the editor's text into the running sandbox, showing the report and the migration.

5. **What one backend cannot yet do stays visible.** Until a slice lands, the runner answers what it reaches with `not run`, naming it, as the core did while it was built (ADR-0153 decision 3), and a session or an example that reaches it is reported as not run on that backend, never as passing.

## Alternatives rejected

- **A publish pane on the core alone.** Quickest, but nothing independent checks what it shows, a session holding a publish replays on one backend, contrary to UC-25's acceptance, and the pane would be redone once the runner publishes.
- **The runner publishing, with no step in the format.** The comparison could then only hold publishes the studio's tests script by hand; examples are where the design's claims about a flow live (ADR-0136), and a version's migration is such a claim.
- **The runner's publish ported from the core's code.** Two implementations sharing one reading of the design share its mistakes, and the comparison could not find them.
- **A `publish` step carrying its source inline.** An examples file would hold a second copy of a module, drifting from the module the design keeps; the design's second versions, `service-v2.yaml` and `inventory-v2.yaml`, are files already.
- **A `publish` under test, with an expectation of its report.** Nothing yet needs it, since the requests after a step say what the version does to objects, and the report's shape would become part of the format; it can be added when an example needs to claim a refusal.

## Consequences

- `TODO.md` tracks the slices of decision 4, each with its tests.
- `flow-format.md` §11.3 and the examples schema gain the `publish` step with slice 5, and `flow-studio.md` §4 and §6 the pane with slice 6.
- `core/crates/objectflow-core/src/publish.rs`'s module documentation, which still says step 1 is a sandbox caller's, is corrected: since ADR-0156 the core runs its own checker in step 1.
