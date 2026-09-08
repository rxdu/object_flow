# ADR-0068: Money keeps its currency in the declaration

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refines:** ADR-0007, ADR-0032, ADR-0061
- **Answers:** open question 9

## Context

`money(ccy)` fixes a currency at declaration time, so one attribute holds one currency and a value of another is a type error. A reviewer modelling a payments ledger reported that this makes a multi-currency ledger untypeable: the alternatives are an attribute per currency, or a `decimal` beside a currency string, which discards every check the type exists to provide.

Evidence from the first consumer:

- **No currency concept exists at all.** Searching its application for "currency" returns two comments about a Jira field it deliberately declines to supply, and nothing else.
- **Money is vestigial.** Four columns carry an amount, all `Float` (`wr:app/models/inventory.py:45`, `wr:app/models/spare_part.py:36`, `wr:app/models/service.py:66`), never rounded, rendered once with a hardcoded dollar sign. Monetary fields were actively removed from the warranty domain by migration `344e1cb44335`. There is no quote, invoice, purchase order or pricing entity.
- Its own design document claims all monetary amounts use `Decimal(10,2)`. None of the columns it names exists, and the ones that do are floating point.

ADR-0032 already placed **currency conversion** outside the store, among the domain formulas a consumer computes and supplies.

## Decision

The currency stays in the declaration. `money(ccy)` names an ISO 4217 code, its scale is that currency's minor unit, and a value of another currency is a publish error rather than a runtime one.

A model that genuinely holds several currencies declares an attribute, or an account, per currency — which is what double-entry systems do regardless, since mixing currencies in one balance is a modelling error in the domain before it is one here.

## Alternatives rejected

- **A currency-carrying value with runtime equality.** Rejected because it moves an error from publish to production for no gain in expressiveness: the store would refuse the addition either way, and refusing it earlier is the whole point of a declared type. It would also make every money comparison a partial function, which §8.2's three-valued rules would then have to absorb.
- **`money` without a currency parameter, currency held beside it.** This is the workaround, and it is available today with `decimal`. Rejected as the *type*, because a type whose unit is not part of it cannot check the one thing money most needs checked.

## Consequences

- A multi-currency ledger is not expressible as a single account, and that is recorded as a known limit rather than an oversight.
- The first consumer's floating-point amounts become `money(SGD)` with a stated scale on the port, which its own design document already believed was the case. A typed money with a scale is exactly what would have refused `Float`.
- ADR-0061's rounding rule stands: multiplication or division by a scalar rounds half to even to the currency's minor unit.
