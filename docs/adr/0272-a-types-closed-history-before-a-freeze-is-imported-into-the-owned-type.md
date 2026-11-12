# ADR-0272: A type's closed history before a freeze is imported into the type the store owns

- **Status:** **Decided** by the author 2026-11-12 ("accept, go ahead"), as proposed the same day for issue #89, which the author directed be addressed in 0.24.0 ("address #89 in 0.24.0"; "keep #89 in 0.24.0"), by the direction recommended that day: a create-only import into an owned type, with an explicit window. Revised twice before it was put, after a specialist look for data integrity found nine holes and two more in the revision, and a cold read. To be built for 0.24.0; built the same day. Three points of the build the text leaves open: the store's record of a closed root is the versions it already holds, each pair of consecutive versions showing a close, rather than a record of its own; decision 2's row is kept as a digest on its object's import event, which an erasure of the object replaces with `erased`; and decision 3's report is the import report's `created`.
- **Date:** 2026-11-12
- **Amends:** ADR-0101 decision 1 ("the import writes only mirrors"): it adds one exception, decision 1 below.
- **Relates to:** ADR-0015 (one writer of a type); ADR-0075 (staged cutover); ADR-0086, ADR-0106, ADR-0163 (what an import keeps of a legacy record); ADR-0154 (the disposition file); `docs/research/backfilling-history-into-a-live-store.md`; issue #89

## Terms

- **The import** writes another system's records into the store, one batch at a time, all or nothing (`publish-and-import.md` §4).
- **A legacy key** is the identifier the retiring system gave a record. The store keeps it qualified by the type's **root**, the base at the top of the type's family (`Unit:U-77`).
- **A mirror** is a type the import alone writes and no request transitions, until a version **cuts it over**, making it owned (`flow-format.md` §4.15). **An owned type** is one requests transition.
- **A class** is a kind of violation a batch's dry run finds, such as a broken invariant, and the **disposition file** decides each class: cleaned upstream, excluded, admitted with a reason, or a reference dropped. **An admission** records its reason, which `exceptions()` reads (`publish-and-import.md` §6).
- **A legacy record's history** is what the import keeps of it: its creation time; the time it **entered** its state and each tracked value; earlier states and values as **intervals**; the **silent spans** its record says nothing of; and its **legacy entries**, fields of its audit trail (ADR-0086, ADR-0106, ADR-0163).
- **D261 and D251**, which ADR-0101 closed: an import could override an object of an owned type, marking it `imported` and hiding it from `exceptions()`; and an import could set a state the type's guards would refuse, on objects users then act on.

## Context

Issue #89, filed 2026-11-12 at 0.23.0, asked for a legacy import into a type the store owns. Its deployment freezes its legacy system: from the freeze, new records are made in ObjectFlow alone, and the history is exported, cleaned and imported batch by batch over weeks. Under ADR-0101 decision 1 the import writes only a mirror, and a mirror takes no transition. So a type either takes new records or receives its history: the business stops recording until the cleaned import lands, or keeps twin types.

A mirror exists because a legacy system still writes: the store refreshes it until the type is cut over, and ADR-0015 forbids two writers. A frozen legacy system writes nothing after the freeze, so it needs no refresh. What remains is a one-way backfill of finished history into types the store owns.

Under a mirror, the publish that cuts it over reports every imported object, and a second person approves it, before any user acts on one. That is what keeps D251's harm away. An object the import created in an owned type, still open, would be one users act on at once, its guards never run. So a backfill must bring only what no user acts on: a record the legacy system finished. Work still open at the freeze is needed from the first day, so it cannot wait weeks for cleaning anyway; it arrives through today's mirror and cutover.

The research note finds the pattern in Dataverse and Odoo, which import a retired system's records into the tables live records use, keeping its creation time and its identifier. Odoo's import updates a record its identifier names; a store whose owned objects only its users write takes the creation alone.

## Decision

