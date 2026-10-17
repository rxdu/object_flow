# ADR-0191: The fifth review's minors, decided

- **Status:** **Decided** 2026-10-13 on the author's standing instruction, "continue the loop to get the stack ready for production, follow your recommendations", each decision the recommendation stated here, but decision 7, which is the author's.
- **Date:** 2026-10-13
- **Refined by:** ADR-0205 — decision 3's derived time past the calendar refuses the read `value_in_range`, where it was answered null. ADR-0211 — decision 7's settings: the `v*` tags' ruleset and Dependabot, `check-all` on `main` waiting for pull requests.
- **Refines:** ADR-0176 and ADR-0189 (`taken_by`), ADR-0185 (`value_in_range`), ADR-0190 decision 1 (the calendar), ADR-0183 decision 3 (`--create`), ADR-0188 (the evaluator's token)
- **Relates to:** `design/production-review.md` §9; `DESIGN.md` §9; `guides/wire.md`; `guides/integrate.md` §6, §9; `flow-format.md` §7

## Context

The fifth production review (`production-review.md` §9) listed minors beside its blocker and majors, which ADR-0190 decided. Some were fixed with those (duplicate keys, a log whose reader has gone, hidden characters in a source); the read gate that held people's writes behind analytics is gone with the three pools (ADR-0190 decision 2). The rest are decided here.

## Decision

1. **A proposal is decided by those who may take its transition.** `reject` by an actor of a kind its transition's `taken_by`, or its type's, does not name is refused as approving it would be, `invariant_violated`, clause `<transition>_taken_by`, remedy `delegable`, where an agent could reject what a person proposed; `withdraw` likewise, but by its proposer, who may always take back their own (DESIGN.md §9: the engine reads a kind only where the flow's author declares one, and the proposed transition's `taken_by` is where they declared it). Rejected: holding reject to people always, which reads a kind no flow declared.
2. **A version names the rules it changes.** The checker's notice `rule_changed`, against the previous version: for each type in both, the conditions and invariants the new version removes or rewrites, and its transitions' guards that refused and no longer do, so a draft making a guard always true is named to whoever approves it (REVIEW-GAP). Rejected: detecting a guard that is always true, which no checker decides in general, and a rule loosened another way would pass unnamed.
3. **What a read answers, a request can send.** Money an effect writes past what an input holds, eighteen digits at its currency's minor unit (sixteen before the point in dollars), refuses its request `value_in_range`, as a decimal past its digits does; a derived time past the calendar is answered without a value, where year 2,739,933 was answered, and an effect may not write one (ADR-0190 decision 1). A guard compares such a time as the number it is, since both backends compute it: making it unknown, as a date past the calendar is, would refuse a renewal of a lease that runs to the legacy 9999-12-31, whose answer the comparison has. A duration stays signed, as the language makes one (`timestamp - timestamp`), but a metric's window, `over`, and a diagnostics' `window` are positive, else `400`.
4. **The service says what it was given.** At start: `--create` over a store that exists, which then guards no unmounted volume; an evaluator's token to be sent over `http://` to a host beyond loopback; a last stop that was not clean, the store's `-wal` holding frames before it is opened; argument errors stamped as every other line. `/metrics` adds `objectflow_build_info{release}` and `objectflow_start_time_seconds`; a store gone from its path answers `/health` `503 store_replaced`, as a write does; `malformed_filter` carries a `reason`; and under `WatchdogSec` the service sends `WATCHDOG=1` at half the interval while its runtime answers, which says the service is alive, `/health`'s `writes_stalled` saying whether writes move.
5. **A supersession group's erasure reads its members by an index**: `superseded_by` indexed, and a store made without it given it as it opens, where each member scanned every superseding type, 1.43 s for 41 members.
6. **Stated, not changed**: `/health` stays `200` while the store's clock leads, the lead in `/metrics` for an alert; `/health` on the main listener answers within the connection cap, and a monitor reads it through `--metrics-bind`, which the cap does not reach.
7. **The repository's settings are the author's** (question 16): required status checks on `main`, a ruleset for `v*` tags, and Dependabot. The workflow files are changed on the instruction: actions pinned by commit at their Node 24 releases, the release split so its write permission is the publishing job's alone, since a permission is a job's and not a step's, and checkout keeping no credential.

## Consequences

- `/v1` refuses what it took: an agent's rejection of what it may not take, money past an input's range from an effect, a non-positive window. Before 1.0.0 this costs no promise (ADR-0180).
- A proposal records its proposer by identity in both backends (D611), which building decision 1 found named two ways.
- An erasure still reads every proposal and every label once, which building decision 5 found and `TODO.md` records to measure.
- Held by tests named in `TODO.md`, each mutation-checked, through both backends where both have the path; the workflows by parsing and each action's `action.yml` at its pinned commit, until they run at the next push.
