# ADR-0101: Closing what the repairs left: the import, the approval, migrations, a mirror's erasure, and what the standard metrics count

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` F7, D4, D8, T1, T3, T4, T5, M1, M3, M4, M6 and UC-16, UC-17, UC-18; repairs D261 to D274
- **Date:** 2026-09-23
- **Refined by:** ADR-0103 — the store is also built with an idempotency retention, which `maintain` will not prune inside. ADR-0106 — a prune keeps a would-be refusal while its clause is on trial.
- **Refines:** ADR-0056, ADR-0075, ADR-0083, ADR-0097, ADR-0098, ADR-0099, ADR-0100

## Context

The five reviewers of ADR-0097 to ADR-0100 were asked to verify their findings against the repairs. Almost all were closed. The findings below are what remained, or what the repairs introduced (`design/defects.md` D261 to D268).

- **D261.** The import could write a type "no ordinary request has created an object of". A type that had been cut over, with all its objects ported and transitioned daily, stayed importable until someone created one. A refresh could therefore still override an owned type, marked `imported` and hidden from `exceptions()`. `ImportBatch.admitted` also carried no reason, which `ok_admission.reason` requires.
- **D262.** `publish` compared the recomputed report against whatever report was attached at commit. The drafter could `refresh` between the approver reading one report and approving, and the approver would install an impact they never read.
- **D263.** `DeclarationChange` declared no visibility. Its report named object ids and its evidence held metric values, so any reader saw what ADR-0100 hides from a verdict.
- **D264.** `maintain(prune_attempts)` had no floor. A mistaken `before` deleted refusal records inside the retention T3 requires.
- **D265. Migrations were underspecified.**
  - Nothing could backfill a new required reference or part.
  - Nothing said whether a migration reaches a terminal object, which "admits no further transitions", or an observation, which D4 says is never edited.
  - A `backfill` copying a personal value into a non-personal attribute escaped check 10.
- **D266.** A `mirror` declares no transitions, `erase` included, so the first consumer's customer could not be erased while it was a mirror (UC-17).
- **D267. Four standard metrics counted the wrong thing.**
  - Most counted the import's own events, so every ported job "completed" in cutover week, and the importer counted as a non-assignee.
  - A span still open on a finished object — an engineer on a closed job — grew with `now` for ever.
  - No standard metric named the jobs that changed hands more than once, which UC-18 asks for.
  - Most could not be split by version or actor kind, and a combined metric has no binder to filter by.
- **D269 to D273**, found by a further verification pass over this decision's first draft:
  - porting every type as a mirror contradicted check 53, which forbade a mirror to compose or extend, so no delivery and none of its parts could be ported, nor any legacy datapoint;
  - `publish`'s version was optional, and its result had no verdict to carry a refusal;
  - clipping a finished object's spans at its first completion made a value written after it negative;
  - `metric()` aggregated over every dimension a reader left unbound, so the per-object metrics collapsed to one value, and those metrics' values were not aggregates;
  - a publish's migrations were counted as flow.
- **D268. Smaller residues.**
  - `whole_open` would have refused an observation's correction.
  - `ok_migration` could not store the new mappings.
  - The state was a tracked member in one section and not in another.
  - `.open` and `.imported_at` had no spelling in the language.
  - The standard metrics were a table the checker never reads.
  - Several documents and shapes had drifted.

## Decision

### 1. The import writes only mirrors

`import_batch` writes a type only while it is declared `mirror`, together with the observations and labels on a mirror's objects, so legacy datapoints port with their subjects. Every type is ported as a mirror and cut over by the publish that removes the marking, which is ADR-0075's mechanism; a big-bang port is one such publish over every type. Once a type is owned, no import reaches it.

Mirrors may therefore compose with and extend each other, and check 53 refuses only a mix: an owned type composing with or extending a mirror. A composition or a family is cut over by one publish. An observation kind's collection on a mirror is not a mix, since an observation is this store's record about the object; but only the import writes one there, and a generated guard, `subject_owned`, refuses a `record` or a `label` request on a mirror's object, so a mirror has one writer until it is cut over.

Every admission in a batch carries its reason. The import never writes a personal attribute of an object whose erasure is recorded, and redacts the legacy entries and intervals it attaches to one as the erasure would have, so a refresh cannot bring back an erased person.

### 2. An approval names the version of the change it read

`publish(actor, change, expected_version)` refuses as `stale` if the change has moved since the approver read it, and `refresh` advances its version. The version is required for any publish but a dry run. So the report compared at commit is the report the approver was shown. `publish` returns the verdict on the transition beside the report, so a stale or refused approval says which clause refused it.

### 3. A flow change is visible to those who draft and approve, and its report is filtered like a verdict

`DeclarationChange` is visible to actors holding `OK_DRAFT_CHANGE` or `OK_APPROVE_CHANGE`. Within it, the report's object ids and the evidence's values are shown only as far as the reader's visibility reaches, with `withheld` set otherwise, as for a verdict (ADR-0100 §4).

### 4. Pruning has a floor

The store is constructed with the deployment's attempt retention. `maintain` refuses a prune whose `before` falls inside it, and reports what it removed. `DESIGN.md` says what it changes: attempt rows past retention, whose counts the rollup keeps, and nothing else any read returns.

### 5. A migration reaches every affected object except an observation

- A publish's migrations reach every object the change affects, terminal ones included. A migration is the publish's own recorded change, not a transition the object takes, so it is the second exception, beside erasure, to "a terminal object admits no further transitions".
- An observation is never migrated, being born final (PRD D4). A publish that removes an enum member an observation holds is refused.
- `backfill` fills a new required attribute or singular reference, from the object's own members and literals.
- A new required singular part on a type with live objects is refused, since filling it means creating an object per whole. It is declared optional, or set-valued.
- Check 10's taint analysis covers a `backfill` expression, as it covers any write.

### 6. A mirror may declare `erase`

`erase` is the one transition a mirror may declare (check 53). Erasure is this store's obligation over its own copy, not a write of the state another system owns. With §1, an erased mirror stays erased through every later refresh.

### 7. The standard metrics count only the store's own flow, clip finished spans, and name objects

- **The import's events and a publish's migrations are excluded.** Every standard metric over transitions leaves them out (`not t.imported and not t.migrated`). They are the port and the publish, not the flow. A ported object's history enters the metrics through its legacy intervals, which are marked as such.
- **A finished object's open spans stop where it finished.** `.duration` of a span is its exit less its entry. A span still current runs to `now` while its object is open. On an object now in a `closed` or terminal state, one that began before the object entered that state runs to that entry, and one that began after it, like a span in that state itself, has no duration; an aggregate leaves out a row whose body is absent. So nothing accrues on a finished object, and no duration is ever negative.
- **Two standard metrics name the objects.** `handoffs_by_object.<r>` and `returns_by_object.<r>` group by the object itself, with flags `changed_hands_more_than_once` and `returned_to_earlier`. **A reader keeps every declared dimension by default**, getting one row per group, and may name fewer with `keep`; only a guard aggregates over what it leaves unbound, since a guard needs one value. So `metric()` returns the jobs UC-18 asks about, one row each.
- **Every standard metric can be split by version and actor kind.** Each declares `version` and `actor_kind` as dimensions, taken from its rows, which a reader drops with `keep` to aggregate over them. A combined metric passes them through from the metrics it combines.
- **Definitions are tightened.** `.returns` counts a transition that changes state into one the object held before, so an `act` is not rework. `avg` yields a `decimal`. Every tracked member has an interval from creation, absence included, so a job never assigned counts as waiting from its creation.

### 8. The residues

- `whole_open` guards a re-parent, a `do` or `act` writing `owner` on an existing object. A creation is covered by check 11, and a recording by `subject_open` with its correction exemption.
- The state is a tracked member everywhere (§8.3, check 58).
- `.open` and `.imported_at` are members every object and every mirror object has (§8.1), reserved by check 33.
- The standard metrics are also given as a text block instantiated for `ServiceJob`, which the checker runs.
- `ok_migration` records every mapping kind.
- `TransitionOffer` names the type an offered creation creates.
- `check` takes an occurred time.
- `draft` takes `source` and `evidence`, the metric and diagnostic references it cites.
- The documents and shapes the reviewers listed are brought into line.

## Alternatives rejected

- **Keep check 53's ban on a mirror composing or extending.** No delivery, part or family could then be ported at all, since the import writes only mirrors.
- **Aggregate over every unbound dimension in a read, as a guard does.** A reader could then never see the groups, and would need the ids it is asking for in order to ask.
- **Close the import once a type has had any ordinary write.** A catalogue nobody touches for months would stay importable, and "has it been written" is a fact a refresh job cannot see before it runs.
- **Freeze the report at submission and forbid `refresh`.** A long review would then approve a report that has aged, which is what `impact_unchanged` exists to prevent. Versioning the change keeps both properties.
- **Migrate observations like any object.** It edits a recorded datapoint, which D4 forbids, and a field added to a kind is already optional for the same reason.
- **Leave a mirror unerasable until it is cut over.** A staged cutover can take months, and a right-to-erasure request does not wait for one.
- **Count the import's events as flow and document the distortion.** Every ported closed job would complete in one week and every figure built on completions would be wrong at cutover, which is when people first read them.

## Consequences

- `DESIGN.md` §5.2, §5.11, §5.12, §6, §7, §8, §9, §10 and §11 change.
- `declaration-syntax.md` §2, §4.2, §6.6, §6.8, §6.9, §6.11, §8.1 and §8.3 change, with check rows 10, 23, 33, 53 and 58 amended.
- `storage-schema.md`'s `ok_migration`, erasure steps and duration storage change.
- `library-api.md`'s `publish`, `check`, `TransitionOffer`, `Event`, `Attempt`, the import shapes and the store's construction change.
- `publish-and-import.md` §1 and §4 change.
- `renderers.md` §3 changes.
- D261 to D274 are resolved.
