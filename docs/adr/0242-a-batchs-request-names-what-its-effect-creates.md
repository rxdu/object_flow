# ADR-0242: A request of a batch names what its effect creates, as an example's step does

- **Status:** **Decided** 2026-11-02 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #56; built the same day.
- **Date:** 2026-11-02
- **Refines:** ADR-0238 (a request of a batch names what an earlier one creates, by the lid it gave)
- **Relates to:** PRD F2; ADR-0228 (the batch); ADR-0173 decision 6 and ADR-0141 (an example's step names what its effect creates); check 11 (a part is created only through its whole); `docs/research/atomic-batches.md` §7

## Context

A request of a batch names the object an earlier request made by the `lid` that request gave (ADR-0238), and a `lid` names the object its own request makes and nothing else. A composite part is created only through its whole (check 11), so a part is always an effect's creation, and no later request of the batch can name it. Issue #56, filed 2026-11-01 at 0.10.0, has the case: a whole, one of its parts, and a record of a module above them that takes the part cannot be made as one act. The whole cannot create that record, since its module cannot name a type above it.

The whole's answer already lists the part: a satisfied verdict names each object its request created as `created`, in the order made (`wire.md`). An example's request step names those objects already, by `creates`: a list, or a mapping by type, in the order the effect creates them, depth-first (`flow-format.md` §11.3, ADR-0173 decision 6). Salesforce indexes an earlier subrequest's response at each use; OData lets a client name each entity of a deep insert once, where it is created (`docs/research/atomic-batches.md` §7, each quotation found in its source's page).

## Decision

1. **A request of a batch may give `creates`**, names for the objects its effect creates other than its own, as an example's step gives them: a list, for objects of one type, or a mapping from each type to a list, each in the order the effect creates them. Any request may give it, a creation or a transition on an object. Its names share one namespace with the batch's `lid`s: a later request names such an object `{"lid": "<name>"}` wherever it would give the object's id, as ADR-0238 decision 1 has it.
2. **What cannot be resolved before any request runs is refused before any runs**, a `400` naming the request, as ADR-0238 decision 2 refuses a `lid`: a name given twice in the batch, by a `lid` or a `creates`; an empty name; a name a request uses before a request gives it; and `creates` that is no list of names and no mapping from a type to a list of names. A request sent alone takes no `creates`, refused `400` as a key it does not take.
3. **Each name is bound once its request applies**, to the objects that request's answer lists in `created` after its own, in their order: a list to objects of one type, and a mapping each type's to its list, by the type exactly. A request that created fewer objects than it names, of a type a mapping names or in all for a list, or that created objects of more than one type where it names them by a list, is answered `400`, naming the request, before the next request runs and with none of the batch applied, as a request asking an evaluator is answered `501` (ADR-0228). A request may name fewer objects than it creates.
4. **A name so bound is a `lid` in every other way** (ADR-0238 decisions 3 to 5): replaced by the id before the request naming it runs, so that request is judged as the id would be; each request recorded as if sent alone; a refused request's row keeping the name as sent; the batch's key digesting each request's `creates` where it gives one, so a batch giving none digests as it did before, and a key recorded before this release replays as it did; and `check_batch` binding names as `batch` does.
5. **The core alone builds it**, as ADR-0238 decision 6 built `lid`: the reference runner serves no batch. The library's `LocalNames` holds a request's `creates` beside its `lid`.

## Alternatives rejected

- **Salesforce's index into the earlier answer at each use**, `{"lid": "o", "created": 1}`. A position restated at each use, and counting the request's own object first.
- **OData's navigation from the named object**, `{"lid": "o", "via": "slots"}`. It names a part only where the whole holds one, and makes a value a path, where a value is an id or a name.
- **Check 11 relaxed in a batch**, the part requested after its whole. A part is made only through its whole wherever its request is sent.
- **The record making the whole**, the issue's fourth option: the reservation's effect creating the work order. It reverses which record makes which.
- **Refusing before any request runs a type the installed version's effect creates at no depth**, as the recommendation for #56 said. Which types an effect creates depends on the transitions its calls reach, which the store would type outside the checker; a type it read wrong would refuse a batch that applies. Decision 3 refuses the same batch once its request runs, nothing of it applied.

## Consequences

- Built in `request.rs` (`Creates`, `LocalNames.creates`, `names_fault`, `bind_created`, `run_batch`, `batch_digest`) and the service's `batch_of`. `wire.md`, `library-api.md`, `DESIGN.md` §10 and `CHANGELOG.md` say so.
- **Tested on both backends**, the core's tests running on a PostgreSQL sandbox in `check-all`'s PostgreSQL gate: `a_batch_names_what_an_earlier_requests_effect_creates`, by a list and by type, a creation's part and the part a transition on an object adds, a later request acting on it by its name, and a mapping over an effect making two types; `a_batch_naming_more_than_an_effect_creates_is_refused_and_keeps_nothing`, a list naming more than made, a type not made, fewer of a type than named, a list over two types and a refusal after an applied request, each naming its request with nothing kept, its check alike; `a_batch_naming_an_effects_creations_amiss_is_refused_before_any_runs`, each case of decision 2; `an_effects_named_creations_are_kept_as_sent_and_held_by_the_key`, the refused row keeping the name as sent, a replay under the key, the same requests naming other objects `key_reused`, and a batch with a `lid` alone digesting as 0.10.0 did; `a_batchs_check_binds_what_an_effect_creates`; and through the service, `a_batch_through_the_wire_names_what_its_effect_creates`.
- **Planted defects, each caught by those tests:** the names left unbound, the request's own object bound, a list naming more than was made bound short, a list over two types taken, the key digesting no `creates`, a name given twice taken, a mapping's type not held to, and the service dropping `creates`.
