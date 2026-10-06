# ADR-0159: A draft's evidence, a refused call's attempt row, and two reads confirmed

- **Status:** Decided by the author, 2026-10-06 ("go with your recommendations on 1 to 3"), given three questions the core's replay of `metric-scenarios.md` raised. Decision 1 corrected the same day: the reference's form was already `library-api.md` §5's `EvidenceRef`, which the search before the question missed (see there).
- **Date:** 2026-10-06
- **Refines:** ADR-0097 §5 and ADR-0101 (the evidence a draft cites), ADR-0122 (what records a refused call), ADR-0100 §6 (the interval export's order), ADR-0048 (what `available` names)
- **Relates to:** `DESIGN.md` §9, §10; `design/storage-schema.md` §6; `design/library-api.md` §5, §6; `design/metric-scenarios.md` §26, §38

## Context

The record was searched for each question before it was put to the author (`docs/LESSONS.md`, "Search the deferred questions before recording a gap as new").

- ADR-0101 has `draft` take "the metric and diagnostic references it cites", and ADR-0097 §5 has `submit` and `refresh` store a snapshot of them as they read, in `t_declaration_change.evidence`. ~~No document gives a reference's form.~~ *(Retracted 2026-10-06: `library-api.md` §5 has defined `EvidenceRef` since ADR-0101's verification pass of 2026-09-23, `{metric | diagnostics, bind, keep, over, filter}`. The search before the question was case-sensitive and missed the class's name; the claim was false.)* `metric-scenarios.md` §26 writes DC1's two in the document's own read notation, `metric(trial_refusals, keep: [clause, actor])`, which is a way of writing a read, not a wire shape.
- `metric-scenarios.md` §12 determines that a refused call's attempt counts under the requested transition with the call's clause, and that the refusal carries the call's verdict, the step and the object. It does not say what the attempt row stores. The core stores the request's verdict, `call_refused`, with the call's clause and remedy, and keeps neither the call's own verdict kind nor its target, which `of_attempt` has no column for. §38's `attempts(D06)` reads the row as the history writes a refusal: the call's verdict, `unsatisfied`, and `call: R20`.
- Building the export (ADR-0100 §6) found that a correction (ADR-0147, which ADR-0100 predates) rewrites the interval rows whose value or `.held` it changed. Building `available` found that `library-api.md` §6's `Page` has no place for the guards it did not evaluate, though §6 says `available` names them (ADR-0048, ADR-0049).

## Decision

1. **A cited reference is the arguments of the read it names**, which `library-api.md` §5's `EvidenceRef` already gives and this decision keeps: a metric reference is `{metric, bind?, keep?, over?, filter?}`, the `metric` operation's own arguments, `over` a duration as that operation takes it; a diagnostics reference is `{diagnostics, over}`, the type and, as `over`, the window `diagnostics` takes. *(Corrected 2026-10-06: this decision first wrote a diagnostics reference as `{diagnostics, window}`, a field `EvidenceRef` does not have; the earlier record stands.)* What this decision adds to `EvidenceRef` follows. `draft` and `revise` take `evidence`, a list of references, refused as `invalid input` where one is not of this form; a `revise` that gives none keeps the citations it had, as an unsupplied optional input skips its write (`DESIGN.md` §5.4). `submit` and `refresh` read each reference on the store as it stands and keep, beside the citations, each read's page as it answered: its rows, flags, `complete` flag and as-of time, or its refusal, which is a value and is kept as such. A metric the draft's own source adds is not yet readable, so the evidence cites what the installed version reads.
2. **A refused call's attempt row keeps the request's verdict, `call_refused`, and gains `call`**: the object the refused call or creation targeted and the kind of the verdict that call received. A row of any other verdict has none. The history's notation, the call's verdict with `call: <object>`, is then read from the row.
3. **Two reads confirmed as built.** The interval export emits a row again when a correction marks it, by the correction's position, since the correction rewrites the row and ADR-0100 re-emits a rewritten row so a consumer keeps its latest. `available` names each object's unevaluated guards on that object's item, the place a page of objects has for what it says about one.

## Alternatives rejected

- **A reference in the scenario's read notation, parsed by the engine.** It would make a document's notation a wire grammar, and give the engine a second parser for what the `metric` operation's arguments already say.
- **Links to the reads, without a snapshot.** ADR-0097 rejected it: a metric is computed on read, so a link shows today's value, not the one the change was argued from.
- **Refusing a submit whose cited read is refused.** The approver should see that the evidence did not read, not lose the change for it; the refusal is kept as what the read answered.
- **Storing the call's verdict in the row's `verdict`.** The row would no longer say that the request was refused by a call, which the refusal it answered did say; and the `refusals` metric's verdict would differ from the verdict the requester was given.
- **Storing only the call's target.** The call's own verdict kind would be lost, and the history's notation could not be read from the row.
- **A separate table for refused calls.** One nullable column holds it, as `applied_position` holds an observing clause's event.
- **Not re-emitting an interval a correction marks.** A consumer keeping the latest row would keep the value the correction replaced.
- **A page-level list of unevaluated guards for `available`.** A guard is unevaluated for an object, and a list beside the page could not say for which.

## Consequences

- `storage-schema.md` §6 says what `t_declaration_change.evidence` holds and gives `of_attempt` its `call`; `library-api.md` §5 gives `Attempt` its `call` and §6 states the reference's form, the export's re-emission on a correction and `available`'s items; `DESIGN.md` §9 says what a reference is.
- The core keeps a draft's citations and its submission's reads, and the attempt row's `call`; `scripts/replay-scenarios.py` compares `evidence of DC1` and reads `attempts` in the history's notation.
- Until the core serves `diagnostics`, a submit citing one stops as not run.
