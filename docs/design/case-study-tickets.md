# Case study: issue tracking (Jira-shaped)

Status: design iteration 2, 2026-09-07. Companion to [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md). Decisions taken here are ADR-0026 to ADR-0029 and an amendment to ADR-0021, all pending author review.

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
| Condition ("only assignee may execute") | actor guard: `actor.id == assignee.id` (ADR-0025) |
| Validator ("resolution required", "fix version required to close") | guard over inputs; requiredness attaches to the transition (ADR-0002) |
| Post-function: set resolution, clear resolution on reopen, set resolved date | outcome writes: `resolution := inputs.resolution`, `resolution := null`, `resolved_at := now` (ADR-0021 amendment) |
| Post-function: assign to project lead | outcome write from a related attribute: `assignee := project.lead` |
| Post-function: fire event | the event log (ADR-0013) |
| Post-function that computes or reaches outside (send mail, call a webhook) | an effect; a consumer subscribed to the event (ADR-0007) |
| Update on the same status (edit fields without transitioning) | action — a self-transition (ADR-0016) |
| Subtask; "parent cannot be Done while subtasks open" | composition; collection guard `none(t in subtasks where t.state.category != done)` |
| Epic link, parent link | reference to another issue |
| Issue link with type and direction (blocks / is blocked by, duplicates, relates to) | a declared relationship with a name and a declared inverse; adding or removing a link is an action; guards read it: `none(blocked_by where state.category != done)` |
| Resolve as duplicate | a terminal transition recording a `duplicate_of` reference; **not** an identity merge |
| Version (unreleased → released → archived), Component, Sprint (future → active → closed) | objects with their own small state machines; issues reference them; "cannot add to a closed sprint" is a guard on the action |
| Comment, worklog | parts — small objects with an author, a timestamp and a posted → edited → removed lifecycle |
| Attachment | `file` attribute (ADR-0017) |
| Watchers, labels | set-valued attributes edited by a declared action; recorded (ADR-0042 removed the free class) |
| History tab | the object's recorded events |
| Custom field; field configuration (required / hidden per context) | attributes on the type; per-context differences are per-type differences (ADR-0026) |
| Permission scheme; issue-level security | capabilities in the actor descriptor; issue-level security is a declared visibility predicate (ADR-0030) |
| Bulk transition | the `batch` operation: N independent requests, each in its own transaction, N verdicts (ADR-0037) |
| Automation rule ("when X then Y"), SLA breach | a consumer subscribed to events (ADR-0012); breach is found by querying the stored `due` against the supplied time, since a clock-dependent derived value cannot be indexed (ADR-0048) |
| Issue key `PROJ-123` | a business identifier minted from a **named sequence** scoped by project (ADR-0029); monotonic, not gapless |
| Move issue to another project; convert subtask ↔ issue | **supersession**: the old object ends in a terminal superseding state naming its successor (ADR-0028) |
| Clone | a creation transition whose inputs are read from another object |
| Editing a live workflow: removing a status that issues are in | **declaration versioning** with a mapping applied as recorded migration transitions (ADR-0027) |

## 2a. Resolution, declared

```text
capability ISSUE_RESOLVE_ANY, ISSUE_REOPEN

do resolve IN_PROGRESS -> DONE accepts resolution, fix_version {
  require may:      actor.id == assignee.id or actor.has(ISSUE_RESOLVE_ANY) because delegable
  require subtasks: none(t in subtasks   where t.state.category != done)    because dependent
  require blockers: none(b in blocked_by where b.state.category != done)    because dependent
  require versioned: resolution == Resolution.FIXED
                     implies inputs.fix_version is not null                 because self_serviceable
  set resolved_at := now
}

do reopen DONE -> IN_PROGRESS {
  input reason : string
  require may: actor.has(ISSUE_REOPEN) because delegable
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
- Issue-level security. A read-visibility predicate per object is needed for this and is deferred to the read-surface design.
