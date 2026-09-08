# The declaration syntax

Status: **draft, iteration 8** (2026-09-08). The format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).

Two goals shape every choice, and where they conflict the second wins.

1. **The common case should cost nothing to write and nothing to read.**
2. **Everything the design promises to check must be decidable from the text.** §10 lists the checks.

## Iterations

Five drafts, four review rounds: three engineers building real systems in it, one auditing it against the model. What each round changed.

**2** made machines optional, stopped a write leaving the object being transitioned, unified references, named guards, and added twelve constructs the model needed and iteration 1 could not express.

**3** reversed two of those. `internal` had replaced the `only via` authority list and so deleted the guarantee it existed to make; and the implicit machine existed in the runtime but not the file, so its state had no category, no terminal state and no creation transition.

**4** merged a model audit: a machine now says what it requires of a binding type, `tracking` is explicit again, deletion cascades name a transition and a bound, and `default` became sugar rather than an ungated write.

**5** fixed what a model audit said not to publish: runtime-maintained inverses were a second write path, the unlisted authority marking re-opened a closed guarantee, and an optional input carrying a default could never be unsupplied.

**6** fixes what an engineer found by building a 243-line purchasing model in iteration 4, and adds a checker that runs this document's own examples against its own rules:

| Problem | Change |
|---|---|
| `int × money` did not type, so a line total was unwritable and the model's own canonical example was rejected | `*` and `/` are scalar; `money * money` is the error instead (§8) |
| An evaluator call yields a `verdict` and a guard had to be `bool`, so the escape hatch could not be used | A verdict is a whole guard clause, never an operand (§8) |
| A shared machine could not require a `part`, and its guards named one type's capabilities, so two types could share a lifecycle only by sharing an authority model | `requires part` and `requires capability`, mapped per binder by `provides` (§4.1) |
| Whether a type binding a shared machine may declare its own transitions was never stated, and both answers broke something | It may; they are private to it, and reachability is checked per binder (§2.1) |
| A part's delete cascade named a target but no trigger, so a whole with two terminal transitions had no answer | `cascade on <transition> to <Type>.<transition>` (§3.2) |
| An accepted attribute with a default, left unsupplied, yielded absence rather than the default | It takes the default (§5.1) |
| A part's creation was independently requestable, so a line could be added to an approved order | Parts are created `only via` their whole (check 11) |
| `limit` bounded the request, so no author could tell which number to raise | It bounds its loop; publishing reports the worst-case product (§5.2) |
| An `act` at a terminal state was legal, contradicting the rule that a deleted object takes no further transitions | Forbidden, and `at any` means non-terminal (§4.2) |
| An erasure could return success and leave the person's identity in the parts | An `erase` must reach every part type holding personal attributes (§6.4) |
| Check 7 rejected local invariants, which read their own row and need no index; check 11 was unreachable | Both rewritten; seven checks added |

**5** fixed what the model audit found wrong, and the three things it said not to publish:

| Problem | Change |
|---|---|
| **Runtime-maintained inverses were a second write path.** Writing one end wrote the far object with an event carrying no transition name, unfilterable by any subscription, and stamped its last-written index so binding a unit invalidated approvals on the delivery | **One end is stored; the inverse is a derived view over it** (§3.3, ADR-0056). Nothing to maintain, one write, one event |
| **`internal` re-opened the guarantee** iteration 3 closed, defended only by a report of derived parents | Dropped. `only via` is the sole form (§4.2) |
| **An optional input with a `default` is never unsupplied**, so the skip rule could never fire and a partial edit cleared the field | `default` is not allowed on an optional input (§5.1) |
| `proposable` was reserved, forbidden and checked, but had no position in any grammar line | Given one (§4.2) |
| A guard calling an evaluator yields `verdict`, and §8 required `bool` | Both are admitted (§8) |
| `timestamp - timestamp` and `days` were used by the model and untypeable here | Added (§8) |
| `changed_since`, on which every approval guard rests, had no grammar | Added (§8) |
| Remedy class names contain hyphens, which lex as subtraction | Renamed to `self_serviceable` and `unreachable_from_here` throughout the record |
| An abstract base type was unwritable, though the model permits one | `abstract` (§2.1) |
| `count` named both a stock attribute and the aggregate | The attribute is `counter`, matching the model's own word |
| `$name` diverged from the model's `inputs.*`, used at fifty sites | `inputs.` restored |
| `removed attr` was required for every dropped attribute, though the model says removal merely hides | Required only for a rename (§6.6) |
| The §7 example failed four of its own checks | Rewritten and mechanically verified (§7) |

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

