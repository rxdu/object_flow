# The declaration syntax

Status: **draft, iteration 18** (2026-09-08). Three reviewers returned non-blocking on iteration 15; this iteration is their residuals. **Ready for author review.** The format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).

Two goals shape every choice, and where they conflict the second wins.

1. **The common case should cost nothing to write and nothing to read.**
2. **Everything the design promises to check must be decidable from the text.** §10 lists the checks.

## Iterations

Eighteen drafts, sixteen review rounds and one coherence pass: engineers building real systems in it, and audits against the model. Each round is summarised by what it changed, because most changes were reversals of the round before.

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

## 1. Shape of a file

A file is a module, extension `.ok`. Declarations may appear in any order; forward references are fine. `#` begins a comment.

```text
module inventory
use   commerce.{Customer}

capability INVENTORY_CREATE, INVENTORY_RETIRE, INVENTORY_ASSERT,
           ERASE_PERSONAL, DELIVERY_EDIT, DELIVERY_COMPLETE, DEAL_VIEW_ALL,
           PO_EDIT, CLAIM_EDIT
category   inbound, live, closed

enum      RetirementReason version 1 { FAILED, DAMAGED, OBSOLETE, LOST }
sequence  unit_serial version 1
evaluator xero version 1 { … }
machine   UnitLifecycle version 2 { … }
type      Robot version 3 { … }
```

Nine top-level forms: `module`, `use`, `capability`, `category`, `enum`, `sequence`, `evaluator`, `machine`, `type`.

**What publishing takes** is a module together with the closure of its `use` imports, and every check in §10 runs over that closure. Every top-level declaration is importable; `use` names what a module depends on so that the closure is computable from the text rather than from a directory listing. Capabilities and categories are **one vocabulary across the closure**, not per module, because a capability that meant different things in two modules would make every shared machine unsafe.

**`capability` and `category` declare vocabularies.** Both are otherwise bare identifiers a typo turns into silence: a mistyped capability is a guard nobody can satisfy, a mistyped category a family-wide guard that never matches. The declaration is a spell-check, not a promise: capabilities are opaque strings the consumer's authentication produces, and nothing here can verify it produces these.

**Versions.** The five declarations that rules depend on carry one: `enum`, `sequence`, `evaluator`, `machine` and `type`. A `module`, a `use`, and the `capability` and `category` vocabularies do not, having no content a recorded object could be interpreted against. An object and an event record the version of their **type**, and advancing a machine, enum, sequence, evaluator or base type requires advancing every dependent type, which publishing enforces (check 22). Otherwise a recorded version would not identify the rules that applied.

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

A **family** is an `abstract` base together with every type extending it. `tracking` is inherited with everything else, so a family states it once on the base; an `abstract` type may state it and needs none, having no objects. A header is written `type <Name> [extends <Base>] version <n> [abstract]`, in that order. Markings elsewhere — on an attribute, a reference, a state — may appear in any order after the part that is required.

