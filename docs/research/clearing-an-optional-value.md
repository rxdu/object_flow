# Clearing an optional value in one update

## The question

Issue #71, filed 2026-11-06 at 0.18.0, asks whether one transition may clear an optional attribute. An edit form that shows an optional field has no way to say "no value now": a null is absence (ADR-0122 decision 10), so an optional input sent as null is not supplied, and only a `clear` step removes a value. Two transitions, one that sets and one that clears, are the format's answer (decision 11). The issue asks how established update interfaces tell "leave it" from "clear it" in one request.

## Findings per source

Each quotation was checked on 2026-11-06 against the page's raw text, fetched with `curl -sL` and its markup stripped, each tag removed.

**JSON Merge Patch, RFC 7396.** A patch names only what changes; a member left out is unchanged, and a member given as null removes the value:

- "If the target does contain the member, the value is replaced. Null values in the merge patch are given special meaning to indicate the removal of existing values in the target."

**GraphQL, the October 2021 specification.** Coercing an input object tells an omitted field from one given null:

- "Otherwise, if the field is not required, then no entry is added to the coerced unordered map. If the value null was provided for an input object field, and the field’s type is not a non-null type, an entry in the coerced unordered map is given the value null. In other words, there is a semantic difference between the explicitly provided value null versus having not provided a value."

**JSON:API 1.1.** An update that leaves out an attribute keeps it:

- "If a request does not include all of the attributes for a resource, the server MUST interpret the missing attributes as if they were included with their current values."

**Google's API Improvement Proposals, AIP-134 and AIP-161.** An update names the fields it writes in a field mask, apart from their values:

- AIP-134: the update request's field "must be called update_mask", and "the service must treat an omitted field mask as an implied field mask equivalent to all fields that are populated (have a non-empty value)."
- AIP-161: "If a user updates a resource with a given mask, and then reads the same resource with the same mask, the exact same data must be returned."

*Inferred, not stated:* a field the mask names and the request leaves unset reads back unset, so the mask clears it. That follows from AIP-161's read-write consistency; neither page says "cleared" in those words.

## Observed, derived and inferred

- Observed: all four tell a value left out, kept, from one written. Merge Patch and GraphQL carry "clear" in the value, a null where the field's type allows one. The AIPs carry it in a list beside the value, the mask.
- Inferred: none lets a caller clear what the interface does not let it write. GraphQL's null applies to a field "not a non-null type", and a mask names fields the resource declares.

## ObjectFlow beside the sources

Reproduced 2026-11-06 on the debug service's sandbox, at `24bafea`, on a flow of the repository's own: a type with `note`, optional, and `marked`, optional and `not_blank`, each first `"first"`; `edit` takes both as optional inputs, and `drop_note` is `clear: [note]`. `edit` with `note: null` or `marked: null` was answered `satisfied` and changed nothing; `marked: ""` was refused `invalid_input`, `malformed`; `note: ""` wrote an empty text; `drop_note` cleared the note. So ObjectFlow keeps JSON:API's half, a value left out is kept, and has no single-request way to clear.

## What it suggests

A recommendation for the author, not a decision:

1. **A transition lists the optional inputs a null clears**, as GraphQL's nullable field and Merge Patch's null do, declared in the flow rather than chosen by each request, since the flow says what each transition may write. Left out, the value is kept, as JSON:API and the field mask's omitted fields keep it.
2. **Every other input keeps decision 10's meaning**, so no flow changes behaviour without saying so.
3. **The offer names the clearable inputs**, as a schema says which fields are nullable, so a form can tell which emptied field to send as null.

## Sources

| Source | URL | Retrieved | Version |
|---|---|---|---|
| RFC 7396, JSON Merge Patch | https://www.rfc-editor.org/rfc/rfc7396.txt | 2026-11-06 | October 2014 |
| GraphQL specification | https://spec.graphql.org/October2021/ | 2026-11-06 | October 2021 |
| JSON:API | https://jsonapi.org/format/1.1/ | 2026-11-06 | 1.1 |
| AIP-134, Standard methods: Update | https://google.aip.dev/134 | 2026-11-06 | changelog to 2026-10-08 |
| AIP-161, Field masks | https://google.aip.dev/161 | 2026-11-06 | changelog to 2026-10-08 |
