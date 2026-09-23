# Library API

Draft, 2026-09-09, amended 2026-09-23. The request and verdict shapes of [`../DESIGN.md`](../DESIGN.md) §6 and the read surface of §10, as one interface a program calls. Transport bindings come after and are not here.

**What is verified.** The Python below executes, and `scripts/check-api-doc.py` compares the operations it offers against the read surface DESIGN.md §10 declares, so the two cannot drift apart silently. It is **not** typechecked — there is no mypy in the environment this was written in, and the annotations are therefore reviewed and not proven.

**Amended 2026-09-23** for ADR-0082 to ADR-0094: `metric`, `diagnostics` and `export`; `publish` of a `DeclarationChange`; the settled cursor for `pull` and `acknowledge`; the read set, writing transaction, occurred time and retry count on an event; the attempt and interval shapes; `Unsatisfied` naming what its remedy points at, and `Stale` its cause; and the injected dependencies, including the connection source.

**Amended again 2026-09-23** for ADR-0097 to ADR-0101, from a review of the whole record against the PRD; each change cites the decision it carries.

## 1. Why Python, and what that does not mean

The core is library-shaped: a call goes in, guards evaluate, a transition and its record come out (§2 of the model). The first consumer is a FastAPI and SQLAlchemy system being rebuilt on this, so a Python interface is the one that will be exercised first and is the one written here.

That is a **binding**, not the design. The operations, their arguments and their results are the API; the dataclasses are one rendering of it. A second binding should offer the same nineteen operations with the same meanings, and the checker's comparison against §10 is written against the operation set rather than against Python.

## 2. Three rules the shapes follow

**A refusal is a value, never an exception.** Every verdict of §5.5 — including `not found`, `stale` and `invariant violated` — is returned. Exceptions are for a fault: the database is gone, the declaration will not load, the caller passed something the type system should have caught. The reason is that a refusal is an ordinary outcome a consumer must handle, and an exception is the thing a consumer writes `except: pass` around.

**Every operation takes an actor.** There is no ambient identity and no session. The actor is a value the consumer's authentication produced, validated for shape and never for truth (ADR-0025), and visibility is applied on every read from it (ADR-0030).

**Nothing returns a mutable view of stored state.** Every shape is frozen. An object returned by `get` is what was true at the moment it was read, and writing to it would be the second write path the model exists to refuse.

## 3. The actor, the request, and what the store is built from

```python
from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Iterator, Mapping, Protocol, Sequence

# ---------------------------------------------------------------- the actor

class ActorKind(Enum):
    HUMAN = "human"
    AGENT = "agent"
    SERVICE = "service"


@dataclass(frozen=True)
class Actor:
    """Produced by the consumer's authentication. The store validates its
    shape, never its truth (ADR-0025)."""

    id: str
    kind: ActorKind
    capabilities: frozenset[str]
    principal: str | None = None


# ---------------------------------------------------------------- requests

@dataclass(frozen=True)
class Request:
    """One transition on one object, or a creation when `object_id` is None
    and `type` names the type to create."""

    transition: str
    actor: Actor
    object_id: str | None = None
    type: str | None = None
    inputs: Mapping[str, Any] = field(default_factory=dict)
    expected_version: int | None = None
    idempotency_key: str | None = None
    context: str | None = None
    occurred_at: datetime | None = None   # a backdatable transition's, or an observation's
                                          # record's; checked by `occurred_within` (ADR-0099)


# ------------------------------------------- what the store is built from

class IdSource(Protocol):
    def new_id(self) -> str: ...          # a UUIDv7 in production (ADR-0077)


class Clock(Protocol):
    def now(self) -> datetime: ...        # read once per request, and recorded


class ConnectionSource(Protocol):
    """Where the store gets its database connections (ADR-0091). Production
    passes a pool; the harness passes connections whose every statement waits
    at a barrier it controls, which is how it interleaves requests."""

    def connect(self) -> Any: ...


class EvaluatorSource(Protocol):
    """The implementations of the declared evaluators (ADR-0100). Production
    passes the real integrations; the harness passes deterministic ones, so a
    run with an external guard reproduces from its seed."""

    def verdict(self, evaluator: str, fn: str, args: Mapping[str, Any]) -> Any: ...


```

