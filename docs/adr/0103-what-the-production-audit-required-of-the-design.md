# ADR-0103: What the production audit required of the design: declared request rules, text bounds, formatted serials, and seven repairs

- **Status:** Accepted — decided 2026-09-24 at the author's direction ("review the design against the PRD again with the updated example walkthrough … ensure the design can fully satisfy what we want to achieve"), against `docs/PRD.md` §2, §8, F1, F3, D5, D12, C1, C3, T2, T3, T5, N1, N2, N5 and UC-6, UC-12; repairs D275 to D279 and D282 to D288
- **Date:** 2026-09-24
- **Refines:** ADR-0014, ADR-0022, ADR-0029, ADR-0041, ADR-0047, ADR-0072, ADR-0100, ADR-0101
- **Refined by:** ADR-0110 — `requests by <kind>` reads the kind the actor's type declares, never one a request claims.
- **Amends:** ADR-0102, in place, since it is still proposed

## Context

An audit of the first consumer's production code ([`design/first-consumer-audit.md`](../design/first-consumer-audit.md)) set every business rule it enforces beside the design. It found eleven gaps after deduplication:
- five were defects of the design, D275 to D279;
- five were left to be decided against the PRD;
- the eleventh, a keyed map, is better remodelled as a part per key.

The unit's journey, written from production as a checked module ([`design/unit-journey.md`](../design/unit-journey.md)), was then read against every requirement and use case. That reading found that the module broke UC-6 (D282).

This decision's first draft was then verified independently, against the PRD, the rest of the record and production. The verification found:
- that its rule for unknown filters reopened, inside guards, the separation-of-duties hole ADR-0047 closed (D284);
- that its rule withholding who acted was stricter than production, and made standard assignment metrics incomplete for ordinary readers (D285);
- five further defects (D286 to D288).

This version is the repaired one. The withdrawn rule is among the rejected alternatives.

The five gaps left to the PRD, read against it:

1. **Agents must send a version and an idempotency key** (`wr:app/services/base_service.py:236-251`; `wr:app/core/idempotency.py:149-179`).
   - The PRD states this as what agents need today (§2): "scoped keys, mandatory idempotency keys and optimistic versions".
   - It counts every refusal site as a rule to declare once (§8).
   - T2's actors are fallible, and F3 lets the kind of actor matter "only where a rule says so". No rule could say so, since no guard reads a request field.
2. **Serials carry the maker's code**, as `{prefix}-{maker}-{NNNNNN}`, drawn from one sequence per type. An empty maker segment is dropped (`wr:app/services/serial_number_service.py:37-41,117-196`).
   - The first release is the first consumer's port (§10).
   - A port whose new serials changed format would change every label production prints. §8 measures success as the first consumer running on the engine.
   - `format` held literal text and `{n}` only.
3. **Text is bounded and checked for format** (`wr:app/services/inventory_item_service.py:102-114`; `wr:app/services/jira_client.py:38-75`).
   - D12 is a Must: values are "checked against their declared types and bounds when recorded".
   - A number can be bounded by an invariant, but a string could not. The language had no string operation beyond equality and membership (`DESIGN.md` §5.7).
4. **Only administrators read the audit trail.** At least, that was the audit's reading of `wr:app/core/permissions.py:33-51`. The code says something narrower.
   - Production shows who acted on a record to anyone who may view it. A delivery's activity feed returns each entry's username under `DELIVERY_VIEW` (`wr:app/api/deliveries.py:966-1035`), which every human role holds (`wr:app/core/permissions.py:23-30`).
   - Every record carries `created_by` (`wr:app/schemas/base.py:27-36`).
   - What `AUDIT_VIEW` restricts is the cross-record trail: the global list, its summary, its CSV export, and the per-entity and per-user feeds (`wr:app/api/audit.py:25-290`).
5. **A derived order status triggers a Jira comment and is filterable in a list** (`wr:app/services/procurement.py:339-375`).
   - The PRD makes causing effects the consumer's job, which it does by observing events.
   - UC-12 already answers the nearest case: "order ready" is a subscription to the transition that fills the last slot.
   - No requirement asks for a list filter on a status derived from other objects, and a first-release list of a few hundred orders (N3) can be filtered by its reader.

