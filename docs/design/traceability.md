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
  - their verification found D261 to D268, which ADR-0101 repaired;
  - the six implementation documents were amended;
  - declaration-syntax iteration 19 spelled the new constructs, with checks 54 to 61.

Current: 67 covered, 0 partial, 0 gaps, 1 unverifiable, 3 revision proposed.

## Requirements

| ID | Covered by | Status | What remains |
|---|---|---|---|
| F1 | DESIGN §5.9, §10; ADR-0010; renderers.md §2, §3; declaration-syntax.md §6.8, §6.9 | covered | |
| F2 | DESIGN §3, §6, §8; ADR-0040, ADR-0042, ADR-0054; library-api.md §6 | covered | |
| F3 | DESIGN §5.8, §10; ADR-0025, ADR-0037; library-api.md §6; ADR-0099 | covered |  |
| F4 | DESIGN §5.5; ADR-0090, ADR-0092; library-api.md §4 | covered | |
| F5 | DESIGN §5.9; ADR-0027; publish-and-import.md §3; ADR-0099; declaration-syntax.md §6.6; ADR-0101 | covered |  |
| F6 | DESIGN §9; ADR-0085; publish-and-import.md §1; ADR-0097 | covered |  |
| F7 | DESIGN §5.9, §9; ADR-0085, ADR-0096; publish-and-import.md §1, §2; storage-schema.md §6; ADR-0097; ADR-0101 | covered |  |
| D1 | DESIGN §7; ADR-0013, ADR-0033; storage-schema.md §2 | covered | |
| D2 | DESIGN §6, §7; ADR-0083, ADR-0088; storage-schema.md §6 | covered | |
| D3 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8; storage-schema.md §3; renderers.md §3; ADR-0099 | covered |  |
| D4 | DESIGN §5.11; ADR-0082, ADR-0095, ADR-0096; declaration-syntax.md §6.8; storage-schema.md §3; ADR-0101 | covered |  |
| D5 | DESIGN §5.4, §5.11; ADR-0083, ADR-0095, ADR-0096; declaration-syntax.md §4.2, §6.8; storage-schema.md §2, §6; ADR-0099 | covered |  |
| D6 | DESIGN §5.11; ADR-0082, ADR-0095; declaration-syntax.md §6.8; storage-schema.md §6 | covered | |
| D7 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8 | covered | |
| D8 | DESIGN §8, §5.11; ADR-0031, ADR-0078, ADR-0082, ADR-0087, ADR-0096; storage-schema.md §9; ADR-0100; ADR-0101 | covered |  |
| D9 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8 | covered | |
| D10 | DESIGN §5.13; ADR-0086; declaration-syntax.md §6.10; storage-schema.md §6; ADR-0099; declaration-syntax.md §5.2 | covered |  |
| D11 | DESIGN §5.2; ADR-0083, ADR-0086; storage-schema.md §6; ADR-0099, ADR-0100 | covered |  |
| D12 | DESIGN §5.11; ADR-0082, ADR-0095, ADR-0096; declaration-syntax.md §6.8; ADR-0097 | covered |  |
| C1 | DESIGN §5.2, §5.12; ADR-0084; declaration-syntax.md §6.9; ADR-0098 | covered |  |
| C2 | DESIGN §5.12; ADR-0084, ADR-0096, ADR-0098; declaration-syntax.md §6.9; renderers.md §2, §3; library-api.md §6 | revision proposed | as written, C2 conflicts with T5 for a consumer who may see only part of the data; PRD §12 proposes the wording the design meets |
| C3 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9; ADR-0098 | covered |  |
| C4 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9 | covered | |
| C5 | DESIGN §5.12; ADR-0084; data-driven-engine.md §7 | unverifiable | the PRD leaves the target to be set by measurement; data-driven-engine.md §7 records an indicative SQLite probe, and the author sets the target |
| C6 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9 | covered | |
| M1 | DESIGN §5.12; ADR-0083, ADR-0084, ADR-0096; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11; ADR-0101 | covered |  |
| M2 | DESIGN §10; ADR-0084; library-api.md §6; ADR-0098 | covered |  |
| M3 | DESIGN §5.12; ADR-0084; declaration-syntax.md §6.9; storage-schema.md §6; ADR-0098; ADR-0101 | covered |  |
| M4 | DESIGN §5.12; ADR-0084, ADR-0096; declaration-syntax.md §6.9, §8.1; ADR-0098; ADR-0101 | covered |  |
| M5 | DESIGN §10; ADR-0094; library-api.md §6 | covered | |
| M6 | DESIGN §5.13; ADR-0086, ADR-0096; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11; ADR-0101 | covered |  |
| M7 | DESIGN §5.7, §5.12; ADR-0084; declaration-syntax.md §8.3; storage-schema.md §3.4 | covered | |
| L1 | DESIGN §5.5, §5.11; ADR-0082; declaration-syntax.md §6.8 | covered | |
| L2 | DESIGN §6; ADR-0088, ADR-0095, ADR-0096; storage-schema.md §4; library-api.md §5 | covered | |
| L3 | DESIGN §6, §10; ADR-0037; library-api.md §6 | covered | |
| L4 | DESIGN §5.12, §9; ADR-0084, ADR-0085; declaration-syntax.md §6.9; publish-and-import.md §1 | covered | |
| L5 | DESIGN §5.7, §5.12; ADR-0084, ADR-0092; declaration-syntax.md §6.9; ADR-0098 | covered |  |
| V1 | DESIGN §5.9; ADR-0085; declaration-syntax.md §2 | covered | |
| V2 | DESIGN §10; ADR-0085; library-api.md §6; storage-schema.md §6 | covered | |
| V3 | DESIGN §5.5; ADR-0085; declaration-syntax.md §5.1; renderers.md §2; adversarial-harness.md §1 | covered | |
| V4 | DESIGN §9; ADR-0085; publish-and-import.md §1; ADR-0097 | covered |  |
| V5 | DESIGN §5.11; ADR-0082; declaration-syntax.md §6.8; publish-and-import.md §1 | covered | |
| T1 | DESIGN §3, §13; adversarial-harness.md §1, §2; ADR-0097, ADR-0100; ADR-0101 | covered |  |
| T2 | DESIGN §1; adversarial-harness.md | covered | |
| T3 | DESIGN §7, §8; ADR-0033, ADR-0083, ADR-0089; storage-schema.md §2; ADR-0099, ADR-0100; ADR-0101 | covered |  |
| T4 | DESIGN §8, §10; ADR-0083; storage-schema.md §3.2; publish-and-import.md §4; ADR-0100; ADR-0101 | covered |  |
| T5 | DESIGN §5.8, §5.12, §6, §10; ADR-0030, ADR-0084, ADR-0095, ADR-0096; library-api.md §6; ADR-0100; ADR-0101 | covered |  |
| N1 | DESIGN §2; ADR-0090; storage-schema.md §7 | covered | |
| N2 | DESIGN §2; ADR-0012, ADR-0091, ADR-0096; library-api.md §1; storage-schema.md §6, §9; ADR-0100 | covered |  |
| N3 | DESIGN §2; ADR-0090; storage-schema.md §7 | covered | |
| N4 | DESIGN §13; ADR-0091; adversarial-harness.md §4; ADR-0100 | covered |  |
| N5 | DESIGN §11; ADR-0075, ADR-0077, ADR-0093, ADR-0096; publish-and-import.md §4, §7; first-consumer-cutover.md §4a; ADR-0100; first-consumer-cutover.md §4a; ADR-0101 | covered |  |