A creation names a `type` and no `object_id`; every other transition names an `object_id`. `expected_version` is the optimistic check of ADR-0023, `idempotency_key` makes a retry safe by replaying the first result rather than refusing it (ADR-0041), and `context` is the free-form route marker that ends up in the event's provenance. A request carrying both a key and an `expected_version` is matched on its key first: a replay returns the recorded verdict whatever version the retry supplies, and `Stale` is possible only for a request that has not been applied (ADR-0076). `occurred_at` is accepted by a transition marked backdatable and by an observation kind's `record`, and is checked by the generated guard `occurred_within`, so a time outside the bound is refused naming that clause (ADR-0083, ADR-0099).

A store is built from a backend, an `IdSource`, a `Clock`, a `ConnectionSource` and an `EvaluatorSource`, all injected, and the deployment's attempt retention, a duration `maintain` will not prune inside (ADR-0101); so a test controls everything nondeterministic about a request (ADR-0077, ADR-0091). The core is **synchronous**: an operation runs to completion on its caller's thread, and an asynchronous service such as the first consumer's wraps it on a thread pool.

## 4. The verdict

One of seven, and the caller must handle all seven. A `match` over them is exhaustive by construction, which is the reason for a union of frozen shapes rather than one class with an optional field per outcome.

```python

class Remedy(Enum):
    SELF_SERVICEABLE = "self_serviceable"
    DELEGABLE = "delegable"
    TEMPORAL = "temporal"
    DEPENDENT = "dependent"
    UNREACHABLE_FROM_HERE = "unreachable_from_here"


class StaleCause(Enum):
    EXPECTED_VERSION = "expected_version"
    RETRIES_EXHAUSTED = "retries_exhausted"


@dataclass(frozen=True)
class Satisfied:
    object_id: str
    version: int
    events: Sequence[int]
    replayed: bool = False


@dataclass(frozen=True)
class Unsatisfied:
    clause: str
    remedy: Remedy
    unknown: bool
    objects: Sequence[str] = ()           # what a dependent remedy says to work on,
                                          # or the object the clause read (ADR-0092)
    capability: str | None = None         # for a clause over actor.has(C)
    proposable: bool = False              # whether a proposal would be accepted
    consulted: Mapping[str, Any] = field(default_factory=dict)
                                          # the metric values and evaluator verdicts
                                          # the clause was decided on (ADR-0096)
    withheld: bool = False                # `objects` omits some the requester cannot see


@dataclass(frozen=True)
class Stale:
    cause: StaleCause
    expected: int | None = None           # present for EXPECTED_VERSION only
    actual: int | None = None


@dataclass(frozen=True)
class NotFound:
    pass


@dataclass(frozen=True)
class NotRequestable:
    parents: Sequence[str]


@dataclass(frozen=True)
class OverLimit:
    loop: str
    bound: int


@dataclass(frozen=True)
class InvariantViolated:
    invariant: str
    objects: Sequence[str]                # only those the requester can see (ADR-0100)
    withheld: bool = False                # others were involved and are not named


Verdict = (
    Satisfied | Unsatisfied | Stale | NotFound
    | NotRequestable | OverLimit | InvariantViolated
)


```

`Unsatisfied` says what the caller should do about it, which is what PRD F4 asks: its remedy class, and what that remedy points at — the objects to work on first, the capability that would satisfy the clause, whether a proposal would be accepted (ADR-0092). `unknown` is the one field that repays explanation: a guard that read something absent is unsatisfied, and not the same answer as a guard that was false (§8.2 of the syntax).

`Stale` carries its cause. Only a stale `expected_version` has an expected and an actual version; exhausted retries — a contended row on PostgreSQL, a busy timeout on SQLite (ADR-0090) — have neither.

`Satisfied.events` is the positions of every event the request appended, the parent's first. A cascade of three parts returns four.

## 5. Reading

