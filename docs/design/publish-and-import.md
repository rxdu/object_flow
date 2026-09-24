# Publish and import

Draft, 2026-09-09, amended 2026-09-23. What `publish` does with a declaration, what it reports, and how the first consumer's production data arrives. Import is not a separate mechanism: it is declaration migration from version zero (ADR-0027), and this document is written so that the two share one report and one disposition format.

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

**What is verified.** The report shapes below execute and are held against the rest of the record by `scripts/check-api-doc.py`, which reads this file as well. The mapping syntax is checked by the declaration checker like any other declaration text.

## 1. Publishing is a transaction, and the approval of a change

A flow changes only through a **`DeclarationChange`**, a built-in type with a declared lifecycle (ADR-0085, ADR-0097). A store is created at declaration version 0, holding the built-in types, capabilities and standard metrics, so the first change is drafted against version 0 and published by the same path as every later one; each version records the built-in module it was published with, and an engine release changes the built-ins in a store only through a publish (ADR-0105). An actor holding `OF_DRAFT_CHANGE` drafts one with the proposed source — a module and the closure of its `use` imports — and `submit` runs steps 1 to 4 below as its **dry run**, attaching the report as the impact report PRD F7 requires, together with a snapshot of the evidence it cites as that evidence then reads. `refresh` reruns the dry run. The `publish` transition is the approval and runs step 5. It requires `OF_APPROVE_CHANGE`, an approver who is neither the drafter nor the drafter's principal nor acting for the drafter, a person where an agent drafted it, and a change drafted against the installed version; the approver names the version of the change they read, and a `refresh` since is refused as `stale` (ADR-0101). A change is visible to actors holding `OF_DRAFT_CHANGE` or `OF_APPROVE_CHANGE`, and its report's ids and evidence are shown only as far as the reader's visibility reaches. `reject` and `withdraw` end a change, and a publish supersedes every other open change drafted against an older version, since its dry run no longer describes what it would do.

A publish either installs a version or installs nothing.

1. **Parse and check.** The sixty-one checks of the syntax document §10. Any failure and the publish is refused with all of them, not the first.
2. **Compare with the installed version.** What changed, and whether each change needs a mapping.
3. **Read the live objects.** Which violate a new invariant, which are in a removed state, which each mapping will rewrite, which pending proposals the change would invalidate, which would gain or lose an available transition under the new guards, which a clause the change stops observing would now refuse, which lack a new required attribute with no `backfill`, and which hold a removed enum member with no mapping (ADR-0097, ADR-0099). **Apply every mapping in simulation** and evaluate every invariant over the objects it writes, as a request's step 7 does, reporting each violation with its objects as a decision, and whether an `admit` in the change covers it (ADR-0105). Name every other open change the publish would supersede. The report keeps the ids of each.
4. **Report.** Everything above, whether or not it is fatal.
5. **If approved**, in one transaction: run steps 1 to 4 again, and refuse the publish as `impact_unchanged` if the recomputed report names any object or proposal the attached one did not; the change stays `SUBMITTED`, and `refresh` attaches the new report for the next approval, since an approval is of the impact it was shown (ADR-0096, ADR-0097, PRD F7). Otherwise write `of_declaration` with the change's id, emit the DDL of the storage schema §10, apply each mapping — a removed state, a removed enum member, a `backfill` — as a recorded migration transition per object, checking every invariant over what it writes and recording each `admit`'s admissions on the violating objects' migration events, open the current interval of every member the change makes tracked for every live object, supersede every other open change drafted against an older version, invalidate the pending proposals the change breaks, and record the publish event on the `DeclarationChange` — which is the object it belongs to (D212) — as the cause of every one of those.

Step 3 is why a publish is not purely a function of the text. A dry run performs 1 to 4 and stops, and it is the same code path, so what a dry run reports is what a publish will do.

## 2. What the report says

```python
from dataclasses import dataclass, field
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
    change_id: str | None = None               # the DeclarationChange this report is attached to
    withheld: bool = False                     # `affected` omits ids its reader cannot see (ADR-0101)
    affected: Mapping[str, Sequence[str]] = field(default_factory=dict)
                                               # ids per impact, compared again at approval (ADR-0096)
```

