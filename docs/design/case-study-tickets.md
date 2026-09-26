# Case study: issue tracking (Jira-shaped)

Status: design iteration 2, 2026-09-07. Companion to [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md). Decisions taken here are ADR-0026 to ADR-0029 and an amendment to ADR-0021, all pending author review.

> **Revisited 2026-09-26 against Jira's documentation and the flow format** (§6 for Jira's software and business spaces, §7 for Jira Service Management), at the author's request. §1 to §5 were written from general knowledge of Jira and against the text language; §6 and §7 check every workflow feature Atlassian documents against the format, and are the current answer.

> **Amended 2026-09-25 for ADR-0114.** ObjectFlow records who acted and never evaluates it. The declarations below lost every clause that read who is asking or declared who may do or see something — capabilities, actor guards, visibility — which the upper layer now decides; their mapping rows say so. The narrative records the model as the study found it, before ADR-0114.

*Vocabulary, noted 2026-09-24:* written before PRD revision 5, this document says "consumer" for an application built on the store, which PRD §5 now calls an upper-layer application, and sometimes for the deployment or a reader; "the first consumer" keeps its meaning (D368).

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
## 1. Why this case

Issue tracking differs from the inventory system in ways that stress different parts of the model. Nothing is physical, so no guard reads a photo or a serial. The lifecycle is **user-configured per context**: the same issue type has a different workflow in different projects. Objects are joined by **typed, directional links** that guards read. Objects change kind — a subtask becomes an issue, an issue moves to another project. Workflows are **edited while thousands of issues are live**. And every issue carries a human-readable key that is minted, not supplied.

The Jira vocabulary below is used because the author's first consumer already reflects into a Jira project (`wr:docs/jira-integration.md`); the shapes are the same in Linear, GitHub Issues, YouTrack and service desks.

## 2. Mapping

| Ticket-system concept | In this model |
|---|---|
| Issue type (Bug, Task, Story, Epic, Subtask) | object type |
| Project | an object; issues reference it; also the scope of a workflow binding |
| Workflow; workflow scheme (project × type → workflow) | a **named state-machine declaration**; a type binds exactly one; "Bug in project A" and "Bug in project B" with different workflows are two types extending one base (ADR-0026) |
| Status; status category (To Do / In Progress / Done) | state; **state category** declared on the state (ADR-0026) |
| Transition; global transition ("any → Cancelled") | transition; from-state set or any-non-terminal (ADR-0016) |
| Transition screen (fields shown on transition) | the transition's **declared** inputs; ADR-0047 withdrew the claim that the list could be derived from the guards, and publishing instead checks the two against each other |
| Condition ("only assignee may execute") | the upper layer's: it reads the assignee and sends the version it read, so its check cannot race a reassignment (ADR-0114) |
| Validator ("resolution required", "fix version required to close") | guard over inputs; requiredness attaches to the transition (ADR-0002) |
| Post-function: set resolution, clear resolution on reopen, set resolved date | outcome writes: `set resolution := inputs.resolution`, `clear resolution`, `set resolved_at := now` (ADR-0021 amendment) |
| Post-function: assign to project lead | outcome write from a related attribute: `assignee := project.lead` |
| Post-function: fire event | the event log (ADR-0013) |
| Post-function that computes or reaches outside (send mail, call a webhook) | an effect; a consumer subscribed to the event (ADR-0007) |
| Update on the same status (edit fields without transitioning) | action — a self-transition (ADR-0016) |
| Subtask; "parent cannot be Done while subtasks open" | composition; collection guard `none(t in subtasks where t.state.category != done)` |
| Epic link, parent link | reference to another issue |
| Issue link with type and direction (blocks / is blocked by, duplicates, relates to) | a **link object** with a reference to each side, since a many-to-many pair has no end to be stored on and publishing rejects it (check 41); adding or removing a link is an action; guards read it: `none(blocked_by where state.category != done)` |
| Resolve as duplicate | a terminal transition recording a `duplicate_of` reference; **not** an identity merge |
| Version (unreleased → released → archived), Component, Sprint (future → active → closed) | objects with their own small state machines; issues reference them; "cannot add to a closed sprint" is a guard on the action |
| Comment, worklog | parts — small objects with an author, a timestamp and a posted → edited → removed lifecycle |
| Attachment | `file` attribute (ADR-0017) |
| Watchers, labels | set-valued attributes edited by a declared action; recorded (ADR-0042 removed the free class) |
| History tab | the object's recorded events |
| Custom field; field configuration (required / hidden per context) | attributes on the type; per-context differences are per-type differences (ADR-0026) |
| Permission scheme; issue-level security | the upper layer's: who may request what and see what (ADR-0114) |
| Bulk transition | the `batch` operation: N independent requests, each in its own transaction, N verdicts (ADR-0037) |
| Automation rule ("when X then Y"), SLA breach | a consumer subscribed to events (ADR-0012); breach is found by querying the stored `due` against the supplied time, since a clock-dependent derived value cannot be indexed (ADR-0048) |
| Issue key `PROJ-123` | a business identifier minted from a **named sequence** scoped by project (ADR-0029); monotonic, not gapless |
| Move issue to another project; convert subtask ↔ issue | **supersession**: the old object ends in a terminal superseding state naming its successor (ADR-0028) |
| Clone | a creation transition whose inputs are read from another object |
| Editing a live workflow: removing a status that issues are in | **declaration versioning** with a mapping applied as recorded migration transitions (ADR-0027) |