```python

class Availability(Enum):
    AVAILABLE = "available"
    AVAILABLE_WITH_INPUT = "available_with_input"
    UNAVAILABLE = "unavailable"


@dataclass(frozen=True)
class TransitionOffer:
    transition: str
    availability: Availability
    creates: str | None                   # the type an offered creation creates: an
                                          # observation kind's record, or a label (ADR-0101)
    verdict: Verdict | None
    inputs: Mapping[str, str]
    unevaluated: Sequence[str]


@dataclass(frozen=True)
class Object:
    id: str
    type: str
    state: str
    version: int
    declaration_version: int
    created_at: datetime                  # from its creation event (ADR-0096)
    created_by_kind: ActorKind
    attributes: Mapping[str, Any]


@dataclass(frozen=True)
class Page:
    items: Sequence[Any]
    cursor: str | None


@dataclass(frozen=True)
class ReadSet:
    """What the rules evaluated for an event read (ADR-0088): enough to
    re-evaluate them from the record."""

    objects: Mapping[str, int]            # object id -> the version read
    scans: Mapping[str, Sequence[str]]    # each type-scan clause -> the ids it matched,
                                          # an empty match recorded as empty (ADR-0088)
    capabilities: Mapping[str, bool]      # each actor.has(C) asked -> its answer
    now: datetime                         # fixed once per request
    withheld: bool = False                # something the reader cannot see was read (ADR-0095)


@dataclass(frozen=True)
class Event:
    position: int
    object_id: str
    object_seq: int
    transition: str
    from_state: str | None
    to_state: str
    changes_state: bool
    source: str
    actor_id: str
    actor_kind: ActorKind
    principal: str | None
    context: str | None
    cause: int | None
    declaration_version: int
    taint_version: int
    txn: str                              # the writing transaction (ADR-0089)
    recorded_at: datetime
    occurred_at: datetime | None          # when a backdated change happened (ADR-0083)
    retries: int                          # serialisation retries its request took
    imported: bool                        # written by import_batch (ADR-0100)
    payload: Mapping[str, Any]
    reads: ReadSet
    consulted: Mapping[str, Any]          # per evaluator or metric guard: the verdict or value given
    as_of: Mapping[str, datetime]         # per evaluator or metric guard: when its value was given


@dataclass(frozen=True)
class LegacyEntry:
    """History that predates the store, attached at import (ADR-0015)."""

    object_id: str
    ordinal: int
    occurred_at: datetime
    payload: Mapping[str, Any]


@dataclass(frozen=True)
class Checked:
    """What `check` returns: a verdict, and the external guards it did not
    evaluate, so the verdict is explicitly partial (ADR-0054)."""

    verdict: Verdict
    unevaluated: Sequence[str]


@dataclass(frozen=True)
class EventPage:
    events: Sequence[Event]
    settled: str                          # the settled cursor: acknowledge this, and
                                          # never anything past it (ADR-0089)


@dataclass(frozen=True)
class MetricRow:
    """One row of a metric the reader may see (ADR-0084)."""

    dimensions: Mapping[str, Any]
    value: Any                            # unknown is None, as division by zero is
    flags: Sequence[str]                  # the declared flags that hold for this row
    time_basis: str = "UTC calendar"      # every metric says so (PRD C6)


@dataclass(frozen=True)
class MetricPage:
    """A metric over the rows its reader may see (ADR-0096)."""

    rows: Sequence[MetricRow]
    cursor: str | None
    complete: bool                        # the reader sees every current object of each type
                                          # the metric reads; otherwise the values are over
                                          # the reader's own rows, and are not the metric's


@dataclass(frozen=True)
class Diagnostic:
    """One place a flow and reality disagree (ADR-0085)."""

    kind: str                             # e.g. "unused_transition", "top_refusal", "reassignment_loop"
    subject: str                          # the transition, state, clause, label or assignee it concerns
    measure: Any                          # the count, duration or rate that put it on the list
    complete: bool                        # computed over every object, as a metric's result says


@dataclass(frozen=True)
class Attempt:
    """A request that did not apply, or an observing clause's would-be
    refusal (ADR-0083). Never its inputs."""

    at: datetime
    actor_id: str
    actor_kind: ActorKind
    principal: str | None
    context: str | None
    type: str | None                      # None where the request named an unknown id
    object_id: str | None
    transition: str | None                # None for a request naming no transition
    verdict: str                          # the verdict kind, or the fault's name
    clause: str | None
    remedy: Remedy | None
    unknown: bool
    enforced: bool                        # False for an observing clause
    reads: ReadSet | None                 # the failing clause's read set (ADR-0088)
    applied_position: int | None          # an observing clause's applied event (ADR-0085)
    consulted: Mapping[str, Any]          # its metric values and evaluator verdicts (ADR-0096)
    declaration_version: int


@dataclass(frozen=True)
class Interval:
    """A span during which an object held one state, or one value of a
    tracked member (ADR-0083, ADR-0086)."""

    object_id: str
    dimension: str                        # "state", or the tracked member's name
    value: Any                            # None for absence: time unassigned is one
    entered_at: datetime                  # occurred time where backdated
    left_at: datetime | None              # None while it is the current value
    entered_by: str | None                # who made the change; a legacy row's where known
    entered_by_kind: ActorKind
    transition: str | None                # the transition that set it; None if legacy
    declaration_version: int
    legacy: bool = False                  # supplied by the import mapping


@dataclass(frozen=True)
class LegacyInterval:
    """A span the legacy record gives. The object's id, the actor kind where
    the record is silent, and the declaration version are the import's."""

    dimension: str
    value: Any
    entered_at: datetime
    left_at: datetime | None
    entered_by: str | None = None         # who made the change, where the record says
    entered_by_kind: ActorKind | None = None   # their kind, where the record says


@dataclass(frozen=True)
class EvidenceRef:
    """A metric or diagnostic a flow change cites; `submit` snapshots what it
    reads (ADR-0097, ADR-0101)."""

    metric: str | None = None             # a metric's name, or
    diagnostics: str | None = None        # a type whose diagnostics are cited
    bind: Mapping[str, Any] = field(default_factory=dict)
    keep: Sequence[str] | None = None
    over: timedelta | None = None
    filter: str | None = None


@dataclass(frozen=True)
class PublishResult:
    """What `publish` returns: the verdict on the `publish` transition, and
    the report it was decided on, filtered for the actor (ADR-0101)."""

    verdict: Verdict                      # Stale if the change moved since it was read
    report: "PublishReport"


@dataclass(frozen=True)
class Admission:
    invariant: str
    reason: str                           # required, as for any admission


@dataclass(frozen=True)
class ImportedObject:
    """One object of a port, as the mapping produced it (ADR-0100)."""

    type: str
    legacy_key: str
    state: str
    attributes: Mapping[str, Any]         # references by legacy key
    legacy_entries: Sequence[Mapping[str, Any]] = ()   # only this object's fields
    legacy_intervals: Sequence["LegacyInterval"] = ()
    created_at: datetime | None = None    # the legacy creation, where known
    created_by_kind: ActorKind | None = None


@dataclass(frozen=True)
class ImportBatch:
    objects: Sequence[ImportedObject]
    admitted: Mapping[str, Sequence["Admission"]] = field(default_factory=dict)
                                          # disposition: legacy key -> admissions


@dataclass(frozen=True)
class ImportReport:
    written: int                          # zero on a dry run
    findings: Sequence[Any]               # the disposition classes of publish-and-import.md §6
    refused_types: Sequence[str] = ()     # owned types, which the import may not write


@dataclass(frozen=True)
class MaintenanceTask:
    """`prune_attempts` or `archive_events`, older than `before` (ADR-0100)."""

    name: str
    before: datetime


@dataclass(frozen=True)
class ExportPage:
    """JSON lines, one row per line, every personal value omitted (ADR-0094)."""

    lines: Sequence[str]
    cursor: str                           # the settled cursor to continue from


```

