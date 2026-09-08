# ADR-0001: The store owns persistence and is the sole write path

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

The originating observation was that SaaS tools (Jira, HubSpot) had degraded into expensive object stores once work moved to agents and automation. That raised a question the project had to answer before anything else: does this layer own the data, or does it describe objects that live in other systems?

The answer is forced by the objective. The goal is trust under delegation — being able to hand the system to unsupervised people and to agents and know that no reachable state requires cleanup. A guard is only a guarantee if the actor cannot go around it. If Jira holds the object and an agent holds a Jira token, any constraint this layer expresses is advisory.

## Decision

The store owns persistence. It is the sole write path to the data it holds. Consumers never receive direct database access.

Storage is delegated to a conventional database (PostgreSQL, or SQLite for embedded use); the project is a layer over a database, not a database.

## Alternatives rejected

### Descriptive layer over foreign systems

Describe objects that remain in Jira/HubSpot/etc., answering "what is this and what can be done with it" without owning the data. Cheapest to adopt and integrates with anything.

Rejected because enforcement becomes advisory. It would still fix the most common agent failure — not knowing what is valid — but it cannot deliver the stated objective, and a layer that claims to guarantee valid operations while only describing them is worse than one that makes no claim, because downstream consumers will trust the guarantee and skip their own checks.

### Gateway proxying foreign systems

Proxy the write path to external systems so agents hold credentials only to this layer. Enforcement is real.

Rejected for the first version: it makes the project infrastructure with an availability requirement and credential custody, and it inherits a synchronisation problem the moment a human edits in the foreign system's own UI. It also does not address the motivating complaint, which is that paying for those systems as object stores no longer makes sense.

## Consequences

- Storage columns must be permissive, because requiredness attaches to transitions rather than attributes (see ADR-0002). The database therefore cannot act as a backstop; all strictness lives in the runtime. *Refined by ADR-0039 and ADR-0041: correctness comes from serialisable isolation, not from the database schema. Some invariant shapes additionally compile to constraints as an optimisation, and which ones is a property of the backend. The database still holds no rule the declaration does not state.*
- Anything that writes around the runtime writes garbage with no complaint. This makes the mediated property an operational commitment (deployment, access control, migration tooling) and not only an architectural one.
- Administrative repair needs a designed path — an authorised, recorded override transition — rather than an out-of-band database edit. The data import of ADR-0015 is the first use of that path, at scale.
- Systems whose records have external consumers (Xero: auditors, tax authorities, banks) stay where they are. The keep/replace test is: does anyone outside the organisation depend on this system's record, does it do real work beyond storage and UI, and is the exit cost permanent.
