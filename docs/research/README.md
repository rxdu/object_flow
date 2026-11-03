# Research

How established systems solve a problem ObjectFlow faces, kept so that a design choice can follow industrial practice where it applies (the author, 2026-10-22). A research document informs a decision; the decision itself is an ADR in [`../adr/`](../adr/), and the PRD stays the baseline it is checked against.

## What a research document holds

- **The question**, and who asked it when.
- **Findings per source.** Each figure and quotation is checked against the page's raw text, fetched with `curl` and its markup stripped, never against a summarising tool's output (`../LESSONS.md`, "A quote is checked against the page's raw text").
- **Observed, derived and inferred kept apart.** A figure computed here says how it was computed. A conclusion the sources do not state is marked as inferred.
- **ObjectFlow beside the sources**, each figure of ours cited to the record that measured it, with its test conditions.
- **What it suggests**, as a recommendation for the author, not a decision.
- **Sources**: each URL, the date it was retrieved, and a version, release or commit where the page gives one.

## To verify a document again

1. Fetch each source in its Sources table with `curl -sL` into its own empty directory.
2. Strip the markup from the page to plain text.
3. Search the text for each quoted figure and phrase.
4. Record any figure that changed, with the date, beside the finding it changes.

## Documents

- [`write-throughput.md`](write-throughput.md): how Jira and Temporal scale writes, and ObjectFlow's one writer beside them (2026-10-22).
- [`atomic-batches.md`](atomic-batches.md): whether a caller may send several requests that apply together or not at all, as DynamoDB, Salesforce and JSON:API allow and domain-driven design advises against (2026-10-23), how a later request names what an earlier one creates, as JSON:API's `lid` and OData's request identifiers do (2026-11-01), and what an earlier request's effect creates, as OData names a deep insert's entities and Salesforce indexes a response (2026-11-02).
- [`moving-objects-between-types.md`](moving-objects-between-types.md): how a renamed, split or merged type's records follow it, as ServiceNow, Camunda and Jira move them in place and an event store splits a stream (2026-10-24), and how an enumeration is renamed and a reference renamed with its target, as PostgreSQL renames an enum type in place and Django's autodetector repoints a renamed model's fields (2026-11-03).
- [`testing-a-version-over-the-one-before.md`](testing-a-version-over-the-one-before.md): how a consumer tests what an object made under one version does under the next, as Terraform's test applies one configuration over another's state and Temporal replays recorded histories in CI (2026-10-26).
- [`file-media-types.md`](file-media-types.md): how a file field says which media types it takes, as HTML's `accept` hints, OpenAPI describes and Dataverse enforces, and what OWASP says of the stated type (2026-11-01).
- [`offering-creations-on-a-record.md`](offering-creations-on-a-record.md): how a record offers the creation of a related record, as Salesforce's object-specific actions and Dataverse's subgrids are placed on the parent and CMMN's planning tables are decided by the engine per case (2026-11-01).
- [`suspending-an-actor.md`](suspending-an-actor.md): how a platform marks an account that may not act and may again, as SCIM's `active`, Entra's `accountEnabled`, Okta's statuses and GitHub's suspension do, apart from removal (2026-11-02).
- [`reading-history-newest-first.md`](reading-history-newest-first.md): how a reader pages through a record's history from its latest entry, as KurrentDB reads a stream backwards and Stripe's lists answer newest first, and how an index matching the order answers a page unsorted, as PostgreSQL and SQLite describe (2026-11-02).
- [`dry-running-table-changes.md`](dry-running-table-changes.md): how a migration tool's dry run relates to the DDL it applies, as Flyway and Liquibase print the statements and Atlas's dev database and `pt-online-schema-change` run them on an empty copy first (2026-11-03).
