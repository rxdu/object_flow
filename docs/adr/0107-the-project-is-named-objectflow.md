# ADR-0107: The project is named ObjectFlow

- **Status:** Accepted by the author, 2026-09-24: "go with ObjectFlow, rename everything"
- **Date:** 2026-09-24
- **Supersedes:** ADR-0011

## Context

ADR-0011 named the project ObjectKeeper when it was a governed object store, for a name that would "indicate data storage and governed access, be obvious rather than metaphorical". The product has moved since:
- ADR-0081 widened the objective to defining flows and collecting the data they produce;
- ADR-0104 made the engine the governed core that upper-layer applications are built on.

The author judged that the name describes the function but does not appeal, and set the criterion: potential users should easily get what the engine is about from the name. A test fits that criterion: say "X is a…" to a developer who has never seen the project, and see what they supply. "ObjectKeeper is a…" draws *object storage*, which names the least distinctive part of the design, and the category DESIGN.md distinguishes itself from. The author also asked for a word that implies the flow running smoothly.

## Decision

**ObjectFlow**, written as one word, with the tagline:

> *Governed flows for business objects. People and agents move objects only along declared transitions; every change is checked, recorded and measured.*

**Why the name works.**
- "ObjectFlow is a…" draws *business objects moving through flows*, which is what a deployment declares: object types, their lifecycles and named transitions.
- "Object" keeps the model's own primitive, as ADR-0011 wanted.
- "Flow" is the PRD's user-facing concept ("define flows and collect data"), and carries the smoothness the author asked for.

What the name does not say — that every change is guarded, recorded and measured — the tagline says.

**What "flow" does not mean.** It names what a deployment declares, not what the engine does. ADR-0011 rejected agent nouns such as Conductor, Shepherd and Pilot, because they advertise driving objects forward, which ADR-0012 designed out. "ObjectFlow" is not an agent noun. The remaining risk, a reader taking it for an orchestrator, is what the tagline and ADR-0104 answer.

**Everything is renamed** (the author):
- the name in every document, decision record and script;
- the internal prefixes that carried the old initials:
  - tables `ok_` become `of_`;
  - the built-in capabilities `OK_…` become `OF_…`;
  - declaration files `.ok` become `.of`;
- ADR-0012's file, whose name held the old name.

**What is not renamed.** ADR-0011 keeps its own words and file, as the record of the first name and why it was chosen. The old name, and the `ok_` and `OK_` prefixes, are retired notation, and `scripts/check-corpus.py` fails on them anywhere else.

## Alternatives rejected

- **Keep ObjectKeeper.** It reads as object storage and leaves out flows, governance and the record, which are what make the project worth adopting.
- **FlowEngine.** It reads as an engine that runs flows, an orchestrator, which ADR-0104 excludes by name. `flowengine` on PyPI is a YAML-driven workflow engine, so the confusion would be immediate.
- **Flowguard.** It passes the test, but `flowguard` on PyPI is a rate-limiting library. To many developers, "flow" plus "guard" means throttling traffic, so the name could be read as the wrong category.
- **Flowrail.** It is free on PyPI and npm, and carries the guarantee: flows that run smoothly because they stay on their rails. It says less about what a user builds than ObjectFlow does, and the author chose clarity about the model.
- **Laminar, Fairway, Glidepath.** These are the best metaphors for a smooth flow, and laminar flow is smooth *because* it is orderly. `laminar` is a workflow framework on PyPI and npm, and `fairway` a "path for AI agents", both in this project's own category; `glidepath` is taken on both registries.
- **Charter, Steward.** Appealing, but nobody would guess what either does, which fails the author's criterion.
- **Smoothflow.** Free, but it reads as a marketing adjective and would be lost in a search.
- **Keep `ok_`, `OK_` and `.ok` as a stable internal prefix under the new name.** This is common practice, and cheaper. The author asked for everything renamed, and a prefix that no longer matches the name is a small confusion every reader of the schema would meet.

## Evidence

Registry state, checked 2026-09-24, with the same caveat ADR-0011 gave: availability is perishable and should be re-checked before anything is published.
- **`objectflow`:**
  - unclaimed on PyPI, npm and crates.io;
  - the GitHub organisation `objectflow` exists (created 2021-08-09, no public repositories), so the repository would be a personal one, such as `rxdu/objectflow`;
  - GitHub also holds `tkellogg/objectflow`, "Lightweight workflows in .NET for busy developers" (64 stars), and a computer-vision paper implementation named ObjectFlow;
  - "object flow" is also a term in UML activity diagrams.
- **The alternatives:**
  - taken: `flowguard`, `flowengine`, `laminar`, `fairway` and `glidepath` on PyPI; `laminar`, `fairway`, `glidepath`, `evenflow` and `steadyflow` on npm;
  - free on both registries: `flowrail`, `flowkeeper` and `smoothflow`.
- **Not checked:** domains and trademarks, which need a registrar and a professional search.

## Consequences

- Every document, decision record and script now says ObjectFlow, with the new prefixes. `scripts/check-corpus.py` retires the old name and prefixes, exempting only ADR-0011 and this record.
- ADR-0011 is superseded, and says so in its header.
- The worked example's page is renamed with the project.
- **Left to the author:**
  - renaming the GitHub repository (`git@github.com:rxdu/object_keeper.git`) and the local working directory, since both are outward-facing or disruptive to work in progress;
  - the first consumer's repository and any page outside this repository that uses the old name, which this change does not reach.
