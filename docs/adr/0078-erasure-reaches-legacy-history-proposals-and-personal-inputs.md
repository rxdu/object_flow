# ADR-0078: Erasure reaches legacy history, proposal inputs and personal inputs

- **Status:** Accepted — decided 2026-09-09 at the author's direction ("rule on D190"), with the recommendation and its evidence below; pending author review
- **Date:** 2026-09-09
- **Refines:** ADR-0015, ADR-0031, ADR-0044, ADR-0051

## Context

ADR-0031 promises that erasure redacts a personal value "on the object and in every recorded event that carried the value — inputs, outcome writes, before-and-after values". `storage-schema.md` §9 listed four things erasure does, and D190 found three places a personal value can rest that none of them reaches: the legacy entries import attaches to an object, the stored inputs of a proposal, and an input recorded in an event payload that was passed only to an evaluator or a `call` and never written to a personal attribute.

The first consumer's data says how much this matters. Its `audit_logs` rows carry `before_values`, `after_values`, `changed_fields`, `description`, `business_reason` and `details` (`wr:app/models/audit.py:40-50`); for a customer those hold name, contact, email, phone and address (`wr:app/models/customer.py:13-22`), which is every personal attribute the type will declare. The same rows carry the acting employee's `ip_address`, `user_agent` and `session_id` (`wr:app/models/audit.py:32-34`), personal data about a third person. Its `notes` are free text attached polymorphically by `(entity_type, entity_id)` (`wr:app/models/note.py:29,37-38`). Under ADR-0015 all of that arrives as legacy entries, and under the schema as it stood none of it could be erased.

## Decisions

### 1. Legacy entries are redacted with the object they are attached to, and the mapping lists what is kept

The import mapping declares, per legacy entry kind, the payload fields **kept** through an erasure of the object the entry is attached to. Erasure replaces every other field with absence. A kind with no list loses its whole payload. The `erased` event records how many entries it redacted.

The direction is deliberate. An allowlist fails safe: the field nobody thought about is redacted. A denylist fails open, and the field nobody thought about is the one an auditor finds. This is the same choice visibility made (§8.2 of the syntax: an unknown predicate is not visible).

Fields the mapping does not import are never a problem. The employee's IP address and user agent belong to a different person and have no place in a customer's history; the mapping leaves them behind at import, which is the same list read the other way.

### 2. Proposals

Erasure redacts `t_proposal.inputs` on every proposal whose target is the erased object or whose inputs carry a personal value, and **invalidates** the pending ones. That is a fourth invalidation cause for ADR-0044, joining a removed transition, a newly required input and an unreachable target: *its inputs carried a value the erasure removed*. Executed, rejected and withdrawn proposals keep their skeleton with the inputs redacted, as an event does.

### 3. An input may be marked `personal`, and a personal input is redacted wherever it was recorded

`input <name> : <type>[?|[]] [personal]`. An input is personal if it is marked, or if it flows into a personal attribute — the taint check 10 already follows. A marked input written to a non-personal attribute is a check 10 error, like any personal value. Erasure redacts personal inputs from every payload that recorded them, including a payload where the input went only to an evaluator or a `call` argument.

## Alternatives rejected

- **Redact every legacy payload whole, always.** Safe and lossy: the non-personal half of a customer's legacy history — which delivery, when, by whom — is exactly the history the port exists to keep. It is the default when a mapping says nothing, not the rule.
- **A denylist of personal fields per entry kind.** Rejected: fails open, see decision 1.
- **Record in the payload only the inputs that were written.** Rejected: the payload is what a decision was made on, and an evaluator's input is part of that. Marking the input costs one token and keeps the record complete.
- **Leave proposals alone, since they are short-lived.** Rejected: a pending proposal to edit an email is a copy of the email, and "short-lived" is not a bound.
- **Reach into another object's free text.** Not possible: a customer's phone number typed into a service note is a value the declaration does not know is personal. Recorded as a limit in `edge-cases.md`; the mapping should not carry free text it cannot classify into a type that will outlive the person.

## Consequences

- `DESIGN.md` §8 names the three further reaches and the one limit.
- `storage-schema.md` §9 gains steps 5 to 7, and `ok_legacy_entry` says its payload is redacted per the mapping.
- `publish-and-import.md` §4 and §5 carry the kept-fields list.
- `declaration-syntax.md` §5.1 gains the marking; check 10 names it.
- ADR-0044 gains the fourth cause; ADR-0015, ADR-0031 and ADR-0051 carry the back-link.
- D190 is resolved.