## Decision

### 1. A module may require fields of a kind of actor's requests

```text
requests by agent require version, key
```

`requests by <kind>, … require <field>, …` names:
- actor kinds: `human`, `agent`, `service`;
- fields: `version`, the request's `expected_version`, and `key`, its `idempotency_key`.

Like the capability vocabulary, it applies across the closure, and several lines add up. Check 19 refuses a kind or a field outside those sets.

A request by an actor of a named kind is refused when it lacks a named field, by a generated clause:
- `versioned`: the request names an existing object and carries no `expected_version`;
- `keyed`: the request carries no `idempotency_key`.

Each has remedy `self_serviceable` and is counted in the attempt log like any refusal. It is printed in the rule set with the kinds it binds, and an agent's tool schema lists the required fields as required.

The check runs after the replay test and before the version test (`DESIGN.md` §6, step 2). A replayed retry is therefore answered from the record.

It applies to what an actor sends:
- transitions, recordings and labels;
- the approval of a proposal, which is a request to the proposal. The recorded request that the approval executes is not re-checked, since its sender is not the one executing it.

It does not apply to:
- `publish`, which has its own required version (ADR-0101) and needs no key, since a second approval of a published change is refused as not being in `SUBMITTED`;
- `import_batch` and `maintain`, which are not requests.

`check` and `availability` do not evaluate the two clauses, which concern the request as sent. `availability`'s input schema lists the fields as required.

The rule is stricter than production. Production requires a key only on an agent's POSTs, and a version only where a service method checks one (`wr:app/core/idempotency.py:153-161`). Here an agent sends both on every request. An agent that already sends both loses nothing, and one that did not is refused naming the missing clause. `first-consumer-audit.md` §5 lists this as a change at cutover.

### 2. Who acted is shown as production shows it, and needs no new rule

`history`, and a read set, show an event's actor to every reader who may see the object. This is what production does for a record's own activity. The cross-record trail that production gates behind `AUDIT_VIEW` is:
- the store's `export` of events and attempts, read into the consumer's own audit pages;
- a subscription's `pull`.

The consumer calls both from routes it authorises, as production's `/audit-logs` routes are authorised today. T5 holds as written: object visibility applies to every read.

### 3. `length` bounds a string, and a string holds no NUL

`length(<string>)` yields an `int`, the number of Unicode code points, and is `unknown` for an absent value. It is allowed:
- in guards;
- in invariants, including an observation kind's field invariants;
- in derivations. An `indexed` derivation may use it over an `indexed` attribute, as check 46 requires of anything an indexed derivation reads.

So D12's bounds reach text:

```text
invariant name_fits: length(name) >= 1 and length(name) <= 150
```

A string value never contains U+0000, and one that does is refused as invalid, like a value of the wrong type. PostgreSQL cannot store the character, and SQLite's `length` stops at it, so without the rule the same expression would count differently on the two backends (N1).

Pattern matching stays out of the language, and so does trimming. Whether a Jira key or a link is well formed is a question about another system's syntax. That belongs to the consumer's schema at its edge, or to an evaluator where the other system must be asked. Turning blank input into absence is input normalisation, which the same schema does (`edge-cases.md`). A regular-expression operator would make a rule's meaning depend on a regex dialect, and Python and the two backends' SQL do not share one.

### 4. A `format` may interpolate a model's code, pad the number, and omit an absent segment

`format` accepts:
- `{n}`, the sequence number;
- `{n:<width>}`, the number zero-padded to at least `width` digits;
- `{<attr>}` or `{<ref>.<attr>}`, a `string` or enum attribute of the object, or of an object one of its references names;
- `[ … ]`, a segment omitted whole when any placeholder inside it is absent.

The rules:
- An attribute or reference a placeholder names must be one every creation writes. It is interpolated after those writes and before the mint, which is the condition `scoped by` already imposes.
- An optional attribute may be named only inside `[ … ]` (check 44).
- An enum is interpolated as its member's name.

The first consumer's serial is then declared as production mints it:

```text
attr serial string identifier from unit_serial format "RBT-[{model.maker_code}-]{n:6}" indexed unique
```

