# The declaration syntax

Status: **draft, iteration 2** (2026-09-08). This is the format in which an ObjectKeeper model is written. It is the primary artefact of a declarative store: the readable rule set, the agent tool schemas, the API and the publish-time checks are all projections of it ([`../DESIGN.md`](../DESIGN.md) §3, §10).

Two goals shape every choice, and where they conflict the second wins.

1. **The common case should cost nothing to write and nothing to read.**
2. **Everything the design promises to check must be decidable from the text.** §9 lists the checks and names the construct that makes each one local.

## What changed in iteration 2

A reviewer wrote four real declarations in iteration 1 and reported what went wrong. Six of its findings changed the language rather than the prose.

| Problem | Change |
|---|---|
| Every composition part needed its own machine, four states and a `tracking` decision. `Delivery` cost four machines | **`machine` is optional.** A type without one gets an implicit single-state lifecycle, so a checklist item is six lines (§2, §4.1) |
| `set in.slot.unit := this` wrote into another object with no event on it, no guard of its own, and nothing in its history — while the model says a write is "on this object" | **`set` cannot leave `this`.** Reaching another object is `call`, which records an event there (§5.2) |
| `only via P.q, R.s` is a reverse dependency maintained in the callee, in another module, and splitting a transition forces an edit to every one of them | **`internal` replaces it.** The checker derives the parent list from call sites and prints it in the rule set (§5.4) |
| `attribute x -> T indexed` and `ref x : T inverse y` were disjoint: a reference could be filterable or traversable, never both | **One `ref` form** taking every marking (§3.2) |
| `->` meant four things and `in` meant four things, three of each able to appear in one line | `implies` for implication, `$name` for inputs, `->` only for a to-state (§8) |
| A verdict must carry "the failing clause", and clauses had no identity | **Guards may be named**, and the verdict carries the name (§5.1) |

Twelve constructs the model requires and iteration 1 had no syntax for are in §6. Two of its findings were defects in the model itself, not the syntax, and are fixed by ADR-0055.

## 1. Shape of a file

A file is a module, extension `.ok`. Declarations may appear in any order and forward references are fine. `#` begins a comment.

```text
module inventory
use   commerce.{Customer, Invoice}

capability INVENTORY_CREATE, INVENTORY_RETIRE, PROCUREMENT_CANCEL
category   inbound, live, closed

enum     RetirementReason { FAILED, DAMAGED, OBSOLETE, LOST }
sequence unit_serial
evaluator xero { … }
machine  UnitLifecycle { … }
type     Robot version 3 { … }
```

Eight top-level forms: `module`, `use`, `capability`, `category`, `enum`, `sequence`, `evaluator`, `machine`, `type`. `use` imports names so cross-module references are not qualified everywhere; without it a name from another module is written `commerce.Customer`.

**`capability` and `category` declare vocabularies.** Both are otherwise bare identifiers that a typo turns into a silent nothing: a mistyped capability is a guard nobody can satisfy, and a mistyped category is a family-wide guard that never matches. Declaring them makes both a publish error, and lets the printed rule set enumerate what a deployment must define.

## 2. Types

```text
type Robot version 3 {
  machine  UnitLifecycle
  summary  serial, model, state

  attr serial  string  identifier from unit_serial scoped by model.family
                       format "RBT-{n}" indexed unique
  attr model   Ref<RobotModel>  indexed
  attr manufacturer_serial string?
  attr photos  file[]

  ref  binding : DeliveryItem? inverse unit indexed

  derive leasable = state == DEVELOPMENT
                and none(l in engagement_lines where l.engagement.state != CLOSED)

  invariant one_open_engagement:
            count(l in engagement_lines where l.engagement.state != CLOSED) <= 1
}
```

**`machine` is optional.** A type that names none gets an implicit machine with a single state `ACTIVE` and, if the type is deletable, `DELETED`. Its transitions are declared directly in the type body. This is what a composition part usually wants, and iteration 1's requirement that every part declare a full machine was the single largest cost in the language.

**`tracking quantity`** is an opt-in marking for stock types (§7). Its absence means the ordinary case, one object per thing; iteration 1 required the choice on every type, including types that hold no stock.

