# Publish and import

Draft, 2026-09-09. What `publish` does with a declaration, what it reports, and how the first consumer's production data arrives. Import is not a separate mechanism: it is declaration migration from version zero (ADR-0027), and this document is written so that the two share one report and one disposition format.

**What is verified.** The report shapes below execute and are held against the rest of the record by `scripts/check-api-doc.py`, which reads this file as well. The mapping syntax is checked by the declaration checker like any other declaration text.

## 1. Publishing is a transaction

A publish takes a module and the closure of its `use` imports, and either installs a version or installs nothing.

1. **Parse and check.** The fifty-two checks of the syntax document §10. Any failure and the publish is refused with all of them, not the first.
2. **Compare with the installed version.** What changed, and whether each change needs a mapping.
3. **Read the live objects.** How many violate a new invariant, how many are in a removed state, how many pending proposals the change would invalidate.
4. **Report.** Everything above, whether or not it is fatal.
5. **If accepted**, in one transaction: write `ok_declaration`, emit the DDL of the storage schema §10, apply each mapping as a recorded migration transition per object, invalidate the pending proposals the change breaks, and record a publish event that is the cause of every one of those.

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
```

Three severities rather than two, because the middle one is the interesting case. A new invariant that eleven live objects violate is not a defect in the declaration and not something to wave through: it is a decision, and §4 is where it gets made. Anything a person must decide blocks the publish until the decision is recorded, and the decision is recorded in a file rather than in an argument to the command.

`worst_case_fanout` is each loop's own declared bound; `observed_fanout` is the maximum that loop actually reaches over the live objects. ADR-0071 dropped the product of the two, which multiplied invented numbers into a total that looked authoritative.

## 3. Mappings

A mapping is declaration text, in the migration forms of the syntax §6.6, carried by the publish that needs it:

```text
removed state LEGACY_HOLD -> AVAILABLE
renamed attr  old_name    -> new_name
```

A removed state's mapping is applied as **one recorded migration transition per object**, with provenance `migrated`, the publish event as its cause and the publishing actor as the actor (ADR-0027). So a hundred objects in a removed state produce a hundred events, each answering "why is this object in this state" for anyone who reads its history later.

A renamed attribute is DDL and no events: the values did not change, only the name they are read under.

An attribute that is **removed** needs no mapping and gets no DDL. It hides from the declaration and its recorded values stay in history, which is what keeps a fold of the log readable.

## 4. Import is version zero

Every object arrives mid-lifecycle at once, which is the situation a migration is, so it uses the same mechanism (ADR-0027 decision 5). What is specific to import is where the objects come from and that nobody has seen them yet.

| Step | What happens |
|---|---|
| **Extract** | The source system produces one record per object, its legacy key, its state, its attribute values, its references by legacy key, and its history |
| **Map** | A per-type mapping from source shape to declared shape, written by hand. This is the part no tool can infer |
| **Dry run** | Every object is evaluated against the declaration. Nothing is written. Out comes the disposition file of §5 |
| **Decide** | A person resolves every class in that file, by cleaning the source or by admitting the violation |
| **Import** | Objects are created by the **built-in assertion** with provenance `asserted`, each receiving a new id and keeping its legacy key as an external identifier with source `legacy` (ADR-0018) |
| **Re-point** | Cross-references are resolved through the legacy-key mapping, which is why every object is created before any reference is written |
| **Attach** | Legacy history becomes read-only entries of kind `legacy`, returned by `history` alongside real events (ADR-0015). Files are content-addressed into the blob store |

Soft-deleted rows land in the type's deleted state (ADR-0024). Cutover cannot be dual-write, because there is one write path.

## 5. The disposition file

The output of a dry run and the input to the import. It exists so that "a person decides each class" is a reviewable artefact in the repository rather than a conversation.

```python
class Disposition(Enum):
    UNDECIDED = "undecided"
    CLEAN_UPSTREAM = "clean_upstream"  # fix the source and re-extract
    ADMIT = "admit"                    # import it violating, and record that
    EXCLUDE = "exclude"                # do not import these objects at all


