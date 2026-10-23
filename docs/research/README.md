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
- [`atomic-batches.md`](atomic-batches.md): whether a caller may send several requests that apply together or not at all, as DynamoDB, Salesforce and JSON:API allow and domain-driven design advises against (2026-10-23).
