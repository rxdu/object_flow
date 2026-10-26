# Moving objects between types

**The question.** When a new version of a flow renames a type, splits it into several or merges several into one, how should the objects of the old type follow? Issue #31 (filed 2026-10-23) proposes a `moved_types` mapping in a version's `migration`, and asks two questions: should a moved object keep its id, or take a new one with a recorded link to the old; and should a move reach across modules? Today a removed type is retired, its objects readable and taking no transition but their erasure (ADR-0099 §7, `storage-schema.md` §10).

## 1. Platforms that re-type a record keep its identity

**ServiceNow CMDB, CI reclassification** (S1, S2). "You can upgrade, downgrade, or switch the class of a CI by modifying its Class attribute." A switch moves the record into another table and keeps its id: "A new record with the CI's current sys_id is inserted to the table of the new class, with the appropriate set of attributes for the class (the sys_id of the CI is retained)." What the new class cannot hold is lost: "If any attributes are unique to the current class and are not defined in the newly reclassified class, they are lost." The documentation advises against the operation: "Avoid the CI class downgrade and CI class switch operations as those can lead to data loss." It is bounded by identity: "CI reclassification is possible only between two classes that have identical identification rules."

**Camunda 8, process instance migration** (S3). "Process instance migration fits a running process instance to a different process definition", including "To a different process definition altogether." Every active element needs a target: "All active elements require a mapping." A mapping keeps an element's kind: "You cannot map an element to an element of a different type." The instance keeps its data: "Any existing variables, user tasks, and jobs continue to exist with the same values as previously assigned." It is one unit: "The migration runs in a transactional manner, meaning that it is migrating all active elements or nothing." The page sets migration against cancelling and re-creating an instance, of which it says: "Note that this results in new keys for the process instance and its associated variables, element instances, and other entities."

**Camunda 7, process instance migration** (S4). "A migration plan consists of a set of migration instructions that in essence are mappings between activities of the two process definitions." A mapping "has to be a one-to-one mapping", and the plan is complete or refused: "There must be a migration instruction for every instance of leaf activities". What the instance holds stays: "Besides that, activity instances, task instances and variable instances are preserved." A synchronous migration of several instances is one act: "the migration should be atomic, i.e., it should be executed immediately and should fail if at least one process instance cannot be migrated".

**Jira Cloud, moving work items** (S5, S6). A move to another work type or space maps each status: "you can map the statuses from your source space to the right statuses in your new workflow by selecting the status your work items should be in after the move." A field is carried only where it fits: "or select Retain to leave the field's value unchanged", which needs the same field type and name. The page warns: "If work types use different workflows, you may lose data when bulk moving work items." A move between spaces changes a work item's key, and "Any existing links to old work item keys will be automatically redirected."

## 2. Splitting one record into two makes new identities

**Greg Young, *Versioning in an Event Sourced System*** (S7). Copy-replace reads a stream and writes another, and it serves a merge or a split: "Have two events that need to be merged into one, or one event that needs to be split into two? This will do." A split writes new streams: "Much like dealing with Copy-Replace, Split-Stream involves reading up from the first stream and writing down to two new ones." An event's identity can follow a copy: "One option is to make sure you include the same identifier on the new event as the one you are copying." It cannot follow a split: "Certainly you can't assign the same identifier to both of the split events." The old and new streams are linked both ways: "Write an event at the end of the stream saying this stream has been migrated", "A single generic event can be used for this StreamMovedTo", and "It is also common to add a MigratedFrom {stream} event at the beginning of the migrated streams." He would keep the old stream: "For auditing purposes, it can be worthwhile to put a pointer event pointing back to the original stream instead of deleting it."

## 3. A rename that is not declared loses the data

**Django, `RenameModel`** (S8). The operation "Renames the model from an old name to a new one." Without it, a rename is a removal and an addition: "to the autodetector, this will look like you deleted a model with the old name and added a new one with a different name, and the migration it creates will lose any data in the old table."