An `Event` carries what the log stores and no more: the actor's id, kind and principal rather than a descriptor; the declaration and taint versions in force; its writing transaction; its read set; and, per evaluator or metric guard, the verdict or value it was given and that value's as-of time (ADR-0049, ADR-0088, ADR-0089, ADR-0095). The capabilities an actor held are not stored, but the ones a rule asked about are, with their answers, in `reads`.

`EventPage.settled` is a **settled cursor**, not a position: the last (transaction, position) pair the reader returned, drawn only from transactions that had finished when its snapshot was taken, so a later commit can never sort before it (ADR-0089). A consumer acknowledges exactly that.

`TransitionOffer.unevaluated` is what makes the read surface honest about external evaluators: `availability`, `available` and `check` never call one, and each says which guards it therefore did not evaluate (ADR-0048, ADR-0049). A caller that treats an offer as a promise is wrong, and the field is there so it cannot claim it was not told.

## 6. The store

```python

class Store(Protocol):
    """One API. Every operation takes an actor and applies visibility."""

    def request(self, req: Request) -> Verdict: ...

    def batch(self, reqs: Sequence[Request]) -> Sequence[Verdict]: ...

    def get(self, actor: Actor, id: str, follow: bool = False) -> Object | None: ...

    def query(self, actor: Actor, type: str, filter: str | None = None,
              order: str | None = None, cursor: str | None = None,
              fields: Sequence[str] | None = None) -> Page: ...

    def lookup(self, actor: Actor, source: str, value: str) -> Object | None: ...

    def availability(self, actor: Actor, id: str) -> Sequence[TransitionOffer]: ...

    def available(self, actor: Actor, type: str, transition: str,
                  cursor: str | None = None) -> Page: ...

    def check(self, actor: Actor, id_or_type: str, transition: str,
              inputs: Mapping[str, Any],
              occurred_at: datetime | None = None) -> Checked: ...

    def history(self, actor: Actor, id: str,
                follow: bool = False) -> Iterator[Event | LegacyEntry]: ...

    def exceptions(self, actor: Actor, type: str,
                   cursor: str | None = None) -> Page: ...

    def declaration(self, actor: Actor, type: str,
                    version: int | None = None) -> Mapping[str, Any]: ...

    def pull(self, actor: Actor, subscription: str,
             cursor: str | None = None) -> EventPage: ...

    def acknowledge(self, actor: Actor, subscription: str,
                    cursor: str) -> None: ...

    def metric(self, actor: Actor, name: str,
               bind: Mapping[str, Any] | None = None,
               keep: Sequence[str] | None = None,     # dimensions to group by; all if None
               over: timedelta | None = None,
               filter: str | None = None,
               cursor: str | None = None) -> MetricPage: ...

    def export(self, actor: Actor, source: str,
               cursor: str | None = None) -> ExportPage: ...

    def diagnostics(self, actor: Actor, type: str,
                    window: timedelta) -> Sequence[Diagnostic]: ...

    def import_batch(self, actor: Actor, batch: "ImportBatch",
                     dry_run: bool = False) -> "ImportReport": ...

    def maintain(self, actor: Actor, task: "MaintenanceTask") -> int: ...   # rows it moved or removed

    def publish(self, actor: Actor, change: str,
                expected_version: int | None,         # required unless dry_run (ADR-0101)
                dry_run: bool = False) -> "PublishResult": ...
```

