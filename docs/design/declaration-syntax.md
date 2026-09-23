# The declaration syntax

Status: **draft, iteration 20** (2026-09-23). Iteration 20 carries ADR-0097 to ADR-0100: the built-in capabilities, `as <collection>`, request-supplied occurred time with its `occurred_within` clause, `clear` on an optional reference, `backfill` and `removed member`, combined metrics, per-object flow collections, `count(distinct …)`, and the standard metrics as declarations (§6.11). A review of those repairs found what they left, and ADR-0101 closed it in the same iteration: the import writes only mirrors, a mirror may declare `erase`, migrations never reach an observation, and the standard metrics exclude the import's events, clip finished spans and are checked as a block instantiated for `ServiceJob`. Iteration 19 (2026-09-23) added the constructs of ADR-0082 to ADR-0087 and ADR-0092, the decisions of ADR-0095 that spelling them required, and eight checks, with the checker extended to parse them and a fixture for each new check. Iteration 18 was the author's rulings on the open questions. **Ready for author review.** The format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).


Two goals shape every choice, and where they conflict the second wins.

1. **The common case should cost nothing to write and nothing to read.**
2. **Everything the design promises to check must be decidable from the text.** §10 lists the checks.

## Iterations

Twenty drafts, sixteen review rounds and one coherence pass: engineers building real systems in it, and audits against the model. Each round is summarised by what it changed, because most changes were reversals of the round before.

| # | What it changed |
|---|---|
| 2 | Machines optional; a write may not leave the object being transitioned; references unified; guards named; twelve constructs the model needed and iteration 1 could not express |
| 3 | Reversed two of those. The unlisted authority marking had deleted the guarantee `only via` exists to make, and the implicit machine existed in the runtime but not the file |
| 4 | A machine says what it requires of a binder; `tracking` explicit again; deletion cascades name a transition and a bound; `default` became sugar rather than an ungated write |
| 5 | Reversed the runtime-maintained inverse, which was a second write path; dropped the unlisted authority marking for good; forbade a default on an optional input |
| 6 | Scalar arithmetic so a line total types; evaluator calls usable in a guard; `requires part` and `requires capability`; a binder may declare its own transitions |
| 7 | Rewrote the checker after it was found to verify none of iteration 6's fixes; added the grammar the document relied on and could not write |
| 8 | Made coverage fixture-derived, so it cannot be claimed without being demonstrated; fixed the capability indirection its own machine bypassed |
| 9 | Eight parser bugs, two of which made the tool unusable on any model with a numbered state; joined the delivery and unit examples so the cascades are real |
| 10 | A wrapped transition head parsed with an empty body, so every body check silently passed; `only via` did not resolve machine-supplied parents; the reserved-word rule the document had abandoned was still enforced |
| 11 | Two reviews, both blocking, both finding the same shape of defect: a term used only by the check list. Eleven of them were defined; the line-continuation rule was replaced by indentation after seven of this document's own lines broke it; `accepts` was pinned to the head it had always been written in; three-valued evaluation was stated, having been the trap a first-time user actually fell into; enum literals, `.state` and `.category` acquired the syntax five guards already assumed |
| 12 | Defects the previous repair introduced. A part with an abstract owner could be declared, given cascades and never created, because one check forbade what three others enabled. Decimal accumulation could not publish, since every addition widened the result past its target. The new indentation rule mis-parsed two of this document's own lines, the new unknown rule was illustrated with the enum syntax the same iteration had just required, and three stored ends carried a marking the same iteration had just said they never carry |
| 13 | A payments ledger, the first domain reviewed that is high-volume and short-lived. The pattern §3.2 recommends for a part serving two wholes was rejected by the checker in both spellings, so prose and tool were drifting in opposite directions on one construct. Uniqueness over two attributes could not be written at all: a type-scan matched the object against itself, and the clause that repaired it was not a symmetric shape. `money` had no scale or rounding. The negative of an external verdict, which the document prescribes for a decline, did not type |
| 14 | Three reviewers re-read their own findings; two returned non-blocking. The remaining defects were all one shape, and the audit named it: a rule changed in the body with its check left alone, four times across three iterations. Check 52 now fails a publish when a normative statement in §1 to §9 cites no check, which is the first check on the document rather than on a declaration |
| 15 | Two of three reviewers non-blocking. The swap test of iteration 14 was under-specified: read literally it rejected the two shapes it claimed to accept, because swapping a comparison also reverses its operands. It now names the four normalisations and is a decision procedure. Check 52's own detector recognised seven phrasings and missed seventeen normative statements in this document; a reviewer ran the check against the four defects it was built for and found it catches one, so the claim that it would have caught them is retracted here rather than softened |
| 16 | All three reviewers non-blocking. Check 52's proximity window gave a live false pass, so citation is now scoped to the sentence, which found four more. A check widened to satisfy a citation came out forbidding more than its rule did — the rule-and-check divergence class arriving inside the repair for it, and invisible to check 52 in both directions, since a wrong citation and an overreaching check both pass a presence test. A binder's own `create` now replaces the machine's, without which the document's own remedy for a required part could not be spelled |
| 17 | The creation-replacement rule of iteration 16 broke its own motivating example: replacing a creation orphans the state it reached, which is what an initial state is, so the type the rule was written to make expressible would not publish. Reachability after a replacement is reported rather than failed. Two of the four checks replacement touches had been updated with it and two had not |
| 18 | The author ruled on all thirteen open questions (ADR-0065 to ADR-0072), which added a third tracking mode, arguments on a cascade clause, a comparable type on a reverse reference and both state spellings. Re-expressing the five older case studies then found that `DESIGN.md` had promised since the repair that a value could be cleared deliberately, and the language had no spelling for it; `clear` is ADR-0073 |
| 19 | The data-driven engine (ADR-0082 to ADR-0086, ADR-0092), checked against the PRD. Seven constructs — the observation kind with `unit` and `occurred within`, the label, the `metric` form with flags, the `observe` marking, the `backdatable` marking, the `assignee` marking with its `actor` identity, and the metric reference in a guard — and checks 54 to 61. Spelling them decided six things the ADRs had left to the syntax, recorded as ADR-0095 rather than only here. An independent review of the PRD coverage then found eighteen defects in three passes (ADR-0096), and this iteration carries their repairs: a generated `forget` on every observation kind, corrections bound to their subject, `closed` as a built-in category, the members `.held`, `.created_at` and `.created_by_kind`, metrics that say whether they are complete for their reader, and the smallest flow that publishes, checked like any example. The checker learned the `observation` and `metric` forms before the examples were written, so the examples are checked from the first draft, and each new check was then shown to catch a mutation of the examples as well as its fixture |
| 20 | A review of the whole record against the PRD by five readers, one per slice, found that several PRD needs could be named in prose and not written in the language: the standard metrics, a refusal rate, handoffs per job, a distinct count, a set-valued dimension, the removal of an assignee, a new required attribute on live objects, a removed enum member. Each now has a form, and the standard metrics are declarations checked like the examples (ADR-0097 to ADR-0100) |

## 1. Shape of a file

A file is a module, extension `.ok`. Declarations may appear in any order; forward references are fine. `#` begins a comment.

```text
module inventory
use   commerce.{Customer}

capability INVENTORY_CREATE, INVENTORY_RETIRE, INVENTORY_ASSERT,
           ERASE_PERSONAL, DELIVERY_EDIT, DELIVERY_COMPLETE, DEAL_VIEW_ALL,
           PO_EDIT, CLAIM_EDIT, SERVICE_EDIT, SERVICE_ASSIGN, PDI_RECORD
category   inbound, live, closed

enum      RetirementReason version 1 { FAILED, DAMAGED, OBSOLETE, LOST }
sequence  unit_serial version 1
evaluator xero version 1 { … }
machine   UnitLifecycle version 2 { … }
type      Robot version 3 { … }
observation InspectionResult version 1 on ServiceJob { … }
metric    open_jobs version 1 { … }
```

Eleven top-level forms: `module`, `use`, `capability`, `category`, `enum`, `sequence`, `evaluator`, `machine`, `type`, `observation` (§6.8) and `metric` (§6.9).

**What publishing takes** is a module together with the closure of its `use` imports, and every check in §10 runs over that closure. Every top-level declaration is importable; `use` names what a module depends on so that the closure is computable from the text rather than from a directory listing. Capabilities and categories are **one vocabulary across the closure**, not per module, because a capability that meant different things in two modules would make every shared machine unsafe.

**`capability` and `category` declare vocabularies.** Both are otherwise bare identifiers a typo turns into silence: a mistyped capability is a guard nobody can satisfy, a mistyped category a family-wide guard that never matches. The declaration is a spell-check, not a promise: capabilities are opaque strings the consumer's authentication produces, and nothing here can verify it produces these.

**`closed` is the one category with a meaning of its own**, and every vocabulary has it whether or not a module declares it: work is **open** until it enters a `closed` or a terminal state, and **completes** when it first does. The standard metrics of `DESIGN.md` §5.12 and §5.13 read it — work in progress, the oldest open work, cycle time — and an observation kind's `RECORDED` state is `closed` (§6.8). A terminal state counts as finished whatever its category, so no lifecycle leaves work open forever by accident (ADR-0096).

**Six capabilities are built in** the same way, and gate the built-in types' transitions: `OK_DRAFT_CHANGE` and `OK_APPROVE_CHANGE` for a flow change, `OK_LABEL` for labelling, `OK_ERASE` for erasing one observation, `OK_IMPORT` for the import and `OK_MAINTAIN` for maintenance. The rule set prints them with the types that use them, and a consumer's authentication emits them like any capability (ADR-0097).

**Versions.** The seven declarations that rules depend on carry one: `enum`, `sequence`, `evaluator`, `machine`, `type`, `observation` and `metric`. A `module`, a `use`, and the `capability` and `category` vocabularies do not, having no content a recorded object could be interpreted against. An object and an event record the **declaration version** of the publish in force, which fixes the version of every type at that instant (DESIGN.md §5.9); advancing a machine, enum, sequence, evaluator, metric or base type requires advancing every type, observation kind and metric that depends on it, which publishing enforces (check 22). Otherwise a recorded version would not identify the rules that applied.

## 2. Types

```text
type Robot version 3 {
  tracking serial
  machine  UnitLifecycle
  provides capability EDIT = INVENTORY_CREATE, RETIRE = INVENTORY_RETIRE, ASSERT = INVENTORY_ASSERT
  summary  serial, model, state

  attr serial string identifier from unit_serial scoped by model
                     format "RBT-{n}" indexed unique in scope
  attr manufacturer_serial string?
  attr label_printed_at    timestamp?
  attr photos file[]
  attr retirement_reason RetirementReason?

  ref  model : RobotModel
  ref  binding : Delivery? inverse units
  ref  engagement_lines : EngagementLine[] inverse unit

  derive leasable = state == DEVELOPMENT
                and none(l in engagement_lines where l.engagement.state != Engagement.CLOSED)

  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != Engagement.CLOSED) <= 1
}
```

**`tracking`** is `serial`, `quantity` or `record`, and is mandatory on any type with objects (check 42). `record` says the type tracks no physical thing — a delivery, a service job, an audit entry — and declares no counters. Most of an operations platform is records, and before ADR-0067 each had to claim it was one physical thing per row. None is a default, because inferring it from an incidental property is the implicit rule the model rejects by name.

A type body's clauses are conventionally written in one order — `tracking`, `machine` or `states`, `provides`, `summary`, `visible when`, then attributes, relationships, derivations, invariants, and last the transitions — but the order is a readability convention and nothing checks it, since a declaration whose clauses are shuffled is no less well defined. The head-and-body split of a transition (§5) is a rule, not a convention, because the two positions mean different things.

A **family** is an `abstract` base together with every type extending it. `tracking` is inherited with everything else, so a family states it once on the base; an `abstract` type may state it and needs none, having no objects. A header is written `type <Name> [extends <Base>] version <n> [abstract|mirror]`, in that order.

**`mirror`** says this store holds the type and does not own it: another system does, and here it is written only by the import path (ADR-0075). A mirror declares attributes, states and its external identifier and **no transitions but `erase`** (ADR-0101), so the rules about a lifecycle — a state needing an outgoing `do` (check 15), a type needing a creation (check 34), a machine needing a terminal state — do not apply to it. Check 53 is what makes the marking mean something: nothing here may write it but the import and its own erasure. **A mirror may compose with and extend other mirrors**, since every type is ported as one (ADR-0101): a delivery and its checklist items, or a family, are mirrors together and are cut over by one publish, and check 53 refuses only a mix, an owned type composing with or extending a mirror. An ordinary type may reference a mirror, read it in a guard and require it in an invariant, which is the point — a migrated delivery can be guarded on a customer that has not migrated yet. Cutting the type over removes the marking and declares the real transitions, which is an ordinary version advance and changes no object's id. A type another system owns for good is **not** a mirror: it is an ordinary type with an `external` identifier whose sync-driven transitions the sync requests, which is what lets it hold local state and supersede on an upstream merge (ADR-0080).

```text
type Customer version 1 mirror {
  tracking record
  states ACTIVE category live, ARCHIVED category closed
  summary legacy_key, name, state

  attr legacy_key string external "legacy" indexed
  attr name       string
}

type Delivery version 2 {
  tracking record
  states PREPARATION category live, DONE category closed terminal
  ref customer : Customer

  create open -> PREPARATION {
    input for_customer : Customer
    require live: inputs.for_customer.state == Customer.ACTIVE because dependent
    require may:  actor.has(DELIVERY_EDIT) because delegable
    set customer := inputs.for_customer
  }
  do finish PREPARATION -> DONE { require may: actor.has(DELIVERY_COMPLETE) because delegable }
}
```

A mirror also **binds no machine, declares no transitions of its own but `erase`, and extends or is extended only by another mirror** — each alternative would hand it a way to be written and defeat the marking (check 53). `erase` is the exception because erasure is this store's obligation over its own copy, not a write of the state another system owns, and an erased mirror stays erased through every later import (ADR-0101). Two further consequences are worth knowing before one is declared. An **invariant over a mirror** is checked when an import writes it and there is no transition to refuse, so an admission is the only recourse; state the invariant on the type that owns the data instead, where a guard can act on it. And a mirror holding a `personal` attribute must be **erased in both places**: its `erase` here redacts this store's copy and its events, and the system that owns the type has to do its own. Markings elsewhere — on an attribute, a reference, a state — may appear in any order after the part that is required.

`extends` inherits every attribute, relationship, derivation and invariant of the base, and nothing else: a machine is always bound explicitly, and transitions are never inherited. A base must be declared and `abstract`, and the inheritance graph must be acyclic (check 43).

**The smallest flow that publishes** has no guards, no invariants, no formulas and no datapoint kinds (PRD V1). It still records every transition with who made it and when, and it still has the standard metrics of `DESIGN.md` §5.12, so a flow can start here and be refined by publishes as its data shows what it needs (ADR-0085):

```text
type Request version 1 {
  tracking record
  states   OPEN category live, DONE category closed terminal
  create open -> OPEN { }
  do finish OPEN -> DONE { }
}
```

The checker runs this example like every other, so the claim that it publishes is tested, not asserted.

### 2.1 Every type has an explicit lifecycle

A type either **binds** a shared machine or **declares one inline** with `states`, which declares a machine named after the type. Exactly one of the two, unless the type is `abstract`, which has no objects and so needs no lifecycle.

**A type that binds a shared machine may also declare its own transitions.** They are private to it and may read anything it declares. The shared machine's transitions are the shared lifecycle; the type's are what only that type does.

**A `create` is the exception: a binder's own creations replace the machine's, they do not add to them.** The machine's creation **guards** are the part that does not go: they bind every creation of every binder, so a replacement may add conditions and may not drop them, and publishing rejects a replacement when an inherited guard reads an input it does not declare (check 8). Without that, a lifecycle's entry condition — a screening test, a capability anyone bringing one of these into existence must hold — silently stopped applying to the binder that most wanted its own creation. Publishing reports which ones were replaced, because the deletion happens in the machine and the edit that caused it is in the binder, so nothing in the text you are looking at shows what went away.

