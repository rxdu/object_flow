# Traceability: the PRD against the design

Every requirement and use case of [`../PRD.md`](../PRD.md), with the parts of the design that meet it and whether they do. The PRD is the baseline (`../DESIGN.md`, preamble), so this is where a design choice shows which requirement it serves, and where a requirement shows whether anything serves it. `scripts/check-traceability.py` holds this table against the PRD: every requirement and use case appears exactly once, every cited section, decision and document exists, and the run fails while any row is partial or a gap. `scripts/check-corpus.py` runs it.

**Status.**
- *Covered*: a mechanism is stated in `DESIGN.md` and in the document that owns its implementation, and no open defect breaks it.
- *Partial*: the model meets it, but an owning document is not yet amended or an open defect limits it.
- *Gap*: nothing in the design meets it yet, or an open defect makes it fail.
- *Unverifiable*: the PRD itself says the requirement cannot yet be verified.
- *Revision proposed*: the requirement, read with the others, cannot hold as written, and the PRD's §12 proposes its revision to the author; the design meets the proposed wording.

The checker refuses either of the last two on any row the PRD does not name for it, so neither can be used to park a hard row.

Covered is a claim about the design, not about software: nothing here is implemented, and the adversarial harness (`adversarial-harness.md`) is what will test the claims once it is.

**History.**
- Baseline of 2026-09-23, taken after ADR-0081 to ADR-0086 were accepted: 21 covered, 43 partial, 7 gaps, over 52 requirements and 19 use cases.
- The same day the design was iterated until every row was covered or unverifiable:
  - ADR-0087 to ADR-0095 closed the gaps and the defects D202 to D217;
  - an independent review then read every cited passage against every clause of its requirement, in three passes, and found eighteen more defects, D218 to D235, which ADR-0096 repaired; since then a covered row must cite a document that owns an implementation, which the checker enforces;
  - five further reviewers, one per slice of the PRD, the decision record and the documents' agreement with each other, then found D236 to D258, which ADR-0097 to ADR-0100 repaired, and found C2, UC-8 and UC-10 unsatisfiable as written beside T5, which PRD §12 proposes to revise;
  - their verification found D261 to D274 in three passes, the last finding nothing that breaks coverage beyond one contradiction, which ADR-0101 repaired;
  - the six implementation documents were amended;
  - declaration-syntax iteration 19 spelled the new constructs, with checks 54 to 61.
- On 2026-09-24 an audit of the first consumer's production code (`first-consumer-audit.md`) and the unit's journey written from it (`unit-journey.md`) were read against every row. They found five defects of the design, two of the checker and the example (D280, D281), and five gaps to decide against the PRD; the review found two more (D282, D283), and an independent verification of its decision five more (D284 to D288). ADR-0103 disposed of every gap and design defect, the rest were corrected in place, and every row stayed covered.

- On 2026-09-24 the author's positioning decision became PRD revision 4: N6 and UC-20 are new, and both are covered by the design as it stood, since the core already had no scheduler and one write path (ADR-0104).

- The same day, revision 5 made the PRD agree with itself; it added L6, the events upper-layer applications observe, which the design already met (DESIGN §7).

- The same day, five readers checked the whole record against revision 5, one slice each, and every row was re-traced. They found sixty-four defects, D290 to D353. ADR-0105 and ADR-0106 decided the ones that needed a decision, and the rest were corrected in place. Three requirements cannot hold as written against what the design can build, and PRD §12 now proposes their revision: D11, which asks for multi-choice attributes to be tracked as a team is not; L1, which asks rules to read labels; and N5, which asks for values the store has no object for to be preserved. Rows UC-7, UC-13, UC-19 and UC-20 gained the citations the re-trace found missing. UC-20's acceptance is given declarations that record the facts it asks about: two of its five questions — which units bought for an order still wait for it, and which deliveries may be marked ready — need `procured_for` and `mark_ready`, which the flow review declares (`flow-review.md` §8) and the author has yet to decide on (§7). *(Closed 2026-09-28: the author made the review's decisions 2 to 7 the operations platform's own (`flow-review.md` §7), and UC-20's checked scenario installs the two as its own version 14 (`metric-scenarios.md` §36).)*

