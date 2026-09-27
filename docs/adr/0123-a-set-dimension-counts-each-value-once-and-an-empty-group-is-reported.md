# ADR-0123: A set dimension counts each value once, and a group with no body is reported

- **Status:** Decided at the author's direction, 2026-09-27: "write the UC-1 to UC-3 checked scenarios, continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed". Awaiting the author's review.
- **Date:** 2026-09-27
- **Refines:** ADR-0098 (a set-valued dimension), ADR-0122 decision 18 (an aggregate over no elements)
- **Relates to:** `declaration-syntax.md` §6.9; `flow-format.md` §4.9; `metric-scenarios.md` §3 and §7

## Context

`metric-scenarios.md` writes UC-1 to UC-3 as a fixed flow, a fixed history and the values their reads return, each derived by hand and recomputed by `scripts/check-scenarios.py`. Two of UC-1's values depend on questions the specification left open (D443, D444).

A delivery's configuration is the model of its units, `i.object.units.model`, a path through the set `units` and one hop past it. ADR-0098 says a set-valued dimension "counts a row once under each member". That wording is exact where the path ends at the set, whose members are its values. Here the members are units and the values are models, so a delivery with two units of one model is either counted once under that model or twice. Counted twice, delivery D02's stay in preparation enters its group's median twice, and the median moves from 1d 1h to 3d 12h (`metric-scenarios.md` §3.5).

A delivery that was delivered and never moved on has a current span in `DELIVERED` with no duration, since nothing accrues on a finished object. A group whose every row is like it has rows and no body. ADR-0122 decision 18 gives the value of an aggregate over no elements, and nothing says whether such a group appears in the result.

## Decision

1. **A set-valued dimension counts a row once under each distinct value its path reaches** (D443). Where the path ends at the set, its values are its members, as before. Where it reaches past the set, two members that reach one value count the row once under it. A path that reaches no value counts the row under the absent value, as any absent dimension does (ADR-0122 decision 7).
2. **A group whose rows all lack a body is reported, with the value its expression has over no rows** (D444). A group exists wherever the source and filter select a row, and its value is computed over the rows whose body is present: absent for `median`, `min`, `max`, `avg` and `percentile`, 0 for `count` and `sum` (ADR-0122 decision 18).

## Alternatives rejected

- **Once under each member**, the old wording read literally. A delivery's wait would weigh as many times as it has units of one model, so a split by configuration would measure units, not deliveries; and two readers, one counting members and one values, would report different medians for one group.
- **Leaving out a group with no body.** A reader could not tell "no delivery is in this state" from "deliveries are in this state and accrue nothing", which is the difference between an unused state and a finished one (PRD V2 asks for states never used). It would also make a group appear and disappear as its last present body was added or aged out.

## Consequences

- `declaration-syntax.md` §6.9 and `flow-format.md` §4.9 state both decisions; ADR-0098 carries a back-link.
- `metric-scenarios.md` §3.2 and §3.5 hold the values the decisions give, and `scripts/check-scenarios.py` recomputes them.
