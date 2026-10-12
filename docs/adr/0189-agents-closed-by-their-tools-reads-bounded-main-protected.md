# ADR-0189: An agent's tools opt-in, heavy reads bounded, `main` protected

- **Status:** **Decided by the author**, 2026-10-12 ("go with your recommendations on questions 13, 14 and 15"), on the recommendations `TODO.md` states with each, after the fourth production review (`design/production-review.md` §8).
- **Date:** 2026-10-12
- **Refines:** ADR-0173 decision 10 (an agent's tools), ADR-0182 (`taken_by`), ADR-0187 decision 1 (`publishing`), ADR-0184 decision 3 (a read's deadline)
- **Relates to:** `PRD.md` T6, N7; `guides/integrate.md` §6, §9; `design/renderers.md` §3; `flow-format.md` §7

## Context

The fourth production review left two majors that needed the author, and one setting only the repository's owner could change.

- **AGENTS-OPEN.** A transition with no `taken_by` is open to every kind of actor. That follows the design (PRD T6, N7: the engine reads a kind only where the flow's author declares one). The consequence is that a flow declaring none hands an agent every transition, through its generated tools and through the wire. The review's agent recorded a bill, raised a policy limit, onboarded people and agents, erased a name and ended an employee. The checker said nothing.
- **READS.** Nothing bounded how many heavy reads ran at once, so nothing bounded their memory. 32 per-person metrics on six years of claims took 3.3 GB, and every one timed out.
- **`main`.** The branch accepted force-pushes and deletion.

## Decision

1. **An agent's tools are opt-in, and the checker says where a flow is open.**
   - **Its tools.** An agent's generated tools offer a transition only where its `taken_by`, or its type's where it declares none, names `agent`. Any other transition is offered only for proposing, where it is proposable, and is otherwise left out. A proposal is executed by the person who approves it. This applies alike in both backends.
   - **Observations.** Recording one stays among an agent's tools, since an observation kind declares no `taken_by`, and a recording adds a fact about its subject and moves nothing.
   - **The wire.** It stays as the flow says: an agent that reaches the wire may take what the flow leaves open, which the gateway withholds.
   - **The checker's notices.** Where a module of the version declares an agent kind of actor, it notices, once for each module, the transitions a request may name that name no `taken_by`, nor their type, counted by type (`agent_open`): a notice for each type, as the question proposed, would have given every module of the samples several, burying the notices that matter. Against the previous version, it notices each `taken_by` or `publishing` rule a version drops or widens (`taken_by_dropped`). Both are notices, not refusals. The samples mark the transitions their agents take.
2. **Heavy reads are bounded, by `--max-reads <n>`, 4 unless given.**
   - **What counts.** The bound covers the reads whose cost grows with the store: `query`, `available`, `metric`, `exceptions`, `export`, `pull`, `diagnostics`, and a change's dry run, a `submit`'s or a `publish` preview's. At most n of them run at once.
   - **Waiting.** A read waits for a slot within its own deadline and is answered `503 read_timeout` past it.
   - **What never waits.** A read whose cost is one object, events by position or the declaration never waits: `get`, `lookup`, `history`, `events`, `availability`, `declaration`, `tools` and `check`. Neither do `/health` and `/metrics`.
   - **What it measures.** `/metrics` gives the reads running and waiting, and the bound. On the six-year store, sixteen per-person metrics sent at once peaked at 1.15 GB under the bound of 4, eight answered and eight refused after their 30 s, where with the bound lifted to 64 they peaked at 1.9 GB and every one was refused, measured 2026-10-12 on a loaded machine; the bound lowers the peak and, since four reads do not starve each other, answers more.
3. **`main` is protected against force-pushes and deletion, admins included.** No review or check is required, so the author's direct pushes continue. This was done through `gh api` on the author's word, 2026-10-12.

## Alternatives rejected

- **(13a) The notices alone.** Rejected: the documented defence, an agent held to its tools, would stay open by default.
- **(13c) A transition with no `taken_by` refused to an agent wherever a flow declares an agent kind.** Rejected: the engine would read a kind the author did not name, which PRD T6 and N7 forbid without a revision.
- **(14a) Admission left to the gateway, documented.** Rejected: the engine is the one place that sees every read, and its bound is the one a supervisor's `MemoryMax` can be sized against.
- **(14) Every read bounded alike.** Rejected: a `get` of about a millisecond would queue behind four metrics of ten seconds each, stalling every worklist for no memory saved.
- **(15) Pull requests required on `main`.** Rejected: it would change how the author works, and the risk the review named was force-pushes and deletion.

## Consequences

- `integrate.md` §6 says an agent's tools are opt-in, and §9 says `--max-reads` and how to size a unit by it. `renderers.md` §3 and `wire.md` say the tools' rule and the bound's `503`. `flow-format.md` §4.8 and §7 list the two notices. `reimbursement.yaml`'s `suggest_nature` names `agent`; its notice shows its agent may still take nine of a claim's transitions on the wire, which the sample leaves to a later change (`TODO.md`).
- Held by these tests, each mutation-checked:
  - `test_an_agent_is_handed_only_what_its_flow_names_it_for`, through both backends, and `test_an_agent_may_only_propose_what_a_person_takes`;
  - the reference checker's plants for both notices, which the core's oracle compares word for word;
  - `a_heavy_read_waits_its_turn_within_its_deadline`, `only_the_reads_that_grow_with_the_store_wait_their_turn`, which holds each of the nine routes to its slot and five light ones to none, and `a_deployments_edges_are_said` for the flag.