An independent verification of the two decisions then found that D5 is met neither for labels nor for overrides, which the first pass had marked covered (D362); PRD §12 proposes D5's revision too.

The same day the worked example's newer cases were written into checked modules — a two-publish returns flow and the journey's `RobotModel` — and rows V1, V5, L4, L5 and UC-13 cite them.

The same day the author's principle on cascades became PRD revision 6: F8, that a transition may cause transitions on related objects when its rule says so clearly enough that none comes as a surprise, and UC-21, which tests it on the first consumer's delivery completion. The design has had cascades since ADR-0019 but printed them only where they start; ADR-0108 prints every cause of a transition on its own type (D378), and both rows are covered.

The same day the author accepted the eight revisions PRD §12 proposed, as PRD revision 7. The seven rows marked `revision proposed` are covered: the design already met the wording that was accepted, and no citation changed.

The same day ADR-0109 decided the question D345 had left open: what every object in a state must carry is an invariant over that state, binding every route into it, and publishing reports what each creation skips. Rows F1 and T1 cite it; every row stays covered.

On 2026-09-25 the author's decision to deploy the engine as an internal service became PRD revision 8: N7, the service every caller reaches through one interface, with authentication and the mapping of roles upstream; T6, an actor's kind declared with its type; and UC-22, which tests both. ADR-0110 meets them, and T2 now names network access to the service.

The same day the first-consumer check (`first-consumer-prd-check.md`), treated as a case study rather than a specification, became PRD revision 9: F9, a rule declared as a flag; T7, who may read a metric, with splits by person restricted by default; M7 widened to the norm observed in history; N3 and N5 stated as general capabilities with the first consumer as their reference and acceptance case; UC-23 and UC-24, which test the first two. ADR-0111 to ADR-0113 meet them. T1 cites the flag, M2 and M6 the audience, M7 and UC-12 the norm held as data; N3 and N5 changed wording and no citation, since the design met the general capability already.

The same day the author decided that ObjectFlow records who acted and never evaluates it, which became PRD revision 10 and ADR-0114. F1, F7, D6, D12, C2, M2, T1, T5, T6, T7 and N7 were rewritten and UC-10, UC-11, UC-15, UC-19, UC-22 and UC-24 changed their acceptance. Every one of them is met by the engine doing less: the rows now cite ADR-0114 and the sections that state the new boundary, and those that cited ADR-0030 or ADR-0112, both superseded, no longer do.

The same day the author said the core's language, Rust (ADR-0132), and accepted what PRD revisions 11 to 14 record: the calendar non-goal kept, seven Inferred rows accepted, machine telemetry out of scope and N5's archive withdrawn, and an agent the default drafter. ADR-0115 to ADR-0133, among them the flow description format (ADR-0116), how flows are authored (ADR-0131) and the metric rules of ADR-0123 to ADR-0129, changed no row's status; F1, F5, F6, D3, D8, T3, T6, N7 and UC-17 now cite them.

On 2026-09-28 a review read the requirements and design whole, in six slices and then the decision records from ADR-0081 on, for whether they still agree, and found D462 to D477. ADR-0134 decided the two that needed a choice: the operator's creation attributed to the operator, and an erasure required of a type taking a personal input. PRD revision 15 made the PRD agree with itself, and the two revisions its §12 proposed, F7 a Must and T6 naming whose kind no rule reads, the author accepted as revision 16; the design already met both, and every row stays covered.

Current: 82 covered, 0 partial, 0 gaps, 1 unverifiable, 0 revision proposed. Goals are not rows here; G4 is met through C2's.

