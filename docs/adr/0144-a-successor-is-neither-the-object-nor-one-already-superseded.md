# ADR-0144: A successor is neither the object nor one already superseded, so a supersession chain has no cycle

- **Status:** Proposed, 2026-09-30, for the author's decision. The questions it leaves are listed at its end.
- **Date:** 2026-09-30
- **Refines:** ADR-0028 decision 1 (a superseding transition takes a successor)
- **Relates to:** `declaration-syntax.md` §6.1 and check 26; `flow-format.md` §4.19; `DESIGN.md` §5.5, §6, §8 and §10; ADR-0087 (erasure follows the chain); D485, D486; `TODO.md`, "Say what check 26's cycle is"

## Context

Check 26 refuses "a cycle among the declared supersession targets". A declaration shows only types, and the cycles among types it can show are a type superseded by its own, as the model's own example merges a `Contact` into a `Contact` (`declaration-syntax.md` §6.1) and the flow format's a `Customer` into a `Customer` (`flow-format.md` §4.19), and a round trip between two types, as a record converted to another kind and later converted back. All of them must be allowed. No checker implements the clause, and its meaning was left open on 2026-09-26.

The cycle that matters is among objects: X superseded by S, whose chain comes back to X. `get(id, follow)` follows supersession "to the live successor" (`DESIGN.md` §10), and erasure erases every member of the chain, predecessors and successors (§8, ADR-0087). A cycle has no end for `follow` to return.

**Nothing refuses one.** Check 26 refuses `supersede this` as written, but a successor given as an input is not checked. Two observations, by the reference runner on 2026-09-30:
- In the flow format's customers module, `merge_in` is an internal transition of the surviving customer that takes `duplicate` and calls `inputs.duplicate.merged_into(successor: this)`. Given `dana.merge_in(duplicate: dana)`, the request applies, and Dana ends in `MERGED`, superseded by herself (D486).
- In the same module with `only_via` removed from `merged_into`, so that its successor comes from the request, `lee.merged_into(successor: dana)` and then `dana.merged_into(successor: lee)` both apply: two ordinary requests make a chain with no end.

A cycle among objects is shown only by the data, so it cannot be a publish check; it has to be refused when a request would close one.

## Decision

1. **Check 26 no longer names a cycle among the declared supersession targets.** A declaration cannot show a cycle worth refusing. Check 26 keeps its other clauses, which D485 completed: a `supersede` outside a superseding to-state, a superseding state entered without one, a `supersede` written twice, inside a loop or naming an optional input, and `supersede this`.
2. **A transition whose `supersede` names an input carries a generated guard, `successor_not_superseded`**: the successor is not the object itself, and is not in a superseding state. Its remedy is `self_serviceable`, since the caller corrects an input of the request, naming another successor, such as the one `get(id, follow)` returns. It is evaluated with the generated guards, after `not_erased`. A successor bound by `create` is new, and needs no guard.
3. **`get(id, follow)` returns the last member of the chain**, the one object in it that is not superseded, in whatever state that object is in. `DESIGN.md` §10 says "the live successor", which a successor since closed or deleted is not.

**Why decision 2 is enough.** A transition's new state is applied before its effect runs, and each transition its effect calls is guarded against the state produced so far and applied at once (`DESIGN.md` §6, steps 4 to 6), so no other transition runs between a transition's guards and its new state. Suppose a cycle formed. Every member of it is superseded; take the member whose superseding state was written last. Its successor is another member, or the member itself. If the successor came from an input, its superseding state was written before this member's, and so before this member's guard ran, which then found it superseded, or found it to be the member itself, and refused. If it was bound by `create`, it was created by this member's effect, after this member's state was written, so its own superseding state was written later still, and this member was not the last. Either way the cycle could not have formed, even within one request, as a merge whose effect calls the other object's merge back would try. Every chain is then a tree of predecessors ending in one object not superseded, which `follow` returns.

## Alternatives rejected

- **Keep a declaration-level cycle among types.** The only such cycle is a type superseded by itself or a round trip between two types, and every one of them is legitimate.
- **Refuse only a successor whose chain ends at this object**, the least that prevents a cycle. It must walk the successor's chain inside the request, a loop no declaration bounds, where every loop the engine runs has a declared bound (ADR-0071). It also admits a successor in the middle of a chain, which a reader then reads through members already superseded.
- **Record the chain's end in place of the successor named.** The engine would record an object the caller did not name, and the event would disagree with the request.
- **Refuse a successor in any final state, as `whole_open` refuses a finished whole.** A final object takes no further transition and cannot be reopened, erasure aside, so a record could never be merged into a closed one, and no declaration could allow it. An author who wants that refusal writes it as a guard on the successor's state.
- **Tolerate cycles, and have `follow` and erasure detect them.** The record would hold a chain with no end, and `follow` would have no answer that means anything.

## Consequences

- `declaration-syntax.md` §6.1 and check 26, and `flow-format.md` §4.19, lose the cycle clause and state the generated guard; `flow-format.md`'s refusal order and `DESIGN.md` §5.5 and §6 list it with its remedy; `DESIGN.md` §10's `get` returns the last member of the chain.
- The reference runner evaluates the guard. The customers examples gain a case: `dana.merge_in(duplicate: dana)` is refused as `call refused`, naming `merged_into`'s call and the clause `Customer.successor_not_superseded`, remedy `self_serviceable`. D486 is resolved, and the TODO item closes.
- `flow-format.md` §3 reserves `<attribute>_provided` among the generated guards' names, and none of the others. That is recorded in `TODO.md` separately, and is not decided here.

## Questions for the author

1. Is a cycle refused by a generated guard on the request (decision 2), rather than by walking the successor's chain, or not refused at all?
2. Is the guard narrow as proposed, so that it refuses a successor already superseded but not one closed or deleted, leaving that refusal to an author's own guard?
3. Is the name `successor_not_superseded` acceptable?
