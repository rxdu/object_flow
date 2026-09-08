# The declaration syntax

Status: **draft, iteration 4** (2026-09-08). The format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).

Two goals shape every choice, and where they conflict the second wins.

1. **The common case should cost nothing to write and nothing to read.**
2. **Everything the design promises to check must be decidable from the text.** §10 lists the checks.

## Iterations

Three reviews have been run: two engineers building real systems in it, one auditing it against the model. What each changed.

**Iteration 2** made machines optional, stopped a write from leaving the object being transitioned, unified references, named guards, and added twelve constructs the model needed and iteration 1 could not express.

**Iteration 3** reversed two of those. `internal` had replaced the `only via` authority list and thereby deleted the guarantee it existed to make: with no declared list, every call site became an authorised parent by construction, so a new module could write `call unit.sell()` and grant itself the power to sell a unit with no delivery. And the implicit machine existed in the runtime but not in the file, so its state had no category, no terminal state and no creation transition. Iteration 3 also made inverses runtime-maintained, restored repeat-over-count, and closed an approval-forgery hole.

**Iteration 4** fixes what the model audit found still wrong:

| Problem | Change |
|---|---|
| A machine holds transitions; the invariants, attributes and markings they read live on the type. A shared machine could not say what it needs of a binder, so several checks were undecidable from a machine body | `requires` on a machine, verified against every binder (§4.1) |
| `tracking quantity` as an opt-in made absence mean serial — the inference ADR-0050 rejects by name | `tracking` is explicit and mandatory (§2) |
| Eager and deferred moved from the guard to the evaluator function, so two guards over one function could not differ | Back on the guard (§5.1) |
| Deletion cascading into parts was asserted in prose: no step, no `limit`, no named transition, invisible to cycle detection | `cascade` on the `part` end (§3.2) |
| `default` was a runtime write appearing in no outcome and no event, a new exception to the mediated property | Defined as sugar the checker expands into every creation's outcome (§3.1) |
| Four independent version counters with no rule for which one an object records | An object records its **type**'s version; bumping a machine, enum or evaluator forces a bump in every binder (§1) |
| Two things the model makes warnings were publish failures | Moved to the reported-not-failed list (§10) |
| The conditional expression was missing, and `?` was already optionality | `if … then … else …` (§8) |
| `final` and `internal` renamed the model's `terminal` and `only via` with no ADR | `terminal` restored; `only via` is the primary form and `internal` its unlisted variant (§4.2) |
| Aggregates required a body the model's own examples omit | The body is optional for every boolean aggregate (§8) |

## 1. Shape of a file

A file is a module, extension `.ok`. Declarations may appear in any order; forward references are fine. `#` begins a comment.

```text
module inventory
use   commerce.{Customer}

capability INVENTORY_CREATE, INVENTORY_RETIRE, INVENTORY_ASSERT,
           ERASE_PERSONAL, DELIVERY_EDIT, DELIVERY_COMPLETE, DEAL_VIEW_ALL
category   inbound, live, closed

enum      RetirementReason version 1 { FAILED, DAMAGED, OBSOLETE, LOST }
sequence  unit_serial version 1
evaluator xero version 1 { … }
machine   UnitLifecycle version 2 { … }
type      Robot version 3 { … }
```

Nine top-level forms: `module`, `use`, `capability`, `category`, `enum`, `sequence`, `evaluator`, `machine`, `type`.

**`capability` and `category` declare vocabularies**, because both are otherwise bare identifiers a typo turns into silence: a mistyped capability is a guard nobody can satisfy, a mistyped category a family-wide guard that never matches.

**Versions.** Every named declaration carries one. An object and an event record the version of their **type**. Because a type's behaviour depends on the machine, enums and evaluators it uses, advancing any of those requires advancing every type that binds them, which the checker enforces; otherwise a type's recorded version would not identify the rules that applied.

## 2. Types

```text
type Robot version 3 {
  tracking serial
  machine  UnitLifecycle
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
                and none(l in engagement_lines where l.engagement.state != CLOSED)

  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != CLOSED) <= 1
}
```

**`tracking`** is `serial` or `quantity`, and is mandatory. Neither is a default, because inferring it from an incidental property is the implicit rule the model rejects by name.

### 2.1 Every type has an explicit lifecycle

A type either **binds** a shared machine or **declares one inline** with `states`. Exactly one of the two, never neither:

```text
type ChecklistItem version 1 {
  tracking serial
  states   ACTIVE category live, DELETED category closed terminal

  owner delivery : Delivery inverse checklist_items
  attr  label   string
  attr  checked bool default false

  create add -> ACTIVE accepts label {
    require may: actor.has(DELIVERY_EDIT) because delegable
  }
  act tick   at ACTIVE { require may: actor.has(DELIVERY_EDIT) because delegable
                         set checked := true }
  act untick at ACTIVE { require may: actor.has(DELIVERY_EDIT) because delegable
                         set checked := false }
  do  delete ACTIVE -> DELETED only via Delivery.delete { }
}
```

Iteration 2 gave such a type an implicit machine, which saved a line and cost far more: no category, so a delete guard reading `state.category` over referrers was undecidable; no terminal state; no way to write its creation; no cascade target for the whole's deletion; and no migration path when it later needed a real lifecycle. Promotion is now moving the `states` line and the transitions into a `machine` block and binding it.

## 3. Attributes and references

### 3.1 Attributes

```text
attr  <name> <type>[?] [marking …]
count <name>                         # a non-negative integer of stock, quantity types only
```

`?` means the value may be absent. Without it the value must be present whenever the object exists, checked at creation and at every write.

| | |
|---|---|
| scalars | `string` `bool` `int` `decimal(p,s)` `money(ccy)` `timestamp` `duration` |
| enum, event, file | `RetirementReason`, `event`, `file` |
| set | append `[]` |

References are always `ref`, `part` or `owner`, never an attribute type.

| Marking | Meaning |
|---|---|
| `identifier from <seq> [scoped by <ref>] format "<pattern>"` | Minted at creation before the outcome runs. A scope may name only a reference the creation writes |
| `unique` / `unique in scope` / `unique where <expr>` | A uniqueness invariant: global, per the identifier's sequence scope, or partial. **`identifier` does not imply it**, and a scoped sequence repeats across scopes |
| `external "<source>"` | Owned by another system; uniqueness per source is automatic and partial over absent values |
| `personal` | Subject to erasure. A required attribute may not be `personal`, since erasure writes absence |
| `indexed` | Available to query filters, type-scan predicates and visibility |
| `default <expr>` | **Sugar.** The checker expands it into a `set` at the head of every creation that does not write the attribute, so it appears in the rule set and in the creation's event like any other write. There is no ungated write |

### 3.2 References and composition

```text
ref   <name> : <Type>[?|[]] [inverse <name>] [marking …]
part  <name> : <Type>[]     inverse <name> cascade <Type>.<transition> limit <n>
owner <name> : <Type>       inverse <name>
```

`part` and `owner` are the two ends of a composition: exclusive membership, lifetime bounded by the whole, and re-parentable by writing the `owner`. **`cascade` names the part transition the whole's deletion drives, and its bound**, because iteration 3 asserted that cascade in prose with no step, no limit, no named transition, and nothing for the cycle check to see.

A reference may omit `inverse`, which only means no back-reference is named for traversal. It remains visible to `referrers` and to the deletion guard.

### 3.3 Inverses are maintained by the runtime

When both ends are declared, writing either updates the other in the same transaction, recording an event on both. An author never writes the companion update, which iteration 2 required and which made the release path a cascade cycle the checker rejects.

## 4. Machines and transitions

```text
machine UnitLifecycle version 2 {
  requires attr label_printed_at timestamp?
  requires attr photos file[]
  requires ref  model : RobotModel
  requires invariant one_open_engagement

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
    require may: actor.has(INVENTORY_CREATE) because delegable
  }
  do ship    REQUESTED   -> PROCUREMENT { require may: actor.has(INVENTORY_CREATE) because delegable }
  do receive PROCUREMENT -> INTAKE      { require may: actor.has(INVENTORY_CREATE) because delegable }

  do inventorize INTAKE -> AVAILABLE {
    require labelled: label_printed_at is not null            because unreachable-from-here
    require photo:    model.label_photo_required
                      implies count(p in photos) >= 1         because unreachable-from-here
  }

  do reserve AVAILABLE -> RESERVED only via Delivery.bind_slot {
    input slot : DeliveryItem
    require matches:  $slot.model == model  because dependent
    require unfilled: $slot.unit is null    because dependent
    set binding := $slot
  }

  do sell RESERVED -> SOLD only via Delivery.complete_sale { }
  do deliver_internal RESERVED -> DEVELOPMENT only via Delivery.complete_internal { }

  do retire DEVELOPMENT -> RETIRED accepts retirement_reason {
    require may: actor.has(INVENTORY_RETIRE) because delegable
  }
  do cancel { REQUESTED, PROCUREMENT, INTAKE } -> CANCELLED {
    require may: actor.has(INVENTORY_RETIRE) because delegable
  }

  act record_label_print at INTAKE {
    require may: actor.has(INVENTORY_CREATE) because delegable
    set label_printed_at := now
  }

  assert correct_state -> { AVAILABLE, DEVELOPMENT, RETIRED } {
    input to : state
    input reason : string
    input admits : invariant[]?
    require may: actor.has(INVENTORY_ASSERT) because delegable
    may admit one_open_engagement
  }
}
```