## 4. ObjectFlow beside them

- **#31's claims hold, each reproduced on 2026-10-24 at `67e3445`**, a debug build, on a SQLite store made with `--create`. `renamed_types` is refused `check:schema`. The publish without it is accepted, with only a `superseded` notice. Both grants stay `HELD` under the retired type, and `revoke` answers `unknown_transition`.
- **A retired type is Django's lost table, kept readable.** Its objects keep their history but no rule reaches them again. A removed state holding objects, by contrast, needs a mapping (`flow-format.md` §4.16), and so does a removed enumeration member that live rows hold (ADR-0099 §7).
- **The layout is ServiceNow's.** Each type has its own table, and the directory, `of_object`, records every object's id beside its type (`storage-schema.md` §2). The log, admissions, intervals, write index, external ids and legacy entries each name an object by its id, a key into the directory. *Inferred:* a move that keeps the id changes the object's directory row and its type's table, and every row keyed by the id follows it unchanged.
- **References name a table on SQLite.** A stored reference's foreign key names its target's table there, or the directory where the target has subtypes; on PostgreSQL every such key names the directory (`storage-schema.md` §3). The runtime checks a reference's type on both. An attribute whose type changes is today dropped and added, every value starting absent (ADR-0148).
- **The issue's split partitions a type, not an object.** Each object goes to exactly one target, since an object covered by no `where`, or by two, is fatal. *Inferred:* every move is one record into one record, as ServiceNow's, Camunda's and Jira's are, and Young's case of one record becoming two does not arise.
- **A type's name is unique across the store** (`flow-format.md` §3, ADR-0163). Probed 2026-10-24 at `67e3445`: a type moved to another module under its own name keeps its objects, and a `revoke` on one is satisfied after the publish. *Inferred:* only a move that also renames needs a mapping, wherever its target is declared.
- **A publish is one transaction**, its DDL and mappings applied with its `of_declaration` row (`storage-schema.md` §10). A move inside it is all or nothing across every object, as Camunda 7's synchronous migration is.

## 5. What it suggests

For the author, not decided here:

- **Keep the id**, as every platform that re-types a record does. Every row keyed by the id then follows the object: its history, admissions, intervals and erasure records. A new id would need every reference re-pointed and a link that erasure must follow, which is Young's split, a case the proposal does not have.
- **Make a removed type holding objects a decision**, moved or retired on purpose, as a removed state already is. The sources that skip the decision lose data: ServiceNow's dropped attributes, Jira's warning, Django's autodetector.
- **Refuse a move that leaves an object without a target**, as Camunda refuses an incomplete plan.
- **Carry a reference to a moved object** where the new version declares the reference to the target type, and refuse a move whose objects a reference could no longer hold. This is the case ServiceNow's identical-identification rule and Camunda's same-kind rule each guard in their own terms.

## 6. A migration a store has nothing for (#41)

**The question.** Issue #41 (filed 2026-10-25) asks that one flow source, its migration kept in it, publish both to a new store and to an older one, where a consumer keeping its flows as code publishes the same source to empty stores and to stores at any earlier version. Should an entry that names what a store does not hold be refused, skipped, or something else?

**Terraform, `moved` blocks** (S9). A refactoring is declared in the configuration, and an instance that never had the old address ignores it: "New instances of the module, which *never* had an `aws_instance.a`, will ignore the `moved` block and propose to create `aws_instance.b[0]` and `aws_instance.b[1]` as normal." The blocks are kept: "Removing a `moved` block is a breaking change because any configurations that refer to the old address will plan to delete the existing object instead of move it. We strongly recommend that you retain all historical `moved` blocks from earlier versions of your modules to preserve the upgrade path for users of any previous version."

