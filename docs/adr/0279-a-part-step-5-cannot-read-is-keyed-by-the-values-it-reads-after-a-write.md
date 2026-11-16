# ADR-0279: A part step 5 cannot read is keyed by the values it reads after a write

- **Status:** **Decided** by the author 2026-11-16 ("go with your recommendations"), on the first of two points put the same day for issue #98, with a recommendation. Built the same day.
- **Date:** 2026-11-16
- **Refines:** ADR-0137 decision 2 (what step 5 decides, and which invariants count against a transition) and decision 4 (a count, one per distinct collection and filter); `flow-format.md` §7.1
- **Relates to:** ADR-0251 (a creation reads the value it supplies); ADR-0274 and ADR-0176 (invariants left out of step 5); `docs/research/deciding-conditions-that-share-nothing-apart.md`; issue #98

## Context

Step 5 checks a transition's guards against the object as it stood before the request. It checks each invariant that reads nothing but what the transition writes against the object after the writes. It assumes nothing of the values before, since an object may stand violated under an admission (ADR-0137 decision 2), and the engine re-checks no invariant whose admitted violation stands (`DESIGN.md` §6, step 7). A part it cannot read, such as a call or a path to another object, is an opaque atom, keyed by its expression's text, as a count over one of the object's own collections is.

Reviewing #98 on 2026-11-16 at `e9b83ba` found three paths by which step 5 refused a transition a request can take, each refused alike by both checkers, "… can never be taken: what it writes, … cannot hold, with guard …, whatever the request":

- **An opaque part or a count read after the write was keyed as the same part read before it.** A transition taking `q1`, guarded `not is_whole(q1)`, with an invariant `is_whole(q1)`, was refused: the guard reads the quantity before, and the invariant the one supplied. So was one taking `item`, guarded `not item.f1`, with an invariant `item.f1`, and one taking a set of approvers, guarded `none(a in approvers)`, with an invariant `any(a in approvers)`.
- **A derived attribute read through `this` was not read as its expression**, so it kept one key before and after the write.
- **A name read through `this` was not counted among what an invariant reads.** `q1 > 0 and this.p > 5` counted against a transition writing `q1` alone, and was refused beside a guard `p <= 5`.

So a version adding `is_whole(q1)`, admitting the lines it finds fractional (`publish-and-import.md` §3) and adding the transition correcting them was refused at publish.

Building the fix found two more of the same kind, each probed the same day on both checkers. A required input whose attribute a loop then writes was read as the input: a transition taking `q1`, guarded `inputs.q1 <= 0`, whose loop sets `q1` to 1, with an invariant `q1 > 0`, was refused, and the core, run with the checks skipped, took it and left `q1` at 1. And keying a part by the value written would itself have tied a guard `not inputs.item.f1` to an invariant `item.f1` on a transition that calls the item's transition setting `f1`, a module both checkers accept today only because the two are spelt apart.

## Decision

1. **A part step 5 cannot read, an opaque atom or a count, is keyed by the values it reads.** A derived attribute is read as its expression, by its name or through `this`; `this.x` is read as `x`; and on a creation an attribute it takes as an input is the input, as ADR-0251 reads it. So a guard and an invariant share one atom only where both read one value.
2. **Read after a transition's writes, each attribute it writes is replaced by the value written**, and `state` by the state it enters. `is_whole(q1)` read after a transition taking `q1` is `is_whole(inputs.q1)`, one atom with a guard reading `is_whole(inputs.q1)`, so a contradiction between them is still found.
3. **Where the value after the writes is not exact, the part read after them is an atom of its own.** That is an attribute whose write step 5 does not follow: one written within a loop, by a set's `add` or `remove`, or from what the effect writes, a derived attribute's own reads included. It is also a `changed_since` naming a written attribute, `entered_at` once the transition enters a state, and a path through another object where the transition may change one: its effect calls or creates, within a loop too, or a part's cascade is on it.
4. **A name read through `this` counts as read wherever step 5 lists what a rule reads**, so an invariant reading `this.p` does not count against a transition that does not write `p`.

## Alternatives rejected

- **An atom of its own for every part read after a write.** It closes the first two paths as surely, and is simpler. But a guard reading `is_whole(inputs.q1)` would no longer meet the invariant `is_whole(q1)`, so step 5 would find fewer contradictions. The third path needs its own fix either way.
- **Leaving them.** A repair transition stays refused, and deciding more sets, #98's second point, would uncover more of them: the issue's six invariants beside `q1 > 0 and this.p > 5`, `undecided` at 0.27.0, were refused by its prototype.

## Consequences

- **What a flow meets.** A module refused by one of these paths loads. A module 0.27.0 accepted may now be refused where it holds a real contradiction step 5 newly reads: two guards reading one value through `this.` and by name, or a derived attribute through `this` against a guard on its expression, or an invariant `this.q1 > 0` against a guard `inputs.q1 <= 0` on a transition taking `q1`. Neither the design's modules nor the 948 planted checks hold one.
- **The code:** `scripts/flowlogic.py`'s `part`, `own`, `key`, `count_key`, `reads`, `writes` and `changes_others`, and `names_in` and `cases` reading `this.`; `check/logic.rs`'s port, each node not exact keyed through `after`.
- **The docs:** `flow-format.md` §7.1.
- **Tested** by twenty cases added to `flowlogic.py`'s self-test, and to its mirror in `check/logic.rs`, `the_reference_self_test_decides_the_same`. Sixteen planted defects in each checker were each caught by a failing case.