**`version`** is the declaration version. A machine, enum, sequence and evaluator each carry their own `version` too, because a shared machine that changes must advance something the checker can see, and in iteration 1 it advanced nothing.

## 3. Attributes and references

### 3.1 Attributes

```text
attr <name> <type>[?] [marking …]
```

`?` means the value may be absent. Without it the value must be present whenever the object exists, which is checked at creation **and** at every write.

| | |
|---|---|
| scalars | `string` `bool` `int` `decimal(p,s)` `money(ccy)` `timestamp` `duration` |
| enum, reference, event, file | `RetirementReason`, `Ref<RobotModel>`, `event`, `file` |
| set | append `[]` |

| Marking | Meaning |
|---|---|
| `identifier from <seq> [scoped by <expr>] format "<pattern>"` | Minted at creation before the outcome runs, so a scope expression may read only attributes the creation's inputs supply |
| `unique [where <expr>]` | Shorthand for a uniqueness invariant, optionally partial. **`identifier` does not imply it**, since a scoped sequence repeats across scopes |
| `external "<source>"` | Owned by another system; uniqueness per source is automatic and partial over absent values |
| `personal` | Subject to erasure; may not reach a non-personal attribute by write, cascade argument or derivation |
| `indexed` | Available to query filters, type-scan predicates and visibility |
| `default <expr>` | Applied at creation when the transition does not write it |

### 3.2 References and relationships

One form, taking every marking an attribute takes:

```text
ref  <name> : <Type>[?|[]] [inverse <name>] [marking …]
part <name> : <Type>[?|[]] [inverse <name>] [marking …]
```

`part` is composition: exclusive membership, lifetime bounded by the whole, cascaded by the whole's deletion, re-parentable by a transition that writes the owner. The child end of a composition is declared `owner`, which is what tells the checker that exclusivity and lifetime bounding apply to it and that a write to it is a re-parenting:

```text
type DeliveryItem version 1 {
  owner delivery : Delivery inverse slots
  ref   unit : Robot? inverse binding
  ref   remembered_unit : Robot? inverse none      # a second ref to the same type
}
```

`inverse none` declares a reference deliberately one-way. It is then unusable by an invariant or a cascade, which the checker enforces rather than leaving to be discovered.

## 4. Machines and transitions

```text
machine UnitLifecycle version 2 {
  state REQUESTED   category inbound
  state AVAILABLE   category live
  state RETIRED     category closed final

  create request -> REQUESTED { … }
  do     inventorize INTAKE -> AVAILABLE { … }
  do     reserve AVAILABLE -> RESERVED internal { … }
  do     cancel { REQUESTED, PROCUREMENT, INTAKE } -> CANCELLED { … }
  do     reopen CANCELLED -> PREPARATION { … }
  act    record_label_print in INTAKE { … }
  assert correct_state -> { AVAILABLE, DEVELOPMENT, RETIRED } { … }
  erase  forget { … }
}
```

`final` replaces iteration 1's `terminal`, which was decorative: a `do` leaving a `final` state is now a publish error, and the walkthrough's `reopen CANCELLED -> PREPARATION` says plainly that `CANCELLED` is not final.

| Keyword | From | To |
|---|---|---|
| `create` | nothing | one state |
| `do` | a state, a `{ set }`, or `any` | one state |
| `act` | `in` a state, a set, or `any` | the same state |
| `assert` | `any` | an input drawn from a declared set (§6.3) |
| `erase` | `any` non-final | the same state, or a declared one (§6.4) |

`act` is sugar for a `do` whose to-state equals its from-state, and it is the only sugar in the language. It exists because "edit fields without transitioning" is the commonest transition of all once every write is a transition, and because a reader scanning for what changes a lifecycle should be able to skip them.

### 4.1 A part type in full

```text
type ChecklistItem version 1 {
  owner delivery : Delivery inverse checklist_items
  attr  label   string
  attr  checked bool default false

  act tick    { require actor.has(DELIVERY_EDIT)  set checked := true  }
  act untick  { require actor.has(DELIVERY_EDIT)  set checked := false }
}
```

