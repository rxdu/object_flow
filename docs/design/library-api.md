# Library API

Draft, 2026-09-09. The request and verdict shapes of [`../DESIGN.md`](../DESIGN.md) §6 and the read surface of §10, as one interface a program calls. Transport bindings come after and are not here.

**What is verified.** The Python below executes, and `scripts/check-api-doc.py` compares the operations it offers against the read surface DESIGN.md §10 declares, so the two cannot drift apart silently. It is **not** typechecked — there is no mypy in the environment this was written in, and the annotations are therefore reviewed and not proven.

## 1. Why Python, and what that does not mean

The core is library-shaped: a call goes in, guards evaluate, a transition and its record come out (§2 of the model). The first consumer is a FastAPI and SQLAlchemy system being rebuilt on this, so a Python interface is the one that will be exercised first and is the one written here.

That is a **binding**, not the design. The operations, their arguments and their results are the API; the dataclasses are one rendering of it. A second binding should offer the same fourteen operations with the same meanings, and the checker's comparison against §10 is written against the operation set rather than against Python.

## 2. Three rules the shapes follow

**A refusal is a value, never an exception.** Every verdict of §5.5 — including `not found`, `stale` and `invariant violated` — is returned. Exceptions are for a fault: the database is gone, the declaration will not load, the caller passed something the type system should have caught. The reason is that a refusal is an ordinary outcome a consumer must handle, and an exception is the thing a consumer writes `except: pass` around.

**Every operation takes an actor.** There is no ambient identity and no session. The actor is a value the consumer's authentication produced, validated for shape and never for truth (ADR-0025), and visibility is applied on every read from it (ADR-0030).

**Nothing returns a mutable view of stored state.** Every shape is frozen. An object returned by `get` is what was true at the moment it was read, and writing to it would be the second write path the model exists to refuse.

## 3. The actor and the request

```python
from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
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
    context: Mapping[str, Any] | None = None


```

A creation names a `type` and no `object_id`; every other transition names an `object_id`. `expected_version` is the optimistic check of ADR-0023, `idempotency_key` makes a retry safe by replaying the first result rather than refusing it (ADR-0041), and `context` is the free-form route marker that ends up in the event's provenance.

## 4. The verdict

One of seven, and the caller must handle all seven. A `match` over them is exhaustive by construction, which is the reason for a union of frozen shapes rather than one class with an optional field per outcome.

```python

class Remedy(Enum):
    SELF_SERVICEABLE = "self_serviceable"
    DELEGABLE = "delegable"
    TEMPORAL = "temporal"
    DEPENDENT = "dependent"
    UNREACHABLE_FROM_HERE = "unreachable_from_here"


@dataclass(frozen=True)
class Satisfied:
    object_id: str
    version: int
    events: Sequence[int]
    replayed: bool = False


@dataclass(frozen=True)
class Unsatisfied:
    clause: str
    object_id: str
    remedy: Remedy
    unknown: bool


@dataclass(frozen=True)
class Stale:
    expected: int
    actual: int


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
    objects: Sequence[str]


Verdict = (
    Satisfied | Unsatisfied | Stale | NotFound
    | NotRequestable | OverLimit | InvariantViolated
)


```

`Unsatisfied.unknown` is the one field that repays explanation. A guard whose expression evaluated to unknown, because it read something absent, is unsatisfied — but it is not the same answer as a guard that was false, and §8.2 of the syntax makes the distinction. A caller showing "you cannot do this yet" wants to know which it was.

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
    attributes: Mapping[str, Any]


@dataclass(frozen=True)
class Page:
    items: Sequence[Any]
    cursor: str | None


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
    actor: Actor
    cause: int | None
    recorded_at: datetime
    payload: Mapping[str, Any]


```

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

    def check(self, actor: Actor, id: str, transition: str,
              inputs: Mapping[str, Any]) -> Verdict: ...

    def history(self, actor: Actor, id: str,
                follow: bool = False) -> Iterator[Event]: ...

    def exceptions(self, actor: Actor, type: str,
                   cursor: str | None = None) -> Page: ...

    def declaration(self, actor: Actor, type: str,
                    version: int | None = None) -> Mapping[str, Any]: ...

    def pull(self, actor: Actor, subscription: str,
             position: int) -> Page: ...

    def acknowledge(self, actor: Actor, subscription: str,
                    position: int) -> None: ...

    def publish(self, actor: Actor, source: str,
                dry_run: bool = False) -> "PublishReport": ...
```

`PublishReport` is defined in [`publish-and-import.md`](publish-and-import.md) §2, which owns it: publishing is where its shape is decided and one shape should have one home.

Fourteen operations: the eleven of §10, the write path of §6, and the two operational calls that are not object operations. The checker holds this list against §10 rather than trusting it.

Three of them are worth reading twice.

**`check` is advice, not a reservation.** It simulates the same sequence internally, takes no locks and has no effect, so the answer can be stale before the caller acts on it. It also costs what the request would cost: simulating a cascade over five hundred parts evaluates five hundred guards.

**`available` requires the transition to be sweepable.** Its guards must decompose into an indexable prefilter plus a residual (ADR-0048). A transition that is not sweepable is refused here rather than scanned, which is a real constraint on how a time-driven guard is written, because this query is how all time-driven work in the system finds its objects.

**`batch` is N transactions, not one.** There is no atomic batch; atomic multi-object semantics are declared cascades. The verdicts come back in request order and any of them may differ from what a single-request caller would have seen, because the requests are independent.

## 7. What is an exception

Everything in §4 is a value. These are the four things that raise, and the list is closed so that a second binding cannot differ on it.

| Raised | When |
|---|---|
| `DeclarationError` | the store has no usable declaration: none installed, or the installed one will not load |
| `UnknownTransition` | the named transition does not exist on that type in the current version. Not a verdict, because a verdict answers "may I", and this is "there is no such thing" |
| `StorageUnavailable` | the database is unreachable, or a transaction failed for a reason that is not a serialisation conflict. A serialisation conflict is retried and then becomes `stale`, which is a verdict |
| `SchemaMismatch` | the installed declaration and the tables disagree, which means a publish did not complete |

Note what is not there. An unknown object id is `NotFound`, an invisible one is also `NotFound`, and a malformed input is `Unsatisfied` on the guard that reads it. Those are answers about the domain and the caller must handle them, so they are values.

## 8. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **Streaming stays as it is:** an iterator for `history`, a page for everything else. History is the only unbounded result, and a cursor is what a caller can hold across a request boundary while an iterator is not.
- **The declaration is loaded from the store**, not from a file. `publish` writes it to `ok_declaration` and a starting process reads the installed version, which keeps a recorded event's declaration version resolvable and makes a process that disagrees with the store impossible rather than unlikely. The `.ok` files stay in version control as the input to a publish.
- **The exception list is closed** at the four of §7.

**Still open.**

- **Async.** The Protocol is synchronous and the first consumer is FastAPI, which is not. A synchronous core with an async wrapper is the recommendation, since the core's one long operation is a database transaction and the wrapper can own the pool — and the harness needs a synchronous surface anyway, to stop between statements. It is still a fork worth confirming before either is built.
