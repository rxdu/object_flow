# ADR-0005: Guards evaluate over current state plus transition inputs

- **Status:** Accepted
- **Date:** 2026-09-07
- **Refined by:** ADR-0047 — a transition declares its inputs rather than deriving them from its guards, which supersedes the derived-parameter consequence; the decision that guards evaluate over state plus inputs stands.

## Context

If a transition requires an attribute that the caller supplies as part of performing it, evaluating the guard against current state alone always fails — the value is not set yet. That forces a two-step sequence: write the attribute, then transition. The first step happens outside any guard, reopening the unmediated write path.

## Decision

Transitions take arguments. Guards are evaluated over current state **and** the transition's inputs.

## Alternatives rejected

### Guards over current state only

Rejected because it requires callers to mutate the object before requesting the transition, which is precisely the unguarded write the design exists to prevent.

## Consequences

- Availability becomes three-way rather than binary: **available**, **available-with-input** (naming what must be supplied), **blocked** (nothing the caller can supply will help).
- ~~A transition's parameter list is derived from its own guards rather than declared separately.~~ **Superseded by ADR-0047**: recovering an input schema from an arbitrary boolean guard is abduction and has no algorithm. Inputs are declared, and the anti-drift property is preserved because inputs and guards live in one declaration, checked against each other at publish.
- The middle case is what makes the interface useful to an agent: "blocked: missing field" invites guessing, while "available if you provide these" names the work to do.