Replacing a creation can leave a state the machine's creation was the only way into, which is the ordinary shape of an initial state. **That is reported, not failed** (check 15), along with the transitions out of it, which are equally dead for that binder: it is a state the type carries and can never occupy, which is worth knowing and is not an error — a binder may legitimately not use one of a shared lifecycle's states, and failing would make a machine unshareable by anyone who needed to be born somewhere else. Every other transition it declares is additional. Birth is where a type's obligations are established — its required attributes written, its required parts filled — and a type that says how it is born says so completely. Without this the machine's generic creation would remain available on every binder, so a card payment obliged to hold its card details from birth could still be created without them by the transition the binder was trying to replace, and the third remedy below would say something the model could not deliver. This is what lets two types share a lifecycle without being identical, and it is why `requires` (§4.1) constrains only the machine body: a type-local transition needs no permission from the machine.

A binder whose creation comes from the machine may declare no required attribute, non-optional `ref` or singular `part` of its own, since the machine's creation cannot write or fill one; each must be optional, carry a `default`, or the binder must declare its own `create` (check 8), which replaces the machine's. A card payment that must hold its card details from birth declares `create take_card`, and the machine's generic `request` is then not available on it.

Reachability, name scoping and the terminal-state rule are checked **per binder**, over the machine's transitions plus that type's own.

```text
machine ApprovalFlow version 1 {
  requires capability EDIT

  state DRAFT     category live
  state SUBMITTED category live
  state APPROVED  category closed terminal
  state REJECTED  category closed terminal

  do submit DRAFT -> SUBMITTED { require may: actor.has(EDIT) because delegable }
  do reject SUBMITTED -> REJECTED { require may: actor.has(EDIT) because delegable }
  do approve SUBMITTED -> APPROVED proposable {
    require may: actor.has(EDIT) because delegable
  }
}

type ExpenseClaim version 1 {
  tracking serial
  machine  ApprovalFlow
  provides capability EDIT = CLAIM_EDIT

  attr receipt file?
  attr claimant_email string? personal

  create file_claim -> DRAFT accepts claimant_email {
    require may: actor.has(CLAIM_EDIT) because delegable
  }
  act attach_receipt at DRAFT {          # this type's own; ApprovalFlow knows nothing of it
    input value : file
    require may: actor.has(CLAIM_EDIT) because delegable
    set receipt := inputs.value
  }
  erase forget {
    input reason : string
    require may: actor.has(ERASE_PERSONAL) because delegable
  }
}
```

```text
type ChecklistItem version 1 {
  tracking serial
  states   ACTIVE category live, DELETED category closed terminal

  owner delivery : Delivery inverse checklist_items
  attr  label   string
  attr  checked bool default false

  create add -> ACTIVE only via Delivery.add_checklist_item accepts label {
    input for_delivery : Delivery
    set delivery := inputs.for_delivery
  }
  act tick at ACTIVE {
    require may: actor.has(DELIVERY_EDIT) because delegable
    set checked := true
  }
  act untick at ACTIVE {
    require may: actor.has(DELIVERY_EDIT) because delegable
    set checked := false
  }
  do  delete ACTIVE -> DELETED only via Delivery.cancel, Delivery.complete_sale,
                                        Delivery.complete_internal, Delivery.delete { }
}
```

## 3. Attributes and references

### 3.1 Attributes

```text
attr    <name> <type>[?] [marking …]
counter <name> [marking …]            # quantity types only
```

`?` means the value may be absent. Without it the value must be present whenever the object exists, checked at creation and at every write (check 8).

| | |
|---|---|
| scalars | `string` `bool` `int` `decimal(p,s)` `money(ccy)` `timestamp` `duration` `identity` |
| enum, event, file | `RetirementReason`, `event`, `file` |
| set | append `[]` |

A **`counter`** is an attribute of type `int`, never absent, and initialised to zero at creation without being written. It is **not** non-negative by construction: that is a fact about stock rather than about counters, so a type that needs it declares `invariant nonneg: on_hand >= 0` like any other fact about itself (ADR-0072). Everywhere a rule or a check says "attribute", a counter is one; it is a separate keyword only because `tracking quantity` is what makes stock arithmetic legitimate (§7).

References are `ref`, `part` or `owner`, never an attribute type. Inputs may be reference-typed.

| Marking | Meaning |
|---|---|
| `identifier from <seq> [scoped by <ref or indexed attr>] format "<pattern>"` | Minted during the creation. A scope names a reference or an indexed attribute the creation writes, and the mint happens after those writes and before every other outcome step, which is the order that lets a serial be scoped by the model it is created under. An attribute is allowed because a tenant is often an identifier mirrored from another system rather than an object here |
| `unique` / `unique in scope` / `unique where <expr>` / `unique with <attr>, …` | Sugar for a uniqueness invariant: global, per the sequence's scope, partial, or over the named attributes together. The compound form is what an idempotency key needs, since it is unique per caller and not globally. **`identifier` does not imply any of them**, since a scoped sequence repeats across scopes |
| `external "<source>"` | Owned by another system; uniqueness per source is automatic and partial over absent values |
| `personal` | Subject to erasure. A required attribute may not be `personal` (check 9), since erasure writes absence. An invariant may read one; erasure admits every invariant that reads what it erased, and records the admission (§6.4) |
| `indexed` | Available to query filters, type-scan predicates and visibility. `.state` and stored relationship ends are always available, so marking one is rejected rather than merely redundant (§8.1, check 7) |
| `default <expr>` | Sugar the checker expands into a `set` at the head of every creation that does not write the attribute, so it appears in the rule set and in the event like any other write. It never writes an object that already exists, including when a publish adds the attribute (§6.6, ADR-0099) |
| `actor` | On an `identity` attribute: this attribute holds the id of the actor the object stands for, which is what an `assignee` reference to this type compares against (§6.10, check 59) |

### 3.2 References and composition

```text
ref   <name> : <Type>[?|[]] [inverse <name>] [stored] [marking …]
part  <name> : <Type>[?|[]] inverse <name>
      cascade on <transition>|{ <transition>, … } to <Type>.<transition>[(<arg> := <expr>, …)] limit <n>  …
      survives [on { <transition>, … }]
owner <name> : <Type>       inverse <name>

cascade  <part> on <transition>|{ … } to <Type>.<transition>[(…)] limit <n>  # in a subtype
survives <part> [on { <transition>, … }]                                # in a subtype
```

```text
type Delivery version 1 {
  tracking serial
  states   PREPARATION category live, DELIVERED category closed terminal,
           CANCELLED category closed, DELETED category closed terminal

  part checklist_items : ChecklistItem[] inverse delivery
       cascade on { cancel, complete_sale, complete_internal, delete }
               to ChecklistItem.delete limit 500
  part approvals : Approval[] inverse subject
       cascade on { cancel, complete_sale, complete_internal, delete }
               to Approval.discard limit 50

  ref  units : Robot[] inverse binding

  attr approved_total money(SGD)?
  attr internal bool default false

  create open -> PREPARATION { require may: actor.has(DELIVERY_EDIT) because delegable }

  act add_checklist_item at PREPARATION {
    input label : string
    require may: actor.has(DELIVERY_EDIT) because delegable
    create ChecklistItem.add(for_delivery := this, label := inputs.label)
  }
  act bind_slot at PREPARATION {
    input unit : Robot
    require may: actor.has(DELIVERY_EDIT) because delegable
    call inputs.unit.reserve(slot := this)
  }
  act approve at PREPARATION {
    require may:  actor.has(DELIVERY_COMPLETE) because delegable
    require once: none(a in approvals where a.approver == actor.id) because delegable
    create Approval.record(for_subject := this, approver := actor.id, at_event := this_event)
  }

  do complete_sale PREPARATION -> DELIVERED {
    require not_internal: not internal                              because unreachable_from_here
    require settled: none(c in checklist_items where not c.checked) because dependent
    require signed:  any(a in approvals
                         where not changed_since([approved_total, checklist_items], a.at_event))
                                                                    because delegable
    require may: actor.has(DELIVERY_COMPLETE) because delegable
    for u in units limit 500 { call u.sell() }
  }
  do complete_internal PREPARATION -> DELIVERED {
    require is_internal: internal because unreachable_from_here
    require may: actor.has(DELIVERY_COMPLETE) because delegable
    for u in units limit 500 { call u.deliver_internal() }
  }
  do cancel PREPARATION -> CANCELLED { require may: actor.has(DELIVERY_EDIT) because delegable }
  do delete CANCELLED -> DELETED {
    require no_referrers: none(r in referrers where r.state.category != closed) because dependent
    require may: actor.has(DELIVERY_EDIT) because delegable
  }
}

type Approval version 1 {
  tracking serial
  states   RECORDED category live, DISCARDED category closed terminal

  owner subject : Delivery inverse approvals
  attr  approver identity
  attr  at_event event

  create record -> RECORDED only via Delivery.approve accepts approver, at_event {
    input for_subject : Delivery
    set subject := inputs.for_subject
  }
  do discard RECORDED -> DISCARDED only via Delivery.cancel, Delivery.complete_sale,
                                            Delivery.complete_internal, Delivery.delete { }
}
```

A cascade drives its transition on each part, and **skips a part already in a terminal state**, since its disposition has happened. That is the only skip **on account of state**, and it belongs to a `cascade` clause alone: a `call` to a transition its target cannot take in its current state fails rather than passing over it, which is what makes an erasure's `call c.forget(…)` reliable even though `erase` is the one transition that runs from a terminal state. A `call` does skip when its **path** does not resolve, which is a different thing and is §5.2's second absence rule. A part whose guard fails **aborts the whole request** and rolls it back (ADR-0038), because a cascade that silently passed over a part it could not dispose of would leave exactly the orphan the coverage rule exists to prevent. That is what lets a delivery be cancelled and later deleted without its parts being driven twice.

**Every terminal transition of the whole must be accounted for**, by a `cascade on` clause naming it or by `survives` (ADR-0058). Three forms, because three things are actually meant:

| Written | Meaning |
|---|---|
| `cascade on { a, b } to T.x limit n` | one clause covering several triggers, which is the ordinary case and was four identical lines until iteration 11 |
| `cascade on { a, b } to T.x(reason := "…") limit n` | the same, passing the part's transition its inputs. Arguments read the **whole's** members and the triggering transition's inputs, since the whole is what is being transitioned, and publishing checks them against the part transition's declared inputs (check 2) |
| `survives` | the part outlives the whole through **every** terminal transition |
| `survives on { withdraw }` | it outlives those, and the rest must still be covered by a cascade |

The middle form is deliberately awkward to write, because a part outliving its whole is usually a modelling error. The third exists because the distinction is real: a delivery's audit photographs are kept when it is deleted, while its checklist items are not, so one part survives `delete` and the other does not. Publishing rejects a terminal transition covered by neither form, and rejects a transition named by both (check 37).

**A part declared in an `abstract` base carries no cascade clauses**, since the base has no transitions to trigger them. Each concrete subtype supplies its own with the top-level `cascade <part> on …` and `survives <part> …` forms, and publishing holds the subtype, not the base, to the coverage rule. The part's `owner` names the base; its creation's `only via` names the concrete subtypes' transitions, since the base has none; and the top-level `cascade` clause counts as a call site exactly as the inline form does:

```text
type Whole version 1 abstract {
  tracking serial                     # inherited by both members of the family
  part notes : Note[] inverse subject
}

type Ticket extends Whole version 1 {
  states OPEN category live, CLOSED category closed terminal
  cascade notes on { close } to Note.file(reason := "the ticket was closed") limit 100
  create raise -> OPEN { require may: actor.has(DEAL_VIEW_ALL) because delegable }
  act annotate at OPEN {
    input body : string
    require may: actor.has(DEAL_VIEW_ALL) because delegable
    create Note.add(on := this, body := inputs.body)
  }
  do close OPEN -> CLOSED { require may: actor.has(DEAL_VIEW_ALL) because delegable }
}

type Incident extends Whole version 1 {
  states LIVE category live, RESOLVED category closed terminal
  cascade notes on { resolve } to Note.file(reason := "the incident was resolved") limit 100
  create report -> LIVE { require may: actor.has(DEAL_VIEW_ALL) because delegable }
  act annotate at LIVE {
    input body : string
    require may: actor.has(DEAL_VIEW_ALL) because delegable
    create Note.add(on := this, body := inputs.body)
  }
  do resolve LIVE -> RESOLVED { require may: actor.has(DEAL_VIEW_ALL) because delegable }
}

type Note version 1 {
  tracking serial
  states DRAFT category live, FILED category closed terminal
  owner subject : Whole inverse notes
  attr  body string
  attr  filed_reason string?
  create add -> DRAFT only via Ticket.annotate, Incident.annotate accepts body {
    input on : Whole
    set subject := inputs.on
  }
  do file DRAFT -> FILED only via Ticket.close, Incident.resolve {
    input reason : string
    set filed_reason := inputs.reason
  }
}
```

Without this, the abstract base that §4.1 recommends for a part serving two wholes could not be written at all — and until iteration 13 the checker rejected it in both spellings while the prose recommended it, which is the worst state for a rule to be in.

A guard invalidated by a change to a part reads the part relationship by name:

```text
require fresh: not changed_since([approved_total, checklist_items], a.at_event)
               because delegable
```

`part` and `owner` are the two ends of a composition: exclusive membership, lifetime bounded by the whole, re-parentable by writing the `owner`. **`cascade on <transition> to <Type>.<transition>` names which of the whole's transitions drives the cascade, which part transition it drives, and a bound.** A whole with several terminal transitions names them in one clause, as the delivery above does. Iteration 4 named the target and the bound but not the trigger, so a type with both a reject and a withdraw had no answer. A cascade clause **counts as a call site** for the purposes of an `only via` list.

A reference may omit `inverse`, which only means no back-reference is named. It remains visible to `referrers` and to the deletion guard.

### 3.3 Both ends are declared; one of them stores the value

Both ends of a relationship are declared, each in its own type, so a type reads completely on its own. Exactly one end **stores** the value; the other is a derived view the store resolves as a query. Which end stores it follows from the declaration, and is never a matter of reading order:

| The pair | The stored end |
|---|---|
| `part` / `owner` | the `owner`. The child holds the parent, which is what makes re-parenting one write |
| `ref` with one singular end (`T` or `T?`) and one set end (`T[]`) | the singular end |
| `ref` with two singular ends | ambiguous; exactly one must be marked `stored` (check 41) |
| `ref` with two set ends | neither end can hold it. Declare a type for the association carrying a `ref` to each side; publishing rejects the pair (check 41) |
| `ref` with **no `inverse`**, so only one end is declared | that end, which must be singular. A set-valued `ref` with no `inverse` has nowhere to live and is rejected (check 41) |

A singular stored `ref` may be marked **`assignee`**, naming it the object's responsibility, which the assignment metrics are over (§6.10, check 59).

So `Robot.binding : Delivery?` stores, and `Delivery.units : Robot[] inverse binding` reads "the robots whose binding is this". Only the stored end is writable: `set binding := inputs.slot` is one write with one event, and the far object's view changes because what it reads changed, not because anything wrote to it. A write naming a derived end is rejected (check 17), and so is a pair with no stored end or two (check 41).

Iteration 9 declared both ends and left which one held the value to the reader, so a set could be `set` in one type and not in another with nothing in the text to say why.

Iteration 3 had the runtime write both ends. That was a second write path: the far event carried no transition name, so no subscription could filter it, `changes_state` was undefined for it, and it stamped the far object's last-written index, so binding a unit invalidated every approval on the delivery. Iteration 2's alternative — a transition per direction — cost the authority twice and made the release path a cascade cycle the checker rejects.

### 3.4 Invariants, and the three forms

```text
invariant <name>: <bool expression>
```

An invariant is checked after every write to the object, and after a write to any other object it reads. Which objects those are is decided by its **form**, which publishing determines and reports, because the three cost very different things to enforce:

| Form | Reads | Re-checked when |
|---|---|---|
| **local** | only members of this object | this object is written |
| **traversal** | through a declared relationship end | this object is written, or any object reachable through that end |
| **type-scan** | every object of a named type | any object of that type is written |

A type-scan names a **type** where an aggregate names a collection — `none(v in Visit where …)` rather than `none(v in visits where …)`. **It ranges over every _other_ object of that type**: the object being written is never in the set — and the same holds for a type-scan in a **guard**, where the object being transitioned is excluded, so identical syntax means the same thing in both places — so no `v.id != this.id` clause is needed and writing one is an error (check 6).