Three severities rather than two, because the middle one is the interesting case. A new invariant that eleven live objects violate is not a defect in the declaration and not something to wave through: it is a decision, and §4 is where it gets made. Anything a person must decide blocks the publish until the decision is recorded, and the decision is recorded in a file rather than in an argument to the command.

`creations_past_first_state` lists each creation into a state other than its lifecycle's first, with the path it skips and, for each transition on it, which clauses the creation carries, which the type holds as invariants of the same name, and which nothing carries (ADR-0109). It never refuses a publish: opening stock and a walk-in intake start partway through the lifecycle on purpose. `scripts/check-syntax-doc.py` computes it, and verifies the journey's, which `unit-journey.md` quotes.

`worst_case_fanout` is each loop's own declared bound; `observed_fanout` is the maximum that loop actually reaches over the live objects. ADR-0071 dropped the product of the two, which multiplied invented numbers into a total that looked authoritative.

## 3. Mappings

A mapping is declaration text, in the migration forms of the syntax §6.6, carried by the publish that needs it:

```text
removed state  LEGACY_HOLD -> AVAILABLE
removed member RetirementReason.OBSOLETE -> FAILED
renamed attr   old_name    -> new_name
backfill       manufacturer_serial := "UNKNOWN"
```

A removed state's mapping is applied as **one recorded migration transition per object**, with provenance `migrated`, the publish event as its cause and the publishing actor as the actor (ADR-0027). So a hundred objects in a removed state produce a hundred events, each answering "why is this object in this state" for anyone who reads its history later.

A removed enum member's mapping and a `backfill` are applied the same way, one recorded migration transition per object that needs one, so a publish never changes a live row without an event (ADR-0099). A renamed attribute is DDL and no events: the values did not change, only the name they are read under.

**A migration is PRD F2's third route, not an assertion** (ADR-0105). Its authority is the publish's approval, and it carries no reason of its own. It is checked against every invariant, in the dry run and again in the publish, so what the dry run reports is what the publish does. A violation it would leave — or a new invariant's — is resolved before the publish by the objects' own transitions, by a different mapping, or by an `admit`:

```text
admit Robot.one_open_engagement because OverrideReason.LEGACY_DATA
```

An `admit`'s reason is a member of a declared enum, as an assertion's is, so overrides stay countable per reason. It records an admission, with its reason, on each violating object's migration event, an object no other mapping changes receiving a `migrated` event carrying only the admission. That admission is an override in the PRD's sense — its Override concept names the migration path — so `exceptions(type)` lists the object and `override_counts` counts it. A new invariant an `admit` covers is not compiled to a constraint, and an existing one that is compiled has its constraint dropped before the migration writes, since a constraint cannot yield for one row; it stays uncompiled while any admission of it stands, and the report says so (`storage-schema.md` §8, §10, ADR-0074).

An attribute that is **removed** needs no mapping and gets no DDL. It hides from the declaration and its recorded values stay in history, which is what keeps a fold of the log readable.

## 4. Import is version zero

Every object arrives mid-lifecycle at once, which is the situation a migration is, so it uses the same mechanism (ADR-0027 decision 5). What is specific to import is where the objects come from and that nobody has seen them yet.

