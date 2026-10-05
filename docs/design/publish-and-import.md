# Publish and import

Draft, 2026-09-09, amended 2026-09-23. What `publish` does with a declaration, what it reports, and how the first consumer's production data arrives. Import is not a separate mechanism: it is declaration migration (ADR-0027), writing into the mirrors a published version declares, and this document is written so that the two share one report and one disposition format. *(ADR-0027 called it migration from version zero. Since ADR-0105 and ADR-0126, version 0 is the store's creation, holding only the built-ins and the operator, and an import writes into a later version's mirrors, §4.)*

**Amended 2026-09-23** for ADR-0083, ADR-0085, ADR-0086 and ADR-0093: a publish is the approval of a `DeclarationChange`, whose dry run is the impact report; the report lists observing clauses; the import's state changes are recorded as `imported`; the mapping may supply entry times and legacy intervals, assignment included; and the cutover order counts the legacy system's writes.

**Amended again 2026-09-23** for ADR-0097 to ADR-0101, from a review of the whole record against the PRD; each change cites the decision it carries.

**Amended 2026-09-24** for ADR-0103: an import raises each sequence past the legacy system's last value.

**Amended 2026-09-24** for ADR-0105 and ADR-0106, from a review of the whole record against PRD revision 5:
- a migration is F2's third route, checked against the invariants, and a publish admits a violation only by an `admit`;
- a store begins at version 0;
- the report names what each mapping rewrites and the changes a publish supersedes;
- an import opens each current interval when the legacy change happened, declares the legacy record's silent spans, and records `recorded_from`;
- a refresh carries what changed since the last import;
- every legacy entry is carried.

**Amended 2026-09-24** for ADR-0109: the report lists each creation that lands past its lifecycle's first state, with what it skips.

**Amended 2026-09-25** for ADR-0114: who may draft, approve or import is the upper layer's, and the change records who did; the report names the metric dimensions that name an actor, in place of audiences, and hides nothing from its reader.

**Amended 2026-09-25** for ADR-0111 and ADR-0112: the report lists every flag beside the observing clauses, and each metric's audience; the checks number sixty-three. *(The audience was withdrawn the same day by ADR-0114, which also withdrew check 63; ADR-0133 withdrew check 45.)*

**What is verified.** The report shapes below execute and are held against the rest of the record by `scripts/check-api-doc.py`, which reads this file as well. The mapping syntax is checked by the declaration checker like any other declaration text.

## 1. Publishing is a transaction, and the approval of a change

A flow changes only through a **`DeclarationChange`**, a built-in type with a declared lifecycle (ADR-0085, ADR-0097). A store is created at declaration version 0, holding the built-in types and standard metrics, and with its operator, the actor who can draft the first change (ADR-0126), so the first change is drafted against version 0 and published by the same path as every later one; each version records the built-in module it was published with, and an engine release changes the built-ins in a store only through a reported, recorded act, the engine upgrade of `storage-schema.md` §12, which installs its own version, as a publish does (ADR-0105, ADR-0147). An actor drafts one with the proposed source — a module and the closure of its `imports` — and `source_ref`, where that source came from, such as a commit; the engine records the reference with a digest of the source, which the repository's side recomputes at the reference to check that the published text is the committed one, and never reads the repository itself (ADR-0131, ADR-0135). `submit` runs steps 1 to 4 below as its **dry run**, attaching the report as the impact report PRD F7 requires, together with a snapshot of the evidence it cites as that evidence then reads. `refresh` reruns the dry run. The `publish` transition is the approval and runs step 5. It requires a change drafted against the installed version; the approver names the version of the change they read, and a `refresh` since is refused as `stale` (ADR-0101). The change records who drafted it and who approved it, with their kinds. Who may draft and approve, that the approver is not the drafter, and that a person approves what an agent drafted, are the upper layer's (PRD F7, ADR-0114). `reject` and `withdraw` end a change, and a publish supersedes every other open change, each drafted against a version older than the one it installs, since its dry run no longer describes what it would do (ADR-0147).

A publish either installs a version or installs nothing.