### 4.1 `requires`

A machine states what a binding type must provide: the attributes, references and invariants its transitions and admissions name. The checker verifies every binder supplies them, with compatible types.

Without it a shared machine reads names that live on the type, so a second binder had to coincidentally declare the same ones, and a check like "an admission naming an invariant the type declares" was undecidable from the machine body.

### 4.2 `only via` and `internal`

```text
do sell RESERVED -> SOLD only via Delivery.complete_sale, Lease.convert { }
do note_seen at ACTIVE internal { … }
```

`only via` makes a transition unrequestable and **names the transitions that may cascade to it**. The checker verifies the list against the call sites it derives: a `call` from a transition not named is a publish error, and a name that never calls it is a publish error too.

`internal` is the same unrequestability with no list, for a transition whose authority does not matter. The checker prints the derived parents.

Iteration 2 offered only `internal`, which deleted the guarantee: with no declared list, every call site became an authorised parent by construction, so a new module could grant itself the power to sell a unit with no delivery, and no publish could refuse it. Checking a declared list against a derived one costs nothing to maintain when it is right.

Neither carries actor guards, since the parent's authority is theirs; writing one is a publish error rather than a guard silently dropped. Neither may be `proposable`, since an unrequestable transition can never yield the verdict that makes a proposal meaningful.

| Keyword | From | To |
|---|---|---|
| `create` | nothing | one state |
| `do` | a state, a `{ set }`, or `any` | one state |
| `act` | `at` a state, a set, or `any` | the same state |
| `assert` | `any` | an input drawn from a declared set (§6.3) |
| `erase` | any state, terminal included | the same state |

`terminal` means no `do` may leave the state.

## 5. Inside a transition

Clauses appear in one order: `input`, `accepts`, `require`, then the outcome.

### 5.1 Inputs and guards

```text
input   <name> : <type>[?] [default <expr>]
accepts <attribute>, …
require <name>: <expression> [eager|deferred] [because <remedy class>]
```

**`accepts`** declares that these attributes are supplied by the caller and written directly. `accepts comment` is exactly `input comment : <the attribute's type>` plus `set comment := $comment`, optionality included. It removes the input–argument–write triplication that made recording one approval eighteen lines.

**Guard names are always required**, because a verdict carries the failing clause's name, and that name should not appear or change shape when someone adds a second guard. A rename is then visibly a change to the type's public surface.

**`eager` or `deferred`** marks a guard that calls an external evaluator, not the evaluator itself, so two guards over one function may differ. Omitted, a guard is eager.

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

**Every write stays on `this`**, and its target may be an attribute or a relationship end; writing an `owner` is how a part is re-parented, which is what makes splitting an object expressible. Reaching another object is `call`, which runs its transition, evaluates its guards and records an event on it.

A path may be rooted at `this`, a relationship, a loop binder or an input.

**Optional inputs.** A step or argument whose expression is *exactly* an unsupplied optional input is skipped. Any larger expression containing one is a publish error, so `set amount := $amount` and `set amount := $amount + 0` cannot be confused. A skipped write does not stamp the attribute's last-written index, so a no-op edit does not invalidate an approval.

`limit` is mandatory on every loop and bounds the whole request rather than each loop.

## 6. The remaining constructs

### 6.1 Supersession

```text
state MERGED category closed terminal superseding

do merged_into ACTIVE -> MERGED only via Contact.merge_in {
  input successor : Contact
  supersede $successor
}
```

The successor may be an input or a name bound by an earlier `create`. A type may declare `ref supersedes : <Type>? inverse superseded_by` to navigate the other way. Self-supersession and cycles are publish errors.