| Step | What happens |
|---|---|
| **Extract** | The source system produces one record per object, its legacy key, its state, its attribute values, its references by legacy key, and its history |
| **Map** | A per-type mapping from source shape to declared shape, written by hand. This is the part no tool can infer |
| **Dry run** | Every object is evaluated against the declaration. Nothing is written. Out comes the disposition file of §6 |
| **Decide** | A person resolves every class in that file: cleaning the source, supplying a value in the mapping, admitting an invariant violation, or excluding the objects (§6) |
| **Assign ids** | Every object receives its new id from the legacy-key mapping **before any row is written**, so every reference, required or optional, resolves to an id at the moment its row is inserted, and an unresolvable legacy key is found here and not by the database (ADR-0018, ADR-0077) |
| **Import** | `import_batch` (ADR-0100), for an actor holding `OF_IMPORT`, writes each object through the **built-in assertion**, and only into a type declared `mirror`, with the observations and labels on a mirror's objects, so legacy datapoints port with their subjects: every type is ported as a mirror and cut over by the publish that removes the marking (§7), so no import reaches an owned type. It never rewrites a personal value an erasure removed, redacts the legacy entries and intervals it attaches to an erased object as the erasure would have, every admission it records carries its reason, and every event it writes is marked `imported` (ADR-0101). Objects are created with provenance `asserted`, and each object's `state_source` is `imported`, so `exceptions(type)` does not list the whole legacy population as overrides (ADR-0083, D205). Each row is written complete with its references and keeps its legacy key as an external identifier with source `legacy`. Rows go in dependency order where one exists, so a failure names the first row that could not be placed; a cycle of required references has no such order and commits with its foreign keys checked at commit, which both backends do for a stored end (`storage-schema.md` §3) |
| **Attach** | Every legacy entry becomes a read-only entry of kind `legacy`, returned by `history` alongside real events (ADR-0015). An entry keeps only fields of its own object: a field describing another ported object, such as a customer's name on a service's audit row, moves to that object's history, so no reader sees another object's values through it; a field about a person or thing the store holds no object for, such as the IP address of the employee who acted, is not ported, and PRD §12 proposes that N5 say so (ADR-0100, ADR-0106). The mapping lists, per entry kind, the payload fields **kept** through an erasure of the object; the rest are redacted with it, and a kind with no list loses its whole payload (ADR-0078). Files are content-addressed into the blob store, and each reference becomes a row of the file reference index (ADR-0087) |
| **Created** | The mapping may supply each object's legacy creation time and the kind of actor that created it, so cycle time and a split by creator reach back past the cutover; where it supplies none, `.created_at` is the earliest time the port knows of the object — its first legacy interval, entry or entry time — or the port itself, the kind is `unknown`, and the object is marked undated, so a measure from its creation counts it as a gap rather than reporting a short cycle. An entry time or a legacy interval earlier than a creation time the mapping supplies is refused (ADR-0096, ADR-0106) |
| **Intervals** | For its state and each tracked member, the mapping may supply `entered`: when the object took its current value and who changed it, where the legacy record says. The import's event opens the current interval at that time and carries it, so one stay is never split at the port (ADR-0106). Earlier values arrive as legacy intervals of state and of tracked members — assignment included — marked `legacy`, each with who made the change where the record says, and the kind `unknown` where it does not (ADR-0083, ADR-0086, ADR-0100). For the first consumer, a service's past engineers come from its audit rows' before-and-after snapshots of `engineer_id` (`wr:app/services/service_service.py:357-372`); how far back those reach is to be sampled. Where the legacy data says nothing, the current interval begins at the import |
| **Silent spans** | The mapping declares, per object, where the legacy record is silent, and no legacy interval may cross one; nothing is invented to fill it. A current interval whose entry time lies before a silent span stays as the record gives it, and counts as a gap wherever a metric reads it. It sets each object's `recorded_from`: its legacy creation where its history is whole, the end of its last silent span where there is one, and the port where it supplies no history. A metric row then counts, as its `gaps`, the objects whose record does not cover the span it measures, and the harness reports gaps rather than failing on them (DESIGN.md §11, ADR-0106) |
| **Refresh** | A refresh of a mirror carries, for each value that changed since the last import, the same `entered`, and may carry the intermediate values the legacy record shows, which the import opens and closes in order on its event, so a reassignment made in the legacy system while the type is a mirror keeps its time and who made it (ADR-0106) |
| **Sequences** | The batch may carry, per sequence, the last value the legacy system minted, and the store raises the sequence past it and never lowers it, so the first identifier minted after cutover follows the last one ported rather than colliding with it (ADR-0103) |

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


class ViolationKind(Enum):
    INVARIANT = "invariant"            # may be admitted (ADR-0054)
    GUARD = "guard"                    # a structural guard: nothing to admit, and NOT NULL besides


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
    shape: str                   # what is wrong, in one line
    count: int
    sample: Sequence[str]        # legacy keys, not ids: nothing is created yet
    disposition: Disposition = Disposition.UNDECIDED
    decided_by: str | None = None
    reason: str | None = None    # required for ADMIT and EXCLUDE