## Use cases

| ID | Covered by | Status | What remains |
|---|---|---|---|
| UC-1 | DESIGN §5.12, §7, §11; declaration-syntax.md §6.9; publish-and-import.md §4; ADR-0098 | covered |  |
| UC-2 | DESIGN §6, §7, §5.12; ADR-0096; declaration-syntax.md §6.9; storage-schema.md §6; ADR-0098; declaration-syntax.md §6.11 | covered |  |
| UC-3 | DESIGN §8, §10; ADR-0096; declaration-syntax.md §6.9; publish-and-import.md §4; ADR-0100 | covered |  |
| UC-4 | DESIGN §5.11, §5.5, §6; declaration-syntax.md §6.8; ADR-0099 | covered |  |
| UC-5 | DESIGN §5.11, §5.12; declaration-syntax.md §6.8, §6.9 | covered | |
| UC-6 | DESIGN §5.4; declaration-syntax.md §4.2 | covered | |
| UC-7 | DESIGN §12; declaration-syntax.md §6.8 | covered | |
| UC-8 | DESIGN §5.12; ADR-0098; declaration-syntax.md §6.9 | revision proposed | as C2 |
| UC-9 | DESIGN §5.12; declaration-syntax.md §6.9 | covered | |
| UC-10 | DESIGN §5.5, §5.12; ADR-0096, ADR-0098; declaration-syntax.md §6.9; library-api.md §4 | revision proposed | naming the value to a requester who may not see what it aggregates would breach T5; PRD §12 proposes the wording the design meets |
| UC-11 | DESIGN §10, §5.12; library-api.md §6; renderers.md §3 | covered | |
| UC-12 | DESIGN §5.12; declaration-syntax.md §8.3 | covered | |
| UC-13 | DESIGN §5.11, §5.12, §10; declaration-syntax.md §6.8, §6.9; library-api.md §6; ADR-0098 | covered |  |
| UC-14 | DESIGN §5.5; declaration-syntax.md §5.1; ADR-0097 | covered |  |
| UC-15 | DESIGN §9; publish-and-import.md §1; ADR-0097 | covered |  |
| UC-16 | DESIGN §5.1, §5.12; ADR-0096; declaration-syntax.md §6.9, §8.1; ADR-0098; ADR-0101 | covered |  |
| UC-17 | DESIGN §8; ADR-0087, ADR-0096; storage-schema.md §9; ADR-0100; ADR-0101 | covered |  |
| UC-18 | DESIGN §5.13, §11; ADR-0096; declaration-syntax.md §6.9, §6.10; ADR-0098; declaration-syntax.md §6.11; ADR-0101 | covered |  |
| UC-19 | DESIGN §5.11, §5.4, §5.5, §13; ADR-0096; adversarial-harness.md §2; ADR-0097, ADR-0099, ADR-0100 | covered |  |
