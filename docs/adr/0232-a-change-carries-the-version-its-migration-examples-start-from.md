# ADR-0232: A change carries the version its migration examples start from

- **Status:** **Decided** 2026-10-26 under the author's direction ("go with your recommendations"), after the review of issue #42.
- **Date:** 2026-10-26
- **Refines:** ADR-0166 decision 3 (an example may publish); ADR-0136 (examples); ADR-0135 (a change's source and its digest); ADR-0156 (the core's checker)
- **Relates to:** PRD F7; `docs/research/testing-a-version-over-the-one-before.md`; `publish-and-import.md` §1 step 1; `flow-format.md` §11.3

## Context

**The issue.** Issue #42, filed 2026-10-25, found that a consumer that ships only the service cannot test what an object made under one version does under the next. Its flows are kept as code, and its rule is that every change carries every object the version before holds, so it wants a migration example for each mapping, run by the engine it ships.

**What was reproduced.** Each claim ran on 2026-10-26 against the published v0.7.0, on a file store made with `--create`:

- A change whose example has a `publish` step is refused `example_not_run`: "it reaches the publish step's file badges.yaml, which no directory beside the examples file gives".
- Each example runs in a store of its own. A change's example issuing the code `B1` applied while the store held a badge with `B1`, which a second issue on the store refused `invariant_violated`.
- A change's source holds modules and examples files alone. One carrying `badges.yaml` beside the module `badges` is refused `invalid_input`, `source: malformed`.

**What the record has.** An example may publish (ADR-0166 decision 3), reading the files of the version it installs beside the examples file, so a migration example runs in the checker and the Flow Studio and never in a change. And each example's store starts at the version under publish (`publish-and-import.md` §1 step 1), so a `publish` step can only install a version after it; an object made under the version before cannot be in an example of the change.

**How tools do it** (`docs/research/testing-a-version-over-the-one-before.md`). Terraform's `terraform test` applies one configuration over another's state, the configuration a run block loads named by local files, never a deployment's state. Temporal recommends a CI check that replays recorded histories against the new code.

## Decision

1. **A change's source may carry the files its examples read**, each keyed by its file name as a `publish` step or a `before` names it, `<name>.yaml`, a name no module or examples file can have. A carried file is not part of the version: it is never loaded, checked or installed as a module, and retires nothing. It is stored with the version's source and counted in its digest, since it is what the version's examples ran with.
2. **An example may start at the version before**, `before: { source: [<file>, …] }`, the files read as a `publish` step's are: beside the examples file in the checker and the Flow Studio, and among the files a change carries in a publish. The example's store holds version 0 and that version, installed as the version under publish is for any other example, and its setups and steps run under it. A file no reader gives leaves the example not run, as a `publish` step's does; files that do not load fail it.
3. **A step `publish: { version: change }` installs the version under publish over it**, as a `publish` step installs its files: drafted, submitted, whose dry run reads the objects the steps before it made and migrates them, and approved. It publishes the version's modules alone, since its examples are the ones running and the files it carries are theirs. The steps after it and the request under test run under the change; a refusal at any of the three fails the example.
4. **The rules of the two keys** (step 3, `names`): a `publish` step names `source` or `version`, not both; `version: change` is written only in the steps of an example with `before`, once; and an example with `before` publishes the change in its steps. A setup takes no `before`: the setups an example is given run under the version it starts at.
5. **The checker, the reference runner, the Flow Studio's replay and the core run them alike.** Each example without `before` runs as before this decision.

## Alternatives rejected

- **The store's installed version as the version before** (the issue's `previous`). The change's verdict would then be the store's: over an empty store nothing is before, and the consumer's tests publish to empty stores; over a store two versions behind, as #41's consumer has, an example written for the version just before runs against an older one, whose setups may not apply, refusing a correct publish or, skipped, hiding a failure.
- **A version the store holds, named by its number or its `source_ref`.** The same dependence on the store, and nothing to name over an empty one.
- **`before` for a whole examples file** (the issue's `versions` block). Every example of the file would start before the change; per example, a file holds plain and migration examples together.
- **A service operation playing a version's setups into a store** (the issue's first alternative). A migration example already builds a store at the version before, holding objects, publishes the change over it and checks what follows, one example at a time; a store's own objects are the dry run's to read, as Temporal's replay reads recorded histories.
- **The change published with its examples in a `version: change` step.** The step's dry run would run the example publishing it again, without end.

## Consequences

- `flow-format.md` §11.3 states `before` and `publish: { version: change }`, §11.5 their rules; `examples.schema.json` holds both; `publish-and-import.md` §1 step 1 and `storage-schema.md`'s `of_declaration.source` name the carried files; `CHANGELOG.md` lists the change under the next release.
- The draft accepts a carried file in both implementations, and every reader of a version's source, the core's, the reference runner's and the Flow Studio's, leaves carried files out of the version.
- A consumer keeps the version before as files, from its repository's history, and carries them in each change whose examples start from it.
