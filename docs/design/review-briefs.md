# Review briefs

Prepared 2026-10-21 at the author's request, for the reviews `TODO.md` holds open for the author: for each document, what was asked and when, what changed since, what deserves the author's attention now, and a recommendation. The author's own reading is still the review; a brief only says where it is worth spending. Each claim names its evidence; where a brief infers, it says so. The briefs were drafted by three agents reading the documents, their history and the code, and their cited facts sampled and checked before this was written: each contradiction listed under DESIGN.md, the PRD's rows, and the stale lines named. The stale lines and contradictions §1 and §4 to §7 name were corrected on 2026-10-22, in place, each with a note saying what it said (`TODO.md`); the PRD's own text, which is the author's, was left. The briefs are as written on 2026-10-21, and their line numbers are of that day.

## 1. The PRD

[`docs/PRD.md`](../PRD.md). **Asked** with the PRD's first draft (2026-09-23): review it, "especially the rows marked Inferred". Seven Inferred rows were accepted on 2026-09-28 as revision 12; "the other Inferred rows remain".

**Changed since.** It is revision 25, 2026-10-14 (`docs/PRD.md:3`). Revisions 13 to 25 (§11) were each at the author's direction, or accepted by the author (revision 23, in ADR-0150), but for 15, a coherence review adding nothing beyond the author's decisions; they need no second reading.

**Worth your attention now.** Thirteen rows are Inferred and were never accepted: D6 (labels and notes on every type), D7 (catalogues as governed data), D12 (datapoints validated like writes), C6 (UTC buckets), M2 (metrics through the one interface), M3 (split by version), M4 (split by actor kind), M5 (export), L4 (thresholds changed only by a governed path), L5 (rules reading metrics, whose one use case the PRD calls hypothetical), V3 (rules on trial), V4 (drafts carry evidence) and V5 (labels promoted). Four more are settled only in part: F6, F7, C2 and C5. The build has outrun them: all but D7, L4 and V5 have mechanism in the core (D6 `model.rs`, D12 and V3 `request.rs`, L5 `expr.rs` and `eval.rs`, M2 to M5 `metric.rs` and `export`), so rejecting one now means removing what is shipped. §10.2 still schedules several of them for a second or third release, and `TODO.md`'s line on what is safe to defer, and `traceability.md`'s "nothing here is implemented", are stale (inferred from the code, not a full traceability check).

**Recommendation: review named parts.** The thirteen rows as one batch, accepting each as built or naming what to remove, L5 first as the largest; the four partly settled rows; and §10.2, which no longer schedules the build. Then close the item.

## 2. The walkthrough

The walkthrough of ObjectFlow in use, a claude.ai page of 2026-09-25. **Asked** the day it was made: ten scenes of the product used as if built, the author's verdict on each saved with the page, to be read back into `TODO.md`. No commit reads any verdict back.

**Changed since.** Its scenes are written in the text language `service.of`, which ADR-0116 (2026-09-26) retired as a written form ("A flow is written in the flow description format … and in nothing else", decisions 1 and 2); no `.of` file exists in the repository's history (`git log --all --name-only | grep '\.of$'` finds none). The same flow lives on in the format as `examples/service.yaml`, `service-v2.yaml` and `people.yaml`, runs on the core, and is what the Flow Studio's guide tours. ADR-0114 removed the roles the scenes show.

**Recommendation: close as superseded.** Open the page once only to read back any verdicts saved on it, which exist nowhere else, and record any that still hold; to test the product against your expectations, walk the Flow Studio's `TESTING.md` on `service.yaml` instead.

## 3. The flow format specification

[`flow-format.md`](flow-format.md). **Asked** twice: as "draft 2" (2026-09-26) and as "the part worth reading slowest" (2026-09-28), since every deployment writes in it.