**`capability` and `category` declare vocabularies.** Both are otherwise bare identifiers a typo turns into silence: a mistyped capability is a guard nobody can satisfy, a mistyped category a family-wide guard that never matches. The declaration is a spell-check, not a promise: capabilities are opaque strings the consumer's authentication produces, and nothing here can verify it produces these.

**Versions.** Every named declaration carries one. An object and an event record the version of their **type**, and advancing a machine, enum, sequence, evaluator or base type requires advancing every dependent type, which publishing enforces. Otherwise a recorded version would not identify the rules that applied.

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

  ref  model : RobotModel indexed
  ref  binding : DeliveryItem? inverse unit indexed
  ref  engagement_lines : EngagementLine[] inverse unit

  derive leasable = state == DEVELOPMENT
                and none(l in engagement_lines where l.engagement.state != Engagement.CLOSED)

  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != Engagement.CLOSED) <= 1
}
```

**`tracking`** is `serial` or `quantity` and is mandatory. Neither is a default, because inferring it from an incidental property is the implicit rule the model rejects by name.

A type body's clauses appear in one order: `tracking`, `machine` or `states`, `provides`, `summary`, `visible when`, then attributes, relationships, derivations, invariants, and last the transitions.

`extends` inherits every attribute, relationship, derivation and invariant of the base, and nothing else: a machine is always bound explicitly, and transitions are never inherited.

### 2.1 Every type has an explicit lifecycle

A type either **binds** a shared machine or **declares one inline** with `states`, which declares a machine named after the type. Exactly one of the two, unless the type is `abstract`, which has no objects and so needs no lifecycle.

**A type that binds a shared machine may also declare its own transitions.** They are private to it and may read anything it declares. The shared machine's transitions are the shared lifecycle; the type's are what only that type does. This is what lets two types share a lifecycle without being identical, and it is why `requires` (§4.1) constrains only the machine body: a type-local transition needs no permission from the machine.

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
  act tick   at ACTIVE { require may: actor.has(DELIVERY_EDIT) because delegable
                         set checked := true }
  act untick at ACTIVE { require may: actor.has(DELIVERY_EDIT) because delegable
                         set checked := false }
  do  delete ACTIVE -> DELETED only via Delivery.delete { }
}
```

## 3. Attributes and references

### 3.1 Attributes

```text
attr    <name> <type>[?] [marking …]
counter <name>                        # a non-negative integer of stock, quantity types only
```

`?` means the value may be absent. Without it the value must be present whenever the object exists, checked at creation and at every write.

| | |
|---|---|
| scalars | `string` `bool` `int` `decimal(p,s)` `money(ccy)` `timestamp` `duration` |
| enum, event, file | `RetirementReason`, `event`, `file` |
| set | append `[]` |

References are `ref`, `part` or `owner`, never an attribute type. Inputs may be reference-typed.

| Marking | Meaning |
|---|---|
| `identifier from <seq> [scoped by <ref>] format "<pattern>"` | Minted at creation before the outcome runs. A scope may name only a reference the creation writes |
| `unique` / `unique in scope` / `unique where <expr>` | Sugar for a uniqueness invariant: global, per the sequence's scope, or partial. **`identifier` does not imply it**, since a scoped sequence repeats across scopes |
| `external "<source>"` | Owned by another system; uniqueness per source is automatic and partial over absent values |
| `personal` | Subject to erasure. A required attribute may not be `personal`, since erasure writes absence |
| `indexed` | Available to query filters, type-scan predicates and visibility |
| `default <expr>` | Sugar the checker expands into a `set` at the head of every creation that does not write the attribute, so it appears in the rule set and in the event like any other write |

### 3.2 References and composition

```text
ref   <name> : <Type>[?|[]] [inverse <name>] [marking …]
part  <name> : <Type>[?|[]] inverse <name>
      { cascade on <transition> to <Type>.<transition> limit <n> }…
owner <name> : <Type>       inverse <name>
```

