# ADR-0102: The unit's journey is one lifecycle, with custody and condition beside it, and a missing unit is a state

- **Status:** Proposed — written 2026-09-24 at the author's request to extend the unit's transitions from requested to serviced and loaned, from the first consumer's production code; awaiting the author's acceptance
- **Date:** 2026-09-24
- **Relates to:** ADR-0079 (a role is a capability), ADR-0085 (a trial clause), and the first consumer's own ADR-0002 (`wr:docs/adr/0002-unit-engagement-and-leasing-model.md`)

## Context

The author asked for the unit's transitions to be extended from requested, through shipping, inventorised and reserved, to delivered, and then serviced and loaned, using the production system as the source. [`design/unit-journey.md`](../design/unit-journey.md) writes that journey as a checked module. Writing it required six modelling decisions, and three of them depart from production:

- The request lists servicing and lending as stages after delivery. Production's accepted ADR-0002 keeps them off the unit's lifecycle, as separate axes.
- Production cancels a unit it flags missing from a shipment, and then leaves `CANCELLED` by an override when the unit turns up (`wr:app/core/state_registry.py:907-925`).
- Production marks four states terminal and leaves each of them anyway.

These are decisions about the first consumer's model, not about the engine. None needs a language change. They are recorded because the harness fixture and the cutover mapping will be built from them.

## Decision

### 1. The lifecycle says what the unit is to the business, and nothing else

- The unit's machine holds the supply stages, availability, reservation, disposal and retirement.
- Who physically has the unit is an `Engagement` with lines, and a `Lease` is the commercial agreement above one.
- Whether the unit works is a `ServiceJob` that names it.
- The unit derives `on_loan`, `in_service`, `fit` and `leasable` from the engagements and jobs that name it.
- Lending out and bringing back are an engagement's transitions. Being in for repair is a job's state. Converting a lease to a sale is the one lending transition that changes the unit's lifecycle, as `convert_lease`, `only via Lease.convert`.

### 2. A missing unit is `MISSING`, an inbound state

`flag_missing` moves a unit from `PROCUREMENT` to `MISSING` and releases its peg. From `MISSING`:
- `restore` returns it to `PROCUREMENT`;
- `rerequest` returns it to `REQUESTED`;
- `discard` cancels it with the reason `DISCARDED`.

`CANCELLED` then has no exit but `delete`. The port maps production's `(CANCELLED, MISSING_FROM_SHIPMENT)` to `MISSING`.

### 3. A state that production leaves is `closed`, not `terminal`

This covers:
- the unit's `CANCELLED`;
- a delivery's `DELIVERED` and `CANCELLED`;
- a service job's `DONE` and `CANCELLED`;
- a shipment's `COMMITTED`.

Each is followed by a terminal `DELETED` or `VOIDED` where production has one, as `declaration-syntax.md` §4.2 advises.

### 4. The hard bind and the soft peg are two references

A unit has `binding`, the delivery it is reserved for, and `peg`, the delivery an inbound unit is earmarked for. Committing a shipment inventorises its units and releases their pegs. It does not fill them, since production retired the commit-time fill (`wr:docs/design/explicit-delivery-assignment.md`).

### 5. A unit consumed by a service is `consume`d, not sold

A service job reserves its parts with `reserve_for_service`. `finish` consumes them, voiding a finished job returns them with `unconsume`, and `sell` stays `only via` a delivery. The two routes into `SOLD` stay distinguishable in every metric over transitions.

### 6. Production's admin role is the capability `ADMIN`

`ADMIN` stands where production writes `required_role="ADMIN"` beside a permission, and both clauses are written (ADR-0079).

## Alternatives rejected

- **`LOANED` and `SERVICED` as lifecycle states.** A leased unit that fails is on loan and under repair at once, and one state cannot hold both. Each loan and repair also has its own duration, owner and history, which a state does not. Production's ADR-0002 rejected `LEASED` as a status for the same reasons, and because an overdue lease and an overdue event deployment would need two mechanisms.
- **A sub-state enum on the unit.** It says what the unit is doing now, but not when it will be free or how much it has been used (production ADR-0002, "Rejected alternatives").
- **Production's `CANCELLED` with the reason `MISSING_FROM_SHIPMENT`.** It makes a cancelled unit sometimes not cancelled, and hides open work. A missing unit awaiting resolution would count as finished and never age.
- **One `binding` for the bind and the peg,** as the specification's fixture has it. It conflates an earmark with a reservation, and production stores them as two links.
- **Keeping `DELIVERED`, `DONE` and `CANCELLED` terminal, and treating production's exits as overrides.** Every revoke, reopen and restore would become an assertion. The rule set would then say these things never happen while the exception list said they happen routinely.

## Consequences

- `design/unit-journey.md` holds the module, a production citation for each of its 32 transitions, and the departures and open questions.
- `first-consumer-walkthrough.md` §2.1 and §5 point to it (`design/defects.md` D280).
- The specification's own `UnitLifecycle` stays the smaller fixture its mutations are written against.
- The table-to-type mapping in `TODO.md` takes this module as its unit slice.
- Production's ADR-0002 open questions are unchanged and listed in `unit-journey.md` §6. Each is a publish when decided.
- Writing the module found D275 (a utilisation ratio cannot be declared) and D281 (two checker false positives).
