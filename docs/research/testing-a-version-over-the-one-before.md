# Testing a version over the one before

**The question.** Issue #42 (filed 2026-10-25) asks how a consumer that ships only the service can test what an object made under one version does under the next. A change's examples run in an empty store holding version 0 and the version under publish, and one whose `publish` step installs another version is refused `example_not_run`. Where should the version before come from, and does the engine run such a test, or the consumer?

## 1. Terraform runs a configuration over another's state in its own test command

**Terraform, `terraform test`** (S1). A test file's run blocks act on configurations the file names: "Each run block also allows the user to change the targeted configuration using the module block." They run in order: "By default, Terraform executes run blocks sequentially." The main configuration's state "is shared by all run blocks that do not have a module block specifying an alternate module to load", and a test may share one state across configurations: "You can override this default behavior with the state_key attribute and force Terraform to use a specific state file for a given run block. This is useful when you want to share state between run blocks that do not reference the same module." The configuration a run block loads is a file the test names, not a deployment's state: "Terraform test files only support local and registry modules within the source attribute."

## 2. Temporal replays recorded histories against the new code in CI

**Temporal, replay testing** (S2). "Replay recreates the exact state of a Workflow Execution." A changed definition must accept what the old one recorded: "Replay succeeds only if the Workflow Definition is compatible with the provided history from a deterministic point of view." The check is the consumer's CI, run with the SDK it ships: "When you test changes to your Workflow Definitions, we recommend doing the following as part of your CI checks:", among them "Download the Event Histories of a representative set of recent open and closed Workflows from each Task Queue" and "Fail CI if any error is encountered during replay."

## 3. ObjectFlow beside them

- **The version before is held.** The store keeps every published version's source, one row per version, never deleted (`storage-schema.md`, `of_declaration`), so the installed version's source, and any earlier one, can be read.
- **An example's publish step reads files beside it.** A `publish` step names the files of the version it installs by their file names beside the examples file; read where the examples file has no directory, as in a change's source, they cannot be found, and the publish refuses the example `example_not_run` (`flow-format.md` §11.3, ADR-0156).
- **An example starts at the version under publish.** Each example runs in a store holding version 0 and the version under publish (`publish-and-import.md` §1 step 1), so its setups run under the new version, and an object made under the version before cannot be one of them.
- **#42's claims hold, each reproduced 2026-10-26 on the published v0.7.0**, on a file store made with `--create`. A change whose example has a `publish` step is refused `example_not_run`: "it reaches the publish step's file badges.yaml, which no directory beside the examples file gives". A change's example issuing the code `B1` applied while the store held a badge with `B1`, which a second issue on the store refused `invariant_violated`, so the example ran in a store of its own.
- **A change's source holds modules and examples files alone.** Probed the same day: a source carrying `badges.yaml` beside the module `badges` is refused `invalid_input`, `source: malformed`.
- **The dry run already reads the store's own objects.** A publish's dry run reads every object of each type the change touches, final ones included, and reports what the version does to each (`publish-and-import.md` §1 steps 3 and 4), as Temporal's replay reads recorded histories; what it cannot show is what a migrated object does next.

## 4. What it suggests

- **Carry the version before in the change,** as Terraform's test names the configuration it applies first by a local source, so that a change's verdict stays its source's alone, over an empty store as over one behind. *Inferred:* taking the store's installed version as the version before ties the verdict to the store, and #41's consumer publishes one source to stores at several versions, where an example written for the version just before would run against an older one.
- **Let an example start at the version before and publish the change over it,** as a `run` block shares one state across two configurations, so a setup made under the version before is migrated by the change and the request under test runs on what it left.
- **Leave the store's own objects to the dry run,** which reads them as Temporal's replay reads recorded histories, rather than a service operation playing a version's setups into a store.

## Sources

S1 and S2 were each retrieved 2026-10-26 with `curl -sSL`, its markup stripped, and each quotation found in the text.

| | Source | Version |
|---|---|---|
| S1 | HashiCorp, *Tests*, Terraform configuration language: https://developer.hashicorp.com/terraform/language/tests | v1.17.x, the page's latest |
| S2 | Temporal, *Testing*, Go SDK: https://docs.temporal.io/develop/go/testing-suite | the page gives none |