That is right for uniqueness and it changes what a **count** means, so the count has to be read as "how many others":

```text
invariant at_most_ten: count(v in Version where v.product == product) < 10   # ten in total
```

Prefer a traversal for a cardinality bound whenever the objects are related — `count(v in product.versions) <= 10` says the same thing over a relationship, reads as the number it is, and is far cheaper than a scan of the type. A type-scan is for when there is no relationship to count over. Without that rule the obvious uniqueness invariant matches the object against itself and is false for every object ever created, so uniqueness over two attributes could not be written at all. The `id != this.id` clause that used to repair it is now redundant rather than wrong — it passes the swap test — and redundancy is the ground check 6 rejects it on. It is the expensive form, and it can be enforced without locking the whole type only if it is **symmetric**: its verdict does not depend on *which* of the objects it relates is the one being written. An asymmetric scan admits a pair that each object accepts on its own turn, and reports a violation on a write that did not cause one.

Symmetry is decided by the **swap test**, not by a list of blessed shapes. Exchange every reference to a member of `this` with the corresponding one on the bound variable, and the bound variable's with `this`'s. The invariant is symmetric when the predicate that comes back is the same as the one you started with, compared like this:

| Compared | Modulo |
|---|---|
| the conjuncts of an `and` | order; they are a set |
| `==` and `!=` | the order of their operands |
| `<` `>` `<=` `>=` | direction, so `a < b` and `b > a` are one comparison |
| a member of `this` | its spelling, so `subject` and `this.subject` are one reference |

Nothing else is normalised. Two predicates that a human can see are equivalent but that differ in any other way are not symmetric for this purpose, because the test has to be a decision procedure an implementer can build rather than an equivalence a reader can argue for.

```text
invariant one_open_per_kind:                         # symmetric: passes the swap test
          none(v in Visit
               where v.subject == subject and v.kind == kind
                 and v.state.category != closed
                 and   state.category != closed)
```

The consequence worth stating is that **a status filter has to be applied to both objects**. Writing only `v.state.category != closed` is the natural first attempt and it is asymmetric: with one visit planned and one cancelled, cancelling the second reports a conflict with the first, while planning the first saw no conflict with the second. The swap test rejects it and names the member that appears on one side only, which is the whole content of the mistake.

Overlap of two ranges and equality on a shared key pass the test under those four normalisations, and only under them. Overlap is the case that shows why: its two conjuncts are `b.start < end` and `b.end > start`, and swapping gives `start < b.end` and `end > b.start`. Neither swapped conjunct matches the original it came from. Each matches **the other one**, and only once direction is discounted — `start < b.end` is `b.end > start`. So the test needs both the set-comparison of conjuncts and the direction rule, and it needs them together. That is why the table above is part of the rule and not a note on it. These were the shapes iterations 1 to 13 listed. They were listed as the *only* shapes, which excluded "at most one open X per Y" — the commonest type-scan there is, and the one DESIGN.md §5.6 names as canonical.

A traversal invariant needs the far end of what it traverses to be declared, or the write that breaks it has no way to find it and re-check (check 4).

## 4. Machines and transitions

```text
enum OverrideReason version 1 { SUPPLIER_EXCEPTION, MIS_SCANNED, REPAIRED_OUTSIDE }

machine UnitLifecycle version 2 {
  requires attr label_printed_at timestamp?
  requires attr photos file[]
  requires ref  model : RobotModel
  requires ref  binding : Delivery?
  requires attr retirement_reason RetirementReason?
  requires invariant one_open_engagement
  requires capability EDIT, RETIRE, ASSERT

  state REQUESTED   category inbound
  state PROCUREMENT category inbound
  state INTAKE      category inbound
  state AVAILABLE   category live
  state RESERVED    category live
  state SOLD        category closed
  state DEVELOPMENT category live
  state RETIRED     category closed terminal
  state CANCELLED   category closed terminal

  create request -> REQUESTED accepts model {
    require may: actor.has(EDIT) because delegable
  }
  do ship    REQUESTED   -> PROCUREMENT { require may: actor.has(EDIT) because delegable }
  do receive PROCUREMENT -> INTAKE      { require may: actor.has(EDIT) because delegable }

  do inventorize INTAKE -> AVAILABLE {
    require labelled: label_printed_at is not null           because unreachable_from_here
    require photo:    model.label_photo_required
                      implies count(p in photos) >= 1        because unreachable_from_here
  }

  do reserve AVAILABLE -> RESERVED only via Delivery.bind_slot {
    input slot : Delivery
    require open: inputs.slot.state == Delivery.PREPARATION because dependent
    set binding := inputs.slot
  }

  do sell RESERVED -> SOLD only via Delivery.complete_sale { }
  do accept_return SOLD -> AVAILABLE {
    require may: actor.has(RETIRE) because delegable
  }
  do deliver_internal RESERVED -> DEVELOPMENT only via Delivery.complete_internal { }

  do retire DEVELOPMENT -> RETIRED accepts retirement_reason {
    require may: actor.has(RETIRE) because delegable
  }
  do cancel { REQUESTED, PROCUREMENT, INTAKE } -> CANCELLED {
    require may: actor.has(EDIT) because delegable
  }

  act record_label_print at INTAKE {
    require may: actor.has(EDIT) because delegable
    set label_printed_at := now
  }

  assert correct_state -> { AVAILABLE, DEVELOPMENT, RETIRED } {
    input to : state
    input reason : OverrideReason
    input detail : string?
    input admits : invariant[]?
    require may: actor.has(ASSERT) because delegable
    may admit one_open_engagement
  }
}
```

### 4.1 `requires`

A machine states what a binding type must provide, one requirement per line:

```text
requires attr <name> <type>[?]
requires ref  <name> : <Type>[?|[]]
requires part <name> : <Type>[?|[]]
requires invariant <name>
requires capability <NAME>
```

It covers the attributes, references, parts, invariants and capabilities its transitions and admissions name. Publishing verifies every binder supplies them with compatible types (check 16), and separately that the machine body names nothing it does not require (check 35) — a completeness check, without which the requirement list drifts from the body it describes.

A required **capability** is a name the machine uses in its guards and each binder maps to its own:

```text
requires capability EDIT                    # in the machine
provides capability EDIT = PO_EDIT          # in one binder
provides capability EDIT = CLAIM_EDIT       # in another
```

Without this, two types sharing a lifecycle must share an authority model, and the only workaround, `actor.has(PO_EDIT) or actor.has(CLAIM_EDIT)`, silently grants every editor of one authority over the other.

A required **part** whose element type differs per binder is declared against an `abstract` base the binders' part types extend, which is also what lets the part's `owner` name a single type. A required part may be singular, since a type that must hold exactly one of something from birth is ordinary; a machine whose creation cannot write it obliges the binder to declare its own `create` (§2.1).

### 4.2 Transition kinds and markings

| Keyword | From | To |
|---|---|---|
| `create` | nothing | one state |
| `do` | a state, a `{ set }`, or `any` | one state |
| `act` | `at` a state, a set, or `any` | the same state |
| `assert` | `any` | an input drawn from a declared set (§6.3) |
| `erase` | any state, terminal included | the same state |

Markings follow the states and precede the body:

```text
do   sell RESERVED -> SOLD only via Delivery.complete_sale, Lease.convert { }
do   approve SUBMITTED -> APPROVED proposable { … }
do   receive PROCUREMENT -> INTAKE backdatable within 2 days { … }
```

A transition a machine supplies is named `<Binder>.<transition>`, not `<Machine>.<transition>`, because authority belongs to the type, not to the lifecycle it borrows. The same holds in a `cascade on` clause. An `owner` may name an **abstract** base, which is how one part type serves two wholes. The wholes need not share a machine, and each supplies its own cascade clauses for the inherited part (§3.2). An `only via` on the part's creation names the concrete wholes' transitions, not the base's, since the base has none.

**`backdatable within <duration>`** lets a request supply `occurred_at`, when the change actually happened, no further back than the duration and never later than the time the request is recorded (ADR-0083, ADR-0096). The event keeps both times and the interval index uses the occurred one; guards still read `now`. The occurred time may not precede the start of the current interval of any state or tracked value the request changes, so no interval is negative, and every transition the request cascades to records the same occurred time, since one request is one change (ADR-0095). The marking is legal on a `create`, a `do` or an `act`, and on an `assert`, an `erase` or an `only via` transition it is a publish error, as is a duration outside §8.3's closed set of units (check 58): an assertion's time is when the store was told, an erasure is not a step of the flow being measured, and an `only via` transition is never requested, so none has an occurred time a request could supply. The bound is a generated guard named `occurred_within`, with remedy `self_serviceable`, so a request that exceeds it is refused naming a rule, as any refusal is (ADR-0099). The occurred time arrives as the request's `occurred_at` field, never as an input. Guards read `now` as the clock, but a guard over `entered_at` reads the occurred time, so backdating can shorten a wait by up to the bound; the publish report lists every such guard (§10). Without the marking, the occurred time is the recorded time, and a request that supplies one is refused by the same clause.

**`only via`** makes a transition unrequestable and names the transitions that may cascade to it. Publishing verifies the list against the call sites it derives: a `call` from a transition not named is an error, and a name that never calls it is an error too (check 13).

Iteration 2 offered an unlisted variant instead, and iteration 4 kept it. Both deleted the guarantee: with no declared list, every call site becomes an authorised parent by construction, so a new module could write `call unit.sell()` and grant itself the power to sell a unit with no delivery, and no publish could refuse it. There is now no unlisted form. A transition either states who may cause it, or anyone may request it.

An `only via` transition carries no **actor guard** — a guard whose expression calls `actor.has` — since the parent's authority is its authority; writing one is a publish error rather than a guard silently dropped (check 14). A guard that reads `actor.id` to compare identities, such as a one-approval-per-person test, is not an actor guard and stays legal. It may not be `proposable` (check 14), since an unrequestable transition can never yield the verdict that makes a proposal meaningful.

`terminal` means no `do` may leave the state and no `act` may be declared at one, since a disposed object admits no further transitions. A **terminal transition** is a `do` whose to-state is terminal; a creation into a terminal state is not one. The coverage rule of §3.2 is about **the whole's** terminal transitions, so a type whose objects are born final has none of its own and asks nothing of anything it owns.

It says nothing about being *owned*. A whole with a terminal transition whose parts are born final has no cascade available — check 18 rejects a cascade naming a transition the child does not have — so that part is marked `survives`, and that is the one shape where `survives` is not a modelling error but the only correct answer. A journal of posted entries, a statement of settled lines, an audit log under the thing it audits: the records outlive the container by design. `at any` therefore means any non-terminal state, and `erase` is the one transition that runs at a terminal state; a publish's migration, which is the publish's own recorded change and not a transition the object takes, is the other thing that reaches one (§6.6, ADR-0101).

**`terminal` is not the same as `closed`, and choosing it early is the mistake this rule invites.** A category says where in the lifecycle a state sits; `terminal` says nothing further will ever be recorded. An object that is finished operationally but must still accept recorded activity — a trial subject who has completed and may still have an adverse event reported against them, an order that is delivered and may still be reviewed — belongs in a `closed` state that is **not** terminal, with a genuinely final state after it if one is needed. Marking the first closed state terminal and discovering the conflict at the first `act` costs a lifecycle restructure and a fresh set of cascade clauses.

## 5. Inside a transition

`accepts` and `only via` are part of the transition **head**, before the `{`, alongside the states and the other markings. Inside the body, clauses appear in one order: `input`, `require`, `corrects` or `may admit`, then the outcome. Iteration 10 listed `accepts` as a body clause, which every example contradicted.

### 5.1 Inputs and guards

```text
<kind> <name> <states> [only via …] [accepts <attribute>, …] [proposable] [backdatable within <duration>] {   # the head
  input     <name> : <type>[?|[]] [personal]
  input     <name> : <type> default <expr>
  require   <name>: <expression> [eager|deferred] [observe] [because <remedy class>]
  corrects  <attribute>, …                                     # §6.4
  may admit <invariant>, …                                     # assert only, §6.3
  …                                                            # outcome steps, §5.2
}
```

**A `default` may not be given for an optional input** (check 20). An optional input carrying a default is never unsupplied, so the skip rule below could never fire for it, and a partial edit would clear the field it meant to leave alone.

An input may be **set-valued**, written `<type>[]`, and a set may be of a reference type — which is how a transition takes a selection, such as the slots to move to a new delivery. The `admits` input of an assertion is the same form over `invariant`.

An input may be marked **`personal`**: erasure then redacts it from every payload that recorded it, whether or not it was written to an attribute, and a write of it into a non-personal attribute is rejected like any personal value (check 10, ADR-0078). An input that flows into a personal attribute is personal without the marking.

**`accepts`** declares that these attributes are supplied by the caller and written directly. `accepts comment` is exactly `input comment : <the attribute's type>` plus `set comment := inputs.comment`, optionality included. It is available on `create`, `do` and `act`, and it accepts a reference as readily as an attribute.

A `default` applies **at creation only**. An accepted attribute not supplied to a `create` takes its default; not supplied to a `do` or an `act`, it is **not written at all**, and the stored value stands. The alternative silently overwrites: a partial edit that omits a field would reset it to the default someone chose for new objects, which is the opposite of what omitting a field means in an edit.

**`observe`** puts a clause on trial (ADR-0085). The clause is evaluated like any other and never refuses: where it would have refused, the request proceeds and the would-be refusal is recorded in the attempt log, linked to the event that applied. The rule set prints it as not enforced, the adversarial harness treats it as absent, and the publish report lists it, so no guarantee is ever claimed for one; enforcing it is a publish that removes the marking. An `observe` on a clause of an `assert` or an `erase` is a publish error (check 57), since the escape hatch and erasure are enforced whole or not at all.

**Guard names are always required.** A verdict carries the failing clause's name, and that name should not appear or change shape when someone adds a second guard.

**`eager` or `deferred`** marks a guard that calls an external evaluator, not the evaluator itself, so two guards over one function may differ. It says *when in a caller's workflow* the evaluator is consulted: an eager guard is consulted whenever the transition's availability is computed, a deferred one only when the transition is actually requested. Neither runs inside the write transaction. Omitted, a guard is eager.

**The five remedy classes** say what the caller can do about a failure: `self_serviceable` (correct something in the request), `delegable` (someone with more authority can do it), `temporal` (**wait, and it will pass**), `dependent` (another object must change first), `unreachable_from_here` (no path from this state at all).

`temporal` and `unreachable_from_here` both arise from a clock and mean opposite things. A window that has not opened is `temporal`; a **window that has closed** is `unreachable_from_here`, because waiting is exactly what will not help. No inference can tell them apart, since both are a comparison against `now`, so **declare the class on any guard about a deadline**. It matters beyond the wording: the publish report singles out the sweepable transitions worth polling by their guards being declared `temporal`, and a consumer polls that list. A closed window left to inference puts a legally dead transition on a scheduler forever.

`because` is optional. Omitted, the checker infers one by the first rule that matches, in this order: a guard calling `actor.has` is `delegable`; one comparing against `now` or a duration is `temporal`; one reading another object, through a relationship or a type scan, is `dependent`; one comparing against an input is `self_serviceable`; anything else is `unreachable_from_here`. The order is what makes the inference deterministic, since most real guards match several rules.

**The inference never contradicts a declared class.** A declared one is the author's statement about their own domain, and the checker has no standing to overrule it — an earlier draft had it do so, and the rule as written rejected this document's own stock guard. Check 20 verifies only that a declared class is one of the five.

### 5.2 Outcome steps

```text
set    <attribute or relationship end> := <expression>     this object only
clear  <optional attribute or optional stored reference>  write absence, this object only
add    <attribute> := <expression>                         this object only
remove <attribute> := <expression>                         this object only
call   <path>.<transition>(<arg> := <expr>, …)
create [<name> =] <Type>.<transition>(<arg> := <expr>, …)
supersede <expression>
for    <name> in <collection> [where <expr>] limit <n> { … }
for    <name> in 1..<expression> limit <n> { … }
```

**Every write stays on `this`**, and its target may be an attribute or a stored relationship end; writing an `owner` re-parents a part, which is what makes splitting an object expressible. Reaching another object is `call`, which runs its transition, evaluates its guards and records an event on it.