```

Five rules make it useful rather than ceremonial.

**A class is identified by its rule and its shape, never by its objects.** Cleaning the source and re-extracting must produce the same class with a smaller count, or a person cannot tell progress from churn.

**`ADMIT` and `EXCLUDE` require a reason, and it is recorded.** An admitted violation becomes an admission on the object, which `exceptions(type)` returns until the invariant holds again (ADR-0054). An excluded object leaves a row in the file and nothing in the store, which is the only record that it was left behind.

**A guard class cannot be admitted.** An admission names an object and an invariant (ADR-0054); a structural guard — a required attribute or reference absent, a creation guard unmet — has no admission shape, and its column is `NOT NULL` besides. A guard class is cleaned upstream, supplied by the mapping, or excluded, and the dry run refuses `ADMIT` on one rather than leaving the import to fail on it (ADR-0077).

**An import will not run while any class is `UNDECIDED`.** This is the whole point of the artefact: the failure mode it exists to prevent is an import that succeeded and quietly dropped what it could not place.

**The file is committed.** It is the record of what the data was on the day it moved, and the reasons are the only place the judgement survives.

## 7. Cutover is staged by type

The author chose staged over big-bang (ADR-0075), and the shape of a stage follows from the one constraint ADR-0015 set: cutover cannot be dual-write. So each type has exactly one owner at any moment, and the order is not free.

**A type may migrate only after every type that references it has migrated.** A migrated type referencing a not-yet-migrated one is fine — the referent is present here as a `mirror`. The reverse is not: a legacy row whose foreign key points at rows it no longer owns needs either dual-write or a new integration inside the system being retired.

**And a type the legacy system writes migrates no earlier than the writer** (ADR-0093), counting every write the legacy system makes — its service-layer methods as well as the transition registry's side-effect lists (ADR-0100). Completing a delivery marks its units sold and creates their warranty contracts (`wr:app/core/state_registry.py:771`), so while deliveries stay legacy-owned, so must units and warranty contracts. The store's side is the same, since check 53 forbids an owned type's outcome from reaching a mirror. The write edges are extracted from the legacy registry's `side_effects=` lists and from each declaration's `call` and `create` steps.

**A cycle in the combined graph — references and writes — is one stage.** Its members have no valid order between them, so they move together or not at all. The unit of a stage is a strongly connected component of that graph, and finding those is the first thing a cutover plan needs. For the first consumer it puts at least seven tables of the operational core in one stage (`first-consumer-cutover.md`).

**A stage is three publishes, not one.** The type arrives as a `mirror` in one, is kept current by repeated import while it is one, and is cut over by a version advance that removes the marking and declares its transitions. No object changes id, because the mirror held real objects from the start.

What this does **not** solve is a legacy *read* of a migrated type. Only writes are forbidden. Where the retiring system must still show data that has moved, that is a read-only projection out of the read surface, and whether each such screen earns its cost is a per-stage judgement about that codebase rather than something this design decides.

## 8. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **The extract is JSON lines**, one object per line: it streams, a failure names a line, two extracts diff usefully, and every language emits it — which matters because the extractor lives in the system being retired.
- **Import is one pass, with every id assigned first.** The first draft had two passes — objects, then references — on the premise that SQLite cannot defer a constraint to commit. It can, `DEFERRABLE INITIALLY DEFERRED` is honoured and `scripts/check-schema-doc.py` runs it; and two passes could not have written a required reference at all, since its column is `NOT NULL` (D191). Assigning every id from the legacy-key mapping before any row is written is what makes one pass possible, and it moves the place an unresolvable reference is found to the mapping, before anything is inserted (ADR-0077).
- **A publish is the approval of a `DeclarationChange`** (ADR-0085), and the publish event belongs to it.
- **Imported state is `imported`, not `asserted`, in `state_source`** (ADR-0083), and legacy intervals are marked as such (ADR-0086).
- **The stage order counts writes as well as references** (ADR-0093).
- **Every legacy entry is carried** (ADR-0106). This section once left open whether every entry earns its place; PRD N5 says legacy history is preserved, and 6,391 audit rows and 684 notes are well within N3's sizing, so the port carries all of them.
- **An import resumes rather than being redone.** Every imported object keeps its legacy key as an external identifier, so `lookup('legacy', key)` says whether it arrived and a second run continues. The alternative turns a failure in hour three of a cutover window into starting again.

**Still open.**

- **The three judgements of §5** — whether a child outlives its parent, whether a status column is a lifecycle or an attribute, and whether a table is a type at all. The rest of the mapping goes by shape; these do not, and each is a question to ask rather than a default to take.
