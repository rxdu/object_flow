# ADR-0167: The runner's replay of the scenarios stops at the port

- **Status:** Decided by the author, 2026-10-08 ("go with your recommendation on R4, option b"), on three options stated with a recommendation.
- **Date:** 2026-10-08
- **Refines:** ADR-0166 (decision 2, the metric scenarios' replay against the reference runner)
- **Relates to:** ADR-0154 decision 8 (what the reference runner's import takes); `design/metric-scenarios.md` §1; `design/publish-and-import.md` §4; `scripts/replay-scenarios.py`

## Context

ADR-0166 decision 2 holds both backends to the record, and says the metric scenarios' replay "runs against the runner's sandbox as well as the core's, so both are held to the values `metric-scenarios.md` derives by hand". Building it (ADR-0166's slice R4, 2026-10-08) found that the scenarios' history begins with a port (`metric-scenarios.md` §1): the legacy system's objects imported with their creation times, the times each entered its current values, its earlier values as legacy intervals, and the spans where the legacy record is silent (`publish-and-import.md` §4). These set each object's `recorded_from`, and make a metric count as gaps the objects whose record does not cover what it measures.

The reference runner takes none of it. ADR-0154 decision 8 made its import the one an example's `import` step needs, new objects with an empty disposition file, answering anything else as not run, and left legacy history to the core; and the runner's metrics hold every record whole, so they count no gap. Its replay of the scenarios therefore stops at the port, as not run, before the first request of the history.

The values the scenarios quote have two holders besides the core: the hand derivation in the document, and `scripts/check-scenarios.py`, which recomputes each value from its own transcription of the flow by `declaration-syntax.md` §6.9's rules and shares no code with either backend. The core's replay is held to both.

Three options were put to the author: the runner takes legacy history and counts gaps, reversing ADR-0154 decision 8; the runner's replay stops at the port, ADR-0166 decision 2 amended to say so; or the replay ports into the runner without the legacy data and holds only the reads that do not stand on it. The author chose the second.

## Decision

1. **The reference runner's replay of the scenarios stops at the port, as not run.** The legacy history the port carries stays the core's, as ADR-0154 decision 8 leaves it, and the runner's metrics count no gap. `scripts/replay-scenarios.py --runner` stays: it loads the port, places its objects as far as the runner takes them, and reports where it stops; its list of reads held, `scripts/scenarios_passing_runner.txt`, stays empty.

2. **ADR-0166 decision 2 holds the runner to the core by the comparisons, not by the scenarios.** What the runner is held to beside the core is what nothing else holds: publishing, by the publishing comparison (`apps/flow-studio/server/tests/test_publishing.py`), and every example, its `publish` steps included, by the comparison of every example through both backends. The scenarios' values are held to the core by its replay, to the document by `check-scenarios.py`, and each to the other.

## Alternatives rejected

- **The runner takes legacy history and counts gaps.** It would reverse ADR-0154 decision 8 for a large slice, the import's legacy intervals, silent spans, `recorded_from` and every metric's gap rule, written a second time, to add a third holder to values that already have two independent of the core. The runner's worth as an oracle is greatest where nothing else holds the core, which publishing was.
- **The replay ports into the runner without the legacy data.** Every read standing on the port's history would differ from the document by construction, so the replay would hold only the reads that do not, and a list of exemptions would grow where no difference is a finding against either backend.

## Consequences

- ADR-0166 names this ADR as refining its decision 2.
- `scripts/replay-scenarios.py`'s description and `scenarios_passing_runner.txt` say the runner's replay stops at the port, by this decision.
- `TODO.md`'s R4 is done, and the question of legacy history in the runner is decided. The scenarios' port, which the flow checker refuses as a version (`TODO.md`), is a separate question this does not decide.
- Should a later decision give the runner legacy history, it revisits ADR-0154 decision 8 and this one together.