1. **Parse and check.** The sixty-four checks of the syntax document §10, checks 45 and 63 withdrawn, over the declaration model, which the reference checker reaches through the internal form the description is converted to and the core reads directly (ADR-0116, ADR-0156), the shape of each module's examples file (`flow-format.md` §11.5), and the contradictions within one type the flow checker's fifth step finds, a transition no request can take, a state no object can be in, or a write that breaks the state it enters (`flow-format.md` §7.1, ADR-0137). Then **the examples run**: every example of the change's source runs against an empty in-memory store holding version 0 and the version under publish, with the clock and the id source injected, so it tests the rules and not the live data; a failing example, or a setup that does not apply, is a fatal finding, `example_failed`, naming the example, what it expected and what the request did (ADR-0131 decisions 4 and 6, ADR-0136), and an example that reaches what the engine does not yet run is fatal as well, `example_not_run`, since the publish cannot say it passed (ADR-0156). A change that breaks an example is therefore refused until it updates the example, which is part of the same source. Any failure and the publish is refused with all of them, not the first.
2. **Compare with the installed version.** What changed, and whether each change needs a mapping.
3. **Read every object** of each type the change touches — one whose declaration it alters, or whose rules read a declaration it alters — final ones included (ADR-0148); observations are read for a removed enumeration member and for a kind's invariants, and a retired type's objects for nothing (ADR-0146, ADR-0147). The dry run fixes a `now`, which the report records, and consults no evaluator and no metric. It names: the objects in a removed state; those each mapping will rewrite, a rename naming none and an object whose row a backfill leaves unchanged named in no category; the pending proposals the change would invalidate, as `DESIGN.md` §9 lists them, its mappings never rewriting a proposal's inputs; the objects that would gain or lose an available transition under the new guards, a removed transition and a narrowed `from` being losses, and a transition whose verdict depends on an input, an evaluator or a metric classed as `availability` lists it, available with input or not evaluated, and so neither gained nor lost; those a clause newly enforced would refuse, judged the same way, a clause newly enforced being any `deny` clause the version adds, an `audit` or `warn` made `deny` included; those lacking a required attribute with no `backfill`, an attribute made required counting as new, or one a backfill leaves absent; and those holding a removed enumeration member with no mapping (ADR-0097, ADR-0099, ADR-0147). **Apply every mapping in simulation** and evaluate every invariant of the version, a new one included, once, after all the mappings, over every object as they leave it, an invariant the version did not change needing no new evaluation over an object that no mapping wrote and whose invariants read none that one did, since it held before; report each violation no standing admission covers, with its objects, as a decision, and whether an `admit` in the change covers it (ADR-0105, ADR-0146, ADR-0147). Name every other open change the publish would supersede. The report keeps the ids of each, keyed by category, a mapping's per mapping and a violation's per invariant.
4. **Report.** Everything above, whether or not it is fatal.
5. **If approved**, in one transaction: run steps 1 to 4 again at the attached report's `now`, and refuse the publish as `fatal` if the recomputed report finds anything fatal, as `impact_unchanged` if it names, in any impact category, an object or proposal the attached one did not name in that category, the changes it will supersede being reported and not compared, and as `decision_unresolved` while it holds a decision unresolved, a violation neither fixed nor backfilled nor admitted, since a publish leaves no object in a violation nobody admitted (ADR-0146, ADR-0147). The change then stays `SUBMITTED`, and `refresh` attaches the new report for the next approval, since an approval is of the impact it was shown (ADR-0096, ADR-0097, PRD F7). Otherwise, the publish event and every migration event recorded and occurring at the approval, the dry run's `now` having served only its evaluation (ADR-0147):
   - write `of_declaration` with the change's id;
   - emit the DDL `storage-schema.md` §10 gives to run before the mappings: adding and renaming columns, setting aside a column whose type changed, dropping the constraint of a compiled invariant the version removes or changes, and widening each state and enumeration `CHECK` to the union of both versions' values (ADR-0148);
   - apply the mappings as **one recorded migration transition per object** whose row they change, in ascending id order, over every object of the type, of its family where a mapping names an abstract base, and of a mirror, a retired type's excepted: renames first, writing no event of their own, so the other mappings read the new names; then removed enumeration members, a set holding the target already keeping it once; then removed states; then backfills in the order written, each reading the object as the mappings before it left it, with the version before the publish as its `declaration_version`, and skipping an erased object's personal attributes (ADR-0147);
   - check every invariant once, after all the mappings, over the objects step 3 evaluates, recording each `admit`'s admissions on the violating object's migration event, an object no mapping changed receiving a `migrated` event carrying only the admission, and a violation a standing admission covers needing none (ADR-0147);
   - only then emit the DDL `storage-schema.md` §10 gives to run after them: replacing each widened `CHECK` by the new version's, applying `NOT NULL` to a new required attribute and dropping it from one removed or made optional, and adding the constraints the change brings (ADR-0148);
   - open the current interval of every member the change makes tracked, for every object, at the event that first gives it a value in the publish, the object's migration where a backfill writes it and else the publish event, each marked `.migrated`, and close at the publish event the open intervals of a member it stops tracking (ADR-0146, ADR-0147);
   - redact, on every object already erased, an attribute the change newly marks personal, recording a `redact` event, which admits every invariant the absence leaves violated, as the erasure it completes would have (`DESIGN.md` §8, ADR-0155, D527);
   - supersede every other open change (ADR-0147), invalidate the pending proposals the change breaks, and record the publish event on the `DeclarationChange` — which is the object it belongs to (D212) — as the cause of every one of those.

Step 3 is why a publish is not purely a function of the text. A dry run performs 1 to 4 and stops, and it is the same code path, so what a dry run reports is what a publish will do.

## 2. What the report says

