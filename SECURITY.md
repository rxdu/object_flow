# Security policy

## Reporting a vulnerability

Report it privately through GitHub's private vulnerability reporting on this repository (the **Security** tab, **Report a vulnerability**), never in a public issue. Say what you did, what happened, and the release or commit it happened on. Its fix, or why there is none, is said in the advisory.

## What is supported

Until 1.0.0, only the latest release receives fixes. From 1.0.0, a major version is maintained for 12 months after the next major version's first release, a security fix coming in a release of its latest minor version (ADR-0182 decision 8, ADR-0180).

## What the engine defends, and what it leaves to you

The service **authenticates nobody** (ADR-0114, PRD N7): it records the actor a request names and enforces the flow's rules, and the application in front of it decides who someone is and what they may do. So whoever can reach the service can act as any actor, which is the design, not a vulnerability: give it a Unix socket whose owner and group are the service's and your application's users (`--socket`); bind it to loopback only where every user of the machine may act as any actor, the operator among them, since any of them can reach a loopback port; never on a network your users reach ([`docs/guides/operate.md`](docs/guides/operate.md)). What an application withholds from an AI agent is in [`docs/guides/agents.md`](docs/guides/agents.md).

A vulnerability is a request that, within that design, does what the design says it cannot. Some examples:

- a request that stops the service, or holds it, for every other caller;
- a request that corrupts the store, or makes it answer what it did not record;
- a write that a flow's rule should have refused but did not;
- a read that returns a value an erasure removed;
- a store's files that hold an erased value.

Reports of these are welcome, and so are reports of a dependency's advisory that reaches the service, which CI checks against the RustSec database on every push.

## The limits a production accepts

These are limits of the design, accepted for production ([ADR-0204](docs/adr/0204-what-production-readiness-means-and-when-the-reviews-stop.md) §10), not vulnerabilities:

- **One writer, no standby service** ([ADR-0178](docs/adr/0178-more-than-one-request-at-a-time.md), [ADR-0213](docs/adr/0213-postgresql-in-1-0-0.md) decision 4). The store is one SQLite file served by one process, or one PostgreSQL database written by one service instance; a second service is no standby. On PostgreSQL, availability is the database's own failover, the service restarted against the new primary.
- **Recovery is the last backup.** What was written since it is lost, so the recovery point is the backup interval you choose ([ADR-0197](docs/adr/0197-the-tenth-reviews-findings-decided.md) decision 8). A restore advances the store's counters past a margin estimated from its own history, so an id an integration kept from the lost period does not come to name another object; this needs the copy marked, which the guide's backup commands do and its restore steps do for a copy made otherwise ([ADR-0207](docs/adr/0207-a-restored-store-skips-what-its-copy-never-saw.md)). A request the lost period applied is applied again if retried, under a new id.
- **An approval holds writes while it runs** ([ADR-0199](docs/adr/0199-the-twelfth-reviews-findings-decided.md) decision 7): a version's approval takes the writer, and every write waits for it.
- **The engine authenticates nobody** ([ADR-0183](docs/adr/0183-production-readiness-the-authors-four-decisions-and-the-services-hardening.md), PRD N7), as the section above says.
- **A store's clock that ran ahead is not brought back** ([ADR-0174](docs/adr/0174-a-deployments-service-its-first-slice.md) decision 4, ADR-0183): the store's clock never moves back, so if the host's clock jumps ahead and returns, every event is stamped at the latest time reached until the wall clock catches up. The service says so in its log, in `/health` and in `objectflow_clock_lead_seconds` ([`docs/guides/operate.md`](docs/guides/operate.md)).
- **Performance beyond the stated targets, and the log's text, are not promised** ([ADR-0180](docs/adr/0180-what-a-release-promises.md)). The names `/metrics` exports are, as `/health`'s fields are (ADR-0183).
