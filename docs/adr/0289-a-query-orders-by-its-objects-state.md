# ADR-0289: A query orders by its objects' state

- **Status:** **Decided** 2026-11-21 under the author's direction ("review open issues and address properly"), on the recommendation formed the same day for issue #101, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-21
- **Refines:** ADR-0127 decision 2, as ADR-0194 decision 7 amended it (`query`'s order keys)
- **Relates to:** ADR-0179 (pages and their cursor); `docs/research/ordering-a-list-by-its-objects-state.md`; issue #101

## Context

`query`'s order keys are `created_at`, `entered_at(<tracked member>)` and an attribute holding one value. A list showing each object's state cannot sort by it. The engine pages the list, so a caller sorting it sorts only the page it holds, the reason ADR-0127 rejected leaving order to the caller. Issue #101 found this; `order: "state"` was refused `unknown_order_key` on 0.31.0 (`TODO.md`, *Issues filed on GitHub*).

The engine already orders a state by its place in its type's declared states. A metric's rows order states that way (`library-api.md` §6), and `query`'s comparison of two values of a key, `rank`, orders a state so, as it orders an enumeration member by its members' declared order. Only a key yielding the object's own state was missing.

A family's types each declare their own states, since a base declares none, unless they bind one state machine (`flow-format.md` §4.10, §4.14).

## Decision

1. **`state` is an order key of `query`**, ascending unless followed by ` desc`. It orders objects by their state's place in their type's declared states, not by the state's name. The object id breaks ties, as for every key, and every object has a state, so none sorts last as absent.
2. **On a family, `state` is an order key where every type of it with objects declares the same states in the same order**, as types binding one state machine do. Otherwise it is refused `unknown_order_key`, with the reason "the family's types declare different states, so no one order holds".
3. **Paging is as for every key**: the cursor names the last object of a page, and the next page is ordered as the first was (ADR-0179).
4. **It needs no index and reads nothing personal.** The query sorts what its filter selects, bounded by the read's pool and deadline, as an unindexed attribute is sorted (ADR-0194 decision 7).

## Alternatives rejected

- **By the state's name.** Alphabetical order says nothing of the lifecycle, and a type's states are declared in the order its author chose.
- **By the state's category, as Linear orders a list by status.** It is coarser, and a type's states within one category would tie. A key `state.category` may be added should a case need it, without changing this one.
- **On a family whose types declare different states, by each object's place in its own type.** It would interleave unrelated orders, the first state of one type beside the first of another, which reads as an order and is none.
- **On a family, by category and then by place.** A family would then order its objects otherwise than each of its types orders its own.
- **An attribute holding each state's place, written by every transition**, as the issue considered. It is a second copy of the state, which every transition must keep equal to the first.

## Consequences

- **The core's `query`** takes the key (`reads.rs`), held by `a_query_orders_by_state_as_its_type_declares_its_states` on both backends.
- **The docs**: `wire.md` and `errors.md`, `integrate.md`, `library-api.md` §6 and `DESIGN.md` §10 name the key.
- **It refuses nothing a release took before.** A key it refused, it now takes.