```python
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Mapping, Sequence


class Severity(Enum):
    FATAL = "fatal"          # the publish is refused
    DECISION = "decision"    # a person must choose before it can proceed
    NOTICE = "notice"        # reported, and it proceeds


@dataclass(frozen=True)
class Finding:
    """One thing publishing has to say. `check` is the check number where the
    finding came from one, and None where it is about the live objects."""

    severity: Severity
    code: str
    message: str
    where: str | None = None            # file:line in the declaration
    check: int | None = None
    affected: int = 0                   # live objects, for a DECISION
    sample: Sequence[str] = ()          # a few of their ids


@dataclass(frozen=True)
class Compilation:
    """Which invariants this backend will enforce, and which it will not."""

    compiled: Sequence[str] = ()
    not_compilable_here: Sequence[str] = ()    # the backend cannot express it
    not_compiled_admissible: Sequence[str] = ()  # ADR-0074


@dataclass(frozen=True)
class SkippedTransition:
    """A `do` transition into a creation's state that the creation jumps (ADR-0109)."""

    transition: str
    only_via: Sequence[str] = ()            # its parents, where it is only via
    carried: Sequence[str] = ()             # its clauses the creation declares too, by name
    held_by_invariants: Sequence[str] = ()  # its clauses the type holds as invariants of the same name
    not_carried: Sequence[str] = ()         # its clauses nothing carries


@dataclass(frozen=True)
class CreationSkip:
    """A creation into a state other than its lifecycle's first: a NOTICE, never a refusal."""

    creation: str                           # Type.creation
    state: str
    skips: Sequence[SkippedTransition] = ()
    checked_on_landing: Sequence[str] = ()  # every invariant of the type


@dataclass(frozen=True)
class PublishReport:
    version: int
    accepted: bool
    now: datetime                              # the time the dry run fixed, which approval reuses (ADR-0147)
    findings: Sequence[Finding] = ()
    invariants: Compilation = field(default_factory=Compilation)
    sweepable: Sequence[str] = ()
    unsweepable: Sequence[str] = ()
    queryable_derives: Sequence[str] = ()
    worst_case_fanout: Mapping[str, int] = field(default_factory=dict)
    observed_fanout: Mapping[str, int] = field(default_factory=dict)
    proposals_invalidated: int = 0
    replaced_creations: Mapping[str, Sequence[str]] = field(default_factory=dict)
    creations_past_first_state: Sequence[CreationSkip] = ()   # ADR-0109
    observing_clauses: Sequence[str] = ()      # guarantee nothing; listed so none is mistaken (ADR-0085)
    flags: Sequence[str] = ()                  # guarantee nothing either, and are permanent (ADR-0111)
    person_dimensions: Sequence[str] = ()      # metric dimensions whose value names an actor,
                                               # for an upper layer's door (ADR-0114)
    change_id: str | None = None               # the DeclarationChange this report is attached to
    examples: Mapping[str, Sequence[str]] = field(default_factory=dict)
                                               # per transition, the examples that request it,
                                               # applied and refused; reported, never required (ADR-0136)
    analysed: Mapping[str, tuple[int, int]] = field(default_factory=dict)
                                               # per type, the guard conditions the contradiction
                                               # analysis decided in full, of all it read (ADR-0137)
    source_ref: str | None = None              # where its source came from, as the change names it,
    source_digest: str | None = None           # and the digest of that source (ADR-0135)
    affected: Mapping[str, Sequence[str]] = field(default_factory=dict)
                                               # ids per impact, compared again at approval (ADR-0096)
```

Each impact has its severity (ADR-0146). Fatal: an object in a removed state, or holding a removed enum member, with no mapping; an object left in a state no transition or assertion of the new version reaches, a cutover's included (ADR-0148); a new required value with no `backfill`, an attribute made required counting as new, or one a backfill leaves absent; and an object whose parts a mapping into a final state would strand (ADR-0147). A decision: an invariant an object would violate after the publish, resolved by fixing the object first, a `backfill`, a real value or a placeholder the author chooses, or an `admit`, the preferred placeholder, which keeps the value absent and lists the object in `exceptions` until the invariant holds. Notices: a clause newly enforced that would refuse objects, transitions gained or lost, proposals invalidated, changes superseded, an attribute dropped where one of its type is added, which may be a rename, an attribute whose type changed, which is a drop and an add of the same name, and an `admit` on objects in a final state, which no transition can discharge, though a later publish's backfill may (ADR-0147).

The report also names which invariants compile on this backend, which transitions are sweepable, which derived attributes are queryable, every guard clause that observes rather than enforces, and each declared metric whose filter the version reroutes, since what it counts changes though its text does not (`DESIGN.md` §5.5 and §10, ADR-0129).

Three severities rather than two, because the middle one is the interesting case. A new invariant that eleven live objects violate is not a defect in the declaration and not something to wave through: it is a decision, and a publish's `admit` (§3), or for an import the disposition file (§6), is where it gets made. Anything a person must decide blocks the publish until the decision is recorded, and the decision is recorded in a file rather than in an argument to the command.

`creations_past_first_state` lists each creation into a state other than its lifecycle's first, with the path it skips and, for each transition on it, which clauses the creation carries, which the type holds as invariants of the same name, and which nothing carries (ADR-0109). It never refuses a publish: opening stock and a walk-in intake start partway through the lifecycle on purpose. `scripts/check-syntax-doc.py` computes it, and `scripts/check-flow-docs.py` verifies that the journey's, which `unit-journey.md` quotes, is the one the journey's module produces.

