# ADR-0209: The log records what each event wrote, and a plain read of an absent value clears

- **Status:** **Decided** 2026-10-16 by the author ("continue the loop with your recommendations"), given after both questions were brought the same day with a recommendation (`TODO.md`); each decision is the recommendation. The three questions brought before them, metric reads at C5, a mirror's `imported_at` and what an erasure reaches through caller taint, are not read as decided by it and stay open.
- **Date:** 2026-10-16
- **Refines:** ADR-0033 decision 2 (how a fold of the log reproduces a row); ADR-0201 decision 7 (an assignment whose value is absent)
- **Relates to:** D692; `design/storage-schema.md` §4 and §9; `design/adversarial-harness.md` §1; ADR-0078 (every input recorded); ADR-0029 (a sequence's gaps); D655

## Context

ADR-0033 decision 2: "A fold of an object's events must reproduce its row; a repair tool may verify and rebuild rows from the log, never the reverse." `storage-schema.md` §4 says how: "`payload` is the writes and the inputs of one transition". The core records the inputs only. Two `bump`s assigning `count := count + inputs.by` and `stamp := now` leave `count: 10` and a stamp on the row and `{"inputs": {"by": 5}}` in each event, observed through the wire on 2026-10-16 (D692). Re-executing the log would not repair it: a sequence's value is allocated outside the request's transaction, so a refused creation leaves a gap (ADR-0029) that a replay of the satisfied requests does not.

ADR-0201 decision 7 refuses an assignment whose value is absent, `value_in_range`, which D655's `count := count + inputs.by` on an unset `count` needed. Probed through both backends on 2026-10-16, it refuses a plain copy as well: `other := count`, `held := peg` between optional references, and `held := if peg is not null then peg else held`, each with both absent. The language has no conditional step, so moving an optional value, `inventorize`'s `procured_for := peg` in `flow-review.md`'s appendix and `metric-scenarios.md`'s version 15, refuses a unit never pegged, and "move it if there is one" takes two transitions.

## Decision

1. **Each event records what it wrote.** Its payload holds `wrote` beside `inputs`: each attribute of the event's object whose stored value the event changed, by an input, an effect, a default, a minted value, a correction, an import, a migration or an erasure, with its value after the event in the wire's notation, and `null` for one it cleared. An event that changed no attribute holds none. A fold of an object's events, its state from each `to_state` and its attributes from each `wrote`, reproduces its row.
2. **An erasure redacts a personal attribute's writes as it redacts its inputs**: a personal attribute's value in `wrote` is set to `null` in each event of the erased object, and in each event of another object whose copy of the value it redacts. What it clears is itself written, `null`, in the erasure's and each `redact` event's `wrote`.
3. **`export` omits a personal attribute's writes** as it omits personal inputs. `history`, `events` and `pull` answer `wrote` as they answer `inputs`.
4. **The adversarial harness folds the log** after each request and fails a run where a fold does not reproduce a row (`adversarial-harness.md` §1).
5. **A plain read of an absent value clears an optional attribute.** An assignment whose expression is a name or a path, with no operator, call or `if`, whose value is absent, to an attribute declared `optional`, clears that attribute, as `clear` does, in both backends. Arithmetic, a call or an `if` whose value is absent, and an absent value assigned to a required attribute, still refuse the request `value_in_range`, as decision 7 decided. Both checkers' `absent` notice no longer notices such a read.

## Alternatives rejected

- **ADR-0033 amended to re-executing the log** (D692's option B). A row would then be verified by the code that wrote it, so a defect in that code reproduces itself, and a sequence's gaps are in no event.
- **Recording only what an effect wrote**, the inputs that write an attribute left to `inputs`. A fold would then need each transition's declaration to know which inputs are attributes; one rule, every change in `wrote`, costs an input's bytes twice.
- **Recording the whole row after each event.** It grows every event by the object's width where `wrote` grows by what changed.
- **Clearing on any absent value**, arithmetic included. D655's bump would then silently do nothing to an unset counter.
- **A conditional step in the language.** An addition to the language for one idiom that a plain read now covers.

## Consequences

- The event payload gains `wrote`, additive on the wire. Events recorded before this decision hold none, so a fold reproduces only rows whose every event has one; the harness's stores are built since.
- `storage-schema.md` §4 states it already; `storage-schema.md` §9 gains the redaction of writes, `guides/wire.md` and `library-api.md` the payload's `wrote`, `flow-format.md` §7 the narrower `absent` notice, and `declaration-syntax.md` §8 the assignment of a plain read.
- `metric-scenarios.md` version 15's expected notices lose the `absent` row this day's notice added, `inventorize`'s read being plain.
- The reference runner computes no payload (`EVENT_NOT_COMPUTED`), so the tee does not compare `wrote`; the harness's fold is its oracle. Decision 5 is in both backends, held alike by the tee.
