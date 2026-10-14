# ADR-0203: The sixteenth review's findings, decided

- **Status:** **Decided** 2026-10-14 on the author's standing instruction, "keep the bar and continue the loop", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-14
- **Refines:** ADR-0202 decision 1 (where an erased actor's identity is replaced); ADR-0148 and ADR-0106 (no metric reads a personal value); ADR-0201 decision 4 (a filter by a personal member)
- **Relates to:** `design/production-review.md` §20; `guides/operate.md`; `guides/errors.md`

## Context

The sixteenth production review (`production-review.md` §20) found no blocker. Its operator found no major and gave GO, having measured every finding of the fifteenth fixed. Its adversary found two majors, so the bar is not met.

**N1.** ADR-0202 decision 1 replaced an erased actor's identity where an effect wrote it from `actor.id` or `actor.principal`. A copy escaped:
- an order created with `requester: requested_by`, the usual way a requester is carried to a child, kept the address in its column and in its creation event's inputs;
- so did a memo's `copied_by := approved_by`.

`get`, queries and the log's export answered it, and the store's file held it. The erasure worked by how a value was written, not by what it was.

**N6.** No metric reads a personal value (ADR-0106, ADR-0148), but check 56 read only a metric item's own members. Through a reference or a derived attribute, both checkers accepted:
- a dimension `d.owner.name` or `d.owner_name`;
- an expression `max(d.owner.salary)`.

An agent's metric read answered each person's name and salary. On the wire, a metric's filter on `d.owner_rich` or `d.owner_name >= "B"` was answered, reopening for metrics the name-range probe ADR-0201 closed for queries.

**The minors**, each measured or read by one of the two:
- `pull` with a cursor past the log, a subscriber's from before a restore, answered an empty page and a settled cursor behind it, where `export` and `acknowledge` refuse it.
- The guide's socket calls needed the service's user or group, unsaid.
- A build from the repository's root takes rustup's default toolchain, not `core/`'s pin.
- An erased value an index entry held stayed in a page's free space after the entry moved: 14 copies in one page.
- A page's memory is what it reads: 145 MB for a 9.8 MB page.
- `export` names each event's actor by its identity, personal or not.
- errors.md gave the wrong code for a decimal with too many places.
- Any actor may run `import_batch`, whose `decided_by` is free text.
- Both checkers accept three constructs the core refuses at run time: `"by " + actor.id`, `count(d)` in a metric, and `actor.principal` assigned where none is sent.
- `/health` said `store_unreadable` while writes were answered, after the file was overwritten in place.
- A subscription to no type, to unknown transitions or with a negative `dead_after` is taken and delivers nothing.

## Decision

1. **An erasure of an actor whose identity is personal replaces it, by value, in every attribute of type `identity` and every recorded input of that type, whatever wrote it**, in both backends. A value of type `identity` reaches an attribute or an input only as one: from `actor.id`, from another such attribute, or from a client. This replaces ADR-0202 decision 1's set, of the attributes and inputs an effect wrote from `actor.id`, which it contains. An identity sent in an input of another type is the flow's to type `identity` or to mark `personal`. (N1, D677)
2. **No metric over objects reads a personal value through a reference or a derived attribute either**, in both checkers. A path from the item is followed through references, and a derived attribute it reaches is judged by check 10's own reading of what carries a personal value. The first hop stays check 56's. **A metric read's filter on a derived attribute reading a personal value is refused `personal`**, as one through a reference was. (N6, D678)
3. **`pull` refuses a cursor past the log `invalid_cursor`**, as `export` and `acknowledge` do, and as errors.md said it did. (D679)
4. **The guides:**
   - the socket's calls run as the service's user or a member of its group (D680);
   - build from `core/` for the pinned toolchain (D681);
   - an erased value an index moved may stay in free space until a `VACUUM` with the service stopped, verified to leave none (D682);
   - a page's memory follows what it reads (D683);
   - `actor_id` is filtered where identities are personal (D684);
   - the decimal codes (D685);
   - `maintain`, `upgrade` and `import_batch` stay with the operators' tools (D686).
5. **Recorded for later** (`TODO.md`):
   - the checkers to refuse what the core cannot run (D687);
   - writes refused once `/health` finds the store unreadable (D688);
   - a subscription's filter checked as it is made (D689).

   **Unchanged:**
   - the sandbox's clock past 9999;
   - the sandbox's load, quadratic in types;
   - an overflowing filter matching nothing;
   - "listening on" said before a busy `--metrics-bind` refuses.

## Alternatives rejected

- **A checker notice for every copy of an identity** (decision 1). It would leave the copy in the store after an erasure; replacing it by value leaves none.
- **Rewriting every text column and every event's inputs by value** (decision 1). A value of another type holding an identity is a flow's error to type it; scanning every column and every payload under the writer for each erasure costs what the types already say.
- **Marking personal every attribute a path to a person reaches** (decision 2). A reference to a person is not their personal data; what the path reads at its end is.

## Consequences

- An erasure rewrites more columns than ADR-0202 decision 1 did, each by its type's table, and the inputs of every transition taking an identity, by the type's objects' events.
- A flow with a metric reaching a personal value through a reference or a derived attribute is refused, a change that may refuse what an earlier release accepted. Before 1.0.0 this costs no promise (ADR-0180).
- Each fix is held by a test that fails without it (`defects.md` D677 to D686), each mutated.