`extends` inherits every attribute, relationship, derivation and invariant of the base, and nothing else: a machine is always bound explicitly, and transitions are never inherited. A base must be declared and `abstract`, and the inheritance graph must be acyclic (check 43).

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
| `default <expr>` | Sugar the checker expands into a `set` at the head of every creation that does not write the attribute, so it appears in the rule set and in the event like any other write |

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
    input reason : string
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
```

A transition a machine supplies is named `<Binder>.<transition>`, not `<Machine>.<transition>`, because authority belongs to the type, not to the lifecycle it borrows. The same holds in a `cascade on` clause. An `owner` may name an **abstract** base, which is how one part type serves two wholes. The wholes need not share a machine, and each supplies its own cascade clauses for the inherited part (§3.2). An `only via` on the part's creation names the concrete wholes' transitions, not the base's, since the base has none.

**`only via`** makes a transition unrequestable and names the transitions that may cascade to it. Publishing verifies the list against the call sites it derives: a `call` from a transition not named is an error, and a name that never calls it is an error too (check 13).

Iteration 2 offered an unlisted variant instead, and iteration 4 kept it. Both deleted the guarantee: with no declared list, every call site becomes an authorised parent by construction, so a new module could write `call unit.sell()` and grant itself the power to sell a unit with no delivery, and no publish could refuse it. There is now no unlisted form. A transition either states who may cause it, or anyone may request it.

An `only via` transition carries no **actor guard** — a guard whose expression calls `actor.has` — since the parent's authority is its authority; writing one is a publish error rather than a guard silently dropped (check 14). A guard that reads `actor.id` to compare identities, such as a one-approval-per-person test, is not an actor guard and stays legal. It may not be `proposable` (check 14), since an unrequestable transition can never yield the verdict that makes a proposal meaningful.

`terminal` means no `do` may leave the state and no `act` may be declared at one, since a disposed object admits no further transitions. A **terminal transition** is a `do` whose to-state is terminal; a creation into a terminal state is not one. The coverage rule of §3.2 is about **the whole's** terminal transitions, so a type whose objects are born final has none of its own and asks nothing of anything it owns.

It says nothing about being *owned*. A whole with a terminal transition whose parts are born final has no cascade available — check 18 rejects a cascade naming a transition the child does not have — so that part is marked `survives`, and that is the one shape where `survives` is not a modelling error but the only correct answer. A journal of posted entries, a statement of settled lines, an audit log under the thing it audits: the records outlive the container by design. `at any` therefore means any non-terminal state, and `erase` is the single carve-out.

**`terminal` is not the same as `closed`, and choosing it early is the mistake this rule invites.** A category says where in the lifecycle a state sits; `terminal` says nothing further will ever be recorded. An object that is finished operationally but must still accept recorded activity — a trial subject who has completed and may still have an adverse event reported against them, an order that is delivered and may still be reviewed — belongs in a `closed` state that is **not** terminal, with a genuinely final state after it if one is needed. Marking the first closed state terminal and discovering the conflict at the first `act` costs a lifecycle restructure and a fresh set of cascade clauses.

## 5. Inside a transition

`accepts` and `only via` are part of the transition **head**, before the `{`, alongside the states and the other markings. Inside the body, clauses appear in one order: `input`, `require`, `corrects` or `may admit`, then the outcome. Iteration 10 listed `accepts` as a body clause, which every example contradicted.

### 5.1 Inputs and guards

```text
<kind> <name> <states> [only via …] [accepts <attribute>, …] [proposable] {   # the head
  input     <name> : <type>[?|[]]
  input     <name> : <type> default <expr>
  require   <name>: <expression> [eager|deferred] [because <remedy class>]
  corrects  <attribute>, …                                     # §6.4
  may admit <invariant>, …                                     # assert only, §6.3
  …                                                            # outcome steps, §5.2
}
```

**A `default` may not be given for an optional input** (check 20). An optional input carrying a default is never unsupplied, so the skip rule below could never fire for it, and a partial edit would clear the field it meant to leave alone.

An input may be **set-valued**, written `<type>[]`, and a set may be of a reference type — which is how a transition takes a selection, such as the slots to move to a new delivery. The `admits` input of an assertion is the same form over `invariant`.

**`accepts`** declares that these attributes are supplied by the caller and written directly. `accepts comment` is exactly `input comment : <the attribute's type>` plus `set comment := inputs.comment`, optionality included. It is available on `create`, `do` and `act`, and it accepts a reference as readily as an attribute.

A `default` applies **at creation only**. An accepted attribute not supplied to a `create` takes its default; not supplied to a `do` or an `act`, it is **not written at all**, and the stored value stands. The alternative silently overwrites: a partial edit that omits a field would reset it to the default someone chose for new objects, which is the opposite of what omitting a field means in an edit.

**Guard names are always required.** A verdict carries the failing clause's name, and that name should not appear or change shape when someone adds a second guard.

**`eager` or `deferred`** marks a guard that calls an external evaluator, not the evaluator itself, so two guards over one function may differ. It says *when in a caller's workflow* the evaluator is consulted: an eager guard is consulted whenever the transition's availability is computed, a deferred one only when the transition is actually requested. Neither runs inside the write transaction. Omitted, a guard is eager.

**The five remedy classes** say what the caller can do about a failure: `self_serviceable` (correct something in the request), `delegable` (someone with more authority can do it), `temporal` (**wait, and it will pass**), `dependent` (another object must change first), `unreachable_from_here` (no path from this state at all).

`temporal` and `unreachable_from_here` both arise from a clock and mean opposite things. A window that has not opened is `temporal`; a **window that has closed** is `unreachable_from_here`, because waiting is exactly what will not help. No inference can tell them apart, since both are a comparison against `now`, so **declare the class on any guard about a deadline**. It matters beyond the wording: the publish report singles out the sweepable transitions worth polling by their guards being declared `temporal`, and a consumer polls that list. A closed window left to inference puts a legally dead transition on a scheduler forever.

`because` is optional. Omitted, the checker infers one by the first rule that matches, in this order: a guard calling `actor.has` is `delegable`; one comparing against `now` or a duration is `temporal`; one reading another object, through a relationship or a type scan, is `dependent`; one comparing against an input is `self_serviceable`; anything else is `unreachable_from_here`. The order is what makes the inference deterministic, since most real guards match several rules.

**The inference never contradicts a declared class.** A declared one is the author's statement about their own domain, and the checker has no standing to overrule it — an earlier draft had it do so, and the rule as written rejected this document's own stock guard. Check 20 verifies only that a declared class is one of the five.

### 5.2 Outcome steps

```text
set    <attribute or relationship end> := <expression>     this object only
clear  <optional attribute>                               write absence, this object only
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

**`clear` writes absence**, and is the only way to. An unsupplied optional input *skips* its write, leaving the stored value alone, which is what a partial edit means; `clear` is the opposite and says so, so the two cannot be confused. It is a write like any other: it stamps the attribute's last-written index, appears in the event, and invalidates an approval that read the attribute. Publishing rejects a `clear` on a required attribute, on a counter, or on a relationship end (check 17) — an end is cleared by writing the optional reference itself.

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

A guard calls one as `<evaluator>.<fn>(<arg>, …)`, which is the only call form outside an outcome:

```text
require invoiced: xero.invoice_valid(order_id) deferred because dependent
```

A function declares its argument types and how stale a verdict may be. Every evaluator returns a **verdict and never a value**, so no return type is written, and a guard may be a `verdict` as well as a `bool`. The consequence is worth stating where it is met rather than leaving it to be discovered: where an external system *decides* something — a randomisation service choosing a treatment arm, a pricing service quoting — the caller supplies the value as an input and the evaluator can only be asked whether it is consistent. The store records such a value as governed data it did not verify. Open question 8 is whether that boundary should move. A verdict too stale to use is refused as `temporal` whatever the guard declares.

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
removed state LEGACY_HOLD -> AVAILABLE
renamed attr  old_name    -> new_name
```

A publish that drops a state carries its mapping. A dropped attribute merely hides, its recorded values staying in history, so only a rename needs one.

### 6.7 `this_event`, visibility, extension

`this_event` yields the event the current transition will record, and an `event`-typed **input accepts only `this_event`**, which is what stops an approval being forged with an old one.

**`visible when`** takes a `bool` expression over `actor`, the object's own members, and stored relationship ends one step out. Every attribute it reads must be `indexed`, since visibility is applied as a query filter and not as a per-row test; `.state` and stored ends qualify without a marking (§8.1). It may not call an evaluator, read a derivation that is not itself indexed, or traverse a set-valued end (check 7), all three for the same reason: the predicate has to become part of a query.

```text
visible when actor.has(DEAL_VIEW_ALL) or owner.id == actor.id
type Bug version 1 abstract { … }
type BugInPlatform extends Bug version 1 { machine PlatformWorkflow  … }
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

**Literals.** A state of another type is `<Type>.<STATE>`; an enum member is `<Enum>.<MEMBER>`; an invariant on another type, which only `may admit` and an `admits` argument name, is `<Type>.<invariant>`. All three are qualified for the same reason, that a bare name would be ambiguous across declarations. A bare state name of *this* type's machine is legal and resolves by §9.2. A category is written bare, since categories are one global vocabulary. Also `42`, `19.99`, `30 min`, `USD 19.99`, `"text"`, `true`, `false`, `{A, B}`. A bare number with no point is an `int`. One with a point is a `decimal`, and it takes the scale of whatever it is compared or combined with, so `balance > 0.5` types against a `decimal(10,4)` without the literal having to be written to four places.

`actor` has the shape of DESIGN.md §5.8's descriptor, and a declaration may read four members of it: `.id` of type `identity`, `.kind` of a fixed enum, `.principal` of type `string`, and `.has(<capability>)` yielding `bool`. The descriptor's optional attributes are deliberately not readable, since they carry no declared types and a guard over an untyped value cannot be checked at publish time. `this` is a reference to the object being transitioned, so `this.id` is its identity and is what an evaluator is passed when it must name the object.

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
- `changed_since([<attribute>, …], <event expression>)` yields `bool`. Each name is an attribute of `this`, **or a part relationship**, which means "any change to any of those parts" — without which an approval on a whole cannot be invalidated by an edit to one of its lines, the single most-cited guard in the model.
- `if <bool> then <a> else <b>` yields the common type of `a` and `b`, and is allowed in a derived attribute and in the value expression of an outcome step. It is not allowed in a guard (check 21), where a conditional would hide which clause failed. Allowing it in an outcome value is what stops a single two-valued choice — a debit or a credit, a rise or a fall — from splitting into two transitions and then splitting every caller of them in turn.
- `in` is membership: an element on the left, and on the right a collection literal, a set-valued member, or an enum name meaning any of its members.

Aggregates are `agg(<name> in <collection> [where <filter>][: <body>])`. The body is required for `sum`, `min` and `max`, and optional for `count`, `all`, `any` and `none`.

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
type       <Name> [extends <Base>] version <n> [abstract] { … }

tracking     serial|quantity|record
machine      <Name>
states       <STATE> category <name> [terminal] [superseding], …   # either spelling,
state        <STATE> category <name> [terminal] [superseding]        # in either place
provides     capability <NAME> = <CAPABILITY>, …
summary      <member>, …
visible when <bool expression>
derive       <name> = <expression> [indexed]
```

Where a grammar line and an example disagree, the example is authoritative and the line is a defect, because the examples are what publishing checks (§10).

### 9.5 The reserved words

`module use capability category enum sequence evaluator machine type version inputs survives tracking serial quantity record states state abstract requires provides attr counter ref part owner inverse cascade derive invariant unique scope where from scoped by format default external personal indexed identifier summary visible when extends create do act assert erase removed renamed only via proposable terminal superseding supersede input accepts require because eager deferred set clear add remove call for limit corrects may admit in at is null not and or implies if then else true false any all none count sum min max now actor this this_event referrers changed_since fn fresh stored with string bool int decimal money timestamp duration event file identity verdict`

`s`, `min`, `h`, `days` and `weeks` are duration units only directly after a numeric literal, which is the one position the aggregate `min` cannot occupy.

## 10. What the checker verifies

Each check names the file, line and declaration. Publishing runs them over the module and the closure of its `use` imports (§1).

**Twenty-four of the fifty-two are implemented today**, listed by `scripts/check-syntax-doc.py` on every run; the rest are specified and not yet built, and the document does not distinguish them in the table because which are built is a property of the tool at a moment, not of the language. Read the tool's output for that.

Four things are needed beyond that text, and nothing else is. Checks 22 and 23 need the **previously published declaration**. The report line naming which invariants compile to a database constraint needs the **backend configuration**. The new-invariant scan and the pending-proposal count in the report need the **live objects**. Every other check is decidable from the closure alone.

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
| 10 | A personal value reaching a non-personal attribute, by a write, by an argument to a `call` or `create`, or through a derivation. The taint follows values transitively |
| 11 | A `create` of a part that is not `only via` a transition of its whole, so a part could be added to a settled whole. Where the `owner` names an abstract base, the whole is each concrete type in its family, which declares the part or inherits it |
| 12 | A cascade cycle. The edges are `call` steps, `create` steps, `part … cascade` clauses and loop bodies |
| 13 | A `call` to an `only via` transition from a transition it does not name; a named transition that neither calls, creates nor cascades to it, or that the binder replaced, which the report says in those words rather than calling it undeclared; an `only via` transition nothing reaches; an `only via` naming `<Machine>.<transition>` where it must name `<Binder>.<transition>` (§4.2) |
| 14 | An actor guard (§4.2) on an `only via` transition; `only via` with `proposable` |
| 15 | A non-terminal state with no outgoing `do`; a state nothing can reach, **except** one that only a creation this binder replaced used to reach, which is reported instead (§2.1); a `do` leaving a `terminal` state; a machine with no `terminal` state; a state with no category. An `act` is a self-transition and does not count as outgoing, and being an `assert` target does not count as reached |
| 16 | A non-abstract type that neither binds a machine nor declares `states`, or does both; a machine `requires` a binder does not satisfy. A binder satisfies one by declaring a member of that name and an identical type, optionality included, directly or inherited |
| 17 | A write whose target is not an attribute, counter or stored relationship end of `this`; a `clear` whose target is not an optional attribute of `this`; a write whose value does not fit the target — a differing scale where §8.3 does not round, or a precision the target cannot hold; an `add` or `remove` on a target that is not set-valued |
| 18 | A `part`/`owner` pair disagreeing on name; a `create` of a part that never writes its `owner`; a `part` whose `cascade` names a transition the child does not have. An `owner` is always singular and required, so there is no cardinality to compare |
| 19 | A name that resolves to nothing under §9.2: a capability, category, state, attribute, counter, relationship end, derivation, enum member, type, machine, sequence, evaluator, invariant, remedy class or imported name |
| 20 | An unnamed guard; a `default` on an optional input; a `because` that is not one of the five classes of §5.1 |
| 21 | A `for` without `limit`; a bare `null` in a comparison; an unbound aggregate; a keyword beginning a clause it does not belong to; `if` in a guard, which would hide which clause failed; `if` anywhere but a derived attribute or an outcome value (§8.3) |
| 22 | A version that did not advance while its content changed, content being the declaration's text without comments or blank lines; a type whose machine, enum, sequence, evaluator or base type advanced without it |
| 23 | A state removed with no mapping. A **rename is reported, not failed**: a rename with no mapping is textually identical to a drop plus an add, and no information in either declaration distinguishes them, so publishing lists each dropped and added pair and asks |
| 24 | An expression that does not type (§8) |
| 25 | An `event`-typed input receiving anything but `this_event` |
| 26 | A `supersede` outside a `superseding` to-state, a `superseding` state entered without one, self-supersession, or a cycle among the declared supersession targets |
| 27 | A `may admit` naming an invariant that is neither the binding type's own nor on a type reachable from it by a declared inverse (§6.3) |
| 28 | `corrects` naming attributes the outcome does not write, or the reverse; a `corrects` with no `reason` input |
| 29 | An `assert` with no actor guard (§4.2) or no `reason` input; an `erase` with no `reason` input |
| 30 | A `cascade` naming `<Type>.<transition>` where that type is `abstract` and some member of its family (§2) does not have that transition, so the cascade would reach a part it cannot drive |
| 31 | A `quantity` type with no `counter`; a `counter` on a `serial` or `record` type |
| 32 | An invariant reading `now`, which no after-write check can enforce |
| 33 | Duplicate names in one scope. The scopes are separate per kind, so a machine may declare `requires attr answer` and `do answer` without collision: members (attributes, counters, relationship ends, derivations) in one; transitions in another; invariants in a third — each over a type with its bases and its bound machine, counting only the machine transitions that binder actually has, so a binder replacing `create request` may reuse the name; where a `requires` line names the binder's member rather than declaring a second one; a machine, for its states; the closure, for top-level declarations, capabilities and categories. Also a state name colliding with a member name of the same type, or a category with either; and a state or category named `any`, `terminal` or `superseding`, or a transition named `any`, which would collide with the wildcard and the state markings — the transition case is only `any`, since `terminal` and `superseding` never appear where a transition name is expected (§9.2) |
| 34 | A non-abstract type with no creation transition, counting one its machine supplies only where the binder declares none of its own (§2.1) |
| 35 | A machine body naming a type-level attribute, counter, reference, part, invariant or capability it does not `require`. Naming means reading it in a guard, an outcome or an admission; a type-local transition is exempt (§2.1) |
| 36 | A `part`/`owner` pair whose types disagree; a **re-parent** — a `do` or `act` writing an `owner` on an existing object — whose declared owner type is `abstract`, so which source whole's invariants must be re-checked is not determined by the text (ADR-0058). A creation writing its `owner` is not a re-parent: it has no source whole, and forbidding it would make a part with an abstract owner uncreatable |
| 37 | A `part … cascade` whose trigger names a transition the whole does not have; a terminal transition of the whole covered by neither a `cascade on` clause nor `survives`, or covered by both |
| 38 | An `assert` with `may admit` but no `admits` input, or with no `state`-typed target input |
| 39 | A part type holding a personal attribute, inherited ones included, that the whole's `erase` body does not reach — reaching meaning a `call` to that type's own `erase`, directly or through another part's |
| 40 | An `act` declared at a terminal state |
| 41 | A `ref` pair with two set ends; two singular ends with neither or both marked `stored`; a `stored` on a set end; a set-valued `ref` with no `inverse`, which has no end to be stored on; an `inverse` the far type does not declare, or whose far end names a different near end |
| 42 | A non-abstract type with no `tracking`, inherited or declared, or one that is none of `serial`, `quantity` and `record`; an `enum`, `sequence`, `evaluator`, `machine` or `type` with no `version` |
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

Reported without failing: which transitions enter a closed-category state with no `only via` list, so the choice is visible where it is made rather than mandatory (ADR-0070); which of a machine's creations a binder has replaced by declaring its own, named one by one along with the guards carried over from each, since adding a creation to a binder silently removes them and nothing in that type's own text shows it; a `cascade` whose target transition's from-states do not cover every non-terminal state of the part's machine, naming the uncovered ones — the strict form, deliberately, since whether an uncovered state is reachable when the trigger fires depends on guards on other transitions and nothing tracks that; how many live objects would violate an invariant this publish adds, and a sample of them; a declared input nothing reads; a guard whose remedy class was inferred, and what was inferred; a guard whose value can be `unknown` through an optional it never tests (§8.2); which invariants compile to a database constraint on this backend; which transitions are **sweepable** (ADR-0048), meaning their guards decompose into an indexable prefilter over stored attributes, state and category, plus a residual evaluated only on the candidates that prefilter returns — a transition that is not sweepable is refused by `available` rather than silently scanning a type, so a time-driven transition that is not sweepable has nothing to find its objects. Of those, the ones worth polling are the ones with a guard declared `temporal` and none declared `unreachable_from_here`, which is what excludes a window that has already closed (§5.1); which derived attributes are queryable; each loop's declared bound, attributed to the loop, and the **observed** maximum fan-out over the live objects, replacing the worst-case product across nested loops, which multiplied invented numbers into a total that looked authoritative and meant nothing (ADR-0071); each dropped-and-added attribute pair that may be a rename; and how many pending proposals a publish would invalidate.

## 11. Questions the author has answered

All thirteen were ruled on by the author on 2026-09-08, after a survey of the first consumer's production code supplied the evidence. `wr:` citations point into `~/RduWs/wr_inventory_management`.

| # | Question | Answer | Where |
|---|---|---|---|
| 13 | What a replacing creation owes the machine whose creation it replaced | **Changed.** The machine's creation guards bind any creation that replaces it. The hole was live in production: a warranty contract is creatable under a delivery permission by a principal holding no warranty permission at all | ADR-0065 |
| 7 | Whether a `cascade on` clause should carry arguments | **Changed.** It may. The first-consumer walkthrough already wrote one, and the production cascades carry behaviour for the same reason | ADR-0066 |
| 12 | Whether `serial` and `quantity` are the right two tracking modes | **Changed.** A third, `record`. Three of the first consumer's entities are serial-identified, twelve or more are not physical things, and none is quantity-tracked with counters | ADR-0067 |
| 9 | Whether `money` should carry its currency at runtime | **Refused.** The currency stays in the declaration, so a mismatch is a publish error rather than a production one, and a multi-currency model declares an account per currency as ledgers do anyway | ADR-0068 |
| 8 | Whether an evaluator should return a value | **Refused.** A verdict only. An external system that assigns is a mirror, which is what the first consumer's own Xero design intent already proposes | ADR-0069 |
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
