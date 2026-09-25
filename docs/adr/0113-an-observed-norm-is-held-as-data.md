# ADR-0113: A norm observed in history is held as data, refreshed from a metric

- **Status:** Accepted by the author, 2026-09-25 ("yes to all four, go ahead and write them"), against `docs/PRD.md` revision 9: M7, L4 and UC-12.
- **Date:** 2026-09-25
- **Refines:** ADR-0084 (metrics and the rules that read them)
- **Relates to:** ADR-0104 (managing the flow is an upper layer's), `design/first-consumer-prd-check.md` §3.2

## Context

PRD M7 now lets a business-exception condition compare against the norm observed in the flow's own history, for example the usual range of time in a state for that kind of work, beside a committed date, a threshold held as data and a fixed value. The case study's principle is "observed, never declared". But a metric may be read only in a `require` clause (check 56), never in a derivation, a condition or an outcome value. Those uses would carry the value past the two rules that make a metric guard sound: its arguments resolved twice, and its value recorded on the event. The same section already names the way through: "where a sweepable transition or a person's judgement needs the value, a declared transition writes it into an attribute and guards read the attribute" (ADR-0084 §7).

## Decision

**A norm is held as data.** A deployment declares where it lives, for example an attribute on a catalogue object per state or per kind of work, such as `p80_waiting`. Something outside the engine refreshes it from the metric, as a recorded transition:
- an upper-layer job, on its own schedule (ADR-0104);
- or a person, after reading the metric.

A condition then compares against the attribute, exactly as it compares against a reorder point (L4). The norm is observed, since its value comes from the engine's own metric over history. It is governed, since it changes only by a recorded, attributed transition, whose event keeps the value. And it is explainable, since a condition's verdict reads a value on the record. No new mechanism is needed.

## Alternatives rejected

- **A condition, derivation or flag that reads the metric directly.** Check 56 forbids it for the reasons above. It would also recompute a metric over every row for every object read, and make a derived value depend on rows its reader may not see (T5).
- **A metric's `flag` comparing against another metric.** That works for a flag over the metric's own rows, not for a condition on one object. It may be added later for that narrower use.
- **Fixed values only.** M7 now names the observed norm, and the case study depends on it.

## Consequences

- `DESIGN.md` §5.12 describes the pattern. Its staleness is the refresh interval, which the deployment chooses and the recorded refresh events show.
- The conditions of M7 against observed norms ship in the second release (PRD §10), with the formulas and metrics across many objects they read.