**Liquibase, preconditions and the changelog table** (S10, S11). A changeset can be made to depend on the database's state: "Preconditions are tags you add to your changelog or individual changesets to control the execution of an update based on the state of the database." By default a failed precondition stops everything, `HALT`: "Halts the execution of the entire changelog (default)." A changeset may opt out, `MARK_RAN`: "Skips over the changeset but marks it as executed. Continues with the changelog." What ran is recorded: "Liquibase tracks which changesets have been run by using the DATABASECHANGELOG (DBCL) table."

**Flyway, the schema history table and repeatable migrations** (S12, S13). "To keep track of which migrations have already been applied when and by whom, Flyway adds a special schema history table to your schema." A repeatable migration runs again only when it changes: "Instead of being run just once, repeatable migrations are (re-)applied to a database on migrate every time their checksum changes." It serves a definition kept as code: "This is very useful for managing database objects whose definition can then simply be maintained in a single file in version control."

**ObjectFlow beside them.**

- **#41's claims hold, each reproduced on 2026-10-25 against the published v0.6.0**, on a file store made with `--create`: a move or a retirement of a type the installed version lacks is refused `moved_type`, over an empty store and over one that retired the type.
- **A kept entry is refused again once it applied.** Published a second time, a move is refused `moved_type`, and a rename, a removed state and a removed member each `check:names`, "which the previous version did not have". Over an empty store the last three publish, since the checker compares a module with its installed version only where that version has the type.
- **A kept backfill runs at every publish.** `if count is null then 1 else count + 1` took a count from 10 to 11, 12 and 13 over three publishes of one source, each recorded as a migration, as F2 requires, and none intended.
- **The store already records what ran.** The installed version's source holds its migration (`storage-schema.md` §5), as Liquibase's table holds a changeset's id and Flyway's a migration's checksum. *Inferred:* comparing a backfill with the installed version's gives Flyway's rule without a table of its own.
- **What protects live objects is elsewhere.** A removed type, state or member holding objects needs a mapping whatever the entry says (ADR-0229 decision 1, `flow-format.md` §4.16). *Inferred:* an entry that acts on nothing can become a notice without opening a route to an unrecorded change.

**What it suggests,** decided as ADR-0230:

- **Keep migrations in the source,** as Terraform keeps `moved` blocks, each entry skipped where the store has nothing for it, a notice rather than Liquibase's default halt.
- **Run a backfill once on a store, and again when it changes,** as Flyway runs a repeatable migration, the installed version's migration standing for the history table.
- **Keep refusing a name the version before lacks where the checker is told that version,** since there it is a mistake in the source.

## 7. A backfill rewriting a value an object holds (#41's comment)

**The question.** #41's filer commented the same day that a backfill names every object it writes under one notice, `mapping:backfill:<Type>.<attribute>`, whether the object held no value or one the backfill replaces. A backfill written for stores of the version before an attribute, published over a store already past it, then rewrites held values under the notice a fill gets. Should the report tell the two apart?

**Terraform, the plan** (S14). A change is previewed before it is made: the `terraform plan` command "creates an execution plan, which lets you preview the changes that Terraform plans to make to your infrastructure". The plan marks each kind of change apart: "Resource actions are indicated with the following symbols:", among them `+ create` and `~ update in-place`. Within one resource's in-place update, a key set where there was none is marked `+ "Name" = "api"`; a changed value is shown from old to new, `"ami-02c98622c4c3f017d" -> "ami-0d7d6fe23ca71032d"`, in the page's example a change that forces the resource's replacement. The summary counts them apart: "Plan: 2 to add, 1 to change, 2 to destroy."

**ObjectFlow beside it.**

- **The dry run is the plan.** A publish's report is what the approver reads before the version applies (`publish-and-import.md` §1), as Terraform's plan is read before `apply`.
- **A fill and a rewrite read alike.** Probed 2026-10-26 on the published v0.7.0, over a store at a version holding `membership` and no backfill of it: the backfill `Level.STAFF` rewrote a held `COLLABORATOR` under `mapping:backfill`, the notice a fill of an absent value gives.
- **The core already compares.** A backfill writes an object only where the value it gives differs from the one the object holds (`publish.rs`, the backfill loop), so whether the object held a value is known where the notice is given. *Inferred:* telling the two apart costs one comparison, not a second pass.

