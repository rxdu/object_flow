# ADR-0179: What an application acts on: a refusal's detail, and pages

- **Status:** **Accepted** by the author, 2026-10-11, by ADR-0182 ("go with your recommendations on the fifteen questions"). Before: Built at the author's direction ("go ahead with your recommendations, /loop until it's ready", 2026-10-11), for the author's review.
- **Date:** 2026-10-11
- **Refined by:** ADR-0181 — a refusal names no object that does not exist, an invariant refusal tells `proposable`, and a filter is typed against its literals. ADR-0192 — `history` answers pages.
- **Refines:** ADR-0092 (what a remedy points at), ADR-0150 decision 2 (the wire's notation), ADR-0122 (a refusal's objects)
- **Relates to:** `DESIGN.md` §5.5, §10; `design/library-api.md` §4, §5; `guides/wire.md`; `design/procurement-second-review.md` §3 (I2-5, I2-12, the integrator's `not_computed` list)

## Context

The second procurement integrator listed what the wire left out that an application acts on, first "a refusal's `objects` and `proposable`, since an agent is told to act on them", then paging. Both backends marked a refusal's `unknown`, `objects`, `proposable` and `consulted`, a flag's `unknown` and `objects`, an invariant violation's conflicting `objects` and a refused call's verdict beyond its clause as `not_computed`; and every read answered one page, so a worklist of ten thousand objects arrived whole. `DESIGN.md` §5.5 and `library-api.md` §4 already define the refusal's fields; `library-api.md` §5 gives `query`, `available`, `metric`, `pull` and `export` a cursor and no page size.

## Decision

1. **A refusal tells its caller what its remedy points at**, on both backends alike: an `unsatisfied` verdict its clause `unknown` or false, the `objects` its clause names, whether a proposal of the transition requested would be accepted, `proposable`, and what it `consulted`, which the reference runner does not record and lists as `not_computed`; a flag its `unknown` and `objects`; an invariant violation every failure, on every object, and the `objects` in conflict, each failure's but the object it fails on; and a refused call its verdict whole, its clause qualified, its `proposable` false, since a call is no request a caller files. An offer's refusal tells as much, but nothing consulted, as an offer asks no evaluator.
2. **The objects a clause names are noted as it is evaluated**, by the rule of `DESIGN.md` §5.5, settled here where it was open: every object a name or a path evaluates to, the object the clause is evaluated on excepted; both sides of `and`, `or` and `implies`, the side that does not decide included; the derived attributes it reads; of a collection, only the elements the outermost aggregate's `where` selects, a nested aggregate, a name an aggregate binds and the collection a membership test reads naming none; a generated guard its stated objects. The runner, whose values carry no type, takes a text equal to an object's id for a reference, where the core never would; a flow with such a text is a divergence the tee would show.
3. **`query`, `available`, `pull` and `export` answer pages**: at most `limit` items, 1 to 1000, **100 where none is asked**, and the cursor of the next page, none where the page is the last. A cursor is opaque; it carries the read it came from, and continues only that read, any other being refused `malformed_cursor` (`query`, `available`), and the last object it gave. **A page goes on after that object in the read's own order**, whatever was written since, so an object created between pages is met once in its place and none is met twice, unless an object's order key changes between pages, when it may be met again or passed over, which only a snapshot held across pages, not offered, would prevent. `pull`'s page is its settled cursor, the last event given where more remain; `export`'s page never ends inside the rows one cursor value names.
4. **A metric's read stays one page**, and **`exceptions`** too: a metric's rows are groups, as many as the dimensions kept take values, which `bind`, `keep` and `filter` narrow; and the exceptions of a type are what a deployment must work through whole.
5. **The default page changes what a client that ignores cursors reads**: from today, the first 100 items. It is the moment to change it, before the wire is promised stable; the guides say to follow `cursor` until it is null.

## Alternatives rejected

- **Offset paging** (`offset`, `limit`): an object created or moving into the filter between pages shifts every later page, so an item is met twice or passed over under any write; a keyset cursor fails only where the item's own order key moves.
- **A cursor holding the order key's values**: it needs every key's values written and read back in the wire's notation, and a comparison in that notation that matches the store's order of states, members and objects; naming the last object and reading its keys as they stand gives the same place, objects never being deleted.
- **Paging a metric** by its dimension values: a page of groups is rarely what a reader wants, and a read keeping fewer dimensions is the narrower answer.
- **Leaving `consulted` out of the core's refusal** to match the runner: the core records it for the attempt log already; the runner, a reference and not a deployment, lists it as not computed, which the tee reads.

## Consequences

- `library-api.md` §4 and §5, `wire.md`, `integrate.md` and `DESIGN.md` §10 say what a refusal tells and how a read pages.
- The scenarios' replay reads every paged answer in pages of three, so its 123 compared reads exercise paging too.
- `TODO.md` closes the integrator's list but a publish report's sections and `/v1/batch`, kept with a reason.