## 2a. Resolution, declared

```text

do resolve IN_PROGRESS -> DONE accepts resolution, fix_version {
  require subtasks: none(t in subtasks   where t.state.category != done)    because dependent
  require blockers: none(b in blocked_by where b.state.category != done)    because dependent
  require versioned: resolution == Resolution.FIXED
                     implies inputs.fix_version is not null                 because self_serviceable
  set resolved_at := now
}

do reopen DONE -> IN_PROGRESS {
  input reason : string
  clear resolution
  clear resolved_at
}
```

Two Jira post-functions appear here as ordinary outcome writes, `resolved_at := now` and clearing the resolution on reopen. The validator that requires a fix version for a FIXED resolution is a guard over an input, which is where requiredness belongs (ADR-0002). `blocked_by` is a declared inverse, which ADR-0045 requires of anything an invariant traverses and which a guard may traverse freely.

## 3. What the model could not say, and what was decided

**One type, several workflows.** ADR-0003 gives every type exactly one state machine, and a ticket system needs Bug-in-A and Bug-in-B to differ. The two honest choices were a union machine with per-project guards, or two types. The union machine makes the printed lifecycle a diagram nobody's project uses, which defeats legibility. Two types is right, provided declarations can share everything but the machine. Hence **ADR-0026**: attributes, relationships, invariants and derived attributes compose by `extends`; state machines are named declarations a type binds whole; a query may target a family (every type extending a base) and results carry the concrete type; every state declares a **category** so family-wide guards and views need not know each machine's state names.

**Outcomes need values.** "Set resolved date" and "assign to project lead" are outcome writes whose value is not an input. The ADR-0021 language already had attribute paths, `now` and `actor`; the amendment says outcome writes take `attribute := expression` in that language. Still no arithmetic.

**Objects change kind.** Move-to-project and convert-subtask both change an object's type and therefore its machine. An object's type is fixed — one machine per type, and a history read under two machines is unreadable — so the answer is a new object plus a recorded link from the old one. **ADR-0028**: a type may declare a terminal *superseding* state whose transition names a successor; the read surface follows it on request; references may be re-pointed by declared cascade. Duplicates are not supersession: a duplicate closes with a reference and both remain.

**Workflows are edited under live objects.** This was TODO.md's schema-evolution question, unanswered. Jira's own behaviour is the design: removing a status forces a mapping for issues in it; adding anything applies forward. **ADR-0027**: declarations are versioned; every object records the version in force at its last change; publishing a version that removes a state requires a mapping applied as recorded migration transitions with provenance `migrated`; additions apply forward, new guards bite at the next transition (ADR-0002), and a new invariant is checked against live objects at publish and reported like an import. Import (ADR-0015) is the same mechanism from version zero.

**Keys are minted.** `PROJ-123` is a business identifier, and ADR-0018 correctly keeps it separate from the object id. But its consequence "no sequence primitive" left minting to the consumer, which is a second write path for that value and, for a ticket system, the one thing every user sees. **ADR-0029** withdraws that consequence: the store offers named, scoped, atomic sequences, and a business-identifier attribute may declare that it is minted from one at creation.

## 4. What held without change

*Revisited after the repair: the mechanisms below held, but four of them changed shape. Guards gained three-valued semantics and declared remedy classes (ADR-0047); an action's outcome is explicitly unrestricted (ADR-0041); only-via transitions are no longer listed in availability (ADR-0041); and the verdict set gained `over-limit` (ADR-0041).*

Requiredness on transitions (ADR-0002) is Jira's validator model exactly. Conditions are actor guards; post-functions that write are outcomes; post-functions that reach outside are effects for subscribers. Subtasks are composition; links are typed references with inverses; parent-blocked-by-children is a collection guard; typed-link blocking is a collection guard over a relationship. Comments are parts. Global transitions are from-state sets. Self-transitions are actions. The availability query answers "what can I do to this issue". The event log is the history tab. The core mechanisms of ADR-0019 to ADR-0025 all applied, and the repair later changed how three of them work without changing what this study needed from them: ADR-0038 replaced ADR-0019's guard ordering, ADR-0046 replaced its outcome description, and ADR-0039 replaced ADR-0023's locking.

## 5. Rare cases, recorded in `edge-cases.md`