```text
type Delivery version 1 {
  tracking serial
  states   PREPARATION category live, DELIVERED category closed terminal,
           CANCELLED category closed terminal

  part checklist_items : ChecklistItem[] inverse delivery
       cascade on cancel to ChecklistItem.delete limit 500
       cascade on complete to ChecklistItem.delete limit 500

  attr approved_total money(SGD)?

  create open -> PREPARATION { require may: actor.has(DELIVERY_EDIT) because delegable }
  act add_checklist_item at PREPARATION {
    require may: actor.has(DELIVERY_EDIT) because delegable
  }
  do complete PREPARATION -> DELIVERED {
    require settled: none(c in checklist_items where not c.checked) because dependent
    require may: actor.has(DELIVERY_COMPLETE) because delegable
  }
  do cancel PREPARATION -> CANCELLED { require may: actor.has(DELIVERY_EDIT) because delegable }
}
```

Every terminal transition of the whole must be covered by a `cascade on` clause, or the part is orphaned under a closed whole; publishing checks it (ADR-0058).

A guard invalidated by a change to a part reads the part relationship by name:

```text
require fresh: not changed_since([approved_total, checklist_items], a.event)
               because delegable
```

`part` and `owner` are the two ends of a composition: exclusive membership, lifetime bounded by the whole, re-parentable by writing the `owner`. **`cascade on <transition> to <Type>.<transition>` names which of the whole's transitions drives the cascade and which part transition it drives, with a bound.** A whole with several terminal transitions says which one cascades; iteration 4 named the target and the bound but not the trigger, so a type with both a reject and a withdraw had no answer. A cascade clause **counts as a call site** for the purposes of an `only via` list.

A reference may omit `inverse`, which only means no back-reference is named. It remains visible to `referrers` and to the deletion guard.

### 3.3 One end is stored; the inverse is derived

A relationship is stored on **one** end. Its declared `inverse` is a derived view over that stored reference, resolved by the store as a query.

So `Robot.binding` is stored and `DeliveryItem.unit` reads "the robot whose binding is this". Writing `set binding := inputs.slot` is one write with one event, and the far object's view changes because what it reads changed, not because anything wrote to it.

Iteration 3 had the runtime write both ends. That was a second write path: the far event carried no transition name, so no subscription could filter it, `changes_state` was undefined for it, and it stamped the far object's last-written index, so binding a unit invalidated every approval on the delivery. Iteration 2's alternative — a transition per direction — cost the authority twice and made the release path a cascade cycle the checker rejects.

## 4. Machines and transitions

```text
machine UnitLifecycle version 2 {
  requires attr label_printed_at timestamp?
  requires attr photos file[]
  requires ref  model : RobotModel
  requires ref  binding : DeliveryItem?
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
    input slot : DeliveryItem
    require matches:  inputs.slot.model == model  because dependent
    require unfilled: inputs.slot.unit is null    because dependent
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
requires part <name> : <Type>[]
requires invariant <name>
requires capability <NAME>
```

It covers the attributes, references, parts, invariants and capabilities its transitions and admissions name. Publishing verifies every binder supplies them with compatible types, and separately that the machine body names nothing it does not require — a completeness check, without which the requirement list drifts from the body it describes.

A required **capability** is a name the machine uses in its guards and each binder maps to its own:

```text
requires capability EDIT                    # in the machine
provides capability EDIT = PO_EDIT          # in one binder
provides capability EDIT = CLAIM_EDIT       # in another
```

Without this, two types sharing a lifecycle must share an authority model, and the only workaround, `actor.has(PO_EDIT) or actor.has(CLAIM_EDIT)`, silently grants every editor of one authority over the other.

A required **part** whose element type differs per binder is declared against an `abstract` base the binders' part types extend, which is also what lets the part's `owner` name a single type.

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

**`only via`** makes a transition unrequestable and names the transitions that may cascade to it. Publishing verifies the list against the call sites it derives: a `call` from a transition not named is an error, and a name that never calls it is an error too.

Iteration 2 offered an unlisted variant instead, and iteration 4 kept it. Both deleted the guarantee: with no declared list, every call site becomes an authorised parent by construction, so a new module could write `call unit.sell()` and grant itself the power to sell a unit with no delivery, and no publish could refuse it. There is now no unlisted form. A transition either states who may cause it, or anyone may request it.

