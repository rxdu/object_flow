# ADR-0097: A flow change has a declared lifecycle and declared authority, and a proposal re-checks every guard

- **Status:** Accepted — decided 2026-09-23 at the author's direction ("iterate until the specifications and design can fully satisfy the needs described in the PRD"), against `docs/PRD.md` F6, F7, V4, T1 and UC-14, UC-15; repairs D236 to D239
- **Date:** 2026-09-23
- **Refined by:** ADR-0101 — `publish` names the version of the change the approver read; a change is visible to drafters and approvers, and its report is filtered by visibility; `draft` takes source and evidence. ADR-0105 — a store begins at declaration version 0, and each version pins the built-in module.
- **Refines:** ADR-0036, ADR-0044, ADR-0085, ADR-0096

## Context

PRD F7: "A change to a flow is governed: who may draft and who may approve are declared, the approver is not the drafter, a change an agent drafted is approved by a person, and an impact report — what the change affects among live objects and pending work — exists before it applies."

A review of the whole record against the PRD found the `DeclarationChange` of ADR-0085 to be a set of states in a `CHECK` constraint, with no transitions, guards or capabilities (`design/defects.md` D236).
- "The drafting capability" and "the publish capability" were never named.
- Nothing enforced "a person, when an agent drafted it".
- `DESIGN.md` said approval publishes, while `library-api.md` had `publish()` install an already-approved change. The schema kept `approved` and `published` as separate states, beside a `submitted` state no document mentioned.
- Nothing wrote `superseded`. A change drafted against version 7 and approved after version 8 would republish version 7's text and drop version 8's guards unseen (D237).
- The impact report named invariant violations, removed states and invalidated proposals, but not the objects a changed guard would let move or stop (D238).
- The evidence was links to metrics, which store nothing, so what the approver saw was not in the record.
- The separation of drafter and approver compared actor ids only, so an agent drafting on Alice's behalf could be approved by Alice.

The same review found that `DESIGN.md` re-evaluates every guard when a proposal executes, actor guards included, while ADR-0036 says "every non-actor guard". No decision recorded the change (D239). The rule in `DESIGN.md` is the one T1 needs: skipping actor guards would let an approver execute a transition they could not request themselves.

## Decision

### 1. `DeclarationChange` is a built-in type with a declared lifecycle

Its declaration is part of every store, printed in the rule set like any type:

| Transition | From → to | Guards |
|---|---|---|
| `draft` (creation) | → `DRAFTED` | the actor holds `OF_DRAFT_CHANGE` |
| `revise` (act) | at `DRAFTED` | the actor is the drafter |
| `submit` | `DRAFTED` → `SUBMITTED` | the actor is the drafter. The outcome runs the dry run and attaches its report and an evidence snapshot (§4, §5) |
| `refresh` (act) | at `SUBMITTED` | the drafter or a holder of `OF_APPROVE_CHANGE`. It reruns the dry run and replaces the report |
| `publish` | `SUBMITTED` → `PUBLISHED` | see §2. This transition is the approval, and its outcome installs the version |
| `reject` | `SUBMITTED` → `REJECTED` | the actor holds `OF_APPROVE_CHANGE`; a `reason` |
| `withdraw` | `DRAFTED`, `SUBMITTED` → `WITHDRAWN` | the actor is the drafter |
| `supersede` | `DRAFTED`, `SUBMITTED` → `SUPERSEDED` | `only via DeclarationChange.publish` |

`PUBLISHED`, `REJECTED`, `WITHDRAWN` and `SUPERSEDED` are `closed` and terminal. The operation `publish(actor, change)` requests the `publish` transition. `publish(actor, change, dry_run=True)` is the dry run `submit` and `refresh` run, and changes nothing.

### 2. Approving is publishing, and its guards say who may

`publish` requires, each as a named clause:
- `may`: the actor holds `OF_APPROVE_CHANGE`;
- `not_drafter`: the actor is neither the drafter nor the drafter's principal, and does not act with the drafter as principal;
- `person_for_agent`: where the drafter was an agent, the actor is a person;
- `current`: the change was drafted against the installed version;
- `impact_unchanged`: the recomputed report names no object or proposal that the attached report did not.

A refused publish leaves the change `SUBMITTED`, and its verdict names the clause. On `impact_unchanged`, `refresh` attaches the new report, which the next approval is then of.

### 3. A publish supersedes every change drafted against an older version

The `publish` outcome cascades `supersede` to every other `DRAFTED` or `SUBMITTED` change whose base version is older than the version it installs, in the same transaction. A superseded change is redrafted against the new version, so no approval can install text that silently drops a guard published since it was drafted.

### 4. The impact report names what a changed guard does to live objects

Beside the report's existing findings — objects violating a new invariant, objects in a removed state, invalidated proposals — it names:
- the live objects that gain or lose an available transition, found by evaluating availability under the installed and the candidate guards, with external evaluators unevaluated and listed as such;
- for each clause whose `observe` marking the change removes, the live objects it would now refuse.

It keeps the ids of every object it names (`PublishReport.affected`), which is what `impact_unchanged` compares.

### 5. The evidence an approver saw is recorded

`submit` and `refresh` store an evidence snapshot on the change: the metric pages and diagnostics the draft cites, as they read at that moment, with their `complete` flags and as-of times. Links alone would point at values computed on read, which change.

### 6. The built-in capabilities are declared

Every vocabulary has these capabilities, whether or not a module declares them, as it has the category `closed` (ADR-0096 §6):
- `OF_DRAFT_CHANGE` and `OF_APPROVE_CHANGE`;
- `OF_LABEL`, for labelling;
- `OF_ERASE`, for erasing one observation;
- `OF_IMPORT`, for the import (ADR-0100);
- `OF_MAINTAIN`, for maintenance (ADR-0100).

The rule set prints them with the built-in types that use them. The consumer's authentication emits them, as it emits every capability (ADR-0025). A type narrows labelling with `labels by`, as before.

### 7. A proposal's execution re-evaluates every guard, actor guards included

This records what `DESIGN.md` §9 has said since its rewrite of 2026-09-08, and corrects ADR-0036 decision 3: approving a proposal executes the recorded request as the approver, and every guard of the transition is evaluated. An approval therefore succeeds only if the approver holds whatever authority the transition's guards demand.

## Alternatives rejected

- **Keep approval and publish as separate steps.** It adds a state in which a change is approved and not installed. The version it was approved against can move in that gap, and nothing in F7 asks for it.
- **Name the capabilities in each deployment's configuration.** They would then be declared nowhere a reader of the rule set could see, and F7 and D12 say *declared*.
- **Rebase a stale change automatically.** Merging two declaration texts is not a decision the store can make, and a merged text would not be the one the approver read.
- **Store links to the evidence, as ADR-0085 did.** A metric is computed on read, so a link shows today's value, not the one that justified the change.
- **Re-evaluate only non-actor guards on a proposal, as ADR-0036 said.** An approver lacking the transition's authority could then execute it, which is the route around the rules T1 forbids.

## Consequences

- `DESIGN.md` §9 and §10 describe the lifecycle, the guards and the built-in capabilities, and `declaration-syntax.md` §1 lists the built-in capabilities.
- `storage-schema.md`'s `t_declaration_change` gains the states, the drafter's principal, the approver's kind and the evidence snapshot.
- `publish-and-import.md` §1 and §2 describe the lifecycle and the report.
- `library-api.md`'s `publish` requests the transition, and `renderers.md` prints the built-in type.
- ADR-0036 decision 3 and ADR-0085's links to evidence are annotated.
- D236 to D239 are resolved.
