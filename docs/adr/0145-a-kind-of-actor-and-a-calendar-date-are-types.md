# ADR-0145: A kind of actor and a calendar date are types, and an amount in another currency is recorded beside the one the rules compute with

- **Status:** Accepted by the author, 2026-09-30: "follow your suggestions", on the three questions `reimbursement-cold-draft.md` §5 put, which decisions 1 to 3 answer as suggested.
- **Date:** 2026-09-30
- **Refines:** ADR-0114 decision 1 (an outcome may record `actor.kind`), ADR-0068 (the currency is in the declaration)
- **Refined by:** ADR-0173 — an invariant over a recorded kind refuses with the remedy `delegable`, and `authoring-flows.md` states the multi-currency pattern decision 3 implies.
- **Relates to:** `declaration-syntax.md` §3.1, §6.9, §8.1, §8.3; `flow-format.md` §4.3, §5, §11.3; `storage-schema.md` §3.1; `reimbursement-cold-draft.md`; D493, D494

## Context

The reimbursement worked example, drafted by an agent that had read only the flow format, asked three things the model could not answer (`reimbursement-cold-draft.md` §5).

**The kind of actor has no type.** ADR-0114 decision 1 lets an outcome write `actor.kind` into an attribute, and `declaration-syntax.md` §8.1 types it as "a fixed enum" with no name, so no attribute can be declared to hold it. The business asked that the agent never commit a financial write, and recording who confirmed a line's nature, with an invariant that a person did, is the way the declaration could say it. That has the shape of the rule ADR-0114 kept, one approval per person as an invariant over the recorded approver; ADR-0114's rejected alternatives also name "an agent never commits a financial write" as a rule about who may, which the upper layer decides.

**A calendar date has no type.** An expense has a day, not an instant, and a metric buckets by UTC calendar (`declaration-syntax.md` §6.9, ADR-0106). A day written as its UTC midnight is bucketed right only while every writer keeps that convention; an application storing Singapore's 1 March local midnight as the instant it is, 28 February 16:00 UTC, puts it in February.

**A claim line may be in any currency.** ADR-0068 keeps the currency in the declaration, one per attribute, and a model with several currencies declares an account per currency; a line is in whichever currency was spent.

## Decision

1. **`actor_kind` is a built-in type**, whose values are `human`, `agent` and `service`, written bare in an expression and in an example. `actor.kind` is of it, as a row's `.actor_kind` is. An attribute of it records the kind of actor that acted, written by an outcome from `actor.kind`, and **an invariant, a guard, a derivation or a metric may read such an attribute**, since it reads what the record holds, as one approval per person reads the recorded approver. Who may make a request stays the upper layer's; an invariant over a recorded kind refuses a request whose recorded outcome the flow does not accept. Check 64 still refuses reading `actor` anywhere but an outcome value.
2. **`date` is a built-in type**: a calendar day, with no time and no zone. Dates compare and order as days; `d1 - d2` is a duration of whole days, and a date plus or minus a duration of days or weeks is a date. `day`, `week`, `month`, `quarter` and `year` bucket a date by its own calendar, with no conversion. `date(<timestamp>)` is the UTC calendar day of an instant; a guard comparing a date a person gives with `date(now)` compares it with the UTC day, which east of UTC is a day behind the local one for part of each day, so a flow that must accept a local "today" allows a day. A date as a metric's time dimension is placed at the start of its day, UTC, for a window. In an example a date is written `2026-03-01`. It is stored as `DATE` on PostgreSQL and as ISO-8601 text on SQLite, which orders as the day does.
3. **ADR-0068 stands.** A value in another currency than the one a flow computes with is recorded beside it, as a decimal amount and a currency code, which the engine stores and shows and never computes with; the amount the rules and the metrics read is in the declared currency. A claim line carries its amount in the home currency, which is what is reimbursed, and records the amount spent and its currency.

## Alternatives rejected

- **`actor.kind` written to a `string`**, as the drafter did: a comparison with `"agent"` is text, so a misspelling compares false and passes.
- **Refusing every rule over a value written from `actor`, except the uniqueness ADR-0114 kept**: it removes the rule the first consumer asked for, whose shape is the one ADR-0114 kept.
- **An enumeration of kinds declared in each module**: it restates a built-in vocabulary, and nothing holds the copy to it.
- **Keeping a day as a timestamp at UTC midnight**: correct only by a convention the store cannot check.
- **A store time zone that decides which day an instant is**: a store-wide setting a flow's meaning would depend on, where buckets are UTC by decision (ADR-0106); a date that is a day already needs none.
- **Money carrying its currency at runtime**, refused by ADR-0068, and **an attribute per currency**, which fits an account, not a line in whichever currency was spent.

## Consequences

- `declaration-syntax.md` §3.1, §8.1 and §8.3, `flow-format.md` §4.3, §5 and §11.3, and `storage-schema.md` §3.1 state the two types and the pattern; the flow schema and the text checker know them.
- The flow checker refuses a type that is neither built in, nor an enumeration the module declares, nor a name it imports, which it did not before (D493).
- The strict loader keeps a date as the text it is written as, as the core's loader will (D494), and the reference runner computes with both types.
- The reimbursement module records who confirmed each line's nature, and an invariant requires a person; its expense date is a `date`; its lines record the amount spent and its currency.