`worst_case_fanout` is each loop's own declared bound; `observed_fanout` is the maximum that loop actually reaches over the live objects. ADR-0071 dropped the product of the two, which multiplied invented numbers into a total that looked authoritative.

## 3. Mappings

A mapping is declaration text, the `migration` section of the module the publish carries (`flow-format.md` §4.16, ADR-0120), which the conversion writes in the model's migration forms (`declaration-syntax.md` §6.6). Each mapping names its type or enumeration, as the model's lines do not:

```yaml
migration:
  removed_states:
    Robot: { LEGACY_HOLD: AVAILABLE }
  removed_members:
    RetirementReason: { OBSOLETE: FAILED }
  renamed_attributes:
    Robot: { old_name: new_name }
  backfill:
    Robot: { manufacturer_serial: '"UNKNOWN"' }
```

The mappings are applied as **one recorded migration transition per object**, carrying every mapping that reaches it, with provenance `migrated`, the transition name `migrate`, the publish event as its cause and the publishing actor as the actor (ADR-0027, ADR-0146). So a hundred objects in a removed state produce a hundred events, each answering "why is this object in this state" for anyone who reads its history later. A mapping into a final state drives no cascade, so a publish whose objects in the removed state have parts the mapping would strand is refused, a part being stranded when it is not in a final state and its end does not declare `survives: true`, since a `survives` list names transitions and no migration (ADR-0147), and a mapping into a superseding state is refused, since it would name no successor (step 3, `names`).

A removed enum member's mapping and a `backfill` ride the same transition, so a publish never changes a row without an event (ADR-0099). A `backfill` writes every object of its type; one that fills only an absent value says so, `if x is null then … else x`; and it skips an erased object's personal attributes, as the import does (ADR-0147). A renamed attribute is DDL and no events: the values did not change, only the name they are read under, so a rename keeps the attribute's type and optionality, step 3 refusing one that changes either, carries its history, its intervals included, and never applies to an observation kind's field (ADR-0146).

**A migration is PRD F2's third route, not an assertion** (ADR-0105). Its authority is the publish's approval, and it carries no reason of its own. It is checked against every invariant, in the dry run and again in the publish, so what the dry run reports is what the publish does. A violation it would leave — or a new invariant's — is resolved before the publish by the objects' own transitions, by a different mapping, or by an `admit`:

```yaml
migration:
  admit:
    - { invariant: Robot.retired_with_reason, reason: OverrideReason.LEGACY_DATA }
```

An `admit`'s reason is a member of a declared enum, as an assertion's is, so overrides stay countable per reason. It records an admission, with its reason, on each violating object's migration event, an object no other mapping changes receiving a `migrated` event carrying only the admission. That admission is an override in the PRD's sense — its Override concept names the migration path — so `exceptions(type)` lists the object and `override_counts` counts it. A new invariant an `admit` covers is not compiled to a constraint, and an existing one that is compiled has its constraint dropped before the migration writes, since a constraint cannot yield for one row; it stays uncompiled while any admission of it stands, whatever recorded it, and is compiled at the next publish after its last is discharged, and a mirror's invariants are never compiled, since the import may admit them; the report says so (ADR-0147) (`storage-schema.md` §8, §10, ADR-0074).

An attribute that is **removed** needs no mapping and gets no DDL. It hides from the declaration and its recorded values stay in history, which is what keeps a fold of the log readable.

## 4. Import uses the migration mechanism

Every object arrives mid-lifecycle at once, which is the situation a migration is, so it uses the same mechanism (ADR-0027 decision 5). What is specific to import is where the objects come from and that nobody has seen them yet.

