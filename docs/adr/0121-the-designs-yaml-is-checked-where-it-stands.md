# ADR-0121: The design's YAML is checked where it stands

- **Status:** Decided at the author's direction, 2026-09-26 ("rewrite the design's examples in yaml"); the author's own acceptance pending.
- **Date:** 2026-09-26
- **Refines:** ADR-0116 (the format replaces the text language; its decision 5 rewrites the design's examples in YAML)
- **Relates to:** ADR-0109 (the publish report of creations that skip), `flow-format.md` §4.1 and §7, `scripts/check-flow-docs.py`

## Context

ADR-0116 decided that the design's examples are rewritten in YAML, `unit-journey.md` first, and that `declaration-syntax.md` and the decision records keep the text notation. The text examples are checked where they stand: `scripts/check-corpus.py` runs `scripts/check-syntax-doc.py` over every design document with a `text` block, which resolves a module's `use` lines against the document that declares that module and compares a quoted creation report with the one the declarations produce. Nothing did the same for YAML. `scripts/check-flow-format-doc.py` checked the specification's one complete example, and its ten excerpts were checked by nothing: seven of them had drifted from the examples they were taken from, each by a shortened description or a condensed mapping.

Three facts shape how the documents' YAML is checked:

1. **A module is explained in parts.** `unit-journey.md` writes one module in six blocks, with the reasoning for each type between them, and a YAML module is one mapping.
2. **A document shows parts of a module.** The specification shows a metric, an attribute or a transition taken from an example, without the keys around it.
3. **A module may import from one written in text.** `unit-journey.md` imports `User`, `Customer`, `RetirementReason` and `OverrideReason` from the reference module `inventory` of `declaration-syntax.md`, which keeps the text notation. The format's example `inventory.yaml` is a different module of the same name.

A probe found a fourth: `scripts/check-flows.py` did not resolve imports at all. A module importing `peple: [Usr]`, neither a module nor a type anywhere, passed all four steps.

## Decision

1. **A module may be written across blocks.** A `yaml` block beginning `module:` begins a module, and a block whose first line is `# <module>, continued` continues the module of that name begun earlier in the same document. The blocks are checked as one module, and a finding is reported at the document's line. A document that begins a module twice writes two versions of it, as `returns-module.md` writes two publishes, and the later is checked against the earlier, as the format's examples are against the version before (`flow-format.md` §4.16); an import resolves to the later.
2. **Every other `yaml` block is an excerpt, and is part of a checked module.** Each key it shows is in a module of the documents or an example, at one place, with the same value. The keys beside it may be left out, and nothing it shows may differ.
3. **A document's imports resolve among the design documents' modules**, never the format's examples, which are a corpus of their own. A module written in YAML is checked before the one that imports it. A module still written in the text notation must declare each name imported from it, and step 4 of the checker, which converts to the text notation, resolves the rest through the text checker's own import resolution. A module declared by more than one document names its home, as the text checker's `MODULE_HOMES` does.
4. **`scripts/check-flows.py` resolves an import whenever the module is among its files**: each name must be one that module declares (step 3, `names`). Where the module is not among them, step 3 reports an `imports` notice, since steps 2 and 3 cannot check what is imported from it.
5. **A quoted creation report is the one the YAML modules produce**, computed by the text checker over their conversion, as ADR-0109's report is for text.
6. **A fragment whose module was never written gets a module sized to it.** The CRM, orders, tickets and approvals case studies each showed one declaration of a module they never wrote out, and `publish-and-import.md` showed migration lines of none. Each now carries, in an appendix, a module that holds what its fragment needs and no more, and the fragment is an excerpt of it (the author's direction, 2026-09-26: "go with your recommendations").
7. **What keeps the text notation**: `declaration-syntax.md` and the decision records, as ADR-0116 decided, and `first-consumer-walkthrough.md`, which the unit's journey superseded on 2026-09-24 and which stays the record it is.
8. **A state diagram of a module is drawn from it, never by hand.** `scripts/flow-diagram.py` draws a type's lifecycle as a Mermaid state diagram in UML's notation, and a `mermaid` block whose first line is `%% <module>.<Type>` must be exactly what it draws, so a diagram cannot drift from the module it shows (added 2026-09-27, for the catalogue of Jira's flows).

`scripts/check-flow-docs.py` implements the first three and the fifth, proves each, and the check of a later version, with a planted mistake, and runs from `scripts/check-corpus.py` beside the text checker.

## Alternatives rejected

- **One block per module.** It keeps the checker simple but moves every type's reasoning away from its declaration, which is how `unit-journey.md` and `flow-review.md` are read.
- **Every block after a module continues it.** No marking is needed, but an excerpt placed after a module would silently become part of it, and a reader could not tell a continuation from an excerpt.
- **A marking the reader does not see**, such as an HTML comment before the block. It is as checkable, but the reader is the one who needs to know that the block continues a module.
- **Excerpts matched as text**, verbatim or with a mark for what is left out. Leaving out the keys around a part is what an excerpt is for, and a text match turns every such omission into a mark to maintain. Comparing the YAML's structure checks what the excerpt says, and nothing of its layout.
- **Excerpts left unchecked, as illustrations.** Seven of the specification's ten had drifted from the examples they were taken from, which is the defect a check exists to stop.
- **The reference module's imported declarations written in YAML beside it.** It would make every import resolve in YAML, but `declaration-syntax.md` keeps the text notation (ADR-0116), and a second copy of its declarations would drift from the first.
- **Resolving the documents' imports among the examples too.** The examples' `inventory` is not the reference module the documents import, and one namespace would resolve `inventory_journey`'s import to the wrong module.
- **A case study's fragment kept in text, as a record of its time.** Each case study says its declarations are current, and the prose around them records the history; a current declaration in a superseded notation is neither.
- **A case study's fragment replaced by an excerpt of the example built for its domain.** `customers.yaml`, `issues.yaml` and `approvals.yaml` hold other transitions than the fragments show, and a case study's point is its own declaration.
- **The migration lines of `publish-and-import.md` kept in the model's form.** An author writes the `migration` section, and the model's lines name no type, a defect of its grammar `TODO.md` records.
- **Diagrams drawn by hand beside the YAML.** A hand-drawn diagram is the excerpt problem again: it says what the module does in a second form nothing holds to the first.
- **Refusing an import from a module not among the files.** A single module is a legitimate thing to check, and the publish resolves its imports against the store; a notice says what was not checked without refusing it.

## Consequences

- `flow-format.md` §4.1 states that an import resolves when its module is among the files checked, and §7 adds the `names` finding and the `imports` notice. The specification's seven drifted excerpts now show their examples' text.
- `scripts/check-flows.py` resolves imports and plants an undeclared name and an absent module. `scripts/check-syntax-doc.py` runs its checks only when run, so the doc checker can use its parser and its creation report.
- `scripts/check-corpus.py` runs `scripts/check-flow-docs.py`.
- `unit-journey.md` and `returns-module.md`, which imports it, are the first documents rewritten, in one change, since the text checker would otherwise resolve the returns module's import against `flow-review.md`'s variant of the journey. Their blocks are regrouped into the format's order: the machine before the types, and each metric with its type (ADR-0118). Rewriting them found D384 to D387, each resolved in place: the format refused the members every object has, refused `default: false`, and overstated check 64, and two sentences of the journey had survived ADR-0114.
- The flow review's appendix, the payments ledger, the four case studies' fragments and `publish-and-import.md`'s mappings followed, and found D389 to D393: a shared machine reading an observation kind, a money literal read as a state, the literal rule covering too few expressions, a validator reading its own transition's write, and an edit claiming to skip required writes.
- A qualified name whose type nothing declares or imports, such as `Usr.ACTIVE`, still passes all four steps. The probe that found import resolution missing found this too, and `TODO.md` records it.