No machine, no states, no `tracking`. In iteration 1 this type cost twenty-odd lines and four decisions nobody wanted to make.

## 5. Inside a transition

```text
do complete_sale PREPARATION -> DELIVERED {
  input  none

  require is_sale:      type == DIRECT_SALE                        because unreachable-from-here
  require checklist:    all(c in checklist_items: c.checked)       because self-serviceable
  require slots_filled: none(s in slots where s.role in {PRIMARY, INCLUDED}
                                          and s.unit is null)      because dependent
  require checks_pass:  all(r in check_records where r.required: r.result == PASS)
                                                                   because dependent
  require invoice:      xero.invoice_valid(order_id)               because dependent
  require may:          actor.has(DELIVERY_COMPLETE)               because delegable

  for s in slots where s.unit is not null limit 500 {
    call s.unit.sell()
  }
  for s in slots where s.warranty_product is not null limit 500 {
    create WarrantyContract.issue(robot := s.unit, product := s.warranty_product,
                                  customer := customer)
  }
}
```

### 5.1 Guards

```text
require [<name>:] <expression> [because <remedy class>]
```

A guard may be **named**, and the name is what an unsatisfied verdict carries, so a caller matches on `slots_filled` rather than on a line number or a pretty-printed string that changes with an edit. Naming is optional for a single-clause transition and required where a transition has more than one guard, because that is exactly when a verdict has to say which failed.

`because` is optional; omitted, the checker infers the class and prints what it inferred. It refuses to guess where the shape is ambiguous, which iteration 1 demonstrated by inferring inconsistently for two structurally identical clauses four lines apart.

### 5.2 Outcome steps

```text
set    <attribute> := <expression>          on this object only
add    <attribute> := <expression>          insert into a set-valued attribute
remove <attribute> := <expression>          delete from a set-valued attribute
call   <path>.<transition>(<arg> := <expr>, …)
create [<name> =] <Type>.<transition>(<arg> := <expr>, …)
for    <name> in <collection> [where <expr>] limit <n> { … }
```

**`set` cannot leave `this`.** Its left-hand side is an attribute of the object being transitioned, never a path through a reference or an input. Reaching another object is `call`, which runs that object's own transition, evaluates its guards and records an event on it. Iteration 1 allowed `set $slot.unit := this`, which parses, passes every check, and changes another object with no guard, no event and nothing in its history — the precise thing the mediated property forbids, produced by the language's own convenience.

**Optional inputs.** A `set`, `add`, `remove` or argument whose expression is exactly an unsupplied optional input is skipped. It must be *exactly* that: `set amount := $amount` is skipped, and `set amount := $amount + 0` is an expression containing an unsupplied input, so it evaluates to unknown and would clear the field. The checker rejects any expression that merely *contains* an unsupplied-optional input outside the bare form, so the two cannot be confused. A skipped write does not stamp the attribute's last-written index, so a no-op edit does not invalidate an approval.

**`create`'s name is optional** and required only if a later step reads it.

`limit` caps a loop's fan-out and is **mandatory**, because iteration 1's check for "unbounded collection" was undecidable: nothing in the syntax bounds a collection, so the check could never fire.

## 6. The constructs iteration 1 was missing

### 6.1 Supersession

```text
state MERGED category closed final superseding

do merged_into ACTIVE -> MERGED internal {
  input successor : Ref<Contact>
  supersede $successor
}
```

`superseding` marks the state; `supersede` is the outcome step that records the pointer, which `get(id, follow)` and `history(id, follow)` then follow. The successor may be an input or a name bound by a `create` in the same outcome, which is what lets an order supersede the cart that created it.

### 6.2 External evaluators

```text
evaluator xero version 1 {
  fn invoice_valid(order_id : string) -> verdict  eager  fresh 30 min
  fn contact_exists(id : string)      -> verdict  deferred
}
```

An evaluator declares its functions, their argument types, and per function whether it is eager or deferred and how stale a verdict may be. Without this, `xero.invoce_valid(…)` is indistinguishable from a real call, which is what iteration 1 shipped.

