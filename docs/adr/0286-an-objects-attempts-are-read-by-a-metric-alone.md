# ADR-0286: An object's attempts are read by a metric alone

- **Status:** **Decided** by the author 2026-11-18 ("go with your recommendations"), on the point put the same day, with a recommendation, for the finding that the checks refused an object's `attempts` outside a metric as a member its type does not declare, found addressing it at the author's word ("address the three follow-ups in TODO"). Built the same day.
- **Date:** 2026-11-18
- **Refines:** `declaration-syntax.md` §8.3 (*A derivation may aggregate over its own object's flow data*)
- **Relates to:** `declaration-syntax.md` §6.9 (the sources a metric reads); `DESIGN.md` §7 (the attempt log and the event log); ADR-0216 (a type's own member of the name comes first)

## Context

Both runners answer `o.attempts`, an object's attempt log as rows, since a metric over objects reads it: §6.9 lists `o.attempts` among an object's collections a metric's row has. A derivation may aggregate over `this.intervals`, `this.intervals(<member>)` and `this.transitions` (§8.3), and §8.3 said nothing of attempts.

Outside a metric, check 24 and step 3 refused `this.attempts`, saying it "reads attempts of Account, which Account does not declare". That reason pointed an author at declaring an attribute named `attempts`, which would have hidden the attempt log from the type's metrics too (ADR-0216).

## Decision

1. **An expression outside a metric does not read an object's attempts**: a derivation, an invariant, a condition, a uniqueness condition, an effect and a backfill. Check 24 and step 3 refuse it, saying the attempt log is read by a metric alone.
2. **A guard reads attempts through a declared metric**, over `<Type>.attempts` or `<Type>.attempt_counts`, called by `metric(…)` with the window it needs (§6.9).
3. **The runners keep answering `o.attempts`** for a metric over objects. A published module reads it nowhere else.

## Why

- **The attempt log is not the object's history.** It is appended by requests that did not apply, and its rows are pruned after the deployment's retention period (`DESIGN.md` §7, §10). A derivation or an invariant over it would change without a write to its object, so it could not be stored, indexed or held after a write.
- **A metric already says what pruning keeps.** Its window bounds the rows it reads, and `<Type>.attempt_counts` keeps the daily counts permanently, so a guard counting refusals per week reads them over the whole history.

## Alternatives rejected

- **Read as rows outside a metric, as `this.intervals` and `this.transitions` are.** Spans and events come from the event log, which is never pruned (`DESIGN.md` §7); attempts come from neither.
- **The runners also refuse `o.attempts` outside a metric.** The checks refuse it at publish, so no published module reaches it, and the code would hold nothing.