## Requirements

| ID | Covered by | Status | What remains |
|---|---|---|---|
| F1 | DESIGN §5.8, §5.9, §10; ADR-0010; renderers.md §2, §3; declaration-syntax.md §1, §6.8, §6.9, §8.1; ADR-0105; storage-schema.md §6; ADR-0109; publish-and-import.md §2; ADR-0114; ADR-0116, ADR-0131; flow-format.md §1, §4, §7 | covered | |
| F2 | DESIGN §1, §3, §5.9, §6, §8; ADR-0040, ADR-0042, ADR-0054; library-api.md §6; ADR-0105; publish-and-import.md §3 | covered | |
| F3 | DESIGN §5.8, §10; ADR-0025, ADR-0037; library-api.md §6; ADR-0099; declaration-syntax.md §1; ADR-0103 | covered |  |
| F4 | DESIGN §5.5; ADR-0090, ADR-0092; library-api.md §4; ADR-0105 | covered | |
| F5 | DESIGN §5.9; ADR-0027; publish-and-import.md §3; ADR-0099; declaration-syntax.md §6.6; ADR-0101; ADR-0105; ADR-0131; flow-format.md §4.16 | covered |  |
| F6 | DESIGN §9; ADR-0085; publish-and-import.md §1; ADR-0097; ADR-0131 | covered |  |
| F7 | DESIGN §5.9, §9; ADR-0085, ADR-0096; publish-and-import.md §1, §2; storage-schema.md §6; ADR-0097; ADR-0101; ADR-0114 | covered |  |
| F8 | DESIGN §5.4, §6, §12; ADR-0012, ADR-0019, ADR-0020, ADR-0038, ADR-0108; declaration-syntax.md §5.2; renderers.md §2; library-api.md §6 | covered | |
| F9 | DESIGN §5.5, §6, §13; ADR-0085, ADR-0111; declaration-syntax.md §5.1, §6.11; library-api.md §4, §5; storage-schema.md §4, §6; renderers.md §2, §4; adversarial-harness.md §1 | covered | |
| D1 | DESIGN §7; ADR-0013, ADR-0033; storage-schema.md §2 | covered | |
| D2 | DESIGN §6, §7; ADR-0083, ADR-0088; storage-schema.md §6 | covered | |
| D3 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8; storage-schema.md §3; renderers.md §3; ADR-0099; flow-format.md §4.4 | covered |  |
| D4 | DESIGN §5.11; ADR-0082, ADR-0095, ADR-0096; declaration-syntax.md §6.8; storage-schema.md §3; ADR-0101 | covered |  |
| D5 | DESIGN §5.4, §5.11; ADR-0083, ADR-0095, ADR-0096; declaration-syntax.md §4.2, §6.8; storage-schema.md §2, §6; ADR-0099; ADR-0103; ADR-0106 | covered | |
| D6 | DESIGN §5.11; ADR-0082, ADR-0095; declaration-syntax.md §6.8; storage-schema.md §6; ADR-0114 | covered | |
| D7 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8; adversarial-harness.md §2; ADR-0105 | covered | |
| D8 | DESIGN §8, §5.11; ADR-0031, ADR-0078, ADR-0082, ADR-0087, ADR-0096; storage-schema.md §9; ADR-0100; ADR-0101; ADR-0105, ADR-0106; ADR-0133, ADR-0134; declaration-syntax.md §6.4, §10; flow-format.md §4.13 | covered | |
| D9 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8 | covered | |
| D10 | DESIGN §5.13; ADR-0086; declaration-syntax.md §6.10; storage-schema.md §6; ADR-0099; declaration-syntax.md §5.2 | covered |  |
| D11 | DESIGN §5.2; ADR-0083, ADR-0086; storage-schema.md §6; ADR-0099, ADR-0100; ADR-0106 | covered | |
| D12 | DESIGN §5.11; ADR-0082, ADR-0096; declaration-syntax.md §6.8, §8.3; ADR-0103; ADR-0114 | covered |  |
| C1 | DESIGN §5.2, §5.12; ADR-0084; declaration-syntax.md §6.9; ADR-0098; declaration-syntax.md §8.3; ADR-0103; ADR-0106 | covered |  |
| C2 | DESIGN §5.2, §5.12; ADR-0084, ADR-0098; declaration-syntax.md §6.9; renderers.md §2, §3; library-api.md §5, §6; ADR-0106; ADR-0114 | covered | |
| C3 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9; ADR-0098 | covered |  |
| C4 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9 | covered | |
| C5 | DESIGN §5.12; ADR-0084; data-driven-engine.md §7 | unverifiable | the PRD leaves the target to be set by measurement; data-driven-engine.md §7 records an indicative SQLite probe, and the author sets the target |
| C6 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9 | covered | |
| M1 | DESIGN §5.12; ADR-0083, ADR-0084, ADR-0096; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11; ADR-0101; ADR-0105, ADR-0106 | covered |  |
| M2 | DESIGN §5.2, §5.12, §10; ADR-0084; library-api.md §6; ADR-0098; ADR-0106; ADR-0114 | covered |  |
| M3 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9; storage-schema.md §6; ADR-0098; ADR-0101; ADR-0106 | covered |  |
| M4 | DESIGN §5.12; ADR-0084, ADR-0096; declaration-syntax.md §6.9, §8.1; ADR-0098; ADR-0101; ADR-0106 | covered |  |
| M5 | DESIGN §10; ADR-0094; library-api.md §6; ADR-0106 | covered | |
| M6 | DESIGN §5.13; ADR-0086, ADR-0096; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11; ADR-0101; ADR-0106; ADR-0114 | covered |  |
| M7 | DESIGN §5.7, §5.12, §10; ADR-0048, ADR-0084, ADR-0113; declaration-syntax.md §6.9, §8.3; storage-schema.md §3.4 | covered | |
| L1 | DESIGN §5.5, §5.11; ADR-0082; declaration-syntax.md §6.8 | covered | |
| L2 | DESIGN §6; ADR-0088, ADR-0095, ADR-0096; storage-schema.md §4, §6; library-api.md §5; ADR-0105; adversarial-harness.md §1 | covered | |
| L3 | DESIGN §6, §10; ADR-0037; library-api.md §6 | covered | |
| L4 | DESIGN §5.12, §9; ADR-0084, ADR-0085; declaration-syntax.md §6.9; publish-and-import.md §1; unit-journey.md §2 | covered | |
| L5 | DESIGN §5.7, §5.12; ADR-0084, ADR-0092; declaration-syntax.md §6.9; ADR-0098; returns-module.md §2 | covered |  |
| L6 | DESIGN §7; ADR-0013, ADR-0014, ADR-0089; library-api.md §6; ADR-0105; storage-schema.md §7; adversarial-harness.md §1 | covered |  |
| V1 | DESIGN §5.9; ADR-0085; declaration-syntax.md §2; ADR-0105; data-driven-engine.md §9; returns-module.md §1 | covered | |
| V2 | DESIGN §10; ADR-0085; library-api.md §6; storage-schema.md §6 | covered | |
| V3 | DESIGN §5.5; ADR-0085; declaration-syntax.md §5.1; renderers.md §2; adversarial-harness.md §1; ADR-0106; storage-schema.md §6 | covered | |
| V4 | DESIGN §9; ADR-0085; publish-and-import.md §1; ADR-0097 | covered |  |
| V5 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8; publish-and-import.md §1; returns-module.md §2 | covered | |
| T1 | DESIGN §3, §8, §13; adversarial-harness.md §1, §2; ADR-0097, ADR-0100; ADR-0101; ADR-0105; ADR-0109; declaration-syntax.md §3.4; ADR-0111; ADR-0114 | covered |  |
| T2 | DESIGN §1, §13; ADR-0110; adversarial-harness.md | covered | |
| T3 | DESIGN §7, §8, §9; ADR-0033, ADR-0083, ADR-0089; storage-schema.md §2, §6; ADR-0099, ADR-0100; ADR-0101; ADR-0103; ADR-0105; ADR-0126, ADR-0134 | covered |  |
| T4 | DESIGN §8, §10; ADR-0083; storage-schema.md §3.2; publish-and-import.md §4; ADR-0100; ADR-0101 | covered |  |
| T5 | DESIGN §5.8, §5.12, §10; ADR-0114; declaration-syntax.md §6.7, §6.9; library-api.md §5, §6; storage-schema.md §4 | covered |  |
| T6 | DESIGN §5.8, §6; ADR-0110, ADR-0114; declaration-syntax.md §3.1, §6.10, §8.1; library-api.md §3; storage-schema.md §6; ADR-0125, ADR-0126 | covered | |
| T7 | DESIGN §5.12, §10; ADR-0114; declaration-syntax.md §6.9; library-api.md §6; publish-and-import.md §2; renderers.md §2 | covered | |
| N1 | DESIGN §2, §5.12; ADR-0090; storage-schema.md §2, §7; ADR-0106; declaration-syntax.md §6.9; data-driven-engine.md §7 | covered | |
| N2 | DESIGN §2, §7, §8; ADR-0012, ADR-0091, ADR-0096; library-api.md §1; storage-schema.md §6, §9; ADR-0100; ADR-0103; ADR-0105; data-driven-engine.md §9 | covered |  |
| N3 | DESIGN §2; ADR-0090; storage-schema.md §7; first-consumer-cutover.md §5 | covered | |
| N4 | DESIGN §13; ADR-0091; adversarial-harness.md §1, §2, §3, §4; ADR-0100; ADR-0105, ADR-0106 | covered |  |
| N5 | DESIGN §11; ADR-0075, ADR-0077, ADR-0093, ADR-0096; publish-and-import.md §4, §7; first-consumer-cutover.md §4a; ADR-0100; ADR-0101; declaration-syntax.md §3.1; ADR-0103; ADR-0106 | covered | |
| N6 | DESIGN §2, §6, §7, §10, §13; ADR-0012, ADR-0022, ADR-0104, ADR-0105; library-api.md §6 | covered |  |
| N7 | DESIGN §2, §5.8, §13; ADR-0037, ADR-0110, ADR-0114; library-api.md §3, §7; storage-schema.md §6; declaration-syntax.md §8.1; first-consumer-cutover.md; ADR-0132 | covered | |