### 6.3 Assertions and admissions

```text
assert correct_state -> { AVAILABLE, DEVELOPMENT, RETIRED } {
  input to     : state
  input reason : string
  require actor.has(INVENTORY_ASSERT) because delegable
  admits one_open_engagement
}
```

`input to : state` names the target-state input, so a guard can constrain it and a caller knows what to supply. `admits` lists the invariants this assertion may knowingly violate, which is the difference between a refusal and a recorded admission on the most security-relevant path in the system.

### 6.4 Erasure and correction

```text
erase forget {
  require actor.has(ERASE_PERSONAL) because delegable
  input reason : string
}

act correct_delivery_date in any {
  input value  : timestamp
  input reason : string
  corrects date
  set date := $value
}
```

`erase` is a transition kind whose outcome is the erasure of every `personal` attribute. `corrects <attribute>` marks an action as a correction, which is what produces the `corrected` provenance the model lists and iteration 1 left unreachable.

### 6.5 Deletion guards

```text
do delete ACTIVE -> DELETED {
  require no_referrers: none(referrers where state.category != closed) because dependent
}
```

`referrers` is every object holding a live reference to this one, derived by the checker from declared inverses. Iteration 1 required a type author to hand-enumerate every referencing type, in other modules, forever — which is the 102-delivery cascade the deletion decision exists to prevent, reintroduced as a maintenance burden.

### 6.6 State-removal mappings

```text
machine UnitLifecycle version 3 {
  removed LEGACY_HOLD -> AVAILABLE      # every object in LEGACY_HOLD migrates here
  …
}
```

A publish that drops a state must carry its mapping in the same file that drops it.

### 6.7 `this_event`, `proposable`, and the rest

`this_event` is an expression yielding the event the current transition will record, usable in a `create` argument, which is what the whole approval pattern rests on. `proposable` is a transition marking written beside `internal`. The remaining pieces the model needs and this section does not repeat are ordinary: `visible when` on a type, `extends` on a type, and `admits` above.

## 7. Quantity tracking

```text
type Stock version 1 tracking quantity {
  ref  model    : ProductModel inverse stock indexed
  attr on_hand  int
  attr reserved int

  derive available = on_hand - reserved
  invariant reserved_bounded: reserved >= 0 and reserved <= on_hand
}
```

`tracking quantity` requires the type to declare the counters and makes a slot referring to it a quantity slot. A slot declares which fill form it takes, and a slot with both a unit reference and a quantity is a publish error.

`reserved_bounded` is a **local** invariant, reading only this object's attributes. Iteration 1 permitted only traversal and type-scan forms, so the commonest invariant in any real type could not be written at all (ADR-0055).

## 8. Lexical rules

**Reserved words** may not be used as names: `module use capability category enum sequence evaluator machine type version attr ref part owner derive invariant unique state create do act assert erase input require because set add remove call for limit in where implies is null not and or true false any none all count sum min max now actor this this_event supersede admits corrects removed referrers internal proposable final superseding indexed personal external identifier default tracking summary visible extends machine fn eager deferred fresh`.

**One meaning per symbol.** Iteration 1 gave `->` four meanings and `in` four.

| | |
|---|---|
| `->` | a to-state, and a function's return in an evaluator. Never implication, never a reference type |
| `implies` | implication, written as a word |
| `Ref<T>` | a reference type in an attribute or input position |
| `$name` | an input. Never confusable with an attribute of the same name |
| `in` | a loop binder, an aggregate binder, membership, and an `act`'s from-state |
| `:=` | assignment and argument binding |
| `=` | binding a created name and defining a derivation |
| `{a, b}` | every collection literal, comma-separated, as are `summary`, `enum` and `changed_since` lists |
| `limit` | a loop cap. `max` is only the aggregate |

**Aggregates have one form**, `agg(<name> in <collection> [where <filter>]: <body>)`, with the body omitted for `count`. Iteration 1 had two interchangeable spellings.

