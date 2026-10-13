# ADR-0193: The guides, written for people and agents, and checked against the engine

- **Status:** **Decided** 2026-10-13 on the author's request, "start preparing user-facing documentation, make it both human and agent friendly, keep it technically accurate and complete but concise and easy to read and follow", each decision the recommendation stated here; built 2026-10-13
- **Date:** 2026-10-13
- **Refines:** ADR-0180 (what a release promises, which the guides restate)
- **Relates to:** `guides/README.md`; `scripts/check-guides.py`; `llms.txt`

## Context

The user-facing guides were `integrate.md`, `wire.md`, `write-a-flow.md` and the flow format card. They held the facts an application and an operator need, but in the design record's voice: long sentences, a decision's or a defect's number on most clauses, and the operator's guidance in bullets of several hundred words (`integrate.md` §9). Six production reviews had added to them, each fact correct and each harder to find. Nothing held them to the engine: a code the service answers could be missing from every page, and an example's answer could drift from what the service sends. Writing the new pages found one such drift: `wire.md` said an order key is an indexed attribute, as ADR-0127 decided, and the service orders by any attribute holding one value (`core/crates/objectflow-core/src/reads.rs`, the `order` keys; recorded in `TODO.md`).

## Decision

1. **One set of guides, in `docs/guides/`,** each page with one job: an index (`README.md`), a quickstart, concepts, writing a flow, building an application, agents, operating a deployment, the API reference, errors and refusals, and the flow format card. The pages that existed keep their paths, so the links into them from the README, `SECURITY.md`, the core's README and the checker's messages hold; `integrate.md` becomes building an application, its agents' and operators' parts moving to `agents.md` and `operate.md`.
2. **Written for a reader acting on them:** plain words and short sentences, in the second person; each page opens with what it covers and for whom; every enumerated code in a table, exactly as the service sends it, with what to do about it; a measured number with the build, the machine and the date it was measured on; no decision's or defect's number in the running text, the reasons staying in `docs/adr/` and the design documents, which remain normative and which the guides link to.
3. **Written for an agent too:** `llms.txt` at the repository's root indexes the guides with a line each, as the llms.txt convention has an agent read first; `errors.md` gives the whole decision a client makes, status, `kind`, then `remedy`; every YAML block of a tutorial names its file in its first line.
4. **Checked against the engine,** by `scripts/check-guides.py`, which `scripts/check-all` runs with every gate, and so CI: the tutorials' commands (`quickstart.md`, `agents.md`, `write-a-flow.md`) run against a fresh sandbox of the service, each answer compared with the page; every verdict kind, remedy, input reason, generated clause, read refusal and fault code, read from the source rather than listed in the check, appears in `errors.md`, every operation and key of `wire-v1.json` in `wire.md`, and every flag and metric in `operate.md`; and both indexes link every page.
5. **Where a guide and the code differ, the guide says what the service does** and the difference is recorded as a finding, to be decided as any is; a guide never states a design the service does not yet run.

## Alternatives rejected

- **Keeping the guides and adding a quickstart.** The facts were there; finding and following them was the trouble, and a quickstart in front of the same pages leaves it.
- **A separate manual beside the guides** (`docs/manual/`). Two sets of pages saying the same things drift apart, and the links into the guides would need choosing between them.
- **A documentation site generator.** A dependency and a build for what Markdown on GitHub already renders; `llms.txt` and the index give an agent and a person a way in without one.
- **Listing the codes in the check by hand.** The list would drift as the pages do; the check reads the codes from the core's and the service's source, and names a word it meets that is not a code (`NOT_CODES`, `NOT_FAULTS`) so a new one fails rather than passes unseen.
- **Generating the reference pages from the source.** Generated prose reads as the code does; the check gives completeness, and people write the words.

## Consequences

- The rewrite was audited term by term: each backticked term of the old `integrate.md` and `wire.md` is in the new pages or was a fragment of a sentence rewritten, and the seven facts the audit found missing were restored.
- The checker's `veto` notice and the service's restore message cite the guides by page and heading (`write-a-flow.md, Patterns`; `operate.md, Backups and restore`), not by section numbers the pages no longer have; the core's planted cases were exported again to match.
- Each later change to what the service answers updates the guides in the same commit; the check fails until it does where it can see the change.
- The guides describe the service at 7d050a6's behaviour; the seventh production review's findings (CUTSTALL, PUBLISH-SLOW, a stalled write's wait) change what `operate.md` says, each in its own fix.
