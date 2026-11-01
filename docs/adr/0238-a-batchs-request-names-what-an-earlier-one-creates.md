# ADR-0238: A request of a batch names what an earlier one creates, by the lid it gave

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #43; built the same day.
- **Date:** 2026-11-01
- **Refines:** ADR-0228 decisions 4 to 8 and 12 (the batch and its check), whose Consequences left such a name out "until a case needs it, as JSON:API's `lid` allows"
- **Relates to:** PRD F2; ADR-0181 decision 2 (a refusal names no object that does not exist); D581; `docs/research/atomic-batches.md` §6

## Context

A batch applies its requests whole or not at all, in one transaction (ADR-0228). Each request is what `request` takes, and a request names an object by its id, which the store mints when it creates the object (`DESIGN.md` §2). So a later request of a batch cannot name an object an earlier one creates, and two creations where the second references the first cannot be one act. Issue #43, filed 2026-10-31 at 0.8.0, has the case: a record and a second record that references it, kept apart because fewer people may read it. Sent one after the other, a refusal of the second leaves the first without the record its person sent with it.

The issue asks for a reference by the earlier request's position. Every platform read that lets a later request of one call name what an earlier one created names it, none by position alone: JSON:API's `lid`, Salesforce's `referenceId`, OData's request identifier and Dataverse's Content-ID. OData forbids a reference to a later request, and Dataverse answers one `400` (`docs/research/atomic-batches.md` §6, each quotation found in its source's raw page).

## Decision

1. **A request of a batch that creates may give a `lid`**, a name its caller chooses. A later request names that object `{"lid": "<name>"}` wherever it would give the object's id: as its `object_id`, as a reference input, or as an element of a set of references. A value holding more than the one member `lid` is no name, and is judged as any value is.
2. **What cannot be resolved is refused before any request runs**, a `400` naming the request: a `lid` given twice; a `lid` on a request that creates nothing, one with an `object_id` or naming its object by a `lid`; a name no earlier request gives, a request's own among them; a name that is not a non-empty string; and an object named both by an id and by a `lid`. A request sent alone takes no `lid`, refused `400` as a key it does not take.
3. **The name is the id, before the request runs.** In the batch's transaction, each name the request holds is replaced by the id of the object its request made, and the request is then judged as it would be with that id: every guard, invariant, effect and check on what a batch holds (ADR-0228 decision 10) reads the id. A name adds no rule and bypasses none, and a reference to an object of another type is refused `no_such_object`, as the id would be (F2).
4. **Each request is recorded as if sent alone** (ADR-0228 decision 7). An applied request's event holds the id in its `payload.inputs`. A refused request's attempt row keeps its inputs as it sent them, the `lid` among them, so the row names no object the rollback unmade (ADR-0181 decision 2), where the id would name one a later object then takes. The batch's key digests each request's names with it, so the same requests naming other objects are another batch under the key, `key_reused`. A request without names digests as it did before them, so a key recorded before this release replays as it did.
5. **`check_batch` resolves names as `batch` does**, its run rolled back whole (ADR-0228 decision 12).
6. **The core alone builds it.** The reference runner serves no batch, deferred on 2026-10-25 until a case needs the batch in the sandbox or as its oracle (`TODO.md`, *The reference runner serves no batch*). The library keeps `batch` and `check_batch` as they were, and `batch_named` and `check_batch_named` take each request's names by its index, so no caller of the library changes.

## Alternatives rejected

- **A reference by the earlier request's position**, as the issue asks. A position shifts silently when a caller adds or moves a request, and no platform read uses one alone. The batch's answers stay positional, as JSON:API's results are.
- **Salesforce's `@{name.id}` inside a string.** A string input could hold that text as its value, so a value's meaning would depend on what it says.
- **Recording the name in the applied event.** The event would differ from the one the request sent alone writes, and a reader of the history would need the batch to read it (ADR-0228 decision 7).
- **The refused row naming the resolved id.** The rollback unmakes the object, and in a sandbox the next object takes the id, so the row would name another object (D581).
- **Names on every `Request`, sent alone as well.** A request sent alone has no earlier request to name, and the field would change every caller of the library.

## Consequences

- Built in `request.rs` (`LocalNames`, `names_fault`, `resolved`, `batch_named`, `check_batch_named`), `store.rs` (`keep_sent_inputs`) and the service's `batch_of`. `wire.md`, `library-api.md`, `DESIGN.md` §10 and `CHANGELOG.md` say so.
- **Tested on both backends**, the core's tests running on a PostgreSQL sandbox in `check-all`'s PostgreSQL gate: `a_batch_names_what_an_earlier_request_creates_by_its_lid`, as the object acted on and as an input, a value holding more than a lid judged by its type; `a_sets_elements_name_what_earlier_requests_create`; `a_batch_naming_what_it_cannot_resolve_is_refused_before_any_runs`, each case of decision 2 for the batch and its check, nothing written; `a_refused_request_keeps_its_lids_as_sent`, the row's inputs as sent and no unmade object named, nothing of the batch kept; `a_batchs_key_holds_its_local_names`, a replay and the same requests naming another object refused `key_reused`; `a_batchs_check_resolves_its_local_names`; and through the service, `a_batch_through_the_wire_names_what_it_creates_by_lid`.
- **Planted defects, each caught by those tests:** the digest ignoring names, the refused row keeping the resolved ids, no refusal before the batch runs, a set's elements left unresolved, the target left unresolved, a name accepted before it is given, a `lid` on a request that creates nothing, and a `lid` given twice. A replay resolving no names was not caught, since a replay reads its requests only to tell a refusal, and a replayed batch applied; the resolution there was removed as reading nothing.
