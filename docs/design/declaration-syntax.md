# The declaration syntax

Status: **draft, iteration 1** (2026-09-08). This is the format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).

Two goals shape every choice here, and where they conflict the second wins.

1. **A type author should be able to write the common case without ceremony**, and read someone else's type without a manual.
2. **Everything the design promises to check must be checkable from the text alone**, before anything runs. §7 lists the checks and points at the syntax that makes each one decidable.

## 1. Shape of a file

A file is a module with the extension `.ok`. It contains declarations in any order; forward references are fine. `#` begins a comment to end of line.

```text
module inventory

enum RobotStatus { REQUESTED PROCUREMENT INTAKE AVAILABLE RESERVED SOLD DEVELOPMENT RETIRED CANCELLED }

machine UnitLifecycle { … }

type Robot version 3 { … }
```

Four top-level forms: `module`, `enum`, `machine`, `type`. A module name scopes the declarations in the file; a type is referred to as `Robot` inside its module and `inventory.Robot` outside.

## 2. Types

```text
type Robot version 3 {
  tracking serial                       # or: tracking quantity
  machine  UnitLifecycle
  summary  serial, model, state

  attribute serial   string  identifier from unit_serial format "RBT-{n}"
  attribute model    -> RobotModel  indexed
  attribute manufacturer_serial string?
  attribute label_printed_at    timestamp?
  attribute photos   file[]
  attribute retirement_reason   RetirementReason?
  attribute legacy_id string? external "legacy"

  part      photos_evidence : IntakePhoto inverse unit
  ref       binding : DeliveryItem? inverse unit
  ref       procurement_order : ProcurementOrder? inverse units

  derive    leasable = state == DEVELOPMENT
                   and none(l in engagement_lines where l.engagement.state != CLOSED)

  invariant unique serial
  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != CLOSED) <= 1
}
```

**`version`** is the declaration version (DESIGN.md §5.9). Publishing a file whose type version has not advanced, when its content differs, is an error.

**`tracking`** is `serial` or `quantity`. It has no default: a type author must decide, because the two are not interchangeable and nothing can infer it (DESIGN.md §5.10).

**`machine`** names the state machine this type binds. Two types may bind the same one.

**`summary`** lists the attributes a listing returns by default. Omitted, it is the identifier attributes plus `state`.

### 2.1 Attributes

```text
attribute <name> <type> [?] [marking …]
```

A trailing `?` means the attribute may be absent. Without it the attribute must have a value once the object exists, and the creation transition that does not write it is rejected at publish.

| Type | Written |
|---|---|
| scalars | `string` `bool` `int` `decimal(p,s)` `money(ccy)` `timestamp` `duration` |
| enum | the enum's name |
| reference | `-> TypeName` |
| event reference | `event` |
| file | `file` |
| set | append `[]` to any of the above |

Markings, each optional and independent:

| Marking | Meaning |
|---|---|
| `identifier from <seq> format "<pattern>"` | Minted from a named sequence at creation; `{n}` is the number |
| `external "<source>"` | Owned by another system; uniqueness per source is automatic |
| `personal` | Subject to erasure; may not be copied into a non-personal attribute |
| `indexed` | Available to query filters, type-scan predicates and visibility |
| `default <expression>` | Applied at creation when the transition does not write it |

### 2.2 Relationships

```text
part <name> : <Type>[?] [inverse <name>]      # composition
ref  <name> : <Type>[?] [inverse <name>]      # reference
```

A collection end is written `<Type>[]`. `inverse` names the attribute or end on the other type that points back; declaring it is what lets an invariant traverse the relationship and what lets a cascade reach it (DESIGN.md §5.6). The checker verifies both ends agree.

`part` means composition: exclusive membership, lifetime bounded by the whole, cascaded by the whole's deletion, and re-parentable by a transition that writes the owner.

### 2.3 Derived attributes

```text
derive <name> [: <type>] = <expression>
```

