# Security policy

## Reporting a vulnerability

Report it privately through GitHub's private vulnerability reporting on this repository (the **Security** tab, **Report a vulnerability**), never in a public issue. Say what you did, what happened, and the release or commit it happened on. Its fix, or why there is none, is said in the advisory.

## What is supported

Until 1.0.0, only the latest release receives fixes. From 1.0.0, a major version is maintained for 12 months after the next major version's first release, a security fix coming in a release of its latest minor version (ADR-0182 decision 8, ADR-0180).

## What the engine defends, and what it leaves to you

The service **authenticates nobody** (ADR-0114, PRD N7): it records the actor a request names and enforces the flow's rules, and the application in front of it decides who someone is and what they may do. So whoever can reach the service can act as any actor, which is the design, not a vulnerability: give it a Unix socket whose owner and group are the service's and your application's users (`--socket`), or bind it to loopback behind the application, never on a network your users reach (`docs/guides/integrate.md` §9).

A vulnerability is a request that, within that design, does what the design says it cannot. Some examples:

- a request that stops the service, or holds it, for every other caller;
- a request that corrupts the store, or makes it answer what it did not record;
- a write that a flow's rule should have refused but did not;
- a read that returns a value an erasure removed;
- a store's files that hold an erased value.

Reports of these are welcome, and so are reports of a dependency's advisory that reaches the service, which CI checks against the RustSec database on every push.
