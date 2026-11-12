# Backfilling history into a live store

**The question.** Issue #89 (filed 2026-11-12) asked for a legacy import into a type the store owns. Its deployment freezes the legacy system, makes new records in ObjectFlow from that day, and imports the cleaned history class by class over weeks. Today the import writes only a mirror, and a mirror takes no transition (ADR-0101 §1), so one type cannot both take new records and receive its history. This note asks how established platforms bring a retired system's records into the tables live records go to.

## Findings per source

### Microsoft Dataverse: a migrated record keeps the time it was created

Dataverse imports into the tables its live records live in, and lets a migration keep a record's original creation time: "To import data in the createdon column, map the source column that contains this data to the overriddencreatedon column. During import, the record's createdon column is updated with the value that was mapped to the overriddencreatedon column and the overriddencreatedon column is set to the date and time that the data was imported." Doing so takes a privilege of its own: "To override the value in the createdon column during import, you need the prvOverrideCreatedOnCreatedBy privilege." What else a migration may set is bounded: "You cannot import data into the modifiedon, createdby, and modifiedby columns."

### Odoo: a record from the previous system keeps its identifier

Odoo imports into the same models its records are made in, and keys each imported record by the identifier the previous system gave it: "It is possible to use an External ID (ID) from previous software to facilitate the transition to Odoo." The identifier lets an import be run again, and relations be rebuilt: "Update imports: import the same file several times without creating duplicates." and "To recreate relationships between different records, the unique identifier from the original application should be used to map it to the External ID (ID) column in Odoo." It warns of the identifier's one hazard: "It should be noted that conflicts occur if two (or more) records have the same External ID."

## What the sources share, and where they part

- **History lands where live records live.** Neither keeps a second table for migrated records.
- **The previous system's identity is kept**: its creation time in Dataverse, its identifier in Odoo, so a relation and a time read alike for a migrated record and a live one.
- **A migration is marked and bounded.** Dataverse records when a record was migrated, beside when it was created, and lets a migration set the creation time alone, under a privilege of its own.
- **They part on updating.** Odoo's import with an identifier updates a record already there; Dataverse's creation-time override applies as a record is imported. *Inferred, which neither page states:* a store that must never let an import override what its own users recorded would take the creation alone.

## ObjectFlow beside the sources

- **The import writes only a mirror** (ADR-0101 §1), written by the import alone until a publish cuts it over (`publish-and-import.md` §7). It closed D261: a refresh could otherwise override a type already owned, its objects marked `imported` and hidden from `exceptions()`. ADR-0101 rejected closing the import once a type has had any ordinary write, since that boundary is invisible to a refresh job.
- **A mirror exists because the legacy system still writes.** ADR-0015 forbids two writers of one type, so the store refreshes a mirror from the legacy system until the type is cut over. A frozen legacy system writes nothing after the freeze: no refresh is needed, and no second writer exists.
- **The import already keeps a legacy record's history** on the object it creates: its legacy key, the time it was created, its earlier states and tracked values as intervals, the spans its record is silent on, and its legacy entries (`import.rs`, ADR-0086, ADR-0106, ADR-0163). Each object is written through the built-in assertion, each class of violation decided by the disposition file, each admission carrying its reason (`publish-and-import.md` §4, §6).
- **A refresh keeps an erased person out** because it reaches the same object, whose erasure is recorded (ADR-0101 §1). An import that only creates reaches no object of the store, so it cannot tell a legacy row about a person erased since the freeze.

## What it suggests

*A recommendation for the author, not a decision.*

Let the import create objects of a type the store owns while that type's history is open, as Dataverse and Odoo import into the tables live records use, and never update one, as a store whose owned objects only its users write must not:

- the type declares its history open in a version, and the version that drops the marking closes it for good;
- each object is new, keyed by its legacy key, with the creation time and history the legacy record gives, through the checks and the disposition file a mirror's import takes;
- a sequence is raised past the legacy system's last value at the freeze, before any number is minted;
- a person erased since the freeze is the disposition file's to exclude, which the engine cannot tell.

## Sources

Each retrieved 2026-11-12 with `curl -sL`, its markup stripped, and each quotation searched in the text. Salesforce's help on setting audit fields at a record's creation, and ServiceNow's on an import set's coalesce, render only by script, so neither is quoted.

| Source | URL | Version |
|---|---|---|
| Microsoft Learn, Run data import (Dataverse) | https://learn.microsoft.com/en-us/power-apps/developer/data-platform/run-data-import | as served |
| Odoo, Export and import data | https://www.odoo.com/documentation/18.0/applications/essentials/export_import_data.html | 18.0 |