Never stored. The type may be omitted and is inferred. Publishing rejects a cycle among derived attributes, and reports which are queryable: those over stored indexed attributes of the same object, with no aggregate and no `now` (DESIGN.md §5.2, §10).

### 2.4 Invariants

Two forms, because two are all the runtime can enforce in bounded work (DESIGN.md §5.6).

```text
invariant unique <attribute> [scope <expression>]     # shorthand
invariant <name>: <expression>                        # general
```

The general form is either a **traversal** invariant, which reads only relationships whose inverses are declared, or a **type scan**, which must be symmetric. The checker classifies it and says which it decided, so a type author never has to guess:

```text
invariant no_overlap:
          none(b in Booking where b.resource == this.resource
                              and b.state.category == live
                              and b.start < this.end
                              and b.end > this.start
                              and b.id != this.id)
```

## 3. Machines

```text
machine UnitLifecycle {
  state REQUESTED    category inbound
  state PROCUREMENT  category inbound
  state INTAKE       category inbound
  state AVAILABLE    category live
  state RESERVED     category live
  state SOLD         category closed
  state DEVELOPMENT  category live
  state RETIRED      category closed terminal
  state CANCELLED    category closed terminal

  create   request  -> REQUESTED { … }
  create   add      -> INTAKE    { … }

  action   record_label_print in INTAKE { … }
  do       inventorize INTAKE -> AVAILABLE { … }
  do       reserve AVAILABLE -> RESERVED only via Delivery.complete_sale, IntakeBatch.commit { … }
  do       cancel { REQUESTED PROCUREMENT INTAKE } -> CANCELLED { … }
  do       retire DEVELOPMENT -> RETIRED { … }
  assert   correct_state -> { AVAILABLE DEVELOPMENT RETIRED } { … }
}
```

Four transition keywords, differing only in the shape of their from- and to-state, so that a reader can see at a glance what kind of thing each one is:

| Keyword | From | To | Notes |
|---|---|---|---|
| `create` | nothing | one state | Named, so `create Robot.request(…)` is unambiguous |
| `do` | one state, a `{ set }`, or `any` | one state | The ordinary transition |
| `action` | `in <state or set or any>` | the same state | Sugar for a self-transition; it is one (DESIGN.md §5.4) |
| `assert` | `any` | a declared `{ set }` | The repair path; the target state is an input constrained to the set |

`only via P.q, R.s` marks a transition unrequestable and reachable only as a cascade from those parents. `proposable` marks one a caller lacking authority may file a Proposal for.

### 3.1 Inside a transition

```text
do inventorize INTAKE -> AVAILABLE {
  require label_printed_at is not null
  require model.manufacturer_serial_required -> manufacturer_serial is not null
  require model.label_photo_required -> count(p in photos) >= 1
}

do reserve AVAILABLE -> RESERVED only via Delivery.complete_sale, IntakeBatch.commit {
  input slot : -> DeliveryItem
  require in.slot.model == model      because dependent
  require in.slot.unit is null        because dependent
  set     binding := in.slot
  set     in.slot.unit := this
}
```

Three keyword-led line kinds, in this order:

```text
input   <name> : <type>[?] [default <expression>]
require <expression> [because <remedy class>]
```

then the outcome, one step per line:

```text
set     <path> := <expression>
create  <name> = <Type>.<transition>(<input> := <expression>, …)
call    <path>.<transition>(<input> := <expression>, …)
for     <name> in <collection> [where <expression>] [max <n>] { … }
repeat  <name> in 1..<expression> [max <n>] { … }
```

`in.<name>` reads an input, keeping it distinct from an attribute of the same name. A `set` whose right-hand side is an unsupplied optional input is skipped (DESIGN.md §5.4).

**Remedy class.** `because <class>` is optional; omitted, the checker infers it from the clause shape and reports what it inferred. It refuses to guess when the shape is ambiguous and asks for one, which is the only place the syntax makes an author write something a machine could sometimes work out.

### 3.2 A worked transition with a cascade