| Step | What happens |
|---|---|
| **Extract** | The source system produces one record per object, its legacy key, its state, its attribute values, its references by legacy key, and its history |
| **Map** | A per-type mapping from source shape to declared shape, written by hand. This is the part no tool can infer |
| **Dry run** | Every object is evaluated against the mirror's installed declaration, the invariants a mirror may declare included; the owned version's violations surface in the cutover publish, whose `admit` is counted. Nothing is written. Out comes the disposition file of §6, in which a reference into an excluded object, and a field about an object not yet held, are classes of their own, of kind `REFERENCE` and `FIELD`, decided by `DROP`, which clears the optional reference or drops the field, or by `EXCLUDE` (ADR-0146, ADR-0147) |
| **Decide** | A person resolves every class in that file: cleaning the source, supplying a value in the mapping, admitting an invariant violation, or excluding the objects (§6) |
| **Assign ids** | Every object receives its new id from the legacy-key mapping **before any row is written**, so every reference, required or optional, resolves to an id at the moment its row is inserted, and an unresolvable legacy key is found here and not by the database (ADR-0018, ADR-0077). A legacy key is unique within its family, and kept in `of_external_id` under source `legacy` qualified by the family's root type, `<Type>:<key>`; a key naming nothing in the batch or the store refuses the batch, whatever an earlier file excluded, since the store keeps no record of an exclusion (ADR-0154, C23, C29) |
| **Import** | `import_batch` (ADR-0100), taking the decided disposition file, is **one transaction**: a refusal refuses the whole batch, naming the first row it could not place in its report's `refusal`, and a resumed run sends the batch again (ADR-0146, ADR-0154). A batch naming an owned type is refused at its first such object, `refused_types` listing every owned type it named; its classes are found once, after every object of the batch is placed, over the objects it writes; and a refresh that leaves an admitted invariant holding discharges the admission (ADR-0054, ADR-0154, C14, C33, C34). It writes each object through the **built-in assertion**, which keeps none of a declared assertion's rules and evaluates no generated guard, so a legacy observation dated years back, one on a finished subject, and a part left live under a whole the legacy record has finished all port as the record has them (ADR-0147): it may write attributes with the state unchanged, and an object in a final state, as a refresh mirrors the legacy record, and only into a type declared `mirror`, with the observations and labels on a mirror's objects, so legacy datapoints port with their subjects: every type is ported as a mirror and cut over by the publish that removes the marking (§7), so no import reaches an owned type. It never writes a personal attribute of an object whose erasure is recorded, skipping each and applying the rest of the row, redacts the legacy entries and intervals it attaches to an erased object as the erasure would have, every admission it records carries its reason, and every event it writes is marked `imported` (ADR-0101). Objects are created with provenance `asserted`, and each object's `state_source` is `imported`, so `exceptions(type)` does not list the whole legacy population as overrides (ADR-0083, D205); the admissions it records are listed, since the exclusion covers the states it asserts and not the violations it admits (ADR-0146). Each row is written complete with its references and keeps its legacy key as an external identifier with source `legacy`. Rows go in dependency order where one exists, so a failure names the first row that could not be placed; a cycle of required references has no such order and commits with its foreign keys checked at commit, which both backends do for a stored end (`storage-schema.md` §3) |
| **Attach** | Every legacy entry becomes a read-only entry of kind `legacy`, returned by `history` alongside real events (ADR-0015). An entry keeps only fields of its own object: a field describing another ported object, such as a customer's name on a service's audit row, moves to that object's history where the store already holds that object when the entry is attached, an object earlier in the same batch included (ADR-0147), so no reader sees another object's values through it, and is otherwise not ported, the dry run having reported it as a class to decide, such as by porting that type first (ADR-0146); a field about a person or thing the store holds no object for, such as the IP address of the employee who acted, is not ported, and PRD §12 proposes that N5 say so (ADR-0100, ADR-0106). The mapping lists, per entry kind, the payload fields **kept** through an erasure of the object; the rest are redacted with it, and a kind with no list loses its whole payload (ADR-0078). Files are content-addressed into the blob store, and each reference becomes a row of the file reference index (ADR-0087) |
| **Created** | The mapping may supply each object's legacy creation time and the kind of actor that created it, so cycle time and a split by creator reach back past the cutover; where it supplies no time, `.created_at` is the earliest time the port knows of the object — its first legacy interval, entry or entry time — or the port itself, and the object is marked undated; where it supplies no kind, the kind is `unknown`, whether or not it supplies a time (ADR-0154, C37); so a measure from its creation counts it as a gap rather than reporting a short cycle. An entry time or a legacy interval earlier than a creation time the mapping supplies is refused (ADR-0096, ADR-0106) |
| **Intervals** | For its state and each tracked member, the mapping may supply `entered`: when the object took its current value and who changed it, where the legacy record says. The import's event opens the current interval at that time and carries it, so one stay is never split at the port (ADR-0106); the interval records the legacy changer, as a ported actor, where the mapping names one, and the kind `unknown` otherwise, never the importing actor (ADR-0146): an interval beginning at the port records the kind `unknown`, a named changer is recorded by its identity with the kind the mapping gives, `unknown` where it gives none, the store's own actors not consulted, since the change was made in the legacy system (ADR-0154, C28), and an imported observation's `.recorded_by_kind` is the legacy recorder's where the mapping names one and `unknown` otherwise (ADR-0147). Earlier values arrive as legacy intervals of state and of tracked members — assignment included — marked `legacy`, each with who made the change where the record says, and the kind `unknown` where it does not (ADR-0083, ADR-0086, ADR-0100). A legacy interval with no end ends where the next value of its dimension, or the current one, begins; one whose dimension is not tracked, whose value the declaration does not hold, which ends before it begins, or which overlaps the next value of its dimension or the current one, is refused, refusing its batch (settled building the core's import, 2026-10-05). For the first consumer, a service's past engineers come from its audit rows' before-and-after snapshots of `engineer_id` (`wr:app/services/service_service.py:357-372`); how far back those reach is to be sampled. Where the legacy data says nothing, the current interval begins at the import |
| **Silent spans** | The mapping declares, per object, where the legacy record is silent, and no legacy interval may overlap one at all, the import refusing rather than trimming it; nothing is invented to fill it (ADR-0147). A current interval whose entry time lies before a silent span stays as the record gives it, and counts as a gap wherever a metric reads it. It sets each object's `recorded_from`, computed and never supplied, once, when the object is first imported, a refresh leaving it, one value per object, read from its state's intervals and those the mapping supplies for a tracked member, which are whole when they run unbroken from the legacy creation (ADR-0154, C24): its legacy creation where they are whole, the end of its last silent span where there is one, and the port where its state's current interval begins at the port, the latest of those that apply, and for an object none describes the earliest entry of those intervals. A member whose current interval begins at the port, its earlier history not supplied, does not move it: its reads from before then are gaps of their own (ADR-0146, ADR-0148). A metric row then counts, as its `gaps`, the objects whose record does not cover the span it measures, and the harness reports gaps rather than failing on them (DESIGN.md §11, ADR-0106) |
| **Refresh** | An object whose legacy key has arrived is refreshed. A refresh of a mirror carries, for each value that changed since the last import, the same `entered`, and may carry the intermediate values the legacy record shows, which the import opens and closes in order on its event, so a reassignment made in the legacy system while the type is a mirror keeps its time and who made it (ADR-0106). A refresh that changes nothing, its current values, intermediate values and legacy entries being those the store holds, writes no event and does not move `imported_at` (ADR-0147), and one whose legacy times precede the latest interval entry of the same dimension, state or tracked member, is refused, which refuses its batch, intervals and entries the store already holds, sent again, being left as they are (ADR-0146, ADR-0147) |
| **Sequences** | The batch may carry, per sequence and scope, the scope empty for an unscoped sequence, the last value the legacy system minted, and the store raises the sequence past it, so the next number minted is the one after it, and never lowers it (ADR-0154, C36), so the first identifier minted after cutover follows the last one ported rather than colliding with it (ADR-0103) |

Soft-deleted rows land in the type's deleted state (ADR-0024). Cutover cannot be dual-write, because there is one write path.

## 5. Mapping a table to a type

The per-type mapping is written by hand, but less of it than it first appears. Most of a relational schema maps by shape, and naming the rules is what leaves the genuine judgements visible instead of buried among the mechanical ones.

| Shape in the source | Becomes |
|---|---|
| a table whose rows are things with a lifecycle | a **type**, `tracking serial` if each row is one physical thing, `record` otherwise |
| a child table with a required parent reference and no independent existence | a **`part`**, and the parent's cascade covers its terminal transitions |
| a child table whose rows outlive the parent | a type with a `ref`, not a part. This is the distinction that matters most and the one only a person can make |
| a table of `(parent_id, ordinal, value)` with no other columns | a **set-valued attribute** on the parent |
| a pure join table, two foreign keys and nothing else | a relationship: one end stored, the other derived. **No type** |
| a join table carrying payload columns | an **association type**, since the payload needs somewhere to live and a reference carries no attributes |
| a lookup or catalogue table | a type, usually `record`, and usually one of the last to migrate |
| an audit or history table | the **event log**. Not a type; its rows become legacy entries (§4), each kind with a kept-fields list for erasure. The first consumer's audit rows also carry the employee's IP address and user agent, personal data about a third person, which the mapping leaves behind |
| a soft-delete flag | a terminal state in the machine, not an attribute |
| a status column with an enum | the machine's states, and every value present in production must appear or the import cannot place those rows |
| a denormalised copy kept deliberately, such as a snapshot | an attribute, and **not** a reference, because its whole purpose is to stop tracking the thing it came from |
| a session, token or cache table | infrastructure. Not imported |

Three of these are judgements rather than rules, and each is worth asking about explicitly rather than deciding by default:

**Does a child outlive its parent?** A part's lifetime is bounded by its whole, so getting this wrong either orphans rows or destroys ones that should have survived. The first consumer's notes are the live example: they are append-only, attach polymorphically to eleven types, and have no foreign key, so nothing in the schema answers it.

**Is a status column a lifecycle or an attribute?** Not every enum is a machine. A column that only ever moves forward through a fixed sequence is a lifecycle; one that flips freely is an attribute, and declaring it a machine invents transitions nobody performs.

**Is a table a type at all?** A table can exist because the relational model needed one, not because the domain has that thing in it. Join tables are the obvious case; a table of one row holding settings is another.

## 6. The disposition file

The output of a dry run and the input to the import. It exists so that "a person decides each class" is a reviewable artefact in the repository rather than a conversation.

```python
class Disposition(Enum):
    UNDECIDED = "undecided"
    CLEAN_UPSTREAM = "clean_upstream"  # fix the source and re-extract
    ADMIT = "admit"                    # import it violating, and record that; INVARIANT classes only
    EXCLUDE = "exclude"                # do not import these objects at all
    DROP = "drop"                      # REFERENCE and FIELD classes: clear the optional reference,
                                       # or drop the field about an object not held (ADR-0147)


class ViolationKind(Enum):
    INVARIANT = "invariant"            # may be admitted (ADR-0054), a kind's on an observation included
    GUARD = "guard"                    # a structural guard: nothing to admit, and NOT NULL besides;
                                       # a value outside the declaration, and an external identifier
                                       # two objects share, too (ADR-0154)
    REFERENCE = "reference"            # a reference into an excluded object (ADR-0146, ADR-0147)
    FIELD = "field"                    # a legacy field about an object not yet held


@dataclass(frozen=True)
class ViolationClass:
    """Every object failing the same rule in the same way, as one decision.

    The identity of a class is (type, rule, shape) and not the object set, so
    re-extracting after a clean produces the same class with a smaller count
    rather than a new one."""

    id: str
    type: str
    kind: ViolationKind
    rule: str                    # an invariant name, or a guard's clause name
    shape: str                   # what is wrong: a fixed text for the kind of failure, never a value (ADR-0154)
    count: int
    sample: Sequence[str]        # legacy keys, not ids: nothing is created yet
    disposition: Disposition = Disposition.UNDECIDED
    decided_by: str | None = None
    reason: str | None = None    # required for ADMIT and EXCLUDE


# The file: the classes as a person decided them (ADR-0154).
DispositionFile = Sequence[ViolationClass]
```

Five rules make it useful rather than ceremonial.

**A class is identified by its rule and its shape, never by its objects.** Cleaning the source and re-extracting must produce the same class with a smaller count, or a person cannot tell progress from churn. So a shape is a fixed text for its kind of failure and never a value: an invariant `does not hold`, a required attribute `is absent`, a value `is not declared` or `is not of its type`, an external identifier `is shared`, a reference `names an excluded object`; and a violation the file has no class for refuses the batch, as an `UNDECIDED` one does (ADR-0154, C31).

**What each kind covers** (ADR-0154). A state, an enumeration member or a value of the wrong type is a `GUARD` class, so its objects can be excluded or cleaned upstream, while an attribute the type does not declare refuses the batch, as the mapping's error (C35). A mirror's `unique` is an `INVARIANT` class like its other invariants, since §8 of `storage-schema.md` compiles none of a mirror's, and an external identifier two objects share is a `GUARD` class, `of_external_id`'s key holding it (C22). An excluded object is one this batch's file excludes and the store does not hold, and a reference to it is a `REFERENCE` class; one the store holds is left as last imported, and a reference to it resolves; `DROP` on a required reference refuses the batch, and the dry run reports it (C29).

**`ADMIT` and `EXCLUDE` require a reason, and it is recorded.** An admitted violation becomes an admission on the object, which `exceptions(type)` returns until the invariant holds again (ADR-0054). An excluded object leaves a row in the file and nothing in the store, and one the store already holds is left as last imported, which the dry run reports (ADR-0147), which is the only record that it was left behind.

**A guard class cannot be admitted.** An admission names an object and an invariant (ADR-0054); a structural guard — a required attribute or reference absent — has no admission shape, a creation guard being one no import meets, since a mirror declares no creation (ADR-0147), and its column is `NOT NULL` besides. A guard class is cleaned upstream, supplied by the mapping, or excluded, and the dry run refuses `ADMIT` on one rather than leaving the import to fail on it (ADR-0077).

**An import will not run while any class is `UNDECIDED`. `import_batch` takes the file, and a class applies to every object in it at import: an object newly in an `ADMIT` class is admitted with that class's reason, the file being the one way an import admits, and an object in a `CLEAN_UPSTREAM` class refuses the batch, since its source was to be cleaned (ADR-0146, ADR-0154, C32).** This is the whole point of the artefact: the failure mode it exists to prevent is an import that succeeded and quietly dropped what it could not place.

**The file is committed.** It is the record of what the data was on the day it moved, and the reasons are the only place the judgement survives.

## 7. Cutover is staged by type

The author chose staged over big-bang (ADR-0075), and the shape of a stage follows from the one constraint ADR-0015 set: cutover cannot be dual-write. So each type has exactly one owner at any moment, and the order is not free.

**A type may migrate only after every type that references it has migrated.** A migrated type referencing a not-yet-migrated one is fine — the referent is present here as a `mirror`. The reverse is not: a legacy row whose foreign key points at rows it no longer owns needs either dual-write or a new integration inside the system being retired.

**And a type the legacy system writes migrates no earlier than the writer** (ADR-0093), counting every write the legacy system makes — its service-layer methods as well as the transition registry's side-effect lists (ADR-0100). Completing a delivery marks its units sold and creates their warranty contracts (`wr:app/core/state_registry.py:771`), so while deliveries stay legacy-owned, so must units and warranty contracts. The store's side is the same, since check 53 forbids an owned type's outcome from reaching a mirror. The write edges are extracted from the legacy registry's `side_effects=` lists and from each declaration's `call` and `create` steps.

**A cycle in the combined graph — references and writes — is one stage.** Its members have no valid order between them, so they move together or not at all. The unit of a stage is a strongly connected component of that graph, and finding those is the first thing a cutover plan needs. For the first consumer it puts at least seven tables of the operational core in one stage (`first-consumer-cutover.md`).

**A stage is two publishes and the imports between them, not one publish.** The type arrives as a `mirror` in one, is kept current by repeated import while it is one, and is cut over by a second, a version advance that removes the marking and declares its transitions. No object changes id, because the mirror held real objects from the start.

What this does **not** solve is a legacy *read* of a migrated type. Only writes are forbidden. Where the retiring system must still show data that has moved, that is a read-only projection out of the read surface, and whether each such screen earns its cost is a per-stage judgement about that codebase rather than something this design decides.

## 8. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **The extract is JSON lines**, one object per line: it streams, a failure names a line, two extracts diff usefully, and every language emits it — which matters because the extractor lives in the system being retired.
- **Import is one pass, with every id assigned first.** The first draft had two passes — objects, then references — on the premise that SQLite cannot defer a constraint to commit. It can, `DEFERRABLE INITIALLY DEFERRED` is honoured and `scripts/check-schema-doc.py` runs it; and two passes could not have written a required reference at all, since its column is `NOT NULL` (D191). Assigning every id from the legacy-key mapping before any row is written is what makes one pass possible, and it moves the place an unresolvable reference is found to the mapping, before anything is inserted (ADR-0077).
- **A publish is the approval of a `DeclarationChange`** (ADR-0085), and the publish event belongs to it.
- **Imported state is `imported`, not `asserted`, in `state_source`** (ADR-0083), and legacy intervals are marked as such (ADR-0086).
- **The stage order counts writes as well as references** (ADR-0093).
- **Every legacy entry is carried** (ADR-0106). This section once left open whether every entry earns its place; PRD N5 says legacy history is preserved, and 6,391 audit rows and 684 notes are well within N3's sizing, so the port carries all of them.
- **An import resumes rather than being redone.** Every imported object keeps its legacy key as an external identifier, so `lookup('legacy', key)` says whether it arrived and a second run continues: a batch is one transaction, so a batch that failed wrote nothing and is sent again, and one that arrived is skipped by the client, which `lookup` tells, its objects refreshed only where the legacy record changed (ADR-0146, ADR-0147). The alternative turns a failure in hour three of a cutover window into starting again.

**Still open.**

- **The three judgements of §5** — whether a child outlives its parent, whether a status column is a lifecycle or an attribute, and whether a table is a type at all. The rest of the mapping goes by shape; these do not, and each is a question to ask rather than a default to take.

## Appendix: the two versions the mappings of §3 belong to

The mappings of §3 are excerpts of the second version's `migration`, written on 2026-09-26 so that they are part of a checked module (ADR-0121): a unit that had a hold for the legacy system, a retirement reason later merged, a name later renamed, no manufacturer serial, and no rule that a retirement records its reason. `scripts/check-flow-docs.py` checks both versions, and the second against the first, as successive versions of one module.

```yaml
module: units
categories: [live, closed]

enumerations:
  RetirementReason: [FAILED, DAMAGED, OBSOLETE, LOST]
  OverrideReason: [LEGACY_DATA, MIS_SCANNED]

types:
  Robot:
    description: A robot unit, as the version before the migration declares it.
    tracking: serial

    attributes:
      serial:            { type: string, indexed: true, unique: true }
      old_name:          { type: string, optional: true }
      retirement_reason: { type: RetirementReason, optional: true }

    states:
      AVAILABLE:   { category: live }
      LEGACY_HOLD: { category: live }
      RETIRED:     { category: closed, final: true }

    transitions:
      add:
        kind: initial
        to: AVAILABLE
        required_inputs: [serial]
      name:
        kind: internal
        from: [AVAILABLE, LEGACY_HOLD]
        optional_inputs: [old_name]
      hold:
        kind: external
        from: AVAILABLE
        to: LEGACY_HOLD
      release:
        kind: external
        from: LEGACY_HOLD
        to: AVAILABLE
      retire:
        kind: external
        from: [AVAILABLE, LEGACY_HOLD]
        to: RETIRED
        optional_inputs: [retirement_reason]
```

```yaml
module: units
categories: [live, closed]

enumerations:
  RetirementReason: [FAILED, DAMAGED, LOST]
  OverrideReason: [LEGACY_DATA, MIS_SCANNED]

types:
  Robot:
    description: A robot unit, as the version the migration publishes declares it.
    tracking: serial

    attributes:
      serial:              { type: string, indexed: true, unique: true }
      new_name:            { type: string, optional: true }
      manufacturer_serial: { type: string }
      retirement_reason:   { type: RetirementReason, optional: true }

    states:
      AVAILABLE: { category: live }
      RETIRED:   { category: closed, final: true }

    invariants:
      retired_with_reason:
        description: A retired unit records why it was retired.
        expression: state != RETIRED or retirement_reason is not null

    transitions:
      add:
        kind: initial
        to: AVAILABLE
        required_inputs: [serial, manufacturer_serial]
      name:
        kind: internal
        from: AVAILABLE
        optional_inputs: [new_name]
      retire:
        kind: external
        from: AVAILABLE
        to: RETIRED
        required_inputs: [retirement_reason]

migration:
  description: >-
    Version 2 releases every unit held for the legacy system, merges the
    obsolete retirement reason into failed, renames the unit's name, records a
    manufacturer serial for every unit, and admits the units retired without a
    reason before one was required.
  removed_states:
    Robot: { LEGACY_HOLD: AVAILABLE }
  removed_members:
    RetirementReason: { OBSOLETE: FAILED }
  renamed_attributes:
    Robot: { old_name: new_name }
  backfill:
    Robot: { manufacturer_serial: '"UNKNOWN"' }
  admit:
    - { invariant: Robot.retired_with_reason, reason: OverrideReason.LEGACY_DATA }
```
