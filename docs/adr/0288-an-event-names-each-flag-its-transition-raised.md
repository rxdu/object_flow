# ADR-0288: An event names each flag its transition raised

- **Status:** **Decided** 2026-11-18 under the author's direction ("there is an issue #100, read and handle it properly"), on the recommendation formed the same day for issue #100, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-18
- **Refines:** ADR-0111 decision 3 (the event names a raised flag)
- **Relates to:** ADR-0083 (the attempt log); ADR-0100 (its prune); ADR-0158 (what a guard consulted, held in the payload); ADR-0209 (`wrote`); issue #100

## Context

ADR-0111 decision 3 says a raised flag "is written to the attempt log, marked `flagged` and linked to the event that applied, and the event names it". `DESIGN.md` §5.5 and §6 step 4 say the same. `storage-schema.md` puts the flags in the event's payload, "so what a request was told outlives the prune of its attempt rows".

The core never wrote them there. The verdict, the idempotency record and the attempt log held each flag, and the wire listed an event's `flags` as not computed. The attempt log is pruned after its retention, and its rollup keeps counts alone. Once an act's attempt rows were pruned, nothing said which warning it passed, while its event kept the caller's reason in `context`. Issue #100 found this; it was reproduced at `6429346e` (`TODO.md`, *Issues filed on GitHub*).

`library-api.md` typed `Event.flags` as clause names. A name alone drops two things the caller was told: whether the clause was false or unknown, having read something absent, and which objects its remedy points at.

## Decision

1. **An event's payload names each flag its transition raised**, under `flags`, in the shape a verdict gives a flag: `{clause, remedy, unknown, objects}`. A payload holds no `flags` where its transition raised none.
2. **A flag is on the event of the transition whose guard raised it**, the event its attempt row links to. Its clause is named as that transition names it, unqualified. A verdict qualifies another type's clause because it answers the whole request; an event names its own type and transition.
3. **`history`, `events` and `pull` give an event's `flags`**, `[]` where it names none, and no longer list `flags` as not computed. `export` gives the payload as stored, `flags` within it, as it gives `consulted`. An erasure leaves the flags, which hold no personal value, and archival, once built, carries them with the payload.
4. **An event a 0.30.0 or earlier service recorded names no flag.** Its flags stay in the attempt log until pruned, each row's `applied_position` naming its event, without the objects its remedy named, which an attempt row does not keep. A deployment keeps them by exporting its types' `.attempts` sources first. The operator's `upgrade` event to the first release after 0.30.0 marks where a store's events begin to name their flags. Nothing rewrites an older event's payload, as nothing wrote `wrote` into an event recorded before 2026-10-16.

## Consequences

- `library-api.md` types `Event.flags` as a sequence of `Flag`. `storage-schema.md` and `wire.md` say what the payload holds, and that an older event names none.
- A payload grows by one object for each flag its transition raised, and not at all where it raised none. A flag's `objects` lists what its remedy names: each object a reference in its clause reaches, and, for an aggregate, each element its filter selects. A flag over a type-wide count therefore holds every matching id. The same event's `reads` already records each object its rules read and each scan's matches, so the flags add a second copy of those ids at most. The page bound counts them, as it counts every byte of an event.
- The flags a whole request raised are its events' flags together, the events its verdict lists.

## Alternatives rejected

- **A flag without its objects, as an attempt row keeps it.** The event would lose which objects a dependent remedy pointed at, as clause names alone would (below). Their size is bounded by what the event's `reads` already holds.
- **At most some number of objects, with a count of the rest.** It records less than the caller was told, for a bound no measurement has asked for.
- **Clause names alone, as `library-api.md` typed them.** They drop whether a clause was unknown and the objects its remedy names. The caller was told both, and no later read can recompute them without evaluating the clause anew, under a declaration that may since have changed.
- **The request's own event naming every flag it raised, qualified as the verdict names them**, as the issue read it. A call's flag would sit in the history of the object requested, apart from the call's own event, to which its attempt row links.
- **An older event answered as unknown where its transition declares a `warn` guard.** Every read would consult the declaration of each event's version, for good, to qualify events a 0.30.0 or earlier service wrote. A sentence in `wire.md` serves the reader, as it does for `wrote`.
- **The upgrade copying retained flagged attempt rows into older events' payloads.** It rewrites recorded events, where the log is append-only but for an erasure (`storage-schema.md` §2), and it still misses every row already pruned.
- **Keeping flagged attempt rows from the prune.** The flag would stay in `of_attempt`, which is not history, and outside the event that `pull` gives and archival carries.
- **A column of its own on `of_event`.** It needs a store upgrade for what the payload already holds by design, beside `consulted`.