```text
do complete_sale PREPARATION -> DELIVERED {
  require type == DIRECT_SALE                                     because unreachable-from-here
  require all(c in checklist_items: c.checked)                    because self-serviceable
  require none(s in slots where s.role in { PRIMARY INCLUDED }
                            and s.unit is null)                   because dependent
  require all(r in check_records where r.required: r.result == PASS)
  require xero.invoice_valid(order_id)                            because dependent
  require actor.has(DELIVERY_COMPLETE)

  for s in slots where s.unit is not null {
    call s.unit.sell()
  }
  for s in slots where s.warranty_product is not null {
    create c = WarrantyContract.issue(robot   := s.unit,
                                      product := s.warranty_product,
                                      customer := customer)
  }
}
```

## 4. Visibility

```text
type Deal version 1 {
  …
  visible when actor.has(DEAL_VIEW_ALL)
            or owner.id == actor.id
            or owner.team in actor.teams
}
```

Absent, a type is visible to any actor the deployment admits. Failing it means not found (DESIGN.md §5.8).

## 5. Extension

```text
type BugInPlatform extends Bug version 1 {
  machine PlatformWorkflow
  attribute component -> Component indexed
}
```

`extends` inherits attributes, relationships, invariants and derived attributes. The machine is always bound explicitly and never inherited, which is what lets two projects share a type's data shape and differ in workflow (DESIGN.md §5.9). Querying `Bug` returns the family.

## 6. Sequences and enums

```text
sequence unit_serial scope model.family
sequence issue_key   scope project

enum RetirementReason { FAILED DAMAGED OBSOLETE LOST }
```

## 7. What the checker verifies

Every check below runs at publish, before any object is touched, and names the file, line and declaration. This list is the reason the syntax looks the way it does; each row points at the construct that makes the check local.

| # | Check | Made decidable by |
|---|---|---|
| 1 | A guard or outcome names an undeclared input | `input` lines in the same block; `in.` prefix distinguishes them from attributes |
| 2 | A declared input nothing reads | same |
| 3 | Derived attributes form a cycle | `derive` right-hand sides are expressions over declared names |
| 4 | An invariant traverses a relationship with no declared inverse | `inverse` on relationship declarations |
| 5 | A type-scan invariant is not symmetric | the checker pattern-matches the scan form and reports which class it decided |
| 6 | A required attribute a creation transition never writes | `?` marks the optional ones; every `create` is checked against the attribute list |
| 7 | A personal value written into a non-personal attribute | `personal` marking plus the static `set` list |
| 8 | A personal attribute declared as a required external identifier | markings are on one line, so this is a local check |
| 9 | `create Robot(…)` where the type has several `create` transitions | creations are named |
| 10 | A cascade cycle across transitions | `call` and `create` name their target transition statically |
| 11 | An only-via transition no parent actually cascades to | `only via` lists parents; the checker matches them against `call` sites |
| 12 | A non-terminal state with no outgoing transition | states and transitions are in one machine block |
| 13 | A state no creation or transition can reach | same |
| 14 | A transition that is not sweepable | guards are expressions over attributes marked `indexed` |
| 15 | A query filter or visibility predicate over an unindexed attribute | `indexed` marking |
| 16 | A bare `null` in a comparison | parser |
| 17 | An aggregate that does not bind its element | parser |
| 18 | A guard whose remedy class cannot be inferred and is not declared | `because` |
| 19 | A `for` over a relationship with no `max` where the collection is unbounded | `max` on the loop |
| 20 | Two ends of a relationship whose `inverse` names disagree | both ends are declared |
| 21 | A type version that did not advance while its content changed | `version` on the type |
| 22 | An expression using a construct the language does not have | parser |

The report also states, without failing: which invariants compile to a database constraint on the configured backend, which transitions are sweepable, which derived attributes are queryable, and how many pending proposals a publish would invalidate.

## 8. Worked example: the first consumer's unit