## Use cases

| ID | Covered by | Status | What remains |
|---|---|---|---|
| UC-1 | DESIGN §5.12, §7, §11; declaration-syntax.md §6.9; publish-and-import.md §4; ADR-0098; ADR-0106; ADR-0123; metric-scenarios.md §3 | covered |  |
| UC-2 | DESIGN §6, §7, §5.12; ADR-0096; declaration-syntax.md §6.9; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11; metric-scenarios.md §4 | covered |  |
| UC-3 | DESIGN §8, §10; ADR-0096; declaration-syntax.md §6.9; publish-and-import.md §4; ADR-0100; metric-scenarios.md §5 | covered |  |
| UC-4 | DESIGN §5.11, §5.5, §6; declaration-syntax.md §6.8; ADR-0099; metric-scenarios.md §9 | covered | |
| UC-5 | DESIGN §5.11, §5.12; declaration-syntax.md §6.8, §6.9; metric-scenarios.md §10 | covered | |
| UC-6 | DESIGN §5.4; declaration-syntax.md §4.2; ADR-0103; metric-scenarios.md §11 | covered | |
| UC-7 | DESIGN §12; declaration-syntax.md §6.8; data-driven-engine.md §3.7; ADR-0081; metric-scenarios.md §14 | covered | |
| UC-8 | DESIGN §5.12; ADR-0098; declaration-syntax.md §6.9; metric-scenarios.md §15 | covered | |
| UC-9 | DESIGN §5.12, §11; declaration-syntax.md §6.9; ADR-0106; metric-scenarios.md §16 | covered | |
| UC-10 | DESIGN §5.5, §5.12; ADR-0098, ADR-0114; declaration-syntax.md §6.9; library-api.md §4; metric-scenarios.md §19 | covered | |
| UC-11 | DESIGN §10, §5.12; library-api.md §6; renderers.md §3; ADR-0114; metric-scenarios.md §20 | covered | |
| UC-12 | DESIGN §5.12; declaration-syntax.md §6.9, §8.3; ADR-0113; metric-scenarios.md §21 | covered | |
| UC-13 | DESIGN §5.9, §5.11, §5.12, §10; declaration-syntax.md §6.6, §6.8, §6.9; publish-and-import.md §3; library-api.md §6; ADR-0098; ADR-0105; returns-module.md §1, §2; metric-scenarios.md §24, §27 | covered |  |
| UC-14 | DESIGN §5.5; declaration-syntax.md §5.1; ADR-0097; storage-schema.md §6; ADR-0106; metric-scenarios.md §25 | covered |  |
| UC-15 | DESIGN §9; publish-and-import.md §1; ADR-0097; ADR-0114; metric-scenarios.md §26 | covered |  |
| UC-16 | DESIGN §5.1, §5.12; ADR-0096; declaration-syntax.md §6.9, §8.1; ADR-0098; ADR-0101; ADR-0106; ADR-0124; metric-scenarios.md §30 | covered |  |
| UC-17 | DESIGN §8, §5.12; ADR-0087, ADR-0096; storage-schema.md §9; ADR-0100; ADR-0101; ADR-0105, ADR-0106; ADR-0133, ADR-0134; metric-scenarios.md §32 | covered |  |
| UC-18 | DESIGN §5.13, §11; ADR-0096; declaration-syntax.md §6.9, §6.10; ADR-0098; declaration-syntax.md §6.11; ADR-0101; ADR-0106; unit-journey.md §2; metric-scenarios.md §31 | covered |  |
| UC-19 | DESIGN §5.11, §5.4, §5.5, §13; ADR-0096; adversarial-harness.md §2; ADR-0097, ADR-0099, ADR-0100; declaration-syntax.md §6.8; ADR-0105; ADR-0114; metric-scenarios.md §35 | covered |  |
| UC-20 | DESIGN §2, §5.8, §5.13, §6, §7, §10; ADR-0012, ADR-0022, ADR-0084, ADR-0104, ADR-0105; library-api.md §6; declaration-syntax.md §8.3; flow-review.md §2, §8; metric-scenarios.md §37 | covered | |
| UC-21 | DESIGN §5.4, §6, §7; ADR-0019, ADR-0038, ADR-0108; declaration-syntax.md §5.2; renderers.md §2; library-api.md §6; unit-journey.md §2; first-consumer-walkthrough.md §3.3; metric-scenarios.md §38 | covered | |
| UC-22 | DESIGN §2, §5.8, §6, §13; ADR-0110, ADR-0114; declaration-syntax.md §3.1, §6.10; library-api.md §3; storage-schema.md §6; ADR-0125; metric-scenarios.md §41 | covered | |
| UC-23 | DESIGN §5.5, §6, §13; ADR-0111; declaration-syntax.md §5.1, §6.11; library-api.md §4; storage-schema.md §6; renderers.md §2; adversarial-harness.md §1; metric-scenarios.md §42 | covered | |
| UC-24 | DESIGN §5.12, §10; ADR-0114; declaration-syntax.md §6.9, §6.11; library-api.md §6; renderers.md §2; metric-scenarios.md §43 | covered | |
