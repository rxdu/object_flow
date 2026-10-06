# ADR-0160: What diagnostics selects, the erasure's walk down, and the attempt counts export

- **Status:** Decided by the author, 2026-10-06 ("go with your recommendations on 1 to 3"). The message the author answered had posed the three as questions without recommending, so the choices in decisions 1 and 3 were formed after the approval, as the author's delegation; they are marked for the author's review. Decision 2 is the record's own: the question was raised without finding `DESIGN.md` §7 and §8 (ADR-0147), which answer it.
- **Date:** 2026-10-06
- **Refines:** ADR-0085 (diagnostics), ADR-0094 and ADR-0106 (the `attempt_counts` export)
- **Relates to:** ADR-0100 §3 and ADR-0147 (the erasure's walk down and the `redact` event); `DESIGN.md` §7, §8, §10; `design/declaration-syntax.md` §6.9, §6.11; `design/library-api.md` §5, §6; `design/storage-schema.md` §9

## Context

Building the core found three things to settle before building them.

- `DESIGN.md` §10 and ADR-0085 list what `diagnostics(type, window)` reports as "a fixed, deterministic list" built on the standard metrics, and PRD V2 words it: transitions and states never used, rules that refuse most, states reached mostly by override, unmodelled information recorded often, states where work waits longest. No document says how many rows "most" returns, what share is "mostly", which measure ranks a wait, or what counts as backdated.
- The erasure's walk down (`storage-schema.md` §9 step 12) was asked about as unspecified. *(It is not: `DESIGN.md` §8 says every object whose value an erasure redacts, an attribute it holds or a value in one of its past events, gets an event with source `erased`, caused by the erasure's own event, naming what it redacted and never the values, its version advancing, and such an object is not itself erased; §7 names that event `redact`. The question missed both, and the walk up already built (`storage-schema.md` §9 step 3) records no such event, which this decision's consequences repair.)*
- `library-api.md` §6 lists `<Type>.attempt_counts` as an export source. *(Its rows' shape is `declaration-syntax.md` §6.9's, which the question missed: day, object, transition, verdict, clause, remedy, actor kind, enforced, flagged, declaration version and count.)* What no document gives is how it pages: a count grows while its day lasts, and a pruned count, in the rollup, has no event to page by.

## Decision

1. **`diagnostics(type, window)` reports, in this order, each kind's rows in descending measure and then by subject**, over the window ending now, by occurred time for events and intervals and by recorded time for attempt rows, each row a `Diagnostic` of `library-api.md` §5:
   - `unused_transition`: each transition of the type that took no event in the window;
   - `unused_state`: each state no object entered in the window and none is in now;
   - `top_refusal`: each clause that refused an enforced request in the window, by remedy and kind of actor, its count the measure, every such clause listed, since the type's declared clauses bound the list;
   - `state_entries`: each state's entries in the window, by transition, by override and by creation (ADR-0130), the measure the entries by override; and `mostly_override`, each state more than half of whose entries in the window were by override;
   - `label_frequency`: each label name applied to the type's objects in the window, its count the measure, and `label_by_state`, each name with the state its subject was in;
   - `longest_wait`: each state, the median time its intervals entered in the window lasted, an interval still open counting to now, every state listed;
   - `rework`: each state re-entered in the window, its re-entries the measure;
   - `reassigned_back`: each assignee an object was handed back to in the window, its count the measure, the subject naming an actor;
   - `acted_by_non_assignee`: each transition taken in the window by an actor other than the object's assignee, the share of its events so taken the measure;
   - `backdating_rate`: each transition, the share of its events in the window whose occurred time precedes their recorded time;
   - `would_be_refusal`: each observing clause's would-be refusals in the window, and `top_flag`, each flagged clause's raised flags;
   - `pending_file_deletion`: each file deletion still pending, which the core, holding no files yet, reports none of.
2. **The walk down is `DESIGN.md` §8's** (ADR-0100 §3, ADR-0147). From each of the erased object's events, each event it caused on another object is followed: the step that caused it maps what carried the erased values, the tainted inputs and the erased object's personal attributes, into the caused event's inputs, and those that write a personal attribute are redacted in the caused event; the attribute itself is cleared on the object only while the object still holds what flowed in, its last write being that event, a value it has since replaced staying. The walk continues down from each caused event. Every other object whose value is redacted, an attribute it holds or a value in a past event, up the cause chain or down it, gets one `redact` event, source `erased`, caused by the erasure's event, naming what it redacted; its version advances, and it is not itself erased. *(The reading that a replaced value stays is this decision's; §8 says the walk erases "what flowed", which a replaced value no longer holds.)*
3. **`<Type>.attempt_counts` exports one row per day and key, `declaration-syntax.md` §6.9's, once its UTC day is over**, the rollup's counts and the remaining rows' together; the cursor is the last day exported. A day's counts are settled once the store's clock has passed it, as attempt rows are dated by when they were recorded, so a row never changes after it is exported, and a prune, which moves a count without changing it, changes nothing a consumer read.

## Alternatives rejected

- **For decision 1, a cut-off, the first ten clauses or waits.** A type's declared clauses and states bound each list, and a cut-off would hide the eleventh rule that refuses; the order already puts the most first.
- **"Mostly" as a plurality, override the largest of the three routes.** A state entered by override a third of the time beside two other thirds is not reached mostly by override.
- **The longest single wait rather than the median.** One stuck object would mark its state, where PRD V2 asks where work waits.
- **Backdated as beyond a margin.** Any margin is a number to choose per deployment, and a transition backdated by a minute still records the mistake ADR-0085 watches for.
- **For decision 2, clearing the attribute whatever it holds now.** It would erase a value the object was given later, from another source.
- **For decision 3, re-emitting a count as it grows**, by the transaction of the latest attempt it counts. A pruned count has no transaction to page by, so the rollup would need a column, and every consumer would have to keep the latest of each key.
- **Exporting today's counts.** They would change, and a consumer would have to replace them.

## Consequences

- The core serves `diagnostics`, its rows `library-api.md` §5's `Diagnostic`, and a cited diagnostics read in a draft's evidence reads it.
- The core's erasure walks down, and records a `redact` event on every other object it redacts, up the cause chain as well as down.
- The core exports `<Type>.attempt_counts` by settled day.
- `library-api.md` §6 states the export's order for counts and what `diagnostics` returns; `DESIGN.md` §10 cites this ADR.
