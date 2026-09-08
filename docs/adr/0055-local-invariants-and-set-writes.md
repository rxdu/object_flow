# ADR-0055: A third invariant form for single-object properties, and outcome steps that add to and remove from a set

- **Status:** Accepted — repair of D48 and D49, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0045, ADR-0052, ADR-0046

## Context

Writing the declaration syntax surfaced two things the model cannot express. Both were invisible while the model was described in prose and became obvious the moment someone tried to write a real type in it.

**Single-object invariants are rejected.** DESIGN.md §5.6 admits two forms and says publishing rejects anything else: an invariant that traverses relationships, which needs declared inverses, and one that scans a type, which must be symmetric. The rules exist so the affected set is computable. But the simplest invariant of all reads only the object's own attributes — "a unit in `RESERVED` has exactly one binding", "a closed order has a closed date" — and it is neither form. The affected set for it is trivially the object just written, which is why it was overlooked: it needs no rule, and so it got none.

**A set-valued attribute can only be replaced wholly.** The outcome grammar has one write step, `attribute := expression`, and the expression language has no set operators. So `photos`, `watchers` and `labels` can be assigned but not appended to. Every case study that has a set assumes otherwise: attaching an intake photo, adding a watcher, adding a typed link. Writing `set photos := photos + in.photo` requires a set union the language deliberately does not have, and replacing the whole set makes a concurrent second attachment silently discard the first.

## Decision

### 1. Invariants have three forms, not two

| Form | Rule that makes the affected set computable |
|---|---|
| **Local** | Reads only the object's own attributes and its own state. The affected set is the object written. No further rule. |
| **Traversal** | Reads related objects, and may traverse only relationships with declared inverses (ADR-0045). |
| **Type scan** | Reads other objects of a type, and must be symmetric (ADR-0052). |

Publishing classifies each invariant and reports which form it decided, so an author is never left guessing why one was rejected. Anything that fits none is still rejected.

The local form is the one most types will use and the cheapest to enforce, since it needs neither reverse traversal nor a scan.

### 2. Outcomes may add to and remove from a set

Two further outcome steps, alongside `set`:

```text
add    <path> := <expression>       insert an element; already present is a no-op
remove <path> := <expression>       delete an element; absent is a no-op
```

Both are read-modify-write at the moment they are applied, like every outcome write (ADR-0038), so two adds in one request accumulate and two concurrent adds serialise under serialisable isolation rather than one overwriting the other.

The expression language gains **no set algebra**. There is no union, difference or intersection operator, and `set` on a set-valued attribute still replaces it wholly, which stays available for the case where replacement is what is meant. Keeping element-wise change in the outcome grammar rather than in expressions means an add is visible as a step in the readable rule set, and it keeps `count`, `all`, `any` and `none` the only things that read a set.

## Alternatives rejected

### Treat a single-object invariant as a guard on every transition

The pre-ADR-0009 position, rejected there for the reason that still applies: the copies drift, and a new transition silently omits one.

### Classify a single-object invariant as a degenerate traversal

It traverses nothing, so the declared-inverse rule has nothing to check and the classification would be a lie in the publish report. Naming the form is clearer and costs one row.

### Set union in the expression language

`set photos := photos + in.photo`. Rejected: it opens set algebra in a language that has stayed small by refusing exactly this kind of extension, and it hides an element-wise change inside an expression, where the readable rule set cannot show it as the discrete act it is.

### A `Photo` part instead of a `file[]` attribute

Workable for photos, and it is what the first consumer does. Rejected as the general answer: a watcher or a label is not worth an object, and forcing one would make every tag list a table.

## Consequences

- DESIGN.md §5.6 gains the local form and §5.4 gains the two steps.
- The declaration syntax gains `add` and `remove` alongside `set`, and the checker classifies invariants into three buckets rather than two.
- A type with a set-valued attribute and no `add` action can be written, and now has a visible reason to exist rather than being an oversight.
- D48 and D49 are resolved.