The code itself is not computed from the maker's name, because the language has no substring. `RobotModel` holds it as an optional `maker_code`, which the port fills with production's derivation: the name stripped to letters and digits, upper-cased and cut to three characters, unless a curated override applies (`wr:app/services/serial_number_service.py:59-61,178-196`). An operator sets it for a new model.

**A port does not reuse a sequence value.** An import batch may carry a high-water mark for each sequence it names. The store raises the sequence to it and never lowers it, so the first serial minted after cutover follows the last one production minted (`wr:app/services/serial_number_service.py:88-116`).

### 5. A quotient of two like quantities is a decimal

`<duration> / <duration>` and `<money(C)> / <money(C)>` yield a `decimal(19,6)`, rounded half to even. A zero divisor yields `unknown`, as for every division. A utilisation, a share of time or a share of revenue can therefore be declared (C1), including as a combined metric over two metrics that each yield a duration. Dividing quantities of different kinds, or money of different currencies, stays an error (check 24).

**A metric buckets a span by when it began.** `month(i.entered_at)` puts a whole span in the month it started, as it always has. A share of time is therefore right over a unit's whole history, but not per month. `unit-journey.md`'s `pool_utilisation` is declared per model, over the whole history. Apportioning a span across buckets would need intervals split at bucket boundaries, which the metric form does not do. `edge-cases.md` records that as a known limit.

### 6. Where an unknown filter selects nothing, and where it keeps a guard closed

An element whose filter evaluates to `unknown` is not selected, as a SQL `WHERE` leaves out a row, in:
- a metric's `from`;
- a metric's `count(where …)`;
- the `where` of a `for` in an outcome.

In a guard, an invariant, a visibility predicate or a derivation, an aggregate keeps §8.2's rules:
- an element whose filter is `unknown` makes `all`, `any` or `none` unknown, unless another element decides it;
- it makes `count`, `sum`, `min` and `max` unknown.

So a guard still fails closed, and the separation-of-duties rule of ADR-0047 and D20 holds: `none(a in approvals where a.approver == actor.id)` over an erased approver is unknown, and refuses. Production's rule that a slot with no role never blocks completion (`wr:app/core/state_registry.py:544-545`) is written with a presence test, which Kleene `and` makes definite:

```text
require filled: none(s in slots where s.role is not null and s.role == Role.PRIMARY and not s.filled)
```

The publish report lists every guard, and now every filter, that can be `unknown` through an optional it never tests.

### 7. `maintain` prunes idempotency records

`maintain` gains a third task, `prune_idempotency(before)`. The store is constructed with the deployment's idempotency retention, and refuses a prune whose `before` falls inside it. After a prune, a retry carrying a pruned key is a new request, so the replay promise holds for exactly the retention.

The retention must be at least the longest period over which any caller repeats a key, including:
- a scheduler's sweep, whose key is derived from the object and the period (ADR-0022, ADR-0041);
- the window over which an at-least-once consumer may re-deliver a request (ADR-0014).

A repeat after a prune is otherwise applied again. Idempotency records are not governed state, so T3 is not reduced.

### 8. A closed state needs no exit

Check 15 no longer requires a `closed` state to have an outgoing `do`. A closed state is finished work, not stuck work. It may still take actions, recordings, erasure and assertions, and a lifecycle may end in one. `declaration-syntax.md` §4.2 already advised "a genuinely final state after it if one is needed", and the check now agrees with it. A machine still needs a terminal state.

A retired unit can therefore stay `closed` with no way out, as in production, and still take the label reprints and edits production allows. The checker is also brought into line with check 15's text: an action does not count as leaving or reaching a state, and an assertion's target does not count as reached (D283).

### 9. The record says the first consumer keeps a stock level

It does, for bulk accessories. The service layer counts `Accessory.quantity` down and clamps it at zero (`wr:app/services/service_service.py:751-773`). The design would model a bulk lot as a `tracking quantity` type with a `counter` and a `nonneg` invariant, beside the serial-tracked accessory, so the first consumer would exercise the counter of ADR-0072. Production's clamp would become a refusal naming `nonneg`, rather than a silent zero.

