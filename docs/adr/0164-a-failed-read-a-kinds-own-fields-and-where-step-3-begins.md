# ADR-0164: A failed read is unavailable storage, a kind's invariant reads its own fields, and ADR-0162's step 3 begins with the evaluator

- **Status:** Decided by the author, 2026-10-07 ("go with your recommendations on 1 to 3"), on recommendations stated with the questions (`TODO.md`, building ADR-0162 steps 1 and 2).
- **Date:** 2026-10-07
- **Refined by:** ADR-0165 — the uniqueness key's index and an object's parts read on first use, which step 3 measured next.
- **Refines:** ADR-0146 (a kind's new invariant), ADR-0153 decision 3 (what the slice does not run), ADR-0162 decision 2 (the steps)
- **Relates to:** `DESIGN.md` §5.11; `design/library-api.md` §7; `design/declaration-syntax.md` §10; `design/scoped-reads.md` §7; D532

## Context

Three questions building ADR-0162's first two steps raised, each with the evidence it was raised on.

- **A failed read.** Since ADR-0162 step 1 an operation reads the store as it goes, and a read that fails stops it with `NotRun`, naming the read, which the service answers 501 Not Implemented. `NotRun` is a construct the core does not run (ADR-0153 decision 3); a read that fails is the store's fault, which `library-api.md` §7 raises as `StorageUnavailable`.
- **A kind's invariant beyond its own fields.** `DESIGN.md` §5.11 lets a kind declare "invariants over its own fields, checked when it is recorded", and check 54 refuses "a kind's invariant reading anything but its own fields". Both checkers decide that clause by a text pattern, which catches a path through the subject and misses a type scan by name: a check kind declaring `count(Check) < 2` passes both (observed 2026-10-07 running `check-flows.py` and the core's `check::check`). The core checks a kind's invariants on the recording alone (D532) and the reference runner on every request, so such an invariant would be enforced differently by each. *(The question as put to the author said the checkers accept `subject.label != ""`, inferred from `Library::load` accepting it; `Library::load` runs no check, and both checkers refuse that form. Corrected here before this record was committed, on running them.)*
- **Where step 3 begins.** Timed in phases at 4,000 objects, about 170 of a metric read's 228 ms is computed after its rows are read (`design/scoped-reads.md` §7), where step 3's index ranges shorten reads.

## Decision

1. **A read of the store that fails is unavailable storage, never a construct not run.** In the core, `NotRun` has two cases: a construct the core does not run, and a read of the store that failed, which every read of the world gives, naming what it read. The service answers the second 500, as it answers the store's own errors, and a binding raises it as `StorageUnavailable` (`library-api.md` §7).
2. **Check 54's clause on a kind's invariant is decided over the parsed expression**, in both checkers: an aggregate, a type named as a scan, a metric or any call is refused, as the text pattern already refuses a path through the subject or another reference, `now` and the actor. No check is added; check 54 says it already, and the core still checks a kind's invariants on the recording alone, which this makes the only reading there is.
3. **ADR-0162's step 3 begins with the evaluator's cost of a metric read**, profiled and reduced where it is spent, and then the index ranges the decision names, for a windowed metric and a query's filter, each measured with the probe as the steps before it were.

## Alternatives rejected

- **Keeping one `NotRun` for both.** A 501 tells a caller the feature does not exist, so it does not retry, and a broken store would read as a missing feature.
- **Converting a failed read into the store's error at each operation's entry.** A request's transaction returns it beside its verdict, but a read takes a world and answers a value or not run; two surfacings of one fault, chosen by the operation, would be the inconsistency this removes.
- **A new error type in every signature that reads the store.** It would carry the same two cases through every evaluator function for no difference in what a caller can do.
- **Checking a kind's invariant wherever a write reaches what it reads.** ADR-0146 holds that nothing changes an observation, and D532 showed an invariant re-checked over old observations refusing every request.
- **Keeping the form and checking it on the recording alone without saying so.** The declaration would say one thing and the store enforce another, which is the silent difference the checkers exist to prevent.
- **A new check for it.** Check 54 states the rule; what was missing is its decision over the expression, not a rule.
- **Step 3's index ranges first.** They shorten a metric's reads, about a quarter of its time by the measurement, and leave the rest.

## Consequences

- `eval::NotRun` is an enum of the two cases; every read of the world gives the second, and the service, the examples' runner and the store's own error read it so.
- `library-api.md` §7 says a read that fails partway through an operation raises `StorageUnavailable`.
- Check 54's clause is decided over the expression in `check-syntax-doc.py`, which the flow checker runs, and the core's checker, a plant holding the type scan the pattern missed; TODO's finding, corrected, is resolved.
- `design/scoped-reads.md` §7 and TODO order step 3's work: the evaluator first, then the index ranges.