A path may be rooted at `this`, a relationship, a loop binder or an input.

**`clear` writes absence**, and is the only way to. An unsupplied optional input *skips* its write, leaving the stored value alone, which is what a partial edit means; `clear` is the opposite and says so, so the two cannot be confused. It is a write like any other: it stamps the attribute's last-written index, appears in the event, and invalidates an approval that read the attribute. It reaches an optional, singular, stored `ref` end as well, which is how an assignee is removed or a binding released. Publishing rejects a `clear` on a required attribute, a counter, a required or set-valued end, a `part` or an `owner` (check 17, ADR-0099).

Without it a bug that is reopened keeps the resolution it was closed with, which is what `case-study-tickets.md` needed and could not write.

**Absence skips a step, and only in two places.** A step or argument whose expression is *exactly* an unsupplied optional input is skipped, and a `call` whose path runs through an absent optional relationship end is skipped. The second is what makes an erasure's `call card.forget(…)` correct when the card is optional and absent; everywhere else absence in a path is an error, since it would otherwise hide a missing write (check 48). Neither is the state-based skip of §3.2, which belongs to a cascade: a `call` on an object that exists and cannot take the transition fails rather than being passed over. Any larger expression containing one is a publish error (check 48), so `set amount := inputs.amount` and `set amount := inputs.amount + 0` cannot be confused. A skipped write does not stamp the attribute's last-written index, so a no-op edit does not invalidate an approval.

`limit` is mandatory on every loop and on every cascade clause (checks 21 and 47), and bounds **that loop**. Publishing reports each loop's own bound and the observed maximum fan-out over the live objects (ADR-0071). It does not report a product across nested loops: multiplying bounds an author invented produces a precise number that means nothing. A request exceeding a loop's bound is refused with `over-limit` naming that loop.

## 6. The remaining constructs

### 6.1 Supersession

```text
state MERGED category closed terminal superseding

do merged_into ACTIVE -> MERGED only via Contact.merge_in {
  input successor : Contact
  supersede inputs.successor
}
```

The successor may be an input or a name bound by an earlier `create`. Self-supersession and cycles are publish errors (check 26).

### 6.2 External evaluators

```text
evaluator xero version 1 {
  fn invoice_valid(order_id : string) fresh 30 min
  fn contact_exists(id : string)
}
```

A guard calls one as `<evaluator>.<fn>(<arg>, …)`, the one call to something outside the store; the other calls an expression may make — `metric(…)`, the time functions of §8.3 and the time buckets of §6.9 — read the store's own data:

```text
require invoiced: xero.invoice_valid(order_id) deferred because dependent
```

A function declares its argument types and how stale a verdict may be. Every evaluator returns a **verdict and never a value**, so no return type is written, and a guard may be a `verdict` as well as a `bool`. The consequence is worth stating where it is met rather than leaving it to be discovered: where an external system *decides* something — a randomisation service choosing a treatment arm, a pricing service quoting — the caller supplies the value as an input and the evaluator can only be asked whether it is consistent. The store records such a value as governed data it did not verify. Open question 8 is whether that boundary should move. A verdict too stale to use is refused as `temporal` whatever the guard declares.

**Arguments are resolved twice** (ADR-0092): against committed state and the inputs before the transaction, where the evaluator is consulted, and again inside it. A mismatch means the world moved in between, and the request consults again within its serialisation retry bound. An argument that reads a value the outcome reaching the guard writes — an input bound from a member its caller sets, in a transition reached by a `call` or `create` — could never agree before the transaction, so it is a publish error (check 61).

### 6.3 Assertions and admissions

`may admit` lists the invariants an assertion is *permitted* to violate; the `admits` input carries the ones a particular request actually admits, so an admission names an object and an invariant rather than blanket-admitting on every use. An assertion must declare a capability guard and a `reason` input.

An admitted invariant may be **this type's, or one on a type reachable from it by a declared inverse**, written `<Type>.<invariant>`. The invariant an assertion breaches is often not its own: putting a subject back into an enrolled state breaks the site's cap on enrolment, and the site is where that rule belongs. Restricting admission to the asserting type's own invariants left those assertions with no path at all, which is how a declared override becomes an out-of-band database edit.

### 6.4 Erasure and correction

```text
erase forget {
  input reason : string
  require may: actor.has(ERASE_PERSONAL) because delegable
  for a in approvals limit 1000 { call a.forget(reason := inputs.reason) }
}

act correct_delivery_date at any {
  input value  : timestamp
  input reason : string
  corrects date
  set date := inputs.value
}
```

An `erase` transition erases every `personal` attribute of its object, runs from **any** state including a terminal one, since erasure requests arrive for closed accounts, and may cascade into parts. Declaring one is optional, but if a type declares one it must reach every part type that holds personal attributes, which publishing checks (check 39). Otherwise an erasure returns success and leaves the person's identity in the parts.

**Erasure follows the supersession chain** (ADR-0087): erasing any member erases every predecessor and successor, each through its own type's `erase`, in one request. A type with a personal attribute that takes part in supersession and declares no `erase` is a publish error, since the chain could not be erased through it (check 60). Erasure also reaches the caller's events and the proposals whose inputs flowed into what it erases, by check 10's taint analysis, and the notes of the object's labels (DESIGN.md §8).

**Erasure admits the invariants it breaks.** Writing absence can violate an invariant that reads what was erased, and refusing the erasure is not an option the law leaves open. So an `erase` carries an automatic admission for every invariant reading an attribute it erased, recorded on the event with the reason, exactly as a declared admission is. This is what makes an invariant asserting that a consent signature is present both writable and erasable; without it the only safe model is one where nothing personal may be asserted at all. `corrects <attribute>, …` produces the `corrected` provenance; the outcome must write exactly the attributes named, and a `reason` input is required.

### 6.5 Deletion guards

```text
do delete ACTIVE -> DELETED {
  require no_referrers: none(r in referrers where r.state.category != closed)
                        because dependent
}
```

`referrers` is every object holding a live reference to this one, derived from every declared reference, and it binds its element like any aggregate. Parts are excluded, since they cascade.

### 6.6 Migration mappings

```text
removed state  LEGACY_HOLD -> AVAILABLE
removed member RetirementReason.OBSOLETE -> FAILED
renamed attr   old_name    -> new_name
backfill       manufacturer_serial := "UNKNOWN"
```

A publish that drops a state carries its mapping. A dropped attribute merely hides, its recorded values staying in history, so only a rename needs one.

**A publish changes a live object only by a recorded migration** (ADR-0099). DDL never writes a row: a new column is added nullable, and a `default` applies at creation only. So:
- a new **required attribute** on a type with live objects needs a `backfill`, whose expression reads the object's own members and literals, and publishing applies it to each live object as a migration transition with provenance `migrated`. Without one, the publish is refused and the report counts the objects (check 23);
- a removed **enum member** that live objects hold needs a `removed member` mapping, applied the same way. Events keep the old value, which is read under the version that wrote it (check 23);
- a field added to an existing **observation kind** must be optional, since an observation is born final and nothing could fill it (check 22);
- a new required singular **reference** is filled by `backfill` in the same way, from an expression the object's own members yield; a new required singular **part** on a type with live objects is refused, since filling it means creating an object per whole, so it is declared optional or set-valued (check 23, ADR-0101);
- a migration reaches **every object the change affects, terminal ones included**: it is the publish's own recorded change, not a transition the object takes, and so the second exception, beside erasure, to a terminal state admitting nothing further (ADR-0101). It never reaches an **observation**, which is born final (PRD D4), so a publish removing an enum member any observation holds is refused (check 23);
- a `backfill` expression is a write like any other for check 10, so it may not copy a personal value into a non-personal attribute;
- a removed **type** or **observation kind** is retired: nothing is created in it and no transition applies, and its objects, table and history stay readable under the versions that wrote them.

### 6.7 `this_event`, visibility, extension

`this_event` yields the event the current transition will record, and an `event`-typed **input accepts only `this_event`**, which is what stops an approval being forged with an old one.

**`visible when`** takes a `bool` expression over `actor`, the object's own members, and stored relationship ends one step out. Every attribute it reads must be `indexed`, since visibility is applied as a query filter and not as a per-row test; `.state` and stored ends qualify without a marking (§8.1). It may not call an evaluator, read a derivation that is not itself indexed, or traverse a set-valued end (check 7), all three for the same reason: the predicate has to become part of a query.

```text
visible when actor.has(DEAL_VIEW_ALL) or owner.id == actor.id
type Bug version 1 abstract { … }
type BugInPlatform extends Bug version 1 { machine PlatformWorkflow  … }
```

### 6.8 Observations and labels

```text
observation <Name> version <n> on <Type> as <collection> {
  field       <name> : <type>[?] [unit "<unit>"] [personal]
  recorded by <bool expression>
  occurred within <duration>
  invariant   <name>: <bool expression>
}
```

An **observation kind** records a fact about an object without a transition for each kind of fact (ADR-0082, PRD D3). It is declared once, `on` its subject type, and names the collection it adds to the subject with `as`. Adding a kind is that one declaration: the subject's text is not edited, and the subject's version does not advance (ADR-0099).

```text
enum CheckOutcome version 1 { PASS, FAIL, NOT_APPLICABLE }

type InspectionCheck version 1 {
  tracking record
  states   ACTIVE category live, RETIRED category closed terminal
  attr     title string
  create add -> ACTIVE accepts title { require may: actor.has(SERVICE_EDIT) because delegable }
  do retire ACTIVE -> RETIRED { require may: actor.has(SERVICE_EDIT) because delegable }
}

observation InspectionResult version 1 on ServiceJob as inspections {
  field check   : InspectionCheck
  field outcome : CheckOutcome
  field value   : decimal(10,3)? unit "V"
  field note    : string?
  recorded by   actor.has(PDI_RECORD)
  occurred within 7 days
  invariant in_range: value is null or (value >= 0.0 and value <= 60.0)
}
```

`InspectionCheck` is a catalogue: its objects list what to check, so changing a checklist creates and retires objects and needs no publish (DESIGN.md §5.11, PRD D7).

**Declare a kind on the object whose gate reads it.** `changed_since` reaches only `this` (§8.3), so a sign-off on a delivery is invalidated by a later result only if the results are the delivery's: `observation PdiResult version 1 on Delivery as pdi_results { field unit : Robot … }`, not a kind on each robot (PRD UC-4).

**A kind is sugar for a type**, and the rule set prints what it expands to:
- a `tracking record` type of the same name, with one state, `RECORDED`, of category `closed` and `terminal`;
- on the subject, `part <collection> : <Kind>[] inverse subject`, and on the kind `owner subject : <Type>`;
- each `field` as a member of the same optionality and markings: a field whose type names a declared type is a singular `ref` with no inverse, which stores it, and any other field is an `attr`;
- a creation, `create record -> RECORDED`, taking each field as an input, optional exactly where the field is, plus `subject`, and an optional `corrects`, an observation of the same kind and subject that this one replaces;
- an erasure, `erase forget`, taking a `reason`, which erases the observation's personal fields (ADR-0096).

Every observation also has `.recorded_at`, the time its event was recorded; `.recorded_by_kind`, the kind of actor that recorded it; and `.occurred_at`, when the fact happened, stored in its own column.

The creation carries five guards, named in a verdict like any other:
- `recorded_by`, the `recorded by` expression, which reads `actor` and the inputs, `inputs.subject` included;
- `subject_open`, that the subject is not in a terminal state, for a new observation. A correction is exempt, since it adds no fact to a settled whole but replaces one already recorded, and PRD D4 allows no exception (ADR-0096);
- `corrects_current`, that a `corrects` names an observation of this kind on the same subject which nothing has corrected yet, so a correction can neither reach another subject's results nor branch a chain;
- `occurred_within`, that the request's `occurred_at`, if it supplies one, is no further back than the kind's `occurred within` bound and no later than the time it is recorded (ADR-0099);
- `subject_owned`, that the subject is not a `mirror`. A mirror's observations and labels are written by the import alone, as its attributes are, so a mirror has one writer until it is cut over; labelling a mirror's object is refused the same way (ADR-0101).

A correction is a recording, so `recorded_by` applies to it as to any other: whoever may not record on a subject may not correct its results either.

**Recording is a request.** `record` is requested like any creation, by a caller or by an outcome step — `create InspectionResult.record(subject := this, check := c, outcome := CheckOutcome.PASS)` — so it has an actor, an event, an idempotency key and a place in `history`, and there is no second write path. `availability(subject)` offers it beside the subject's own transitions, and `check` answers for it given the type in place of an id (ADR-0099, PRD F3).

**Recording stamps one thing on the subject**: the position of its collection, which is what `changed_since([inspections])` reads (§8.3). It advances no version and writes no attribute, so it conflicts only with a transition whose guard read that collection, which serialisable isolation then retries (ADR-0099).

**Two part rules bend, for observation kinds only.** An ordinary part is created only via a transition of its whole, and a terminal transition of its whole must say what happens to it (check 11, check 37). An observation is exempt from both: it is recorded directly, and it is implied `survives`, since a born-final object has no transition for a cascade to drive. What check 11 protects is kept by `subject_open`: nothing new is added to a settled whole. That is why §4.2 advises a `closed` state that is not `terminal` for an object that must still accept new records.

**`recorded by` and `as` are mandatory**, and a subject that declares a part of an observation kind itself is a publish error (check 54). A kind anyone could record would let a gate that reads it be opened by anyone (PRD UC-19), and the collection is what erasure reaches (check 39) and what `changed_since` names.

**Fields.** A numeric field may declare a `unit`: a label that the rule set prints and a read returns beside the value, which arithmetic neither converts nor checks. A `unit` on a field that is not `int` or `decimal` is a publish error, as is a field named `subject`, `corrects`, `occurred_at`, `recorded_at`, `recorded_by_kind`, `created_at` or `created_by_kind`, the members every observation already has (check 54). A field may be `personal`, and check 9 and check 10 apply to it as to any attribute. A field added to a kind that already exists must be optional (check 22), since an observation is born final and nothing could fill it (§6.6).

**A kind's invariants may read its own fields only** (check 54). They are checked when the observation is recorded, and they are local by construction, so recording one re-checks nothing else.

**`occurred within <duration>`** bounds how far back the request's `occurred_at` may reach, and a duration outside the closed set of units of §8.3 is a publish error (check 58). Omitted, the occurred time is the recorded time, and a request that supplies one is refused by `occurred_within`.

**Nothing is edited.** A correction is a new observation naming the one it replaces in `corrects`. Every read of a kind, through the subject's collection or as a metric's source, sees only the observations nothing has corrected; `history` and `export` keep both (ADR-0095). A correction says a record was wrong. A later result says the thing changed, and is recorded as a new observation, not a correction, so the first result stays readable and a first-pass yield can be measured (ADR-0099). A gate that must pass on the latest result per item reads the latest one, as §6.10's example does. Erasure is the one exception to "nothing is edited", and removes only personal values.

**The erasure is reached, not written.** The subject's `erase` runs `forget` on every observation in its collection, corrected ones included, as it redacts its labels' notes, without a step saying so, and check 39 counts an observation part as reached. An actor holding `OK_ERASE` may also request `forget` on one observation, which is how a personal value recorded about someone other than the subject is removed without erasing the subject (ADR-0096, ADR-0100, PRD D8).