**Expressions** are the language of DESIGN.md §5.7. Precedence, highest first: paths and calls; unary `not`; `* /`; `+ -`; comparison and `is null`; `and`; `or`; `implies`. A guard continues across lines while the line ends in an operator or an open bracket. Literals: `30 min`, `2 h`, `USD 19.99`, `"text"`, `true`, `{A, B}`.

## 9. What the checker verifies

Every check runs at publish, before any object is touched, and names the file, line and declaration.

| # | Check | Made decidable by |
|---|---|---|
| 1 | A guard or outcome names an undeclared input | `input` lines; the `$` prefix |
| 2 | A declared input, or a bound `create` name, that nothing reads | same |
| 3 | Derived attributes form a cycle | `derive` right-hand sides |
| 4 | An invariant traverses a relationship with no declared inverse | `inverse` on both ends |
| 5 | An invariant fits none of the three forms; the report says which it decided | local, traversal and type-scan are distinguishable by what the expression reads |
| 6 | A required attribute a creation never writes, or a later transition sets to absent | `?`; `set` targets are static |
| 7 | A required attribute written only from an optional input | the bare-input rule of §5.2 |
| 8 | A personal value reaching a non-personal attribute by write, cascade argument or derivation | `personal`; all three are static |
| 9 | A personal attribute declared as a required external identifier | markings are on one line |
| 10 | `create Robot.…` naming a transition the type does not have, or omitting it where several exist | creations name their transition |
| 11 | A cascade cycle across transitions | `call` and `create` name their target statically |
| 12 | An `internal` transition no `call` reaches | the checker derives the parent list |
| 13 | An actor guard on an `internal` transition, which the runtime would silently drop | `internal`; `actor.*` is syntactic |
| 14 | A non-final state with no outgoing transition, or a state nothing can reach | states and transitions share a block |
| 15 | A `do` leaving a `final` state | `final` |
| 16 | A machine with no `final` state | same |
| 17 | A `set` whose target is not an attribute of `this` | §5.2 |
| 18 | A `part` or `owner` end whose partner disagrees on name or cardinality | both ends declared |
| 19 | A `create` of a part that never writes its `owner` | `owner` |
| 20 | A capability, category, state, attribute or evaluator function that is not declared | the vocabularies of §1 and §6.2 |
| 21 | An unknown attribute inside `changed_since([…])` | same |
| 22 | A guard whose remedy class cannot be inferred and is not declared | `because` |
| 23 | An unnamed guard on a transition with more than one | `require <name>:` |
| 24 | A `for` without `limit` | `limit` is mandatory |
| 25 | A bare `null` in a comparison, an unbound aggregate, or a reserved word as a name | parser |
| 26 | A visibility predicate over an unindexed attribute | `indexed` |
| 27 | A type version, or a machine, enum, sequence or evaluator version, that did not advance while its content changed | `version` on each |
| 28 | A state removed with no `removed` mapping | §6.6 |
| 29 | A `supersede` in a transition whose to-state is not `superseding` | both are markings |
| 30 | An `admits` naming an invariant the type does not declare | `admits` |
| 31 | A family member whose bound machine lacks a transition another member's cascade calls | `extends` plus the machine binding |
| 32 | A slot declaring both a unit reference and a quantity | `tracking` |

Reported without failing: which invariants compile to a database constraint on this backend, which transitions are sweepable and which are not, which derived attributes are queryable, what remedy class was inferred for each unlabelled guard, the derived parent list for each `internal` transition, and how many pending proposals a publish would invalidate.

Check 27 is the one exception to "decidable from the text": it needs the previously published declaration.

## 10. Open questions for iteration 3

1. Whether `$` for inputs reads well to someone who has not written shell, or whether `arg.name` is worth its length.
2. Whether the implicit machine should also give a part a `DELETED` state by default, or whether deletability should be opt-in.
3. Whether `act` should be dropped now that `do X in S` would say the same thing, given it is the only sugar in the language.
4. Whether `referrers` should be filterable by type, for a delete guard that treats one referencing type differently.
5. Whether a guard's name should be required always, rather than only when a transition has several.
6. Whether an evaluator's `fresh` bound belongs on the function or on the guard that calls it.