An `only via` transition carries no actor guards, since the parent's authority is its authority; writing one is a publish error rather than a guard silently dropped. It may not be `proposable`, since an unrequestable transition can never yield the verdict that makes a proposal meaningful.

`terminal` means no `do` may leave the state, and no `act` may be declared at one, since a deleted object admits no further transitions. `at any` therefore means any non-terminal state, and `erase` is the single carve-out.

## 5. Inside a transition

Clauses appear in one order: `input`, `accepts`, `require`, then the outcome.

### 5.1 Inputs and guards

```text
input   <name> : <type>[?]
input   <name> : <type> default <expr>
accepts <attribute>, …
require <name>: <expression> [eager|deferred] [because <remedy class>]
```

**A `default` may not be given for an optional input.** An optional input carrying a default is never unsupplied, so the skip rule below could never fire for it, and a partial edit would clear the field it meant to leave alone.

**`accepts`** declares that these attributes are supplied by the caller and written directly. `accepts comment` is exactly `input comment : <the attribute's type>` plus `set comment := inputs.comment`, optionality included. It is available on `create`, `do` and `act`, and it accepts a reference as readily as an attribute.

An accepted attribute that carries a `default` and is not supplied takes **the default**, not absence. Without that rule the two features combine into a silent trap: the caller omits a field with a documented default and gets an absent value, which then makes every guard reading it evaluate to unknown and fail.

**Guard names are always required.** A verdict carries the failing clause's name, and that name should not appear or change shape when someone adds a second guard.

**`eager` or `deferred`** marks a guard that calls an external evaluator, not the evaluator itself, so two guards over one function may differ. It says *when in a caller's workflow* the evaluator is consulted: an eager guard is consulted whenever the transition's availability is computed, a deferred one only when the transition is actually requested. Neither runs inside the write transaction. Omitted, a guard is eager.

`because` is optional; omitted, the checker infers the remedy class and reports what it inferred.

### 5.2 Outcome steps

```text
set    <attribute or relationship end> := <expression>     this object only
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

**Optional inputs.** A step or argument whose expression is *exactly* an unsupplied optional input is skipped. Any larger expression containing one is a publish error, so `set amount := inputs.amount` and `set amount := inputs.amount + 0` cannot be confused. A skipped write does not stamp the attribute's last-written index, so a no-op edit does not invalidate an approval.

`limit` is mandatory on every loop and bounds **that loop**. Publishing reports the worst-case product across nested loops and cascades, so a request's total fan-out is visible without being a number an author has to compute. A request exceeding a loop's bound is refused with `over-limit` naming that loop.

## 6. The remaining constructs

### 6.1 Supersession

```text
state MERGED category closed terminal superseding