- Changing an object's type in place. Not supported; supersession is the path.
- Splitting one issue into two with shared history. The objects are expressible since ADR-0046 allows part re-parenting and named creations; history is still not shared, and each successor references the source.
- Gapless key sequences. Sequences are monotonic, not gapless; a rolled-back creation leaves a gap, as it does in Jira.
- A workflow edit that changes what a *past* transition meant. History is interpreted under the version it was recorded under; the readable rule set for an old version remains printable, but no tool will re-derive "what would have been allowed then".
- Issue-level security. ~~Covered by ADR-0030: a declared visibility predicate, where failing it means *not found* rather than blocked.~~ *Corrected 2026-09-26:* ADR-0114 withdrew visibility from declarations; who may see an object is the upper layer's (PRD N7).

## 6. Against Jira's documented workflows, in the flow format (2026-09-26)

The author asked whether the current design "can cover all the workflows supported by Jira software and management flows". This section answers against what Atlassian documents rather than what this study remembered, and against the flow description format (`flow-format.md`) rather than the text language.

**Scope.** Jira Cloud's software and business spaces. Jira Work Management is no longer a separate product: "We’ve taken the best of Jira Work Management and Jira Software to make a single project management tool … we’re calling it, simply, Jira" ([announcement](https://www.atlassian.com/blog/announcements/the-next-era-of-jira)), and "Business spaces are the default on the Jira platform" ([the Jira family](https://support.atlassian.com/jira-software-cloud/docs/what-is-the-jira-family-of-products/)). Jira Service Management (SLAs, queues, the portal) and Marketplace apps are out of scope. Atlassian now says *space* for project and *work item* for issue, and so does this section.

**Method.** Two inventories were taken from Atlassian's documentation on 2026-09-26, one of the workflow engine and one of the features around it, each item quoted from the page's raw text; a sample of the quotes was checked against the pages again before use. Each item is mapped below. The claims marked *Written* are held by a module, [`flow-format/examples/issues.yaml`](flow-format/examples/issues.yaml), which passes all four steps of the checker: a shared machine for Jira's simplified workflow, standard work items with their own workflow, subtasks, typed links, sprints, versions, worklogs, and a business space's document approval.

**Verdicts.**
- **Written**: the format writes it today, and `issues.yaml` holds it, under the name the row gives.
- **Not yet**: the model has it and the format will write it with the construct named, in `TODO.md`'s order.
- **Application**: the PRD places it in an upper-layer application, which the engine serves by query and by its record: presenting work, causing external effects and initiating work (N6), or deciding who may do or see anything (N7, ADR-0114).
- **By decision**: deliberately not offered, with the decision cited.
- **Differs**: expressible, not the way Jira does it; the row says how.

### 6.1 Statuses, categories and resolution