### 6.2 External evaluators

```text
evaluator xero version 1 {
  fn invoice_valid(order_id : string) fresh 30 min
  fn contact_exists(id : string)
}
```

A function declares its argument types and how stale a verdict may be. Every evaluator returns a verdict, so no return type is written. A verdict too stale to use is refused as `temporal` whatever the guard declares.

### 6.3 Assertions and admissions

```text
input admits : invariant[]?
may admit one_open_engagement, serial_unique
```

`may admit` lists the invariants an assertion is *permitted* to violate; the `admits` input carries the ones a particular request actually admits, so an admission names an object and an invariant rather than blanket-admitting on every use. An assertion must declare a capability guard and a `reason` input.

### 6.4 Erasure and correction

```text
erase forget {
  input reason : string
  require may: actor.has(ERASE_PERSONAL) because delegable
  for a in approvals limit 1000 { call a.forget() }
}

act correct_delivery_date at any {
  input value  : timestamp
  input reason : string
  corrects date
  set date := $value
}
```

An `erase` transition erases every `personal` attribute of its object, runs from **any** state including a terminal one, since erasure requests arrive for closed accounts, and may cascade into parts. A type with `personal` attributes and no `erase` is a publish error, as is a `personal` attribute that is required.

`corrects <attribute>, …` produces the `corrected` provenance; the checker requires the outcome to write exactly the attributes named.