`PublishReport` is defined in [`publish-and-import.md`](publish-and-import.md) §2, which owns it: publishing is where its shape is decided and one shape should have one home.

Nineteen operations: the sixteen of §10, the write path of §6, and the operational calls that are not object operations, `acknowledge` and `publish`. `import_batch` and `maintain` are the only other ways anything writes the database (ADR-0100). The checker holds this list against §10 rather than trusting it.

- `metric` returns a `MetricPage`, and `diagnostics` a short fixed list, each computed over what the reader may see and saying whether that is everything (ADR-0096).
- `import_batch` writes a port through the built-in assertion, for an actor holding `OK_IMPORT`, and only into a type declared `mirror` and the observations and labels on a mirror's objects; it never rewrites a personal value an erasure removed, redacts the legacy entries and intervals it attaches to an erased object as the erasure would have, and marks every event it writes `imported` (ADR-0100, ADR-0101). A legacy interval pages in the export by the cursor of the import event that wrote its object. `maintain` requires `OK_MAINTAIN`, refuses a prune inside the store's attempt retention, changes no governed state or history, and is never needed for correctness (ADR-0100, ADR-0101).
- `export` pages an attempt source by its writing transaction's settled cursor, and an interval source by the cursor of the event that last opened or closed each row, so a closing re-emits the row and a consumer keeps the latest per object, dimension and entry (ADR-0100).
- `export` takes a source — `log`, `<Type>.intervals`, `<Type>.transitions`, `<Type>.attempts`, or an observation kind — and returns JSON lines of the corresponding shape above, with every personal value omitted (ADR-0094).
- `history`, `pull` and `export` filter the `reads` and `consulted` of an event or an attempt for the reader: an object the reader cannot see is left out and `withheld` is set, and a metric value is left out unless the reader can see every current object of each type the metric reads (ADR-0095, ADR-0096). A refusal's `Unsatisfied.consulted` follows the same rule for its requester, since a metric in a guard reads rows the requester may not see: the value is given to a requester who can see every current object of each type the metric reads, and otherwise left out.
- `metric` aggregates the rows the reader may see, after narrowing them by `filter`, and a path through an object the reader may not see yields absence. `complete` says whether those rows are all there are, so a partial value is never taken for the metric's own (PRD C2, M2, T5, ADR-0096).
- `publish` takes the id of a `DeclarationChange` (ADR-0085, ADR-0097): with `dry_run` it produces the impact report `submit` and `refresh` attach, and without it it requests the change's `publish` transition, which is the approval and installs the version. `expected_version` is the version of the change the approver read, required unless `dry_run`, and a change that has moved since, by a `refresh`, is refused as `stale`. The result carries the verdict — `Satisfied`, `Stale`, or `Unsatisfied` naming `not_drafter`, `person_for_agent`, `current` or `impact_unchanged` — beside the report, whose ids are filtered for the actor (ADR-0097, ADR-0101).