`edge-cases.md`, the specification's §11 question 10, and every other statement that the first consumer stores no stock level are corrected. The lot type is declared by the table-to-type mapping, not here.

### 10. The unit's receipt is backdatable where it is requested

`Shipment.receive_unit` is `backdatable within 2 days`. The unit's own `receive` is `only via` it, so it cannot carry the marking (check 58), and a cascaded transition records its parent's occurred time (ADR-0095). Time in `PROCUREMENT` therefore ends when the unit arrived, as UC-6 requires.

`Shipment.eta` is also `indexed`, and the shipment derives `overdue`, which finds in-transit shipments past their expected arrival with one query. That is not production's "procurement overdue", which is an order past its `expected_arrival` (`wr:app/models/procurement.py:59`; `wr:docs/proposals/operations-system-design.md:251`). It is declared the same way on the procurement order, which the module leaves out.

## Alternatives rejected

- **Leave the request fields to the consumer's API wrapper.** The rule that decides whether a request is refused would be written outside the declaration, which is §8's scattered rule, and no refusal would be counted against it.
- **Let a guard read request fields.** Every transition would restate the same clause, the duplication §8 counts.
- **Match production's rule exactly: a key on POSTs only, a version where a method checks one.** Which requests are POSTs is a fact about the consumer's HTTP routes, not about the flow. A rule stated in the flow's terms is stricter, and costs a compliant agent nothing.
- **`actors visible when <predicate>`, withholding who acted wherever a read returns it.** This decision's first draft had it. It was stricter than production, which shows who acted on a record to anyone who may view the record. It made `acted_by_non_assignee` and every metric comparing identities incomplete for ordinary readers, against M2 and M6. It also left identities readable through intervals, legacy payloads, subscriptions and attributes that copy an actor's id (D285). What production restricts, the cross-record trail, the consumer already gates.
- **Regular expressions in the language.** Their meaning differs between Python, PostgreSQL and SQLite, and a rule whose meaning depends on the backend breaks N1.
- **Let the consumer supply every serial.** The store would lose the sequence, and uniqueness would be discovered by a refused creation and a retry, which is the check-then-act production already suffers from.
- **A required `maker_code`.** Production has models with no manufacturer and mints their serials without the segment. A required code would change those serials.
- **Treat an unknown filter as unselected everywhere.** This decision's first draft did (D284). Inside a guard it lets an erased value satisfy an exclusion, which is the hole ADR-0047 closed.
- **Keep check 15 and invent an exit for retired units.** An invented exit is a transition no one may take, printed in the rule set as if someone could.
- **Declare a derived status stored, so a list can filter it and a change emits an event.** A value written as a side effect of other objects' transitions is the second write path ADR-0012 refuses. The PRD's route for effects is a subscriber, and no requirement asks for the filter.

## Consequences

- **`declaration-syntax.md`:**
  - §1 gains `requests by`;
  - §3.1 widens `format`;
  - §4.2 and check 15 exempt a closed state;
  - §8.2 says where an unknown filter selects nothing;
  - §8.3 adds `length`, the quotient and the NUL rule;
  - §9.4 and §9.5 add the form and the reserved words;
  - checks 19 and 44 are extended, and the report lists filters;
  - §11 question 10 and the other statements about stock are corrected.
- **Other design documents:**
  - `DESIGN.md` §5.7, §5.8, §6, §10, §13 and §14 change;
  - `library-api.md` gains the task, the retention, the clauses, the proposal and publish rules, and the import's sequence marks;
  - `publish-and-import.md` and `storage-schema.md` state the sequence advance and the idempotency prune;
  - `renderers.md` prints the module line and the tool schemas' required fields;
  - `edge-cases.md` records formats and trimming at the edge, the derived-status filter, span bucketing and the stock level.
- **The unit's journey:** `unit-journey.md` takes the backdatable receipt, production's serial, a closed `RETIRED`, the overdue shipment and a whole-history utilisation. ADR-0102 is amended in place.
- **The audit and the map:** `first-consumer-audit.md` records each gap's disposition, and `traceability.md` cites this decision on the rows it serves.
- D275 to D279 and D282 to D288 are resolved.