**Changed since.** About 80% larger: 24,534 words in 43 headings, from 13,607 in 36; §11, Examples, is new, and §7 (the checker's steps), §4.8, §4.9 and §4.14 grew most. It cites 74 ADRs, 56 of them since. The samples of 2026-10-11 are frozen as `format-1/`, held by `check-compat.py` ("11 bundles, 21 files as frozen, 175 examples held"), and `check-flow-format-doc.py` holds the text to the schema.

**Worth your attention now.** From 1.0.0 a valid description stays valid and means the same (ADR-0180 decision 3), so what a writer relies on cannot be taken back: §3 rule 3 (the reserved names); §5 with `declaration-syntax.md` §8 (absence, unknown, empty aggregates, `now`, `actor`); §4.8 and §7's step table (the order of guards and effects, and which findings are fatal); §11, entirely new and promised; and §4.14 to §4.16, each used by one frozen module only, so their prose rather than the corpus is the promise (inferred from a grep).

**Recommendation: review those five parts slowly before 1.0.0**, and skip Appendices A and B, §8 and §9, whose open questions ADR-0182 deferred. Merge the two items.

## 4. The declaration syntax

[`declaration-syntax.md`](declaration-syntax.md). **Asked** as the model's definition and the internal form the checks read, which no one writes since ADR-0116; the amendments it forced, ADR-0055 to ADR-0064, were accepted on 2026-09-28, which settled half of the item.

**Changed since.** 23% larger (35,818 words); §10, §6.9 and §8.3 grew most, much of it the expression semantics written as built after the differential tests of 2026-10-15. Its status line still says "draft, iteration 26 (2026-09-25)" (`:5`).

**Its role now.** The flow format defines meaning by it ("the meaning of every form is the meaning of the declaration it converts to", `flow-format.md:3`); its §10 numbers the checks whose codes ADR-0180 promises from 1.0.0; the core runs "the sixty-four checks of §10" (`check/mod.rs:10`); `check-syntax-doc.py` holds it, partially enforcing 36 of the 64.

**Recommendation: close the item as written**, its premise, a surface people write with amendments pending, having ended; fold into the flow-format review a reading of §8 and §9.5 beside `flow-format.md` §5, and of §10's numbers as the codes 1.0.0 will promise.

## 5. The Jira coverage study

[`case-study-tickets.md`](case-study-tickets.md) §6 and §7. **Asked** 2026-09-26: Atlassian's documented workflow features mapped to the format.

**Changed since.** Barely: three commits, two guards corrected on 2026-09-29 (D481, D482) when the reference runner first ran the examples. Its modules are held: `issues.yaml` (29 of 29 examples pass) and `servicedesk.yaml` (40 of 40) run in `check-flows.py`'s corpus, on the core, and in the frozen corpus. Its tables are not checked by any script.

**Worth your attention now.** Only judgments: fourteen rows marked *Differs*, where the engine departs from Jira by design, the weightiest the automatic transitions an application requests, and one *Written, in part* (SLA conditions on events). One sentence is stale: §7.8 item 2 (`:396`) calls a notice "proposed" that was built on 2026-09-29.

**Recommendation: close the item**, reading the *Differs* rows once if a single pass is wanted.

## 6. DESIGN.md as a whole

[`docs/DESIGN.md`](../DESIGN.md). **Asked** after its rewrite of 2026-09-08. About 6% of today's text survives from that rewrite (771 lines, 25,786 words, from 4,017; an agent's verbatim count), most of the rest restating ADRs the author accepted. The corrections of 2026-10-21 to `storage-schema.md` reached it the next day.

**Worth your attention now.** The ten places this brief found contradicting the build, eight lines and two passages, were corrected in place on 2026-10-22 (`15e7658`), each saying what it said, and checked again on 2026-10-25. What one writer changed in what the design promises is in §2, §5.1, §5.6, §6, §7's cursor and §10.

**Recommendation: read those six sections, then close the item as superseded.**

## 7. The implementation documents of 2026-09-09

**Asked** of six documents "none has been read by the author"; ADR-0074 to ADR-0080, decided with them, were accepted on 2026-09-28.

- **Storage schema** (`storage-schema.md`): checked line by line against the build on 2026-10-21, its built-in tables and 141 column names identical to the store's DDL (an agent's comparison); three parts stay design: §3.4's generated column, §8's constraints and §6 and §7's mechanisms for several writers. *Close as checked; read ADR-0225 and ADR-0226, which decided what its notes record.*
- **Library API** (`library-api.md`): `check-api-doc.py` runs its Python and matches its operation names, but not their arguments; the built interface is `docs/guides/wire.md`. Stale: `:803` (sweepability), `:829` (the cursor), `:108` to `:111` and `:828` (the harness's barrier). *Decide whether it stays a specification held to `wire.md` or is retired; read §2 and §7 only if it stays.*
- **Publish and import** (`publish-and-import.md`): §2's `PublishReport` declares sections the core marks not computed, by a decision the document does not mention; §6's classes were revised building the import (ADR-0154); §7's go-live runs in the studio's tests. *Read §2 and §7.*
- **Renderers** (`renderers.md`): §3 and §4, the agents' tools, are built and checked against the reference; §2, the printable rule set, is still "nothing generates it yet", an obligation DESIGN.md §13 states. *Read §2.*
- **Adversarial harness** (`adversarial-harness.md`): its built first slice is described; its racer, the part that sends requests at once, is not built, and with one writer on both backends there is nothing for it to race (inferred). *Read §1 and one question: does the acceptance test PRD N4 names need a racer while one writer writes?*
- **First-consumer cutover** (`first-consumer-cutover.md`): last changed before ADR-0149, which planned no switch-over, and never mentions it; still cited by `traceability.md` (N3, N5) and DESIGN.md §2. *Close as superseded, marking its header and pointing those citations at `publish-and-import.md` §7.*

**Recommendation: split the item** as above: two documents closed, four read in named parts only.