Three of them are worth reading twice.

**`check` is advice, not a reservation.** It simulates the same sequence internally, takes no locks and has no effect, so the answer can be stale before the caller acts on it. It also costs what the request would cost: simulating a cascade over five hundred parts evaluates five hundred guards. It returns a `Checked`: the verdict, and the external guards it did not evaluate, since `availability`, `available` and `check` call no evaluator (ADR-0054, ADR-0077). It does compute internal metrics.

**`available` requires the transition to be sweepable.** Its guards must decompose into an indexable prefilter plus a residual (ADR-0048). A transition that is not sweepable is refused here rather than scanned, which is a real constraint on how a time-driven guard is written, because this query is how all time-driven work in the system finds its objects.

**`batch` is N transactions, not one.** There is no atomic batch; atomic multi-object semantics are declared cascades. The verdicts come back in request order and any of them may differ from what a single-request caller would have seen, because the requests are independent.

## 7. What is an exception

Everything in §4 is a value. These are the five things that raise, and the list is closed so that a second binding cannot differ on it.

| Raised | When |
|---|---|
| `DeclarationError` | the store has no usable declaration: none installed, or the installed one will not load |
| `UnknownTransition` | the named transition does not exist on that type in the current version. Not a verdict, because a verdict answers "may I", and this is "there is no such thing" |
| `StorageUnavailable` | the database is unreachable, or a transaction failed for a reason that is neither a serialisation conflict nor, on SQLite, a busy timeout. Both of those are retried and then become `stale`, which is a verdict (ADR-0090) |
| `SchemaMismatch` | the installed declaration and the tables disagree, which means a publish did not complete |
| `KeyReused` | this actor applied the idempotency key before, to a different request. A retry is the same request, so a different body under a used key is a defect in the caller's key generation, and neither replaying the other request's result nor applying this one under its key would be honest (ADR-0077) |

Note what is not there. An unknown object id is `NotFound`, an invisible one is also `NotFound`, and a malformed input is `Unsatisfied` on the guard that reads it. Those are answers about the domain and the caller must handle them, so they are values. `UnknownTransition` and `KeyReused` are faults, and each is also recorded in the attempt log, since both are mistakes a caller makes (ADR-0083).

## 8. Decided since the first draft

Each followed from a decision the record already carried, or from what the design made unavoidable.

- **Streaming stays as it is:** an iterator for `history`, a page for everything else. History is the only unbounded result, and a cursor is what a caller can hold across a request boundary while an iterator is not.
- **The declaration is loaded from the store**, not from a file. `publish` writes it to `ok_declaration` and a starting process reads the installed version, which keeps a recorded event's declaration version resolvable and makes a process that disagrees with the store impossible rather than unlikely. The `.ok` files stay in version control as the input to a publish.
- **The exception list is closed** at the five of §7.
- **The core is synchronous** (ADR-0091). The asynchronous question this section once left open is settled: a synchronous core with an asynchronous wrapper for the first consumer's FastAPI service, and the connection source injected, which is what the harness needs to interleave requests at statement boundaries.
- **Cursors are opaque strings.** A settled cursor encodes a (transaction, position) pair on PostgreSQL and a position on SQLite, and a caller never parses it (ADR-0089).