| Jira | Verdict | In the format |
|---|---|---|
| Statuses, each in one of three categories: To do, In progress, Done ([statuses](https://support.atlassian.com/jira-cloud-administration/docs/what-is-a-workflow-status/)) | Written | states and `categories`; Jira's Done is the built-in `closed`, which the standard metrics read as finished (`declaration-syntax.md` §1) |
| A status shared by several workflows, renamed everywhere at once | Differs | a state belongs to one machine or type; a rename is a published version with a mapping (F5), migrations being not yet |
| A work item is "open or closed, based on the value of its Resolution field (not its status)" ([transitions](https://support.atlassian.com/jira-cloud-administration/docs/create-workflow-transitions/)) | Differs | the engine's open and closed follow the state's category; Jira's reading is a derived attribute, `resolved: resolution is not null` |
| Resolution set on the transition to Done, by prompt or automatically; resolved date set; cleared on reopening | Written | `done` takes `required_inputs: [resolution]` and assigns `resolved_at := now`; `to_do` and `start` clear both; `auto_resolve` assigns a fixed `Resolution.DONE` |
| Resolutions allowed only at some statuses, `jira.field.resolution.include`/`exclude` | Written | a guard over the input, `resolution_allowed`: `inputs.resolution in [Resolution.DONE, Resolution.WONT_DO]` |

### 6.2 Transitions

| Jira | Verdict | In the format |
|---|---|---|
| The initial transition, "Create" | Written | `kind: initial` |
| A transition from specific statuses, one-way | Written | `kind: external`, `from` one state or a list |
| Global: "Allow all statuses to transition to this one" | Written | `from: any`, added for this study (§6.10) |
| Looped, "work item actions", "so the work item's status stays the same" | Written | `kind: internal`, from listed states or `any` |
| Shared: "transitions for work items in multiple statuses with a single set of rules" | Written | a `from` list |
| A workflow with no terminal status, Done being reopenable and work items deleted | Differs | every lifecycle needs a final state (check 15); `issues.yaml` uses ARCHIVED, which is Jira's archiving: "it will only appear in Archived work items and can no longer be edited" ([archive](https://support.atlassian.com/jira-software-cloud/docs/archive-an-issue/)) |
| Custom events fired by a transition, and the Generic event | Written | every transition is recorded as its own event, named by the transition |

### 6.3 Conditions

Atlassian's list is the old editor's ([advanced workflows](https://support.atlassian.com/jira-cloud-administration/docs/configure-advanced-issue-workflows/)); the new editor groups the same under *Restrict transition*.

| Condition | Verdict | In the format |
|---|---|---|
| Only Assignee, Only Reporter, Permission, User Is In Group / Any Group / Space Role / Any Space Role / Custom Field / Group Custom Field; *Restrict who can move a work item* | Application | who may request a transition is the upper layer's (N7, ADR-0114); the engine records who acted |
| Separation of Duties: no user performs a transition "if the user has already performed a transition on the work item" | Application | it reads who is asking; the application compares the caller with `.actor_id` on the object's recorded transitions, which the engine keeps |
| Value Field, Compare Number Custom Field; *Restrict to when a field is a specific value* | Written | a guard: `estimated` is `story_points is not null` |
| Previous Status; *has been through a specific status*, with its options | Written | `been_in_review`: `entered_at(IN_REVIEW) is not null`; counting every visit, or ignoring loops, is a derived attribute over `this.intervals` read by a guard, `reviews` and `reviewed_twice` |
| Sub-Task Blocking; *Restrict based on the status of subtasks* | Written | `none(s in subtasks where s.state.category != closed)` |
| Block transition until approval | Written | §6.8 |
| Hide From User, "can only be triggered from a workflow function or from REST"; *Restrict from all users* | Written, and Application | `only_via` when only other transitions take it, as `Subtask.create` is; which callers may use the API is the upper layer's |
| Always False | Written | a transition nothing may take is not declared |
| Conditions grouped with All or Any, nested | Written | one guard's expression with `and`, `or` and parentheses; several guards are all required |

### 6.4 Validators

The old editor's list survives only on a 2021 page ([validators](https://confluence.atlassian.com/servicedeskcloud/available-workflow-validators-for-company-managed-projects-1097176551.html)); the new editor's *Validate a field* rule has the same kinds.

| Validator | Verdict | In the format |
|---|---|---|
| Field Required | Written | `required_inputs`, or a guard over an input; requiredness belongs to the transition (ADR-0002) |
| Field has been modified | Written | `points_changed`: `inputs.story_points != story_points` |
| Field has single value | Written | `one_component`: `count(v in inputs.components) <= 1` |
| Date Compare, Date Window ("adding a time span in days to one of them") | Written | `due_within_window`: `inputs.due_date <= start_date + 3 days`; durations are `s`, `min`, `h`, `days` and `weeks` |
| Regular Expression Check | By decision | a pattern's meaning differs between Python and each backend's SQL, so it is left to the caller's edge or an evaluator (ADR-0103, `edge-cases.md`) |
| Parent Status | Written | `Subtask.parent_in_progress`: `parent.state == WorkItem.IN_PROGRESS` |
| Previous State | Written | as the Previous Status condition |
| Permission | Application | N7 |
| Forms attached or submitted | Application | forms are an interface; what a form submits arrives as the transition's inputs |

### 6.5 Post functions and *Perform actions*

| Post function | Verdict | In the format |
|---|---|---|
| The essential ones: set the status, add the comment, update the history, re-index, fire the event | Written | what taking a transition does; the history is the event log |
| Update Work Item Field, Update Custom Field, Clear Field Value; *Update a work item's field*, including `%%CURRENT_DATETIME%%` | Written | `assign`, `clear`, `add`; the current time is `now` |
| Copy Value From Other Field, "either within the same work item or from parent to subtask" | Written | `assign: { location: x, expr: parent.x }`; `add_subtask` passes the parent's space, as a subtask inherits it |
| Assign to Lead Developer | Written | `assign_to_lead` assigns `space.lead` |
| Assign to Reporter | Written | `assign_to_reporter` assigns `reporter` |
| Assign to Current User | Differs | the engine never evaluates who is asking (ADR-0114); the application supplies its caller as the `assignee` input, as `start` takes it |
| Trigger a Webhook, Trigger agent, notifications | Application | effects outside the store, from the event log (N6) |
| Set security level | Application | who may see an object (N7) |
| Development triggers, which transition a work item on a merged pull request and ignore "any conditions, validators or permissions" ([triggers](https://support.atlassian.com/jira-cloud-administration/docs/understand-workflow-triggers/)) | Differs | the reaction is the application's (N6), and its request passes the guards like any other; a change that must override them is an assertion (§4.13) |

### 6.6 Properties, screens and fields

| Jira | Verdict | In the format |
|---|---|---|
| `jira.issue.editable` false at a status | Written | no internal transition from that state: `edit` runs from To Do, In Progress and In Review only |
| `jira.permission.*` | Application | N7 |
| `opsbar-sequence`, the `jira.i18n` names | Application | presentation (N6); a transition's `description` is the format's |
| Transition screens; *Remind people to update fields* | Written, and Application | the fields a transition takes are its inputs; how they are shown is the interface's |
| A field required per work type (field configuration) | Written | an attribute's requiredness on its type |

### 6.7 Schemes and editing live workflows

| Jira | Verdict | In the format |
|---|---|---|
| A workflow scheme mapping each work type to one workflow | Written | a type binds a shared machine with `state_machine` (§4.10), or declares its own |
| The same work type with a different workflow in each space | Written | two types extending one abstract base, each with its own workflow (ADR-0026, `flow-format.md` §4.14); `servicedesk.yaml`'s asset types are such a family |
| A condition in a workflow shared by work types without subtasks, where Sub-Task Blocking holds vacuously | Differs | a machine reads only what it requires of every binder, so a type that has subtasks takes a workflow of its own, as `WorkItem` does here |
| Drafts, published changes, deleting a status and moving its work items | Not yet | declaration versions and mappings (F5, ADR-0027); the format writes them with migrations |
| Changing the work type hierarchy, which "cannot be undone" | Not yet | a migration |

### 6.8 Approvals

Approvals exist outside Jira Service Management on the Premium and Enterprise plans: "The approvals feature is only available on Premium editions in Jira" ([approvals](https://support.atlassian.com/jira-software-cloud/docs/what-are-approvals/)). In a business space: "Once all of the approvers listed on the work item have signed off, the work item will be transitioned through to the approved transition. If one of the approvers decline, the work item will go through the declined transition" ([approvers](https://support.atlassian.com/jira-software-cloud/docs/how-to-manage-approvers/)).

| Jira | Verdict | In the format |
|---|---|---|
| Approvers named on the work item; each approves or declines | Written | `Document.approvers`, and each decision an observation, `approvals` |
| All approve, or a given number, or one declines | Written | the guards `all_approved` and `one_declined`, counting only decisions since the document entered review; a number is `count(…) >= n` |
| Who may approve, and who is excluded | Application | N7; an exclusion over recorded values, such as the reporter, is a guard |
| The transition taken automatically when the last approver signs off | Differs | the engine starts no transition no request caused (F8, N6): the application requests `approve` when it sees the last decision, and the guard confirms it |
| "if you have the 'any status' transition applied to the status … your approval process can be bypassed" | Differs | the same holds here: a transition into APPROVED without the guard bypasses it, and the rule set shows every transition into the state |

### 6.9 Around the workflow

| Jira | Verdict | In the format |
|---|---|---|
| Hierarchy: epic, standard work item, subtask; levels above epic | Written | types with references: `Epic.children`, `WorkItem.subtasks` as parts; further levels are further types |
| A subtask created only under a parent, archived with it | Written | `only_via: [WorkItem.add_subtask]`, and a cascade on `archive` |
| "You can’t archive a subtask by itself" | Differs | `archive` is the shared machine's, and a binder cannot mark it `only_via`; a subtask with a workflow of its own could |
| Links with types and directions: blocks, duplicates, relates to, clones | Written | `WorkItemLink`, with `source` and `target` naming each other's ends; a guard may read them, as `not_blocked` does, which Jira's built-in conditions do not |
| Keys `PROJ-123`, one series per space | Written | `identifier: { sequence, scope: space, format: "{space.key}-{n}" }` |
| Board columns mapped to statuses; the backlog; Done work leaving the board after 14 days | Application | presentation (N6); the engine answers the queries |
| Column constraints, which "trigger a visual indicator" and do not block ([columns](https://support.atlassian.com/jira-software-cloud/docs/configure-columns/)) | Written | a type-scan guard marked `warn`: `under_wip_limit` |
| Sprints, future to active to closed; one active at a time unless parallel sprints are on; reopening | Written | `Sprint`, with `unique: { where: state == ACTIVE }` on its space |
| Completing a sprint, which needs every subtask done, and moves unfinished work to the backlog or a sprint | Written | `complete_to_backlog` and `complete_to_sprint`, guarded by `subtasks_done`, each calling its work items |
| Versions, unreleased to released to archived; releasing moves unresolved work items to another version | Written | `Version`, `release_moving_unresolved` |
| Automation: triggers (field changed, scheduled, webhook), conditions, actions, branches over related work items | Application | initiating work and reacting to events are the application's (N6); its actions are requests under the same rules, and a consequence declared on a transition is a cascade (F8) |
| Forms, whose submission creates a work item | Application | the form is an interface; its submission is an initial transition's request |
| Due and start dates; overdue work | Written | attributes; `overdue` is a derived attribute |
| Time tracking and worklogs | Written | the `worklogs` observation, with a `duration`; working hours per day and days per week are a non-goal (PRD §3) |
| Bulk transition and bulk move | Written | the library's batch operation (ADR-0037); the format is not involved |
| Clone, which links the copy | Written | `clone` creates a work item and a `CLONES` link |
| Recurring work items | Application | a schedule (N6) requesting `clone` |
| Move to another space; change the work type | Not yet | supersession (ADR-0028), the last construct |
| A default value for a field, such as priority | Not yet | attribute defaults, with quantity tracking |
| Metrics: cycle time, velocity, time logged | Written | a fixed measure and two formulas (§4.9) |

### 6.10 Found and fixed by this study

The instrument found four defects, each fixed and committed with a probe or a plant:
1. The conversion dropped a reference's `unique`, `indexed` and `personal`, so uniqueness on a reference enforced nothing (commit `b0fe878`).
2. `from: any` had no written form, although Appendix B called the transition kinds written (`0501838`).
3. Step 3 refused the built-in category `closed` unless a module listed it (`fbc927f`).
4. The internal checker's check 55 refused an attribute named `labels`, which §9.2 of the model allows (`04d8827`).

The inventories also caught this project's own mistake: a quotation in ADR-0118, taken from a page summary rather than the page, which was retracted in place (`docs/LESSONS.md`).

### 6.11 The answer

Every workflow Jira's engine configures, in either kind of space, can be expressed. The format writes it today, except where three constructs the model already has are still to come: editing a workflow under live work items (migrations), moving a work item or changing its type (supersession), and default field values. A different workflow for one work type in each space, `extends`, was written on 2026-09-26 after this answer was first given.

Where it differs from Jira, a written form still exists. A lifecycle ends in a final state, such as an archived one. A rule that holds for only some of the types sharing a workflow, a subtask blocking condition or an archive only a parent may cause, puts those types in a workflow of their own, as a Jira scheme would. A transition Jira takes by itself, on the last approval or a merged pull request, is requested by the application and checked like any other request. One validator is excluded by decision, the regular expression.

What remains is by design an application's: who may do what, which is most of Jira's conditions; what is shown, which is boards and screens; and what starts work or reaches outside, which is automation, webhooks and notifications. The engine records and answers what each of those needs.

## 7. Against Jira Service Management's documented flows (2026-09-26)

The author asked next: "how about the flows in jira's service management?" This section answers as §6 does, against Atlassian's documentation and the flow format, with the same verdicts.

**Scope.** Jira Service Management Cloud: its workflows, request types, approvals, SLAs, customers and queues, and the IT service management practices around them, incidents, problems, changes, operations and assets. Marketplace apps are out of scope.

**Method.** Two inventories were taken from Atlassian's documentation on 2026-09-26: one of the workflows and the rules around them (261 quotes from 246 pages), one of the IT service management practices and operations (291 quotes from 134 pages). Each quote was taken from the page's raw text and checked by script against it, and a sample was checked against the live pages again before use. The default ITSM workflows' transitions are published only as screenshots, so the module below takes its statuses from the default status list Atlassian publishes as text ([statuses](https://support.atlassian.com/jira-cloud-administration/docs/what-are-issue-statuses-priorities-and-resolutions/)) and not from those diagrams. The claims marked *Written* are held by [`flow-format/examples/servicedesk.yaml`](flow-format/examples/servicedesk.yaml), which passes all four steps of the checker: service requests with approvals and SLAs, incidents, problems, post-incident reviews, changes with peer and board review and freeze windows, alerts, services and assets.

### 7.1 Workflows, request types and the portal

| Jira Service Management | Verdict | In the format |
|---|---|---|
| The default statuses: Waiting for support, Waiting for customer, Waiting for approval, Pending, Escalated, Canceled, Declined, and the change and problem statuses | Written | the states of `Request`, `Incident`, `Problem` and `Change` |
| Resolved, "awaiting verification by reporter", and Closed, "considered finished"; a request closed some days after it is resolved | Written, and Application | RESOLVED is `closed` and not final, CLOSED is final; `close` is guarded by `resolved_three_days` and requested by a schedule (N6) |
| Request types: "one request type can only be connected to one work type", and one work type serves many request types | Written | a type is the work type and its workflow; `request_type` is an enumeration on it |
| Portal groups, request forms, hidden preset fields | Application | the interface; what a form submits arrives as the transition's inputs |
| Status names customers see, mapped per request type | Application | presentation (N6) |
| "Transitioning a work item from the portal ignores validators for the transition" | Differs | every caller's request passes the same guards (F3); a customer's own step, such as replying, is a transition like `customer_replies` |
| The default resolutions, with Known error, Hardware failure and Software failure | Written | the `Resolution` enumeration |
| A new status open to every other ("Any status") | Written | `from: any` |
| Deleting a status and moving its requests, which runs no rules | Not yet | migrations |

### 7.2 Approvals

| Jira Service Management | Verdict | In the format |
|---|---|---|
| An approval step on a status, with approvers from a field or group, a number of approvals or all, and people excluded from approving | Written | `raise_for_approval` into WAITING_FOR_APPROVAL; `approvals_needed` derives the number; `approved` counts approvals since the request entered the state, leaving out the reporter's own |
| An Approve and a Decline transition, and one decline declining the request | Written | `approve` and `decline`, guarded by `approved` and `declined` |
| Transitions blocked while an approval is pending; *Block transition until approval* | Written | WAITING_FOR_APPROVAL is left only by `approve`, `decline` and `cancel` |
| "Once the approval is approved or declined, the request will transition automatically" | Differs | the engine starts no transition no request caused (F8, N6): the application requests `approve` or `decline`, and the guard confirms it |
| Approvers from the affected service; the change advisory board | Written | `Change.cab_approved` reads `affected_service.change_approvers` |
| Customers choosing approvers, approvers who hold no licence, approving by email or in Slack | Application | who may approve and through which channel (N7) |
| Reminders of pending approvals, the *Approval required* and *Approval completed* triggers, auto-approval rules | Application | N6 |

### 7.3 SLAs

| Jira Service Management | Verdict | In the format |
|---|---|---|
| Goals matched in order, by priority or any JQL, with "All remaining priorities" last | Written | `resolution_goal`, derived with `if … else` |
| Goals by a customer's detail, such as a platinum support level | Written | `resolution_goal` reads `organization.support_level` |
| Start, pause and stop conditions on statuses, such as pausing while waiting for the customer | Written | `time_to_resolution` sums the intervals outside the paused states; `paused` is derived |
| Conditions on events: a comment for customers, an assignment, a resolution set | Written, in part | a transition's event is an interval boundary; the first response is measured from the `replies` observation, and any other event needs a recorded fact to measure from |
| Calendars of working hours, holidays and a time zone | By decision | business hours and local time are a non-goal of the first release (PRD §3, `edge-cases.md`); the SLA runs on absolute time |
| A new SLA applies to every request, open and closed | Written | a derived attribute and a metric are computed from the record |
| Editing an SLA recalculates ongoing cycles and keeps completed ones | Differs | a derived attribute is recomputed over the whole history; a result that must stay fixed is written when it happens, as `resolve` writes `sla_met` |
| A priority changed mid-cycle: "time already tracked counts toward the new goal" | Written | the goal derives from the current priority |
| A new cycle on reopening | Differs | the sum covers every cycle; one cycle is the intervals since its start, `where i.entered_at >= …` |
| Breach shown on the request; `breached()`, `paused()` and `remaining()` in JQL | Written, and Application | `resolution_breached` and `paused` are derived and queryable; showing them is the interface's |
| Reports of met against breached, and the success rate | Written | the `sla_success` metric |
| The *SLA threshold breached* trigger | Application | N6, from a query |

### 7.4 Customers, comments, satisfaction and queues

| Jira Service Management | Verdict | In the format |
|---|---|---|
| Licensed agents and unlicensed customers; organizations whose members share requests; request participants; customer permissions and channel access | Application, the data Written | who may see and do what is the upper layer's (N7); `organization`, `participants` and `share` record it |
| Replies to the customer and internal notes | Written, and Application | two observation kinds, `replies` and `notes`; who may read a note is N7 |
| Customer and internal notifications | Application | N6 |
| Satisfaction surveys, sent when a request reaches Done | Written, and Application | the `ratings` observation, its 1 to 5 invariant and the `satisfaction` metric; sending the survey is the application's |
| Queues, "a kind of filter for your requests", often sorted by SLA | Application | the engine answers the query, by `resolution_breached` for instance |

### 7.5 Incidents, problems and reviews

| Jira Service Management | Verdict | In the format |
|---|---|---|
| Impact and urgency, from which a team may "create an an impact urgency priority matrix and use automation to automatically assign priorities" (Atlassian's doubled word) | Written | `Incident.priority` derived from the matrix, declared once and not automated |
| A major incident, moved to its own queue | Written, and Application | `major` and `mark_major`; the queue is the interface's |
| Affected services, whose responders and stakeholders join the incident | Written | `set_affected_service` adds the service's responders in the same transition; several affected services at once are a link type, as `WorkItemLink` is in §6 |
| An incident linked to its problem, "is caused by" | Written | `Incident.problem` and `Problem.incidents` |
| A linked work item's status, reported onto the incident by automation | Written | `problem_open`, derived |
| Incidents closed three business days after they are resolved, by an SLA and an automation rule | Written, and By decision | `close`, guarded by three days of absolute time and requested by a schedule |
| The incident timeline | Written | the event log, with the `timeline` observations |
| Post-incident reviews with a primary incident, created from the incident or by automation | Written | `create_review` creates one with this incident as its primary |
| A problem's root cause and workaround; known errors kept as knowledge base articles | Written, and Application | attributes, required under review and on completion; the resolution `KNOWN_ERROR`; the article is outside the store |

### 7.6 Changes

| Jira Service Management | Verdict | In the format |
|---|---|---|
| Standard, normal and emergency changes | Written | `change_type` |
| Standard changes, pre-authorized and approved by a preset automation rule | Written | `request_standard` creates one straight into PLANNING, guarded by `standard` |
| Emergency changes, fast-tracked | Written | `expedite` |
| Peer and board review: "By default, the change management workflow doesn't force approvals for these steps" | Differs | a transition's guards are its rules: a review is enforced by declaring it, as `approve_peer` and `approve_cab` do |
| Change approvers from the affected service | Written | `cab_approved` |
| Change risk, and AI risk assessment | Written, and Application | `risk`; an agent that assesses it records it by a request |
| The change calendar, with maintenance and freeze windows as "visual indicators" | Written | `ChangeWindow`, and `outside_freeze`, a type-scan guard that enforces the freeze on `approve_cab` |
| Five business days to review a normal change | Written, and By decision | a derived duration on absolute time, as §7.3 |
| Deployment tracking and gating with a CI/CD tool | Application | the pipeline reads the change's state and requests `implement` or `roll_back`; asking an outside system from a guard is an evaluator, a construct to come |
| The preset change automations: type and risk on creation, a transition on the deployment's result, low-risk changes approved | Application | requests under the same guards |

### 7.7 Operations and assets

| Jira Service Management | Verdict | In the format |
|---|---|---|
| Alerts, open or closed, acknowledged, snoozed; "Only one open alert with a specific alias can exist at any given time", a repeat counted on it | Written | `Alert`, with `unique: { where: state == OPEN }` on `alias` and `repeat` counting `occurrences` |
| Closing and reopening, after which the alert must be acknowledged again | Written | `reopen` clears `acknowledged` |
| A snooze of up to seven days | Written | `snooze_within_a_week` |
| An incident created from alerts | Written, and Application | `link_incident`; creating it from the integration is the application's |
| Escalation policies, on-call schedules, rotations and overrides, routing rules, heartbeats, notification policies, maintenance, alert grouping | Application | timers, notifications and choosing whom to call are the application's (N6); the engine answers which alerts need escalating, by `needs_escalation` |
| Asset object types with a parent, and abstract ones | Written | the abstract `Asset`, which `Laptop` and `Server` extend, each binding `AssetLifecycle` (§4.14) |
| Objects, attributes, a key that "cannot be changed", references with a reference type | Written | `Asset`, `identifier`; a reference, or a link type where the reference carries a type |
| An object's status, "active, pending, or inactive" | Written | states in those categories |
| An object's history | Written | the event log |
| The Assets field, "a two-way link between the work item and the object" | Written | `Incident.asset` and `Asset.incidents` |
| AQL, automation on objects, bulk edits | Application | queries, N6, the batch operation |
| The service registry, with tiers and owner teams | Written | `Service` |
| The knowledge base and its deflection, the virtual service agent, live chat, playbooks | Application | N6; what an agent does is a request |

### 7.8 Found and fixed by this study

1. A value containing a colon and a space, which every aggregate with a body has, failed to load with only YAML's message; the specification and the message now say how to write it (commit `0c1b952`).
2. **A guard runs before its transition's writes** (DESIGN.md §6, steps 4 and 5), so a guard that tests a value its own transition supplies reads `inputs.<name>`. `issues.yaml`'s `Document.has_approvers` read the attribute its `submit` writes and would have refused every submission; it now reads `inputs.approvers`, as `servicedesk.yaml` does. A scan of every example found one other such guard, `points_changed`, which compares the input with the old value on purpose. A notice for this is proposed in `TODO.md`.
3. Writing the module, step 3's `required` check found that `Problem.complete` could not show its root cause was present; the module now requires it under review.

### 7.9 The answer

Every flow Jira Service Management's workflows, approvals and SLAs configure can be expressed. Four things JSM does by automation are declarations here: the priority from impact and urgency, a standard change's pre-authorization, an affected service's responders joining an incident, and a linked problem's status. One thing JSM only shows, the freeze window, is enforced. The one limit by decision is **working-hours calendars**: an SLA is written and measured on absolute time, and business hours are a non-goal of the first release. It is the limit a service desk will feel most; asked, the author kept it on 2026-09-26, "keep the calendar non-goal" (PRD revision 11). The constructs still to come are migrations, for deleting statuses under live requests, and evaluators, for a guard that must ask an outside system; `extends`, for asset object types, was written after this answer was first given.

What remains is by design an application's: the portal and what it shows, who may see and do what (customers, organizations, internal notes), notifications, queues, escalations and on-call, the CI/CD tool's side of gating, and the transitions JSM takes by itself, each served by the engine's queries and its record.