### 6.5 Deletion guards and `referrers`

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
removed attr  old_name    -> new_name
```

A publish that drops a state or an attribute carries the mapping in the file that drops it. A rename is a removal plus an addition with a mapping.

### 6.7 `this_event`, visibility, extension

`this_event` yields the event the current transition will record, and an `event`-typed **input accepts only `this_event`**, which is what stops an approval being forged with an old one.

```text
visible when actor.has(DEAL_VIEW_ALL) or owner.id == actor.id
type BugInPlatform extends Bug version 1 { machine PlatformWorkflow  … }
```

## 7. Quantity tracking

```text
type Stock version 1 {
  tracking quantity
  states   ACTIVE category live

  ref   model : ProductModel inverse stock indexed
  count on_hand
  count reserved

  derive available = on_hand - reserved indexed
  invariant reserved_bounded: reserved >= 0 and reserved <= on_hand
}
```

A `quantity` type declares at least one `count`. A `derive` may be `indexed` when it reads only stored indexed attributes of the same object and no clock, which is what makes a low-stock query possible.

`reserved_bounded` is a **local** invariant, reading only this object.

## 8. Expressions and types

The expression language is DESIGN.md §5.7. Its types are the attribute types of §3.1 plus `verdict`, and the checker types every expression:

- arithmetic is `int` with `int`, `decimal` with `decimal` of the same scale, `money` with `money` of the same currency, and `timestamp ± duration`;
- comparison needs both sides of one type; `is null` and `is not null` take anything and yield `bool`;
- `and`, `or`, `not`, `implies` take and yield `bool`; a guard must be `bool`;
- `count` yields `int`, `sum`/`min`/`max` the element type, `all`/`any`/`none` `bool`;
- `if <bool> then <a> else <b>` yields the common type of `a` and `b`, and is allowed only in a derived attribute.

Aggregates are `agg(<name> in <collection> [where <filter>][: <body>])`. The body is required for `sum`, `min` and `max`, and optional for `count`, `all`, `any` and `none`, whose examples in the model omit it.

Precedence, highest first: paths and calls; unary `not`; `* /`; `+ -`; comparison, `in`, `is null`; `and`; `or`; `implies`; `if`. A line continues while it ends in an operator or an open bracket. Literals: `30 min`, `2 h`, `USD 19.99`, `"text"`, `true`, `{A, B}`.

## 9. Lexical rules

**Reserved words** may not be used as names:

`module use capability category enum sequence evaluator machine type version tracking serial quantity states state requires attr count ref part owner inverse cascade derive invariant unique scope where from scoped by format default external personal indexed identifier summary visible when extends create do act assert erase removed only via internal proposable terminal superseding supersede input accepts require because eager deferred set add remove call for limit corrects may admit in at is null not and or implies if then else true false any all none sum min max now actor this this_event referrers fn fresh string bool int decimal money timestamp duration event file verdict`

Duration units `min` and `h` are reserved only after a numeric literal, which is the one place the aggregate `min` cannot appear.

**Symbols.** `->` is a to-state, an assertable-state set, a removal mapping and an evaluator's absent return; it is never implication and never a reference type. `implies` is implication. `$name` is an input. `:=` assigns and binds an argument; `=` binds a created name and defines a derivation; `==` compares. `{a, b}` is a collection literal and `{ … }` a block; the two never occupy the same position. `:` ascribes a type and names a guard or invariant. `in` is a binder and membership, both meaning "element of".

`summary`, `accepts`, `corrects`, `capability`, `category` and `changed_since` take bare comma-separated lists, not brace literals.

## 10. What the checker verifies

Each check names the file, line and declaration. Checks 22, 23 and 30 need the previously published declaration; the rest are decidable from the text.

| # | Check |
|---|---|
| 1 | A guard or outcome names an undeclared input |
| 2 | A `call` or `create` with an unknown argument, a missing required one, or one of the wrong type |
| 3 | Derived attributes form a cycle |
| 4 | An invariant traverses a relationship with no declared inverse |
| 5 | An invariant fits none of the three forms; the report says which it decided |
| 6 | A type-scan invariant that is not symmetric |
| 7 | An invariant or type-scan guard reading an unindexed attribute |
| 8 | A required attribute a creation never writes, one a later transition sets to absent, or one written only from an optional input |
| 9 | A required attribute marked `personal`; a type with `personal` attributes and no `erase` |
| 10 | A personal value reaching a non-personal attribute by write, cascade argument or derivation |
| 11 | A personal attribute declared as a required external identifier |
| 12 | A cascade cycle across transitions, including `part … cascade` targets |
| 13 | A `call` to an `only via` transition from a parent it does not name; a named parent that never calls it; an `only via` or `internal` transition nothing reaches |
| 14 | An actor guard on an `only via` or `internal` transition; either marked `proposable` |
| 15 | A non-terminal state with no outgoing transition; a state nothing can reach; a `do` leaving a `terminal` state; a machine with no `terminal` state; a state with no category |
| 16 | A type that neither binds a machine nor declares `states`, or does both; a machine `requires` a binder does not satisfy |
| 17 | A write whose target is not an attribute or relationship end of `this`; an `add` or `remove` on a target that is not set-valued |
| 18 | A `part`/`owner` pair disagreeing on name or cardinality; a `create` of a part that never writes its `owner`; a `part` whose `cascade` names a transition the child does not have |
| 19 | A capability, category, state, attribute, enum member or evaluator function that is not declared |
| 20 | An unnamed guard |
| 21 | A `for` without `limit`; a bare `null` in a comparison; an unbound aggregate; a reserved word as a name; `if` outside a derived attribute |
| 22 | A version that did not advance while its content changed; a type binding a machine, enum or evaluator whose version advanced without its own |
| 23 | A state or attribute removed with no `removed` mapping |
| 24 | An expression that does not type |
| 25 | An `event`-typed input receiving anything but `this_event` |
| 26 | A `supersede` outside a `superseding` to-state, a `superseding` state entered without one, self-supersession or a cycle |
| 27 | A `may admit` naming an invariant the binding type does not declare |
| 28 | `corrects` naming attributes the outcome does not write, or the reverse |
| 29 | An `assert` with no capability guard or no `reason` input; an `erase` with no `reason` input |
| 30 | A family member whose machine lacks a transition another member's cascade calls |
| 31 | A `quantity` type with no `count`; a `count` on a `serial` type |
| 32 | An invariant reading `now`, which no after-write check can enforce |
| 33 | Duplicate transition, state, attribute or guard names within one scope |

Reported without failing: a declared input nothing reads; a guard whose remedy class was inferred, and what was inferred; which invariants compile to a database constraint on this backend; which transitions are sweepable; which derived attributes are queryable; the derived parent list for each `internal` transition; and how many pending proposals a publish would invalidate.

## 11. Open questions for iteration 5

1. Whether `accepts` should also be available on `do` and `act`, not only `create`.
2. Whether `only via` should be mandatory on a transition entering a closed-category state, which is where authority matters most.
3. Whether runtime-maintained inverses are surprising, given every other write is explicit.
4. Whether `referrers` should be filterable by type.
5. Whether `states` inline and `machine` shared should look more alike.
6. Whether mandatory `limit` and mandatory guard names are worth their friction, both being deviations from the model that need an ADR either way.
