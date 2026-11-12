# Working in this repository

The rules a session working on ObjectFlow follows, whoever or whatever runs it. Each rule points to where it is decided; a rule recorded nowhere else has this file as its record.

## Where things are

- `TODO.md` holds the work: its first table's *Open questions* row lists what waits on the author, and *F. The release gate* each release's gate and its results.
- `docs/LESSONS.md` holds what went wrong and what to do instead. Read it before starting, and again after an error.
- Decisions are ADRs in `docs/adr/`, each with the alternatives it rejected and why. A newer ADR supersedes an accepted one; an accepted one is not edited.
- A finding or follow-up goes in `TODO.md`, with its site (`file:line`) and how it was verified.
- Research on how established systems solve a problem goes in `docs/research/`, as its `README.md` says. A research note recommends; an ADR decides.

## Design

- `docs/PRD.md` is the baseline. A design choice serves a requirement or use case it names (F, D, C, M, L, V, T, N; UC-n). Check the design against the PRD, never the PRD against the design. Where the PRD is silent or wrong, propose a revision, marked Said, Carried or Inferred.
- No write is ungated, under any condition: PRD F2, `DESIGN.md` §5.2. Cite F2, and do not ask whether to add it. Each write path names what makes it all or nothing, and is tested at every boundary on both backends.
- The core's soundness weighs above the speed of building it, since every application on the engine inherits its faults (ADR-0132).
- Before proposing a mechanism, find how proven platforms do it, and say which pattern the proposal follows or why it departs.
- An issue or a request names a need and often a mechanism. Recommend the design that serves ObjectFlow's long-term goals and keeps its maintenance low, which may not be the mechanism asked for: first check what the engine already does for the need, and prefer reusing a construct to adding a parallel one that declares the same data twice. The author directed this on 2026-11-12.
- A worked example or case study tests the design. Judge a case by the mechanism or requirement it stresses, not by its fidelity to a production rule.

## The first consumer

The first consumer is a case study (ADR-0149), and its details stay out of this public repository.

- Restate a failure mode or scenario it surfaces on the repository's own samples: the example modules, a test's flow, a generic case.
- Describe its report generically: "a consumer reported", with the issue's number. Never name its files, scripts, forms, data, people or measurements. `scripts/check-corpus.py` refuses part of this.
- Never specialise the engine for it. Meet a need it shows with a mechanism any deployment could use, and leave its policies to its own declarations.

## The author's acts

- Pushing, tagging and releasing are the author's acts (ADR-0214 decision 5). So is a post on GitHub: an issue reply goes up only as the author approved its final text.
- "Go with your recommendations" decides the questions just put to the author that carried a recommendation, and no others (ADR-0210). It decides a push, a cut or a tag only where the question named that step. Say in the reply which questions the words were taken to decide.
- The author directed on 2026-09-28 that a confident recommendation be accepted (ADR-0127's status). Record such a design decision as accepted in advance under that direction, not as the author's own reading. Bring back a business or scope choice, a measurement, and an uncertain recommendation.
- Confirm before any history rewrite, an amend of an unpushed commit included.

## Commits

- Author and committer are `Ruixiang Du <ruixiang.du@gmail.com>`, and no other identity. Set no repository-local `user.email`.
- `type(scope): subject`, with a body saying why; one logical change a commit.
- Stage files by name, and commit nothing machine-local.
- Markdown prose is one line a paragraph.

## Gates

- Rust: `cargo fmt`, and clippy with every warning an error (`core/scripts/check`), before a commit.
- A commit touching the design record, an ADR, `flow-format.md` or a guide runs every design check first: `scripts/check-all --lane checks` (`docs/LESSONS.md`).
- Every push runs the whole gate on both backends first, as `core/README.md`'s *Checks* says.
- Read a gate's exit status, not only its last lines. Edit nothing in the tree while a gate runs on it.
- A release follows `core/README.md`'s *Releases*: `scripts/release/gate` on the release commit, and `scripts/release/verify` once it is published.

## The machine

- Keep disk use bounded. Delete a scratch build, a profile store, a measurement's database or a downloaded release once its result is recorded. At the end of a release, check `du -sh core/target`, and run `cargo clean --profile dev` past tens of gigabytes.
- Never delete another project's files, Docker images or containers.
- Gates P and I measure on a quiet machine: a run that other work shared is not decided (ADR-0235, ADR-0236). They run on the machine ADR-0258 keeps for them, with no other work beside a gate.