**What it suggests,** decided as ADR-0231: name a rewrite apart from a fill, as the plan marks `+` apart from `->`, beside the notice that names every write, so that whatever reads `mapping:backfill` for what a backfill writes is unchanged.

## Sources

S1 to S8 were each retrieved 2026-10-24, S9 to S13 on 2026-10-25, and S14 on 2026-10-26, with `curl -sL`, its markup stripped, and each quotation found in the text. ServiceNow's documentation pages are rendered by JavaScript, so their text was fetched from the site's own content endpoint, which answers each topic's HTML. Salesforce's documentation refused the fetch (HTTP 403), so its lead conversion, a split into new records linked back to the lead, is left out unchecked.

| | Source | Version |
|---|---|---|
| S1 | ServiceNow, *Manually reclassify a CI*: https://www.servicenow.com/docs/r/servicenow-platform/configuration-management-database-cmdb/t_ManuallyReclassifyCI.html, text from `https://www.servicenow.com/docs/api/khub/maps/zf4CFrQ4oaR8UN3GV4kT5g/topics/51r7E1v9N5~hbhv8WJWh2A/content` | the Brazil release, updated September 10, 2026 |
| S2 | ServiceNow, *CI reclassification*: https://www.servicenow.com/docs/r/servicenow-platform/configuration-management-database-cmdb/c_CIReclassification.html, text from `https://www.servicenow.com/docs/api/khub/maps/zf4CFrQ4oaR8UN3GV4kT5g/topics/Q2WvCK1Im17OhPY2oB72cw/content` | updated September 10, 2026 |
| S3 | Camunda 8, *Process instance migration*: https://docs.camunda.io/docs/components/concepts/process-instance-migration/ | 8.10 |
| S4 | Camunda 7, *Process Instance Migration*: https://docs.camunda.org/manual/7.24/user-guide/process-engine/process-instance-migration/ | 7.24 |
| S5 | Atlassian, *Move multiple work items*, Jira Cloud: https://support.atlassian.com/jira-software-cloud/docs/move-multiple-issues/ | the page gives none |
| S6 | Atlassian, *Migrate between team-managed and company-managed spaces*, Jira Cloud: https://support.atlassian.com/jira-software-cloud/docs/migrate-between-team-managed-and-company-managed-projects/ | the page gives none |
| S7 | Greg Young, *Versioning in an Event Sourced System*, "Copy and Replace": https://leanpub.com/read/esversioning/leanpub-auto-copy-and-replace | the book read online, "Last updated on 2017-04-09" at https://leanpub.com/esversioning |
| S8 | Django, *Migration Operations*: https://docs.djangoproject.com/en/6.1/ref/migration-operations/ | 6.1 |
| S9 | HashiCorp, *Refactoring*, Terraform modules: https://developer.hashicorp.com/terraform/language/modules/develop/refactoring | v1.17.x, the page's latest |
| S10 | Liquibase, *What are preconditions?*: https://docs.liquibase.com/concepts/changelogs/preconditions.html | 5.1, last updated August 21, 2026 |
| S11 | Liquibase, *What is the DATABASECHANGELOG table?*: https://docs.liquibase.com/concepts/tracking-tables/databasechangelog-table.html | 5.1, last updated August 22, 2026 |
| S12 | Redgate, *Flyway schema history table*: https://documentation.red-gate.com/fd/flyway-schema-history-table-273973417.html | page last updated 25 September 2026 |
| S13 | Redgate, *Repeatable migrations*, Flyway: https://documentation.red-gate.com/fd/repeatable-migrations-273973335.html | page last updated 20 June 2025 |
| S14 | HashiCorp, *terraform plan command*, Terraform CLI: https://developer.hashicorp.com/terraform/cli/commands/plan | v1.17.x, the page's latest |