@dataclass(frozen=True)
class ViolationClass:
    """Every object failing the same rule in the same way, as one decision.

    The identity of a class is (type, rule, shape) and not the object set, so
    re-extracting after a clean produces the same class with a smaller count
    rather than a new one."""

    id: str
    type: str
    rule: str                    # an invariant name, or a guard's clause name
    shape: str                   # what is wrong, in one line
    count: int
    sample: Sequence[str]        # legacy keys, not ids: nothing is created yet
    disposition: Disposition = Disposition.UNDECIDED
    decided_by: str | None = None
    reason: str | None = None    # required for ADMIT and EXCLUDE
```

Four rules make it useful rather than ceremonial.

**A class is identified by its rule and its shape, never by its objects.** Cleaning the source and re-extracting must produce the same class with a smaller count, or a person cannot tell progress from churn.

**`ADMIT` and `EXCLUDE` require a reason, and it is recorded.** An admitted violation becomes an admission on the object, which `exceptions(type)` returns until the invariant holds again (ADR-0054). An excluded object leaves a row in the file and nothing in the store, which is the only record that it was left behind.

**An import will not run while any class is `UNDECIDED`.** This is the whole point of the artefact: the failure mode it exists to prevent is an import that succeeded and quietly dropped what it could not place.

**The file is committed.** It is the record of what the data was on the day it moved, and the reasons are the only place the judgement survives.

## 6. Cutover is staged by type

The author chose staged over big-bang (ADR-0075), and the shape of a stage follows from the one constraint ADR-0015 set: cutover cannot be dual-write. So each type has exactly one owner at any moment, and the order is not free.

**A type may migrate only after every type that references it has migrated.** A migrated type referencing a not-yet-migrated one is fine — the referent is present here as a `mirror`. The reverse is not: a legacy row whose foreign key points at rows it no longer owns needs either dual-write or a new integration inside the system being retired.

**A cycle in the reference graph is one stage.** Its members have no valid order between them, so they move together or not at all. The unit of a stage is a strongly connected component, and finding those is the first thing a cutover plan needs.

**A stage is three publishes, not one.** The type arrives as a `mirror` in one, is kept current by repeated import while it is one, and is cut over by a version advance that removes the marking and declares its transitions. No object changes id, because the mirror held real objects from the start.

What this does **not** solve is a legacy *read* of a migrated type. Only writes are forbidden. Where the retiring system must still show data that has moved, that is a read-only projection out of the read surface, and whether each such screen earns its cost is a per-stage judgement about that codebase rather than something this design decides.

## 7. What this leaves open

- **The extract is JSON lines, one object per line.** It streams, so a table of a million rows never has to be held in memory on either side; a failure names a line; two extracts of the same table diff usefully; and every language emits it, which matters because the extractor lives in the source system and not here. CSV cannot carry a nested value or an absent one distinguishably, and a database view would tie the importer to the source's schema, which is the thing being retired. What is genuinely open is the **per-type mapping** from source shape to declared shape, which is written by hand and is the part no tool can infer.
- **Two passes, not deferred constraints.** Every object must exist before any reference is written. PostgreSQL can defer a constraint to commit; SQLite cannot in the same way, and a design that works on one backend and not the other is not the design. So import is two passes: create every object with its own attributes and its legacy key, then write every reference by resolving legacy keys through the mapping. It is also the more debuggable of the two, because a failure in the second pass names the reference that could not be resolved rather than failing the whole commit at the end.
- **How much history is worth carrying.** The first consumer's audit log is 6,391 rows and its notes table 684, both append-only. Importing all of it is possible; whether every legacy entry earns its place is a judgement about that data, not about this design.
- **An import resumes rather than being redone**, and the mechanism already exists. Every imported object keeps its legacy key as an external identifier with source `legacy` (ADR-0018), and `lookup('legacy', key)` answers whether it arrived. So a second run skips what is present and continues, which matters because the alternative — tear down and redo — turns a failure in hour three of a cutover window into starting again. What that leaves is the second pass: a reference written to an object whose referent was created in a run that then failed still resolves, since the referent exists and its legacy key is indexed.