```text
module inventory

sequence unit_serial scope model.family
enum RetirementReason { FAILED DAMAGED OBSOLETE LOST }
enum CancellationReason { ORDER_CANCELLED MISSING_FROM_SHIPMENT REJECTED_QA RETURNED_SUPPLIER OTHER }

machine UnitLifecycle {
  state REQUESTED   category inbound
  state PROCUREMENT category inbound
  state INTAKE      category inbound
  state AVAILABLE   category live
  state RESERVED    category live
  state SOLD        category closed
  state DEVELOPMENT category live
  state RETIRED     category closed terminal
  state CANCELLED   category closed terminal

  create request -> REQUESTED {
    input model : -> RobotModel
    input order : -> ProcurementOrder
    require actor.has(PROCUREMENT_CREATE)
    set model := in.model
    set procurement_order := in.order
  }

  create add -> INTAKE {
    input model : -> RobotModel
    require actor.has(INVENTORY_CREATE)
    set model := in.model
  }

  action record_label_print in INTAKE {
    require actor.has(INVENTORY_CREATE)
    set label_printed_at := now
  }

  action capture_manufacturer_serial in INTAKE {
    input value : string
    require actor.has(INVENTORY_CREATE)
    set manufacturer_serial := in.value
  }

  do inventorize INTAKE -> AVAILABLE {
    require label_printed_at is not null
    require model.manufacturer_serial_required -> manufacturer_serial is not null
    require model.label_photo_required -> count(p in photos) >= 1
  }

  do reserve AVAILABLE -> RESERVED only via Delivery.bind_slot, IntakeBatch.commit {
    input slot : -> DeliveryItem
    require in.slot.model == model   because dependent
    require in.slot.unit is null     because dependent
    set binding := in.slot
    set in.slot.unit := this
  }

  do release RESERVED -> AVAILABLE only via Delivery.unbind_slot, Delivery.cancel {
    set in.slot.unit := null
    set binding := null
  }

  do sell RESERVED -> SOLD only via Delivery.complete_sale { }

  do deliver_internal RESERVED -> DEVELOPMENT only via Delivery.complete_internal { }

  do retire DEVELOPMENT -> RETIRED {
    input reason : RetirementReason
    require actor.has(INVENTORY_RETIRE)
    set retirement_reason := in.reason
  }

  do cancel { REQUESTED PROCUREMENT INTAKE } -> CANCELLED {
    input reason : CancellationReason
    require actor.has(PROCUREMENT_CANCEL)
    set cancellation_reason := in.reason
  }

  assert correct_state -> { AVAILABLE DEVELOPMENT RETIRED CANCELLED } {
    require actor.has(INVENTORY_ASSERT)
    input reason : string
  }
}

type Robot version 1 {
  tracking serial
  machine  UnitLifecycle
  summary  serial, model, state

  attribute serial              string identifier from unit_serial format "RBT-{n}" indexed
  attribute model               -> RobotModel indexed
  attribute manufacturer_serial string?
  attribute label_printed_at    timestamp?
  attribute photos              file[]
  attribute retirement_reason   RetirementReason?
  attribute cancellation_reason CancellationReason?
  attribute legacy_id           string? external "legacy"

  ref binding : DeliveryItem? inverse unit
  ref procurement_order : ProcurementOrder? inverse units
  ref engagement_lines : EngagementLine[] inverse unit

  derive leasable = state == DEVELOPMENT
                and none(l in engagement_lines where l.engagement.state != CLOSED)

  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != CLOSED) <= 1
}
```

`Accessory` and `SparePart` bind the same machine and differ only in their attributes, which is what "two types that share a lifecycle bind the same machine" buys.

## 9. Open questions for this draft

1. Whether `action` earns its keyword or should just be `do X in S -> S`.
2. Whether `set in.slot.unit := this` — writing through an input into another object — is too easy to do by accident, given it is a write to a different object with no visual marker.
3. Whether `max` on loops should be required rather than checked.
4. Whether a file should be allowed to contain several types, or one type per file as an enforced convention.
5. Whether the remedy-class inference table is small enough to be predictable, or whether `because` should always be required.