**Labels.** Every type carries labels without declaring any. A label is an observation of the built-in kind `label`, with a `name`, normalised to lower case and trimmed; an optional `note`, which is personal (ADR-0078's limit on free text); and `subject_state`, the subject's state when it was applied. Its occurred time is its recorded time. Its `forget` redacts the note, so a note is erasable on a type that declares no `erase` of its own. Labelling requires `OK_LABEL`, and a type may narrow it with a condition over `actor`, conjoined with the capability:

```text
labels by actor.has(SERVICE_EDIT)
```

**A label is readable only as a metric's source**, `<Type>.labels` in a `from` (§6.9), and naming one anywhere else — a guard, an invariant, a derivation, a visibility predicate, an outcome value or a `labels by` — is a publish error (check 55). ADR-0082 names the first three. ADR-0095 closes the next two as well, because each would carry a label into a guard at one remove: a derivation the guard reads, or an attribute an outcome copied it into. A label becomes logic by promotion to a state, an observation kind or an attribute, which is a publish (DESIGN.md §5.11).

### 6.9 Metrics

```text
metric <name> version <n> {
  from      <binder> in <source> [where <bool expression>]
  combine   <name> = <metric>, …                 # in place of from (below)
  by        <dimension> = <expression>, …          # a combined metric lists names only
  window on <timestamp expression>
  value     <expression>
  flag      <name> when <bool expression>
}
```

A **metric** is a declared formula across many objects or over time (ADR-0084, PRD C1). It has either a `from` or a `combine`, and a `value`; `by`, `window on` and each `flag` are optional.

```text
metric time_working version 1 {
  from      i in ServiceJob.intervals where i.state == ServiceJob.WORKING
  by        engineer = i.held(engineer), month = month(i.entered_at)
  window on i.entered_at
  value     median(i.duration)
  flag      slow when value > 10 days
}

metric inspection_pass_rate version 1 {
  from      r in InspectionResult where r.outcome != CheckOutcome.NOT_APPLICABLE
  by        engineer = r.subject.engineer
  window on r.occurred_at
  value     count(where r.outcome == CheckOutcome.PASS) * 1.000 / count()
  flag      low when value < 0.900
}
```

**The source** is one of eight, and a `from` naming anything else is a publish error (check 56):

| Source | One row per | Row members |
|---|---|---|
| `<Type>` | current object of the type or family | the type's members, `.id`, `.state`, `.open`, `.created_at`, `.created_by_kind`, `.declaration_version`, and the object functions `o.entered_at(…)` and `o.time_in(…)`, and the collections `o.intervals`, `o.intervals(<member>)`, `o.transitions` and `o.attempts` |
| `<Kind>`, an observation kind | observation nothing has corrected | its fields, `.subject`, `.corrects`, `.occurred_at`, `.recorded_at`, `.recorded_by_kind`, `.declaration_version` |
| `<Type>.labels` | label on an object of the type | `.subject`, `.name`, `.subject_state`, `.occurred_at`, `.recorded_at`, `.recorded_by_kind`, `.declaration_version`; the note may only be counted |
| `<Type>.intervals` | span in one state | `.object`, `.state`, `.entered_at`, `.left_at`, `.duration`, `.entered_by_kind`, `.declaration_version`, `.legacy`, `.held(<member>)` |
| `<Type>.intervals(<member>)` | span holding one value of a tracked member | as above, with `.value` in place of `.state` |
| `<Type>.transitions` | event of a transition on the type | `.object`, `.transition`, `.from_state`, `.to_state`, `.completes`, `.returns`, `.occurred_at`, `.recorded_at`, `.actor_id`, `.actor_kind`, `.asserted`, `.imported`, `.migrated`, `.reason`, `.declaration_version`, `.held(<member>)` |
| `<Type>.attempts` | request that did not apply, or an observing clause's would-be refusal | `.object`, `.transition`, `.verdict`, `.clause`, `.remedy`, `.unknown`, `.enforced`, `.actor_id`, `.actor_kind`, `.at`, `.declaration_version` |
| `<Type>.attempt_counts` | UTC day, object, and each combination of the members | `.day`, `.object`, `.transition`, `.verdict`, `.clause`, `.remedy`, `.actor_kind`, `.enforced`, `.declaration_version`, `.count` |

`.duration` is a span's exit less its entry. A span still current runs to `now` while its object is open. On an object now in a `closed` or terminal state, one that began before the object entered that state runs to that entry, and one that began after it, like a span in that state itself, has no duration; so nothing accrues on a finished object, and no duration is ever negative (ADR-0101). An aggregate leaves out a row whose body is absent, so time in a final state is never counted and an engineer still named on a closed job is not accruing time (ADR-0101). The datasets are those of ADR-0083 and DESIGN.md §7; an attempt row has no inputs, because the attempt log records none. The members ADR-0096 added answer the questions the PRD's use cases ask of them:
- `.held(<member>)` is the value a tracked member held when the span began or the transition happened, so time in a state is attributed to whoever was assigned then, not to whoever is now (UC-18);
- `.created_at` and `.created_by_kind` come from an object's creation event, so work can be split by the kind of actor that created it (UC-16);
- `.imported` marks the import's events, which keep the provenance `asserted` that ADR-0015 requires, so an override count can exclude them (UC-3);
- `.reason` is an assertion's `reason` input where it is a declared enum. Free text is not a dimension, since it groups nothing and may hold personal data, so a type that wants overrides counted per reason declares its reasons (UC-3);
- `.migrated` marks a publish's migration events, which are not flow either (ADR-0101);
- `.open` is true until the object enters a `closed` or terminal state (§1); `.completes` marks the transition that first does so, which is what throughput and cycle time count, and `.returns` a transition that changes state into one the object held before, which is rework — an `act` is never one (ADR-0098, ADR-0101);
- `.actor_id` is the requesting actor's id on a transition or attempt row, and `<reference>.actor_id` reads the `actor` identity of the object a reference names, so "someone other than the assignee acted" is `t.actor_id != t.held(engineer).actor_id` (ADR-0098);
- every source's rows carry `.declaration_version`, so any metric can be split by version (PRD M3);
- `<Type>.attempt_counts` holds the attempt log's daily counts per object, which are kept permanently while its rows are pruned after the deployment's retention period, so refusals can be counted per week over the whole history (PRD T3, UC-2). Keeping the object keeps visibility applicable after a prune; a refused creation has no object, and counts as a row only a reader who can see every current object of the type can see. A tracked member is the state, an enum attribute or a singular stored relationship end, and `intervals(<member>)` naming anything else is a publish error (check 58). A personal enum is tracked, and erasure redacts its values in the interval index as in the events; as a personal value, it may be counted and never be a dimension (ADR-0096).

**Dimensions** are expressions over the binder: a path of at most two hops, which is as far as any guard in the first consumer reaches (DESIGN.md §5.7), a kind of actor, a declaration version, or a time bucket — `day`, `week`, `month`, `quarter` or `year` of a timestamp, in UTC calendar time. A path's hops are counted from the row's object, so `.object` on a dataset row is not a hop, and a dimension path longer than two hops is a publish error (check 56). **A set-valued dimension** — a path through a set, such as a delivery's configurations — counts the row once under each member (ADR-0098, PRD UC-1). A path that reaches an object the reader cannot see yields absence at that step, so the row is counted under an absent value and nothing of the hidden object is read. A stored reference is a value of the object that holds it, so a reader who may see that object sees its id; the referenced object's own members are what visibility protects (ADR-0096, PRD T5). A filter follows the same rule.

**The value** combines aggregates arithmetically. In a metric the aggregates range over the rows of one group, so they take no binder of their own: `count()`, `count(where <filter>)`, and `sum`, `min`, `max`, `avg` and `median` of a body, and `percentile(<p>, <body>)`, and `count(distinct <body>)`, which counts distinct values, so "how many jobs carry the label" counts jobs rather than labels; `avg` yields a `decimal`. **A row's own flow data is a collection** an aggregate in the row may range over — `o.intervals(engineer)`, `o.transitions` — which is how a per-object measure is summarised across objects: handoffs per job are `avg(count(i in o.intervals(engineer) where i.value is not null) - 1)` (ADR-0098). `avg`, `median`, `percentile` and the time buckets exist in a metric alone, and one used anywhere else is a publish error (check 56). §8.3's arithmetic applies unchanged, which is why the pass rate above multiplies by `1.000`: `int / int` discards the remainder.

**A flag** is a named condition over `value` and the dimensions, declared once so that a scheduler or an agent queries the flag rather than restating the threshold (PRD C2). A business exception across many objects is a flag (DESIGN.md §5.12).

**A combined metric** reads other metrics by their dimensions, which is how a rate divides one dataset by another without a join (ADR-0098):

```text
metric delivery_refusal_rate version 1 {
  combine   refused = Delivery.refusals, applied = Delivery.transition_counts
  by        transition, week
  value     refused * 1.000 / (refused + applied)
}
```

Each name in `combine` is a declared or standard metric. `by` lists dimension names the composite groups by. A combined metric that lacks one of them contributes its value to every group that differs only in that dimension, and one it has rows for aggregates over any of its own dimensions the composite does not list. A group a combined metric has no rows for reads as its value over no rows: zero for a `count` or `sum`, `unknown` otherwise. A combined metric may not name itself, directly or through another (check 56).

**A derived attribute in a row is read under the reader's visibility**: its aggregates range over the objects the reader can see, and the types it reads count toward the result's `complete` flag, so a metric over a derived stock count neither counts hidden units nor claims to be complete when it does not see them (ADR-0098).

**Personal values stay out.** A personal attribute or field may be counted, and one used as a dimension, a flag's operand, or in a value other than a count is a publish error (check 56). Erasure therefore needs nothing from any metric.

**A metric is computed on read over the rows the reader can see**, stores nothing, and is retroactive by construction (DESIGN.md §5.12). **The result says whether it is complete**: complete when the reader can see every current object of each type the metric reads, and otherwise a value over the reader's own rows, marked as such. Every reader who gets a complete value gets the same one from one definition, and no one takes a partial value for it (PRD C2); a reader who sees part of the data still gets metrics over that part (PRD M2); and nothing hidden shapes a value (PRD T5) (ADR-0096). A `filter` passed to `metric()` narrows the rows before they are aggregated. A declared metric and a type's standard ones (DESIGN.md §5.12, §5.13) are read the same way, by `metric()` and in the rule set.

**In a guard**, a metric is referenced with its dimension values bound from the object, the inputs and any loop binder in scope — so a delivery's guard can read each unit's model's rate inside `all(u in units: …)` — and optionally a window and a freshness bound:

```text
require pass_rate: metric(inspection_pass_rate, engineer := engineer, over last 30 days) >= 0.900 because dependent
```

- `over last <duration>` restricts the rows to those whose `window on` timestamp falls within that long before `now`; `over last` on a metric that declares no `window on` is a publish error (check 56).
- A declared dimension the reference does not bind is aggregated over: the value is computed over every row matching the bound ones. So a guard can read a monthly metric over its window without a second definition (PRD C2).
- The result has the value's type, and is `unknown` where no row matches and the value divides by a count of zero (§8.2), so a guard over a metric with no data is unsatisfied and says `unknown`.
- A refusal by the clause carries the value in the verdict's `consulted`, given to a requester whose view of it would be complete, as a later reader of the record is, and otherwise withheld: where T5 and UC-10 meet, T5 is a Must and UC-10 a hypothetical case (ADR-0095, ADR-0096). The clause, which the verdict names, holds the threshold. The attempt log records the value too, so a refused decision re-evaluates from the record as an applied one does (ADR-0096, PRD UC-10).
- A metric in a guard is computed over every row its source holds, not under the requester's visibility, as a type-scan guard reads every object of its type: a rule whose verdict depended on who asked would not be a rule. The value and its as-of time are recorded on the event, and a later reader is shown the value only if their view of it would be complete: they can see every current object of each type the metric reads (ADR-0084, ADR-0095, ADR-0096).
- It is consulted before the transaction, as an evaluator is, and its arguments are resolved again inside it; a mismatch means the world moved, and the request consults again within its retry bound (ADR-0084, ADR-0092). An argument that reads a value the calling outcome writes could never agree before the transaction, so it is a publish error (check 61).
- `fresh <duration>`, written last, refuses a value older than the bound when the transaction commits, as `temporal`, as a stale evaluator verdict is refused.

**`metric(…)` is usable only in a `require` clause** of a `create`, `do` or `act`, and anywhere else — an invariant, a derivation, a visibility predicate, an outcome value or another metric — is a publish error (check 56). ADR-0084 rules out the invariant. The rest would carry the value past the two rules that make a metric guard sound: its arguments resolved twice, and its value recorded on the event. Where a sweepable transition or a person's judgement needs the value, a declared transition writes it into an attribute and guards read the attribute (ADR-0084 §7). A transition with a metric guard is not sweepable, and the publish report says so (§10).

**A reader reads a metric as a guard does** (ADR-0098), with one difference in what it gets back. `metric()` takes the same arguments a guard's reference does — dimension values to bind and an `over last` window — and a `filter`, a boolean expression over the declaration's own binder that narrows the rows before they are aggregated. A guard needs one value, so it aggregates over every dimension it leaves unbound. A reader keeps every declared dimension by default and gets one row per group, and may name the dimensions to keep with `keep`, aggregating over the rest (ADR-0101). So a screen, an agent and a rule read one value from one definition, the person a guard refused can read the value it was decided on, and a reader asking which jobs changed hands gets one row per job (PRD C2, UC-10, UC-11, UC-18).

### 6.10 Assignment

```text
type User version 1 {
  tracking record
  states   ACTIVE category live, LEFT category closed terminal
  attr     login identity actor unique
  create add -> ACTIVE accepts login { require may: actor.has(SERVICE_ASSIGN) because delegable }
  do leave ACTIVE -> LEFT { require may: actor.has(SERVICE_ASSIGN) because delegable }
}

type ServiceJob version 2 {
  tracking record
  states   OPEN category inbound, WORKING category live,
           DONE category closed terminal
  ref      engineer : User assignee
  attr     photo file?
  labels by actor.has(SERVICE_EDIT)
  create open -> OPEN accepts engineer { require may: actor.has(SERVICE_EDIT) because delegable }
  act reassign at { OPEN, WORKING } accepts engineer {
    require may:    actor.has(SERVICE_ASSIGN) because delegable
    require active: inputs.engineer.state == User.ACTIVE because self_serviceable
  }
  do start OPEN -> WORKING backdatable within 2 days {
    require mine: engineer.login == actor.id because delegable
  }
  do finish WORKING -> DONE accepts photo {
    require mine:        engineer.login == actor.id because delegable
    require none_failed: none(r in inspections where r.outcome == CheckOutcome.FAIL
                                and none(s in inspections where s.check == r.check
                                                            and s.recorded_at > r.recorded_at))
    require photo_taken: inputs.photo is not null observe because self_serviceable
  }
}
```

**`assignee`** marks a singular stored `ref` as the object's responsibility: who is answerable for it at a given time, as distinct from whoever acts on it (ADR-0086, PRD D10). Its target type marks exactly one `identity` attribute **`actor`**, holding the actor's id, so the engine can tell when someone other than the assignee acts, and a guard compares the same attribute to require the assignee (`engineer.login == actor.id`). `assignee` on a set-valued `ref`, a `part`, an `owner` or an end that does not store its value (§3.3), or on a reference whose target does not mark exactly one `actor` attribute, is a publish error, and so is `actor` on an attribute that is not `identity` (check 59). A type may mark more than one reference, such as an engineer-of-record and a reviewer, and each is a separate dimension named by its reference.

**The marking adds no write path.** Assignment is whatever declared transitions write the reference, each with its own guards on who may assign and who may be assigned — `reassign` above. Every singular stored reference has intervals from its object's creation, absence included, marked or not (DESIGN.md §5.2), so a marking added in a later version reads assignment history back to the first. What the marking adds is the standard assignment metrics over that reference (DESIGN.md §5.13).

`finish` shows the other constructs in place: `none_failed` reads the observations of §6.8, `photo_taken` is on trial (§5.1), and `start` may be recorded up to two days late (§4.2).


### 6.11 The standard metrics

Every type has these without declaring any, and every assignee reference has the second table's (PRD M1, M6, ADR-0098). They are declarations like any other, given here over a type parameter `<T>` and, for assignment, a reference parameter `<r>`; the block after the tables instantiates them for `ServiceJob` and its `engineer`, and the checker runs it as it runs every example. Each is read by name — `<T>.<name>`, or `<T>.<name>.<r>` for an assignment metric — through `metric()` and the metrics tool, and printed with the type in the rule set. A declared metric is named without a type and a standard one with its type, so the two never collide.

| Name | Source and filter | Dimensions | Value |
|---|---|---|---|
| `time_in_state` | `i in <T>.intervals` | `state = i.state`, `month = month(i.entered_at)` | `median(i.duration)` |
| `time_in_state_p80` | as `time_in_state` | as `time_in_state` | `percentile(0.8, i.duration)` |
| `throughput` | `t in <T>.transitions where t.completes and not t.imported and not t.migrated` | `week = week(t.occurred_at)`, `actor_kind = t.actor_kind` | `count()` |
| `work_in_progress` | `o in <T> where o.open` | `state = o.state` | `count()` |
| `oldest_open` | `o in <T> where o.open` | `state = o.state` | `max(now - o.entered_at(state))` |
| `transition_counts` | `t in <T>.transitions where not t.imported and not t.migrated` | `transition = t.transition`, `actor_kind = t.actor_kind`, `week = week(t.occurred_at)` | `count()` |
| `refusals` | `a in <T>.attempt_counts where a.enforced` | `transition = a.transition`, `clause = a.clause`, `remedy = a.remedy`, `actor_kind = a.actor_kind`, `week = week(a.day)` | `sum(a.count)` |
| `refusal_rate` | `combine refused = <T>.refusals, applied = <T>.transition_counts` | `transition`, `clause`, `actor_kind`, `week` | `refused * 1.000 / (refused + applied)` |
| `override_counts` | `t in <T>.transitions where t.asserted and not t.imported and not t.migrated` | `state = t.to_state`, `reason = t.reason`, `week = week(t.occurred_at)` | `count()` |
| `rework` | `t in <T>.transitions where t.returns and not t.imported and not t.migrated` | `state = t.to_state`, `week = week(t.occurred_at)` | `count()` |

| Name | Source and filter | Dimensions | Value |
|---|---|---|---|
| `open_work` | `o in <T> where o.open` | `assignee = o.<r>` | `count()` |
| `time_unassigned` | `i in <T>.intervals(<r>) where i.value is null` | `month = month(i.entered_at)` | `median(i.duration)` |
| `time_to_first_assignment` | `o in <T> where any(i in o.intervals(<r>) where i.value is not null)` | `month = month(o.created_at)` | `median(min(i in o.intervals(<r>) where i.value is not null: i.entered_at) - o.created_at)` |
| `time_with_assignee` | `i in <T>.intervals(<r>) where i.value is not null` | `assignee = i.value` | `sum(i.duration)` |
| `cycle_time_by_assignee` | `t in <T>.transitions where t.completes and not t.imported and not t.migrated` | `assignee = t.held(<r>)`, `month = month(t.occurred_at)` | `median(t.occurred_at - t.object.created_at)` |
| `time_in_state_by_holder` | `i in <T>.intervals` | `state = i.state`, `assignee = i.held(<r>)` | `median(i.duration)` |
| `handoffs` | `o in <T> where any(i in o.intervals(<r>) where i.value is not null)` | `month = month(o.created_at)` | `avg(count(i in o.intervals(<r>) where i.value is not null) - 1)` |
| `reassigned_back` | `o in <T>` | `month = month(o.created_at)` | `count(where count(distinct i in o.intervals(<r>) where i.value is not null: i.value) < count(i in o.intervals(<r>) where i.value is not null))` |
| `acted_by_non_assignee` | `t in <T>.transitions where t.held(<r>) is not null and not t.imported and not t.migrated` | `transition = t.transition` | `count(where t.actor_id != t.held(<r>).actor_id) * 1.000 / count()` |
| `handoffs_by_object` | `o in <T>` | `object = o.id` | `sum(count(i in o.intervals(<r>) where i.value is not null) - 1)`; flag `changed_hands_more_than_once when value > 1` |
| `returns_by_object` | `o in <T>` | `object = o.id` | `sum(count(i in o.intervals(<r>) where i.value is not null) - count(distinct i in o.intervals(<r>) where i.value is not null: i.value))`; flag `returned_to_earlier when value > 0` |

**Every standard metric also has the dimensions `version` and `actor_kind`**, taken from its rows — `declaration_version`, and the kind of actor that created, entered, requested or recorded — which a reader drops with `keep` to aggregate over them, since a reader keeps every dimension by default; a combined one passes them through from the metrics it combines (PRD M3, M4, ADR-0101). **None counts the import's events or a publish's migrations**, which are the port and the publish, not the flow: a ported object's history enters through its legacy intervals, marked as such. `handoffs_by_object` and `returns_by_object` name the objects, so `metric()` returns the jobs that changed hands more than once or went back to an earlier assignee, with the flag saying which (PRD UC-18). `acted_by_non_assignee` reads `.actor_id` against the assignee's `actor` identity, which check 59 guarantees the target type marks.

The same definitions for `ServiceJob` and its `engineer`, named here without the `ServiceJob.` prefix and, for the assignment metrics, the `.engineer` suffix:

```text
metric time_in_state version 1 {
  from      i in ServiceJob.intervals
  by        state = i.state, month = month(i.entered_at), version = i.declaration_version, actor_kind = i.entered_by_kind
  value     median(i.duration)
}

metric time_in_state_p80 version 1 {
  from      i in ServiceJob.intervals
  by        state = i.state, month = month(i.entered_at), version = i.declaration_version, actor_kind = i.entered_by_kind
  value     percentile(0.8, i.duration)
}

metric throughput version 1 {
  from      t in ServiceJob.transitions where t.completes and not t.imported and not t.migrated
  by        week = week(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind
  value     count()
}

metric work_in_progress version 1 {
  from      o in ServiceJob where o.open
  by        state = o.state, version = o.declaration_version, actor_kind = o.created_by_kind
  value     count()
}

metric oldest_open version 1 {
  from      o in ServiceJob where o.open
  by        state = o.state, version = o.declaration_version, actor_kind = o.created_by_kind
  value     max(now - o.entered_at(state))
}

metric transition_counts version 1 {
  from      t in ServiceJob.transitions where not t.imported and not t.migrated
  by        transition = t.transition, week = week(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind
  value     count()
}

metric refusals version 1 {
  from      a in ServiceJob.attempt_counts where a.enforced
  by        transition = a.transition, clause = a.clause, remedy = a.remedy, week = week(a.day), version = a.declaration_version, actor_kind = a.actor_kind
  value     sum(a.count)
}

metric refusal_rate version 1 {
  combine   refused = ServiceJob.refusals, applied = ServiceJob.transition_counts
  by        transition, clause, week, version, actor_kind
  value     refused * 1.000 / (refused + applied)
}

metric override_counts version 1 {
  from      t in ServiceJob.transitions where t.asserted and not t.imported and not t.migrated
  by        state = t.to_state, reason = t.reason, week = week(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind
  value     count()
}

metric rework version 1 {
  from      t in ServiceJob.transitions where t.returns and not t.imported and not t.migrated
  by        state = t.to_state, week = week(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind
  value     count()
}

metric open_work version 1 {
  from      o in ServiceJob where o.open
  by        assignee = o.engineer, version = o.declaration_version, actor_kind = o.created_by_kind
  value     count()
}

metric time_unassigned version 1 {
  from      i in ServiceJob.intervals(engineer) where i.value is null
  by        month = month(i.entered_at), version = i.declaration_version, actor_kind = i.entered_by_kind
  value     median(i.duration)
}

metric time_to_first_assignment version 1 {
  from      o in ServiceJob where any(i in o.intervals(engineer) where i.value is not null)
  by        month = month(o.created_at), version = o.declaration_version, actor_kind = o.created_by_kind
  value     median(min(i in o.intervals(engineer) where i.value is not null: i.entered_at) - o.created_at)
}

metric time_with_assignee version 1 {
  from      i in ServiceJob.intervals(engineer) where i.value is not null
  by        assignee = i.value, version = i.declaration_version, actor_kind = i.entered_by_kind
  value     sum(i.duration)
}

metric cycle_time_by_assignee version 1 {
  from      t in ServiceJob.transitions where t.completes and not t.imported and not t.migrated
  by        assignee = t.held(engineer), month = month(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind
  value     median(t.occurred_at - t.object.created_at)
}

metric time_in_state_by_holder version 1 {
  from      i in ServiceJob.intervals
  by        state = i.state, assignee = i.held(engineer), version = i.declaration_version, actor_kind = i.entered_by_kind
  value     median(i.duration)
}

metric handoffs version 1 {
  from      o in ServiceJob where any(i in o.intervals(engineer) where i.value is not null)
  by        month = month(o.created_at), version = o.declaration_version, actor_kind = o.created_by_kind
  value     avg(count(i in o.intervals(engineer) where i.value is not null) - 1)
}

metric reassigned_back version 1 {
  from      o in ServiceJob
  by        month = month(o.created_at), version = o.declaration_version, actor_kind = o.created_by_kind
  value     count(where count(distinct i in o.intervals(engineer) where i.value is not null: i.value)
                  < count(i in o.intervals(engineer) where i.value is not null))
}

metric acted_by_non_assignee version 1 {
  from      t in ServiceJob.transitions where t.held(engineer) is not null and not t.imported and not t.migrated
  by        transition = t.transition, version = t.declaration_version, actor_kind = t.actor_kind
  value     count(where t.actor_id != t.held(engineer).actor_id) * 1.000 / count()
}

metric handoffs_by_object version 1 {
  from      o in ServiceJob
  by        object = o.id, version = o.declaration_version, actor_kind = o.created_by_kind
  value     sum(count(i in o.intervals(engineer) where i.value is not null) - 1)
  flag      changed_hands_more_than_once when value > 1
}

metric returns_by_object version 1 {
  from      o in ServiceJob
  by        object = o.id, version = o.declaration_version, actor_kind = o.created_by_kind
  value     sum(count(i in o.intervals(engineer) where i.value is not null)
                - count(distinct i in o.intervals(engineer) where i.value is not null: i.value))
  flag      returned_to_earlier when value > 0
}
```

## 7. Quantity tracking

```text
type Order version 1 {
  tracking serial
  states   PLACED category live, SHIPPED category closed terminal

  attr placed_at timestamp

  create place -> PLACED accepts placed_at {
    input stock : Stock
    input qty   : int
    require may: actor.has(INVENTORY_CREATE) because delegable
    call inputs.stock.reserve(qty := inputs.qty)
  }
  do ship PLACED -> SHIPPED { require may: actor.has(INVENTORY_CREATE) because delegable }
}

type Stock version 1 {
  tracking quantity
  states   ACTIVE category live, CLOSED category closed terminal

  ref     model : ProductModel inverse stock
  counter on_hand  indexed
  counter reserved indexed

  derive available = on_hand - reserved indexed
  invariant reserved_bounded: reserved >= 0 and reserved <= on_hand

  create open -> ACTIVE accepts model {
    require may: actor.has(INVENTORY_CREATE) because delegable
  }
  act reserve at ACTIVE only via Order.place {
    input qty : int
    require enough: on_hand - reserved >= inputs.qty because dependent
    set reserved := reserved + inputs.qty
  }
  do close ACTIVE -> CLOSED {
    require empty: on_hand == 0 because dependent
    require may:   actor.has(INVENTORY_RETIRE) because delegable
  }
}
```

A `quantity` type declares at least one `counter`. A `derive` may be `indexed` when it reads only stored indexed attributes of the same object and no clock, which is what makes a low-stock query possible.

`reserved_bounded` is a **local** invariant, reading only this object.

## 8. Expressions and types

### 8.1 The types

Every attribute type of §3.1, and four more that only expressions have, plus `invariant`:

| Type | Values | Members |
|---|---|---|
| a **reference** | an object of a named type | `.id`, and every attribute, counter, derivation and relationship end that type declares |
| **`state`** | a member of one machine's state set | `.category` |
| **`category`** | one of the declared categories | none |
| **`verdict`** | what an evaluator returns | none; usable only as a whole guard clause, or under a single `not` (check 24) |

`invariant` is the type of a name in an `admits` input. `identity` is an ordinary scalar, the type of `.id` and of `actor.id`, which is why an approver is stored in one.

**A member of a family satisfies its base's type** wherever a reference type is expected — an input, a relationship end, a `create` argument — so a machine shared by two payment types may pass `this` to something declared over their common base. Nothing goes the other way: a base-typed value does not satisfy a subtype.

**Every object has `.state`**, of type `state`, alongside its declared members and counting as one wherever a rule says *member* — the swap test of §3.4 and the resolution order of §9.2 both reach it. It is readable, never writable, and always available to a query filter, a type-scan and a visibility predicate without being marked `indexed`. The same holds for a stored relationship end, which the store indexes because it is how the object is found. `indexed` is therefore a marking for attributes, counters and derivations only.

**Every object also has `.created_at`**, a `timestamp`, and **`.created_by_kind`**, the kind of actor whose request created it, both taken from its creation event, or from the import mapping for an object ported from the legacy system (`publish-and-import.md` §4), and readable wherever `.state` is (ADR-0096). A member declared as `id`, `state`, `created_at` or `created_by_kind` would shadow one of them, and is a publish error (check 33). They are what a cycle time is measured from and what work is split by when people and agents both create it.

**Every object also has `.open`**, a `bool`, true until it enters a `closed` or terminal state (§1), which is what work in progress and the oldest open work count. `entered_at(state)` is when it entered its current state, so "oldest in its current state" can be filtered and ordered by one query (ADR-0098, PRD UC-1). **Every mirror object also has `.imported_at`**, the time its last import wrote it, which a guard reading a mirror may bound: `require fresh: customer.imported_at + 1 h >= now because temporal` (ADR-0100). A member declared as `open` or `imported_at` would shadow one of these, and is a publish error (check 33).

**Literals.** A state of another type is `<Type>.<STATE>`; an enum member is `<Enum>.<MEMBER>`; an invariant on another type, which only `may admit` and an `admits` argument name, is `<Type>.<invariant>`. All three are qualified for the same reason, that a bare name would be ambiguous across declarations. A bare state name of *this* type's machine is legal and resolves by §9.2. A category is written bare, since categories are one global vocabulary. Also `42`, `19.99`, `30 min`, `USD 19.99`, `"text"`, `true`, `false`, `{A, B}`. A bare number with no point is an `int`. One with a point is a `decimal`, and it takes the scale of whatever it is compared or combined with, so `balance > 0.5` types against a `decimal(10,4)` without the literal having to be written to four places.

`actor` has the shape of DESIGN.md §5.8's descriptor, and a declaration may read four members of it: `.id` of type `identity`, `.kind` of a fixed enum, `.principal` of type `string`, and `.has(<capability>)` yielding `bool`. The descriptor has no other members: it carries no declared types, so nothing else about an actor can be read by a guard, and a fact with a value lives on an object the guard reads (ADR-0079). `this` is a reference to the object being transitioned, so `this.id` is its identity and is what an evaluator is passed when it must name the object.

`referrers` (§6.5) is a set whose elements are of **no single type**, so an element carries `.id`, `.state` and `.type`, and nothing else. `.type` compares against a declared type name, which is enough for a deletion guard to treat one referencing type differently — notes that do not block, delivery items that do — without opening member access on a value whose type is not known until runtime. Open question 2 asked for full filtering; ADR-0072 gave the type name instead, as the smallest thing that answers the deletion guard. `.state` on one carries `.category` but cannot be compared to a `<Type>.<STATE>` literal, because which machine it belongs to is not known until runtime. That is exactly enough to write a deletion guard and not enough to write anything that would need the machine.

### 8.2 Absence, and why a guard can be unknown

An expression yields a value of its type **or `unknown`**. Reading an absent optional attribute yields `unknown`, and so do reading an unsupplied optional input and dividing by zero.

- Any operator with an `unknown` operand yields `unknown`, with three exceptions.
- `is null` and `is not null` always yield `bool`. They are the only way to test presence.
- `and` yields `false` if either side is `false`, even when the other is `unknown`. `or` yields `true` if either side is `true`.

The four places an expression is evaluated each need an answer, and they are not the same answer:

| Where | An `unknown` result means |
|---|---|
| a **guard** | unsatisfied, and the verdict says `unknown` rather than `false`, so a caller can tell "not yet known" from "no" |
| an **invariant** | it holds. This is what a database `CHECK` does, and the only rule under which a partially filled object is workable |
| a **`visible when`** predicate | not visible. Visibility fails closed, because the alternative discloses an object on the strength of a value nobody has |
| a **derivation** | absent, so a reader of the derived attribute sees the same absence that produced it |

This is the trap the language most invites, so it is worth stating in the shape it arrives in. `derive serious = severity == Severity.SEVERE or outcome == Outcome.FATAL`, where `outcome` is absent until the case closes, is `unknown` for a mild case rather than `false`. A later guard `require routine: not serious` is then `unknown`, and the transition can never be taken by anyone. Write the presence test: `severity == Severity.SEVERE or (outcome is not null and outcome == Outcome.FATAL)`. Publishing reports, without failing, every guard whose value can be `unknown` through an optional it does not test.

### 8.3 Operators

- `+` and `-` take two operands of one type: `int`, `decimal` of one scale, `money` of one currency, or `timestamp - timestamp` yielding a `duration`; `timestamp ± duration` yields a `timestamp`. On `decimal(p1,s)` and `decimal(p2,s)` the result is `decimal(max(p1,p2), s)`.
- `*` and `/` take a scalar — `int` or `decimal` — on at least one side. With one non-scalar operand the result is that operand's type, scale preserved. With two scalars, `int * int` is `int`, `decimal(p1,s1) * decimal(p2,s2)` is `decimal(p1+p2, s1+s2)`, and `int * decimal(p,s)` is `decimal(p+19, s)`. `money * money` is an error, as is `duration * duration` (check 24).
- `/` takes its scalar on the **right** and yields the left operand's type, so `money / int` is money and `int / money` is an error (check 24). **`int / int` is `int` and discards the remainder**, which is stated because it is a trap in any domain where the quotient is money; divide a `decimal` if you need the fraction. Division by zero yields `unknown` (§8.2), never an error, so a guard containing one is unsatisfied rather than a failed request.
- A `set` whose result **scale** differs from the target's is a publish error (check 17), except that a product or quotient rounds half to even to the target's scale, as money does. Rounding is permitted exactly where it is unavoidable and refused where it would be silent loss: addition of two scales is already an error, so nothing is quietly truncated. Precision is checked at runtime instead, and a value that overflows its declared precision refuses the request the way an over-limit loop does. Making precision a publish rule looked stricter and meant that `set total := total + inputs.amount` on a `decimal(8,2)` could never publish, since every addition widened the result. A running total is the commonest thing a decimal is for.
- Comparison needs both sides of one type and is not associative. `is null` and `is not null` take anything.
- `and`, `or`, `not`, `implies` take and yield `bool`. An evaluator call yields a `verdict`, usable **only as a whole guard clause** or under a single `not` (check 24), so a guard is a `bool`, an evaluator call, or a negated evaluator call, and never a mixture. A conditional external check is therefore written as two transitions with opposite guards, and `not` is what lets the second one exist: without it a decline could not be gated on the authority having said no, only on nobody having asked. A verdict too stale to use is refused as `temporal` whichever way it is written, since `not stale` is not `satisfied`.
- `count` yields `int`, `sum`/`min`/`max` the element type, `all`/`any`/`none` `bool`.
- `changed_since([<attribute>, …], <event expression>)` yields `bool`. Each name is an attribute of `this`, **or a part relationship**, which means "any change to any of those parts" — without which an approval on a whole cannot be invalidated by an edit to one of its lines, the single most-cited guard in the model. An observation kind's part is a part relationship like any other, so a sign-off naming `inspections` is invalidated by a later result (ADR-0082).
- `entered_at(<STATE>)` yields the `timestamp` at which the object most recently entered that state, `unknown` if it never has; `entered_at(<member>)`, for a tracked member, when it took its current value; and `time_in(<STATE>)` a `duration`, the total over every span in that state, a current one counted up to `now`. They read the object's own intervals (ADR-0083, ADR-0084). A tracked member is the state, an enum attribute or a singular stored relationship end, and naming anything else is a publish error (check 58). `time_in` reads the clock, so an invariant may not use it (check 32), and neither may an `indexed` derivation (check 46); `entered_at` does not. A query may filter on `entered_at(<member>)`, whose current value is stored and indexed, so a per-object ageing condition such as `unit is null and entered_at(unit) + 14 days <= now` is found by one query (ADR-0084, PRD M7).
- `metric(<name>, <dimension> := <expression>, … [, over last <duration>] [, fresh <duration>])` yields a declared metric's value, and is usable only in a guard (check 56, §6.9).
- `if <bool> then <a> else <b>` yields the common type of `a` and `b`, and is allowed in a derived attribute and in the value expression of an outcome step. It is not allowed in a guard (check 21), where a conditional would hide which clause failed. Allowing it in an outcome value is what stops a single two-valued choice — a debit or a credit, a rise or a fall — from splitting into two transitions and then splitting every caller of them in turn.
- `in` is membership: an element on the left, and on the right a collection literal, a set-valued member, or an enum name meaning any of its members.

Aggregates are `agg(<name> in <collection> [where <filter>][: <body>])`. The body is required for `sum`, `min` and `max`, and optional for `count`, `all`, `any` and `none`. `count(distinct <name> in <collection> [where <filter>]: <body>)` counts the distinct values of the body (ADR-0098).

**Precedence**, highest first: paths and calls; unary `not`; `* /`; `+ -`; comparison, `in`, `is null`; `and`; `or`; `implies`; `if`. Binary operators are **left-associative**, except `implies`, which is right-associative; comparison does not chain.

**Units.** A duration literal is a number and one of `s`, `min`, `h`, `days`, `weeks` — a closed set, with no months or years, because neither has a fixed length and a guard that silently changes meaning in February is worse than one that cannot be written.

**Money.** `money(ccy)` names an ISO 4217 three-letter code, and its scale is that currency's minor unit: two for `USD`, zero for `JPY`, three for `BHD`. The currency is fixed at declaration, so one attribute holds one currency and a value of another is a type error rather than a silent conversion; a model whose currency is chosen at runtime was open question 9, and ADR-0068 refused it: a declaration-time currency makes a mismatch a publish error rather than a production one. Addition and subtraction require the same currency. Multiplying or dividing money by a scalar **rounds half to even** to the currency's minor unit, stated rather than left to a backend because the alternative is a rounding difference that appears as a reconciliation break months later. Half to even is the rule for financial arithmetic because, unlike half up, it does not bias a long run of roundings upward. Splitting an amount into parts that must sum back exactly is not expressible; that is an allocation, and it belongs to the consumer that decides the split.

## 9. Lexical rules

### 9.1 Where a clause ends

A clause ends at the end of a line, **unless the next line is indented more deeply than the line the clause began on**, in which case it continues. One rule, and it covers every wrapped construct in this document: a comma-separated list, a guard that runs past the margin, a transition head with a long `only via` list, and a derivation whose operator opens the next line rather than closing the previous one.

```text
capability INVENTORY_CREATE, INVENTORY_RETIRE,
           ERASE_PERSONAL                     # continues: indented deeper

derive leasable = state == DEVELOPMENT
              and none(l in engagement_lines where l.open)   # continues

do ship    REQUESTED   -> PROCUREMENT { }
do receive PROCUREMENT -> INTAKE      { }     # a new clause: same indent
```

Iteration 10 said instead that a line continues while it **ends** in an operator or an open bracket. Seven lines of this document broke that rule and every comma-separated list broke it, because the operator that joins two lines usually opens the second one. Indentation is what the examples were already doing.

Inside a `{ … }` block the same rule applies to the block's own clauses. For it to be decidable, **a body that does not close on its opening line begins its clauses on the next line**:

```text
do ship REQUESTED -> PROCUREMENT { require may: actor.has(EDIT) because delegable }

act tick at ACTIVE {                       # opens and does not close, so the body starts below
  require may: actor.has(DELIVERY_EDIT) because delegable
  set checked := true
}
```

A one-line body is fine, since nothing after it can be a continuation. What is not allowed (check 51) is a body whose first clause sits on the `{` line and whose second clause sits below it: the second is indented deeper than the first began, so the rule would read it as continuing the first, and the guard would swallow the write that follows it.

### 9.2 Names

A name is a letter or `_` followed by letters, digits or `_`. **Reserved words may be used as names**, because a name's position in the grammar always determines that it is one. Three exceptions, each a genuine collision rather than a precaution:

- a **state** may not be named `any`, `terminal` or `superseding`, which would collide with the wildcard and the state markings (check 33);
- a **category** may not be named `any`, `terminal` or `superseding`, for the same reason (check 33);
- a **transition** may not be named `any` (check 33).

A bare name in an expression resolves in this order, the first match winning:
1. a loop binder in scope;
2. one of `inputs`, `actor`, `this`, `now`, `referrers`, `this_event`;
3. `state`, or an attribute, counter, derivation or relationship end of `this`, inherited ones included;
4. a state of this type's machine;
5. a category.

Outside an expression a name resolves to a declaration **of the kind the grammar requires in that position** — a machine after `machine`, a sequence after `from`, a type after `:`, an evaluator before a `.` in a guard call, an invariant after `requires invariant` — which is why those kinds appear in check 19 and not in the list above.

The order is a resolution rule, not a disambiguation rule: publishing rejects a type whose state names collide with its member names, or a category colliding with either, so no declaration reaches the ambiguous case (check 33). An aggregate is distinguished from a member of the same name by the `(` that must follow it, which is why `count` and `min` are legal attribute names.

### 9.3 Symbols

`->` is a to-state, an assertable-state set and a migration mapping; never implication, never a reference type. `implies` is implication. `inputs.<name>` reads an input. `:=` assigns and binds an argument; `=` binds a created name and defines a derivation; `==` compares. `:` ascribes a type and names a guard or invariant.

`{a, b}` is a **collection literal** and `{ … }` a **block**. Where the two are adjacent, as in `assert fix -> { RECORDED } { … }` and `act poke at { A, B } { … }`, the first group is the collection and the second is the body. A body is always written, empty as `{ }` if the transition has no clauses, which is what makes the rule decidable by position alone. Iteration 10 claimed the two never occupy the same position; these two forms are where they do.

Bare comma-separated lists are taken by `summary`, `accepts`, `corrects`, `capability`, `category`, `states`, `only via`, `may admit`, `unique with`, `provides capability` and `requires capability`. All of them wrap by §9.1 like anything else. An **`enum` body is a braced list**, not a block of clauses, so it wraps the same way and the layout rule of §9.1 — which is about clauses — does not apply to it. `changed_since` brackets its list, matching the model, and a trigger set in a `cascade on` or a from-state set is braced.

### 9.4 Notation, and every form

In grammar lines, `<x>` is a placeholder, `[x]` is optional, `x…` repeats, `a|b` alternates, and `…` elides. These are the document's notation and are not part of the language.

The forms with their own sections are declared there: attributes and counters in §3.1, relationships and cascades in §3.2, invariants in §3.4, `requires` in §4.1, transition heads in §4.2, transition bodies in §5.1, outcome steps in §5.2, and the constructs of §6. The rest are here, so that every form in the language has a production somewhere and check 21 has something to decide against:

```text
module     <name>
use        <module>.{ <name>, … }
capability <NAME>, …
category   <name>, …
enum       <Name> version <n> { <MEMBER>, … }
sequence   <name> version <n>
evaluator  <name> version <n> { fn <name>(<arg> : <type>, …) [fresh <duration>] … }
machine    <Name> version <n> { … }
type       <Name> [extends <Base>] version <n> [abstract | mirror] { … }
observation <Name> version <n> on <Type> as <collection> { … }   # §6.8
metric     <name> version <n> { … }                               # §6.9

tracking     serial|quantity|record
machine      <Name>
states       <STATE> category <name> [terminal] [superseding], …   # either spelling,
state        <STATE> category <name> [terminal] [superseding]        # in either place
provides     capability <NAME> = <CAPABILITY>, …
summary      <member>, …
visible when <bool expression>
derive       <name> = <expression> [indexed]
labels by    <bool expression>                                    # §6.8
removed state  <STATE> -> <STATE>                                 # §6.6, at the top level of a publish
removed member <Enum>.<MEMBER> -> <MEMBER>
renamed attr   <name> -> <name>
backfill       <attribute> := <expression>
```

Where a grammar line and an example disagree, the example is authoritative and the line is a defect, because the examples are what publishing checks (§10).

### 9.5 The reserved words

`module use capability category enum sequence evaluator machine type version inputs survives tracking serial quantity record states state abstract requires provides attr counter ref part owner inverse cascade derive invariant unique scope where from scoped by format default external personal indexed identifier summary visible when extends create do act assert erase removed renamed only via proposable terminal superseding supersede input accepts require because eager deferred set clear add remove call for limit corrects may admit in at is null not and or implies if then else true false any all none count sum min max now actor this this_event referrers changed_since fn fresh stored with string bool int decimal money timestamp duration event file identity verdict on observation field unit recorded occurred within labels metric window value flag over last backdatable observe assignee entered_at time_in held combine distinct backfill member mirror as avg median percentile day week month quarter year`

`s`, `min`, `h`, `days` and `weeks` are duration units only directly after a numeric literal, which is the one position the aggregate `min` cannot occupy.

## 10. What the checker verifies

Each check names the file, line and declaration. Publishing runs them over the module and the closure of its `use` imports (§1).

**Thirty-four of the sixty-one are implemented today**, most of them only in part, listed by `scripts/check-syntax-doc.py` on every run; the rest are specified and not yet built, and the document does not distinguish them in the table because which are built is a property of the tool at a moment, not of the language. Read the tool's output for that. Checks 54 to 59 are also held against fifteen mutations of this document's own examples, each a mistake an author would make, which the tool applies on every run and which the check claiming the rule must catch.

Four things are needed beyond that text, and nothing else is. Checks 22 and 23 need the **previously published declaration**. The report line naming which invariants compile to a database constraint needs the **backend configuration**. The new-invariant scan, the pending-proposal count, and check 23's `backfill` and `removed member` findings need the **live objects**. Every other check is decidable from the closure alone.

| # | Check |
|---|---|
| 1 | A guard or outcome names an undeclared input |
| 2 | A `call`, `create` or evaluator call with an unknown argument, a missing required one, or one of the wrong type |
| 3 | Derived attributes form a cycle |
| 4 | A traversal invariant (§3.4) reading through a relationship end whose far end is not declared, so a write to the far object cannot find the invariant to re-check |
| 5 | An invariant fitting none of the three forms of §3.4; the report says which form it decided |
| 6 | A type-scan invariant that fails the swap test of §3.4; the report names the member that appears on one side only. Also a type-scan carrying an `id` exclusion, which the self-exclusion rule makes redundant |
| 7 | A traversal or type-scan invariant, a type-scan guard, or a `visible when` predicate reading an attribute that is not `indexed`; an `indexed` marking on a stored relationship end, which already is one; a `visible when` predicate that calls an evaluator, reads a derivation that is not itself `indexed`, or traverses a set-valued end (§6.7). A local invariant reads its own row and needs none, and `.state` and stored relationship ends never need one (§8.1) |
| 8 | A creation that replaces a machine's and does not carry its guards, or one where an inherited guard reads an input the replacement does not declare; a required attribute or non-optional `ref` that **any** creation of the type does not write, or one written only from an optional input; a required singular `part` **a creation** does not **fill**, filling meaning a `create` step for the child in the same outcome, since the `part` end is derived and check 17 forbids assigning it. Every creation of the type must fill it. Where a binder declares its own creations those are the ones checked, the machine's having been replaced (§2.1); where it declares none, the machine's creation is the one that must fill it, which is why a required part on such a binder is only satisfiable by declaring a `create`. Set-valued attributes and counters begin present and need no creation write; a creation supplied by the bound machine counts as one |
| 9 | A required attribute marked `personal` |
| 10 | A personal value reaching a non-personal attribute, by a write, by an argument to a `call` or `create`, or through a derivation. The taint follows values transitively; an input marked `personal` is a personal value for this purpose, and one written to a personal attribute is personal without the marking (ADR-0078). A `backfill` expression is a write for this purpose (§6.6, ADR-0101) |
| 11 | A `create` of a part that is not `only via` a transition of its whole, so a part could be added to a settled whole. Where the `owner` names an abstract base, the whole is each concrete type in its family, which declares the part or inherits it. An observation kind's `record` is exempt: it is recorded directly, and `subject_open` keeps what this check protects (§6.8). A `do` or `act` that writes an `owner` on an existing object — a re-parent — carries a generated guard, `whole_open`, which does the same for it (ADR-0100, ADR-0101) |
| 12 | A cascade cycle. The edges are `call` steps, `create` steps, `part … cascade` clauses and loop bodies |
| 13 | A `call` to an `only via` transition from a transition it does not name; a named transition that neither calls, creates nor cascades to it, or that the binder replaced, which the report says in those words rather than calling it undeclared; an `only via` transition nothing reaches; an `only via` naming `<Machine>.<transition>` where it must name `<Binder>.<transition>` (§4.2) |
| 14 | An actor guard (§4.2) on an `only via` transition; `only via` with `proposable` |
| 15 | A non-terminal state with no outgoing `do`; a state nothing can reach, **except** one that only a creation this binder replaced used to reach, which is reported instead (§2.1); a `do` leaving a `terminal` state; a machine with no `terminal` state; a state with no category. An `act` is a self-transition and does not count as outgoing, and being an `assert` target does not count as reached |
| 16 | A non-abstract type that neither binds a machine nor declares `states`, or does both; a machine `requires` a binder does not satisfy. A binder satisfies one by declaring a member of that name and an identical type, optionality included, directly or inherited |
| 17 | A write whose target is not an attribute, counter or stored relationship end of `this`; a `clear` whose target is neither an optional attribute nor an optional, singular, stored `ref` end of `this`; a write whose value does not fit the target — a differing scale where §8.3 does not round, or a precision the target cannot hold; an `add` or `remove` on a target that is not set-valued |
| 18 | A `part`/`owner` pair disagreeing on name; a `create` of a part that never writes its `owner`; a `part` whose `cascade` names a transition the child does not have. An `owner` is always singular and required, so there is no cardinality to compare |
| 19 | A name that resolves to nothing under §9.2: a capability, category, state, attribute, counter, relationship end, derivation, enum member, type, machine, sequence, evaluator, invariant, observation kind, metric, remedy class or imported name |
| 20 | An unnamed guard; a `default` on an optional input; a `because` that is not one of the five classes of §5.1 |
| 21 | A `for` without `limit`; a bare `null` in a comparison; an unbound aggregate; a keyword beginning a clause it does not belong to; `if` in a guard, which would hide which clause failed; `if` anywhere but a derived attribute or an outcome value (§8.3) |
| 22 | A version that did not advance while its content changed, content being the declaration's text without comments or blank lines; a type, observation kind or metric whose machine, enum, sequence, evaluator, metric or base type advanced without it; a field added to an observation kind that already existed, and is not optional (§6.6, ADR-0099) |
| 23 | A state removed with no mapping. A **rename is reported, not failed**: a rename with no mapping is textually identical to a drop plus an add, and no information in either declaration distinguishes them, so publishing lists each dropped and added pair and asks. Also a new required attribute or singular reference on a type with live objects and no `backfill`; a new required singular part on such a type; a removed enum member that any observation holds; and a removed enum member that live objects hold with no `removed member` mapping; these need the live objects, like the new-invariant scan (§6.6, ADR-0099) |
| 24 | An expression that does not type (§8) |
| 25 | An `event`-typed input receiving anything but `this_event` |
| 26 | A `supersede` outside a `superseding` to-state, a `superseding` state entered without one, self-supersession, or a cycle among the declared supersession targets |
| 27 | A `may admit` naming an invariant that is neither the binding type's own nor on a type reachable from it by a declared inverse (§6.3) |
| 28 | `corrects` naming attributes the outcome does not write, or the reverse; a `corrects` with no `reason` input |
| 29 | An `assert` with no actor guard (§4.2) or no `reason` input; an `erase` with no `reason` input |
| 30 | A `cascade` naming `<Type>.<transition>` where that type is `abstract` and some member of its family (§2) does not have that transition, so the cascade would reach a part it cannot drive |
| 31 | A `quantity` type with no `counter`; a `counter` on a `serial` or `record` type |
| 32 | An invariant reading `now`, which no after-write check can enforce |
| 33 | Duplicate names in one scope. The scopes are separate per kind, so a machine may declare `requires attr answer` and `do answer` without collision: members (attributes, counters, relationship ends, derivations) in one; transitions in another; invariants in a third — each over a type with its bases and its bound machine, counting only the machine transitions that binder actually has, so a binder replacing `create request` may reuse the name; where a `requires` line names the binder's member rather than declaring a second one; a machine, for its states; the closure, for top-level declarations, capabilities and categories. Also a state name colliding with a member name of the same type, or a category with either; and a state or category named `any`, `terminal` or `superseding`, or a transition named `any`, which would collide with the wildcard and the state markings — the transition case is only `any`, since `terminal` and `superseding` never appear where a transition name is expected (§9.2). A member named `id`, `state`, `open`, `created_at`, `created_by_kind` or, on a mirror, `imported_at`, which every such object already has (§8.1) |
| 34 | A non-abstract type with no creation transition, counting one its machine supplies only where the binder declares none of its own (§2.1) |
| 35 | A machine body naming a type-level attribute, counter, reference, part, invariant or capability it does not `require`. Naming means reading it in a guard, an outcome or an admission; a type-local transition is exempt (§2.1) |
| 36 | A `part`/`owner` pair whose types disagree; a **re-parent** — a `do` or `act` writing an `owner` on an existing object — whose declared owner type is `abstract`, so which source whole's invariants must be re-checked is not determined by the text (ADR-0058). A creation writing its `owner` is not a re-parent: it has no source whole, and forbidding it would make a part with an abstract owner uncreatable |
| 37 | A `part … cascade` whose trigger names a transition the whole does not have; a terminal transition of the whole covered by neither a `cascade on` clause nor `survives`, or covered by both. An observation kind's part is exempt, being implied `survives` (§6.8) |
| 38 | An `assert` with `may admit` but no `admits` input, or with no `state`-typed target input |
| 39 | A part type holding a personal attribute, inherited ones included, that the whole's `erase` body does not reach — reaching meaning a `call` to that type's own `erase`, directly or through another part's. An observation part is reached without a call, since the subject's `erase` erases its observations (§6.8) |
| 40 | An `act` declared at a terminal state |
| 41 | A `ref` pair with two set ends; two singular ends with neither or both marked `stored`; a `stored` on a set end; a set-valued `ref` with no `inverse`, which has no end to be stored on; an `inverse` the far type does not declare, or whose far end names a different near end |
| 42 | A non-abstract type with no `tracking`, inherited or declared, or one that is none of `serial`, `quantity` and `record`; an `enum`, `sequence`, `evaluator`, `machine`, `type`, `observation` or `metric` with no `version` |
| 43 | An `extends` naming a base that is undeclared, not `abstract`, or part of a cycle |
| 44 | An `identifier` naming an undeclared sequence; a `scoped by` naming a reference or attribute that any one of the type's creations does not write — the same set check 8 uses, the machine's having been replaced where the binder declares its own — or an attribute that is not `indexed`; a `format` with no `{n}`; `unique in scope` with no `scoped by` |
| 45 | An `eager` or `deferred` on a guard that calls no evaluator |
| 46 | An `indexed` derivation reading a clock, an unindexed attribute, or another object (§7) |
| 47 | A `sum`, `min` or `max` with no body; a cascade clause with no `limit` |
| 48 | An unsupplied optional input appearing as a sub-expression rather than as the whole step or argument; a path through an absent optional anywhere but the two places §5.2 permits absence to skip a step |
| 49 | A `supersede` whose operand is neither an input nor a name bound by an earlier `create` |
| 50 | A `use` importing a name its module does not declare; a `default` expression reading an input or another attribute, which would make the expansion order-dependent |
| 51 | A body that does not close on its opening line and puts a clause on that line; a line that can only be a continuation and is not indented deeper than the clause it continues (§9.1) |
| 52 | A normative statement in §1 to §9 — anything the text says is rejected or is a publish error — that does not cite the check enforcing it. It verifies that a citation is **present**, never that it is **right**: two of the first twelve named a check whose text did not cover the statement, and both were found by reading rather than by running. Its reach is bounded in one way worth knowing: it recognises a fixed list of phrasings. A proximity window used to shield any statement near a citation, which gave a demonstrated false pass, so the citation must now be in the same **sentence** as the statement. An earlier draft of this row claimed it would have caught the four rule-and-check divergences that motivated it; running the predicate over those four shows it catches **one**, and the two it misses are the two that produced blocking defects. What it does is enforce the discipline going forward, for the phrasings it knows |
| 53 | A `mirror` that binds a machine or declares a transition other than `erase`, each of which would give it a way to be written; an `extends` or a `part`/`owner` pair joining a mirror to a type this store owns, in either direction — a composition binds the part's lifetime to the whole and a mirror's lifetime belongs to another system, and a mirror composing with or extending another mirror is allowed, since they are ported and cut over together; an outcome that `call`s or `create`s into a `mirror` type (§2, ADR-0075, ADR-0101). An observation kind's collection on a mirror is not such a pair: an observation is this store's record about the object, not the other system's state, and only the import writes one there (§6.8) |
| 54 | An observation kind with no `recorded by`; a kind with no `as`; a subject that declares a part of an observation kind itself, which the kind's `as` adds; a `unit` on a field that is not `int` or `decimal`; a field named `subject`, `corrects`, `occurred_at`, `recorded_at`, `recorded_by_kind`, `created_at` or `created_by_kind`; a kind's invariant reading anything but its own fields (§6.8) |
| 55 | A label named anywhere but a metric's `from` — a guard, an invariant, a derivation, a visibility predicate, an outcome value or a `labels by` (§6.8) |
| 56 | A metric with neither `from` nor `combine`, or with no `value`; a `combine` naming an undeclared metric, or itself directly or through another; a `from` naming none of the eight sources; a dimension path longer than two hops; a personal attribute or field used as a dimension, as a flag's operand, or in a value other than a count; `avg`, `median`, `percentile` or a time bucket outside a metric; `metric(…)` anywhere but a `require` clause of a `create`, `do` or `act`, another metric included; `over last` on a metric with no `window on`; a reference binding a dimension the metric does not declare (§6.9) |
| 57 | `observe` on a clause of an `assert` or an `erase` (§5.1) |
| 58 | A `backdatable` or an `occurred within` without a duration in the closed set of §8.3; `backdatable` on an `assert`, an `erase` or an `only via` transition (§4.2); `entered_at`, `time_in`, `held` or `intervals(…)` naming what is not tracked — the state, an enum attribute or a singular stored reference — `time_in` taking a state only (§8.3, §6.9) |
| 59 | `assignee` on a set-valued `ref`, a `part`, an `owner` or an end that does not store its value; an `assignee` whose target type does not mark exactly one `identity` attribute `actor`; `actor` on an attribute that is not `identity` (§6.10) |
| 60 | A type with a personal attribute, inherited ones included, that takes part in supersession — it has a `superseding` state, or a `supersede` operand is of its type — and declares no `erase` (§6.4, ADR-0087) |
| 61 | An evaluator call or a metric reference in the guard of a transition reached by a `call` or `create`, whose argument reads an input the caller binds from a member its own outcome sets, since no consultation before the transaction can see that value (§6.2, §6.9, ADR-0092) |

Reported without failing: which transitions enter a closed-category state with no `only via` list, so the choice is visible where it is made rather than mandatory (ADR-0070); which of a machine's creations a binder has replaced by declaring its own, named one by one along with the guards carried over from each, since adding a creation to a binder silently removes them and nothing in that type's own text shows it; a `cascade` whose target transition's from-states do not cover every non-terminal state of the part's machine, naming the uncovered ones — the strict form, deliberately, since whether an uncovered state is reachable when the trigger fires depends on guards on other transitions and nothing tracks that; how many live objects would violate an invariant this publish adds, and a sample of them; a declared input nothing reads; a guard whose remedy class was inferred, and what was inferred; a guard whose value can be `unknown` through an optional it never tests (§8.2); which invariants compile to a database constraint on this backend; which transitions are **sweepable** (ADR-0048), meaning their guards decompose into an indexable prefilter over stored attributes, state and category, plus a residual evaluated only on the candidates that prefilter returns — a transition that is not sweepable is refused by `available` rather than silently scanning a type, so a time-driven transition that is not sweepable has nothing to find its objects. Of those, the ones worth polling are the ones with a guard declared `temporal` and none declared `unreachable_from_here`, which is what excludes a window that has already closed (§5.1); which derived attributes are queryable; each loop's declared bound, attributed to the loop, and the **observed** maximum fan-out over the live objects, replacing the worst-case product across nested loops, which multiplied invented numbers into a total that looked authoritative and meant nothing (ADR-0071); each dropped-and-added attribute pair that may be a rename; every `observe` clause, so a trial is never mistaken for a rule (ADR-0085); every guard reading `entered_at` of a state a backdatable transition enters, or of a member one writes, since backdating can shorten the wait it measures, within the bound (ADR-0099, ADR-0101); each transition a metric guard makes unsweepable (§6.9); and how many pending proposals a publish would invalidate.

## 11. Questions the author has answered

All thirteen were ruled on by the author on 2026-09-08, after a survey of the first consumer's production code supplied the evidence. `wr:` citations point into `~/RduWs/wr_inventory_management`.

| # | Question | Answer | Where |
|---|---|---|---|
| 13 | What a replacing creation owes the machine whose creation it replaced | **Changed.** The machine's creation guards bind any creation that replaces it. The hole was live in production: a warranty contract is creatable under a delivery permission by a principal holding no warranty permission at all | ADR-0065 |
| 7 | Whether a `cascade on` clause should carry arguments | **Changed.** It may. The first-consumer walkthrough already wrote one, and the production cascades carry behaviour for the same reason | ADR-0066 |
| 12 | Whether `serial` and `quantity` are the right two tracking modes | **Changed.** A third, `record`. Three of the first consumer's entities are serial-identified, twelve or more are not physical things, and none is quantity-tracked with counters | ADR-0067 |
| 9 | Whether `money` should carry its currency at runtime | **Refused.** The currency stays in the declaration, so a mismatch is a publish error rather than a production one, and a multi-currency model declares an account per currency as ledgers do anyway | ADR-0068 |
| 8 | Whether an evaluator should return a value | **Refused.** A verdict only. An external system that assigns is an externally owned type *(the term ADR-0080 gave what this answer first called a mirror; a `mirror` is now only the cutover marking)*, which is what the first consumer's own Xero design intent already proposes | ADR-0069 |
| 1 | Whether `only via` should be mandatory into a closed state | **Refused, and reframed.** Two of the first consumer's own transitions into closed states must stay requestable. Its real authority boundary is the undo of a committed decision. Reported, not mandated | ADR-0070 |
| 4 | Whether mandatory `limit` and guard names earn their friction | **Kept, and the report changed.** Both stay; the worst-case product across nested loops goes, replaced by each loop's own bound and the observed maximum over live objects | ADR-0071 |
| 2 | Whether `referrers` should be filterable by type | **Changed, smallest version.** An element gains a comparable `.type` | ADR-0072 |
| 3 | Whether inline `states` and a shared `machine` should look alike | **Changed.** Both spellings, in both places | ADR-0072 |
| 5 | Whether declaring an inverse should imply an index | **Already answered; closed.** §3.1 says a stored end is always available to a filter and rejects marking it, which is an index | ADR-0072 |
| 6 | Whether confidentiality should be per-attribute | **Refused.** No need in the first consumer: no field-level access control anywhere, and no cost, margin or salary column to protect | ADR-0072 |
| 10 | Whether a `counter` should hold `money` and go negative | **Deferred.** The counter mechanism is unexercised by the consumer the design was drawn from, which stores no stock level and computes availability instead. Non-negativity does move out of `counter` and into an invariant | ADR-0072 |
| 11 | Whether `only via` needs a form naming a family | **Agreed, not built.** No family in the first consumer needs it; issue tracking does. The family must be named explicitly when it is built | ADR-0072 |

Two answers moved because of the survey rather than the argument. Per-attribute confidentiality looked likely and turned out to have no need at all. The money counter turned out to rest on a mechanism the first consumer does not use.

## 12. What is still open

Not questions about the language, which is settled, but things the record knows it has not established.

- **A consumer that maintains a counter.** ADR-0072 defers the money counter partly because nothing in the record exercises the counter at all. A design that offers a maintained counter as the remedy for a slow scan should be able to point at someone maintaining one.
- **Whether `quantity` earns its place.** The same survey that produced the third tracking mode found no quantity-tracked type in the first consumer. The orders case study needs it and ADR-0050's reasoning holds, so it stays and is worth watching.
- **The commitment boundary.** ADR-0070 records that a consumer's authority attaches to undoing a commitment, and that nothing in the model distinguishes that from any other transition. One consumer names the concept; a second would justify a construct.