1. **A type the store owns may open its history before a freeze**: `history: { before: <instant> }`, the instant the legacy system froze. While a version declares it, the import may create objects of the type, each in a final state. Each row must give its creation time and the time it entered its state. Every time it gives, those and each interval, silent span, legacy entry and occurred time, must be before the freeze. A row in a state that is not final, or missing either time, or giving a time at or after the freeze, is refused. So a backfill brings finished history alone: an object no user acts on, except by an erasure or an assertion its flow declares. *(Closes H1, and D251's harm.)*
2. **The import only creates an object of such a type.** A row whose legacy key an object of the type holds is compared with the row that created that object, which the import's event records. An identical row writes nothing, so a batch sent again is a no-op. A differing row is a class, `legacy_key_held`, which only exclusion decides. A row cleaned again after its object was imported is corrected by the flow, an assertion or a correction a person takes, never by the import. *(Closes D261 for the type, and H9.)*
3. **Each object passes the checks a mirror's import makes**: the built-in assertion that writes it, the type's invariants, uniqueness against every object of the type, live ones included, and references by legacy key. Each class is decided by the disposition file, and each admission carries its reason. Guards are not run, as for a mirror's import, which is safe because the object is finished. The batch's dry run reports every object it would create.
4. **A row names another object by its legacy key**, and resolves only to an object an import created. A legacy row naming an object the store made since the freeze names nothing to resolve: the reference is a class, dropped or the row excluded. An observation or a label the import writes, and a legacy entry's `about`, are on an object the same batch creates, never on an object of the store users act on. *(Closes N1. That a request cannot write a key the import holds, and an entry's `about` cannot reach an owned object today, are fixed apart from this ADR, H3 and H4 in `TODO.md`.)*
5. **The window is opened once and closed for good.**
   - A version opens a type's history only where the type becomes owned: where it is new, or a mirror being cut over.
   - `before` is no later than the publish that opens it, and no later version changes it while the history is open (step 3, against the installed version). *(Closes N2.)*
   - The store records a type's root closed when a version drops the marking, or removes, retires or moves out the type. A version opening history on a root the store records closed is refused.
   - A version moving objects into a type whose history is open, or opening it on a type that receives moved objects, is refused.
   - *(Closes H5.)*
6. **History is open alike across a family and a composition.** A base and each subtype, and a whole and each type of its parts, declare the same `history` or none, which step 3 checks. A part is created with its whole in the same batch, and a part naming a whole an earlier batch created is a class. So a whole and its parts arrive in one batch, and a composition's cleaning is done together. A mirror is never a part of such a type, as check 53 already holds. *(Closes H2.)*
7. **A sequence is raised past the legacy system's last value at the freeze, by a batch of its own, before a number is minted here**, as a batch's `sequences` raise one today. An identifier a sequence mints on a type with its history open is `unique`, or `unique: in_scope` where its numbering is scoped, which step 3 requires. So a missed raise shows as a uniqueness class, which holds the legacy row until the deployment excludes it or frees the number. It is never a silent duplicate. *(Closes H8.)*
8. **An object made since the freeze that duplicates a legacy record is the flow's to resolve.** The legacy row's unique value a live object holds is a uniqueness class, which no admission passes, since the live object's invariant would fail. The deployment excludes the row; or, by an act of its flow, frees the live object's value, imports the row, and links the two as its flow declares, such as by a reference from the live object to its legacy record. The import does not merge. *(Closes H6.)*
9. **A person erased since the freeze is removed from each batch upstream**, against the deployment's register of erasures, applied again before each batch, so an erasure between the export and an import is kept too. The import cannot tell that a legacy row is about an erased person, and a disposition decides a class of rows, never one person's. This is the deployment's obligation, which the operating guide states. *(Closes H7.)*
10. **Figures change as history lands, and a reader can tell.** A backfilled object's intervals count in metrics read over time, as a mirror's do, and its import's events are not counted as flow (ADR-0101). A figure over a type whose history is open may change until the history closes; the version's `history` says which types and from what instant, and the operating guide says how to read them.

## Alternatives rejected

- **Import first, recording paused**, as today's rule allows. The business stops recording for as long as the cleaning takes.
- **Owned twins beside mirrors.** Two types for one thing: every flow written twice, an object's history split.
- **A mirror that takes transitions**, or **create or update by legacy key**, as Odoo's import does. A refresh would update what users recorded, D261 reopened.
- **Open objects backfilled too**, as first drafted, bounded by time alone. Users would act at once on objects whose guards never ran, with no report a second person approved: D251's harm. Open work at the freeze takes the mirror's cutover, which reports it.
- **A second person approving each backfill batch**, so open objects could come in. It adds an approval path to the import where finished history needs none.
- **The window opened and closed by an operator's act at run time.** It needs a new record and write path, where a version already gates and records the change (PRD F2).
- **The window closed after a time, or at a type's first ordinary write.** A boundary a person cannot see before an import runs, which ADR-0101 rejected for the same reason.
- **The disposition deciding each erased person's rows.** A class is decided by rule and shape, and erasure deletes the personal identifier a row would be matched by.

## Consequences

- **The format and both checkers.** `history: { before: <instant> }` goes on a type, in `flow.schema.json` and `flow-format.md` §4.15. Step 3 holds decisions 5, 6 and 7; the publish holds decision 5's closed roots.
- **The store** records a type's root closed, as part of the publish that closes it.
- **The import, in the core and the reference runner**, takes an owned type with its history open, create-only, with decision 1's bound, decision 2's no-op and class, and decision 4's rules.
- **`publish-and-import.md` §7** gains a freeze's cutover beside a staged one. **The operating guide** gains the freeze, the raise, the history's closing, decision 9's obligation and decision 10's reading.
- **Tests**, on both backends, for each decision, and planted defects in each checker and runner.
- **What a flow meets.** A module without the key reads as before.
- **Not done, listed:**
  - A backfilled legacy record is never the one superseded, since supersession runs from a state that is not final (`flow-format.md` §4.19).
  - The engine checks no erasure register; decision 9 is the deployment's.
