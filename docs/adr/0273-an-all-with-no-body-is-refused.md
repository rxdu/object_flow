# ADR-0273: An `all` with no body is refused

- **Status:** **Decided** by the author 2026-11-12 ("proceed with all 3"), on the recommendation put the same day for issue #86. Built the same day.
- **Date:** 2026-11-12
- **Refines:** the model's check 47 (`declaration-syntax.md` §8.3, §10)
- **Relates to:** ADR-0205 (an aggregate's body is a condition each element meets); issue #86

## Context

Issue #86, filed 2026-11-12 at 0.22.0, reported that `all(x in c where p)` with no body passes every check and holds whatever `p` says of the elements. A `where` only selects, so the elements selected have no condition to meet. A deployment wrote four guards so, each meaning "every line meets this", and each refused nothing.

Reproduced 2026-11-12 at `6e63017` on the reference checker and runner, on a flow of the repository's own. A guard `all(l in lines where l.state == Line.OPEN)` passed the checks, and an order with a withdrawn line was confirmed: "expected unsatisfied, and the request applies". Written `all(l in lines: l.state == Line.OPEN)`, the example was refused. Both evaluators take a bodiless `all` as true over what it selects, or absent where its filter is undecided (`eval.rs`, `flowrun.py`).

Check 47 already requires a body of `sum`, `min`, `max`, `avg` and `median`, whose value each is its body, refused at step 2 by both parsers. `count`, `any` and `none` mean something with a filter alone: how many it selects, and whether any or none is selected.

## Decision

1. **An `all` with no body is refused at step 2**, by check 47 in both parsers, with or without a `where`. The message names the form meant: `all(x in c: p)`.
2. **`count`, `any` and `none` keep an optional body.**

## Alternatives rejected

- **Reading a lone `where` on `all` as its body.** One expression would mean two things by position, and `all` alone of the aggregates would read its `where` so.
- **A notice.** The guard would still refuse nothing, and a notice is read only by whoever checks.
- **Leaving it to an example.** Only an example expecting the refusal finds it, and the deployment's form came back once it was fixed.

## Consequences

- `scripts/flowexpr.py` and the core's `expr.rs` refuse it, in the same words; `declaration-syntax.md` §8.3 and check 47's row say so.
- **What a flow meets.** A module writing a bodiless `all` is refused where 0.23.0 accepted it. No module in the repository or format 1's corpus writes one; each such expression held whatever it selected, so the refusal finds a rule that refused nothing.
- **Tested**: the core's parser test, a bodiless `all` with and without a `where` refused and `all(x in c: p)` and `any(x in c where p)` kept; the self-test's plant, a bodiless `all` in `servicedesk.yaml` caught at `syntax`, which both checkers agree on through the core's oracle.