do merged_into ACTIVE -> MERGED only via Contact.merge_in {
  input successor : Contact
  supersede inputs.successor
}
```

The successor may be an input or a name bound by an earlier `create`. Self-supersession and cycles are publish errors.

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

A function declares its argument types and how stale a verdict may be. Every evaluator returns a verdict, so no return type is written, and a guard may be a `verdict` as well as a `bool`. A verdict too stale to use is refused as `temporal` whatever the guard declares.

### 6.3 Assertions and admissions

`may admit` lists the invariants an assertion is *permitted* to violate; the `admits` input carries the ones a particular request actually admits, so an admission names an object and an invariant rather than blanket-admitting on every use. An assertion must declare a capability guard and a `reason` input.

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

An `erase` transition erases every `personal` attribute of its object, runs from **any** state including a terminal one, since erasure requests arrive for closed accounts, and may cascade into parts. Declaring one is optional, but if a type declares one it must reach every part type that holds personal attributes, which publishing checks. Otherwise an erasure returns success and leaves the person's identity in the parts. `corrects <attribute>, …` produces the `corrected` provenance; the outcome must write exactly the attributes named, and a `reason` input is required.

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

```text
visible when actor.has(DEAL_VIEW_ALL) or owner.id == actor.id
type Bug version 1 abstract { … }
type BugInPlatform extends Bug version 1 { machine PlatformWorkflow  … }
```

## 7. Quantity tracking

```text
type Stock version 1 {
  tracking quantity
  states   ACTIVE category live, CLOSED category closed terminal

  ref     model : ProductModel inverse stock indexed
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

The expression language is DESIGN.md §5.7. Its types are the attribute types of §3.1, plus **references** (an object of a named type, carrying `.id` and its declared members), **`state`** (a member of one machine's state set, carrying `.category`), **`invariant`** (a name declared on a type), and **`verdict`**. Two further rules the examples already rely on. Every reference carries **`.id`**, of an opaque identity type comparable only with another `.id`. **`actor`** has the shape of §5.8's descriptor: `.id` of that same type, `.kind` of a fixed enum, `.principal`, and `.has(<capability>)` yielding `bool`. A **state literal of another type** is written `<Type>.<STATE>`, since a bare name would be ambiguous across machines.

The checker types every expression:

- `+` and `-` take two operands of one type: `int`, `decimal` of one scale, `money` of one currency, or `timestamp - timestamp` yielding a `duration`; `timestamp ± duration` yields a `timestamp`;
- `*` and `/` are **scalar**: `int` or `decimal` on one side, and `int`, `decimal`, `money` or `duration` on the other, yielding the non-scalar type. `money * money` is an error, as is `duration * duration`;
- comparison needs both sides of one type; `is null` and `is not null` take anything and yield `bool`;
- `and`, `or`, `not`, `implies` take and yield `bool`. An evaluator call yields a `verdict`, which is usable **only as a whole guard clause**, never as an operand, so a guard clause is a `bool` or a single evaluator call;
- `count` yields `int`, `sum`/`min`/`max` the element type, `all`/`any`/`none` `bool`;
- `changed_since([<attribute>, …], <event expression>)` yields `bool`. The bracketed list is the model's own form. Each name is an attribute of `this`, **or a part relationship**, which means "any change to any of those parts" — without which an approval on a whole cannot be invalidated by an edit to one of its lines, which is the single most-cited guard in the model;
- `if <bool> then <a> else <b>` yields the common type of `a` and `b`, and is allowed only in a derived attribute.

Aggregates are `agg(<name> in <collection> [where <filter>][: <body>])`. The body is required for `sum`, `min` and `max`, and optional for `count`, `all`, `any` and `none`.

Precedence, highest first: paths and calls; unary `not`; `* /`; `+ -`; comparison, `in`, `is null`; `and`; `or`; `implies`; `if`. A line continues while it ends in an operator or an open bracket. Literals: `30 min`, `2 h`, `14 days`, `USD 19.99`, `"text"`, `true`, `{A, B}`.

## 9. Lexical rules

**Reserved words** may not be used as names:

`module use capability category enum sequence evaluator machine type version inputs tracking serial quantity states state abstract requires provides attr counter ref part owner inverse cascade derive invariant unique scope where from scoped by format default external personal indexed identifier summary visible when extends create do act assert erase removed renamed only via proposable terminal superseding supersede input accepts require because eager deferred set add remove call for limit corrects may admit in at is null not and or implies if then else true false any all none count sum min max now actor this this_event referrers changed_since fn fresh string bool int decimal money timestamp duration event file verdict`

`min` and `h` and `days` are duration units only after a numeric literal, which is the one position the aggregate `min` cannot occupy.

**Reserved words are contextual.** A word is reserved only where it could be misread: a keyword at the start of a declaration or clause, and a type name in a type position. The same word may name an attribute or a relationship end, since neither position can hold a keyword. So `attr event event` and `owner.id` are legal, which matters because the model writes both.

**Symbols.** `->` is a to-state, an assertable-state set and a migration mapping; never implication, never a reference type. `implies` is implication. `inputs.<name>` reads an input. `:=` assigns and binds an argument; `=` binds a created name and defines a derivation; `==` compares. `{a, b}` is a collection literal and `{ … }` a block; the two never occupy the same position. `:` ascribes a type and names a guard or invariant. `in` is a binder and membership, both meaning "element of".

`summary`, `accepts`, `corrects`, `capability` and `category` take bare comma-separated lists; `changed_since` brackets its list, matching the model.

## 10. What the checker verifies

Each check names the file, line and declaration. Checks 22 and 23 need the previously published declaration, and the new-invariant scan and pending-proposal count in the publish report need the live objects; the rest are decidable from the text.

| # | Check |
|---|---|
| 1 | A guard or outcome names an undeclared input |
| 2 | A `call` or `create` with an unknown argument, a missing required one, or one of the wrong type |
| 3 | Derived attributes form a cycle |
| 4 | An invariant traverses a relationship with no declared inverse |
| 5 | An invariant fits none of the three forms; the report says which it decided |
| 6 | A type-scan invariant that is not symmetric |
| 7 | A **traversal or type-scan** invariant, or a type-scan guard, reading an unindexed attribute. A local invariant reads its own row and needs no index |
| 8 | A required attribute a creation never writes, one a later transition sets to absent, or one written only from an optional input. A set-valued attribute is present-and-empty unless written, so it needs no creation write |
| 9 | A required attribute marked `personal` |
| 10 | A personal value reaching a non-personal attribute by write, cascade argument or derivation |
| 11 | A `create` of a part that is not `only via` a transition of its whole, so a part could be added to a settled whole |
| 12 | A cascade cycle across transitions, including `part … cascade` targets |
| 13 | A `call` to an `only via` transition from a parent it does not name; a named parent that never calls it; an `only via` transition nothing reaches |
| 14 | An actor guard on an `only via` transition; `only via` with `proposable` |
| 15 | A non-terminal state with no outgoing transition; a state nothing can reach; a `do` leaving a `terminal` state; a machine with no `terminal` state; a state with no category |
| 16 | A non-abstract type that neither binds a machine nor declares `states`, or does both; a machine `requires` a binder does not satisfy |
| 17 | A write whose target is not an attribute or stored relationship end of `this`; an `add` or `remove` on a target that is not set-valued |
| 18 | A `part`/`owner` pair disagreeing on name or cardinality; a `create` of a part that never writes its `owner`; a `part` whose `cascade` names a transition the child does not have |
| 19 | A capability, category, state, attribute, transition, relationship end, enum member or evaluator function that is not declared |
| 20 | An unnamed guard; a `default` on an optional input; a declared remedy class the checker's own inference contradicts |
| 21 | A `for` without `limit`; a bare `null` in a comparison; an unbound aggregate; a reserved word as a name; `if` outside a derived attribute |
| 22 | A version that did not advance while its content changed; a type whose machine, enum, sequence, evaluator or base type advanced without it |
| 23 | A state removed, or an attribute renamed, with no mapping |
| 24 | An expression that does not type |
| 25 | An `event`-typed input receiving anything but `this_event` |
| 26 | A `supersede` outside a `superseding` to-state, a `superseding` state entered without one, self-supersession or a cycle |
| 27 | A `may admit` naming an invariant the binding type does not declare |
| 28 | `corrects` naming attributes the outcome does not write, or the reverse; a `corrects` with no `reason` input |
| 29 | An `assert` with no capability guard or no `reason` input; an `erase` with no `reason` input |
| 30 | A family member whose machine lacks a transition another member's cascade calls |
| 31 | A `quantity` type with no `counter`; a `counter` on a `serial` type |
| 32 | An invariant reading `now`, which no after-write check can enforce |
| 33 | Duplicate transition, state, attribute or guard names within one scope |
| 34 | A type with no creation transition |
| 35 | A machine body naming a type-level attribute, reference, part, invariant or capability it does not `require` |
| 36 | A `part`/`owner` pair whose types disagree |
| 37 | A `part … cascade` whose trigger names a transition the whole does not have, or a whole with several terminal transitions and no trigger named |
| 38 | An `assert` with `may admit` but no `admits` input, or with no `state`-typed target input |
| 39 | An `erase` that does not reach a part type holding personal attributes |
| 40 | An `act` declared at a terminal state |

Reported without failing: a declared input nothing reads; a guard whose remedy class was inferred, and what was inferred; which invariants compile to a database constraint on this backend; which transitions are sweepable; which derived attributes are queryable; and how many pending proposals a publish would invalidate.

## 11. Open questions

1. Whether `only via` should be mandatory on any transition entering a closed-category state, which is where authority matters most.
2. Whether `referrers` should be filterable by type, so a deletion guard can treat one referencing type differently.
3. Whether inline `states` and a shared `machine` should look more alike than they do.
4. Whether mandatory `limit` and mandatory guard names are worth their friction; both are deviations the model should ratify or reject.
5. Whether a derived inverse is fast enough without an index on the stored end, or whether declaring an inverse should imply one.
