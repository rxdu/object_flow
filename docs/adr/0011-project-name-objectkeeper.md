# ADR-0011: The project is named ObjectKeeper

- **Status:** Accepted, then superseded on 2026-09-24
- **Date:** 2026-09-07
- **Superseded by:** ADR-0107 — the project is now named ObjectFlow. This record keeps the first name and why it was chosen, and its words are left as they were.

## Context

The working directory name was `agentic_object_store`, which had two problems. "Agentic" narrows the project to one of its two audiences and dates quickly, when the design explicitly makes human interfaces and agent APIs interchangeable consumers (ADR-0010). "Object store" means S3-style blob storage to most engineers, and also mis-describes the layer, since storage is delegated to a conventional database (ADR-0001).

The name needed to indicate data storage and governed access, be obvious rather than metaphorical, sound professional, and be memorable.

## Decision

**ObjectKeeper**, written as one word, with the tagline *"a governed object store: your data, and the rules that constrain how it changes."*

`object` matches the model's own primitive (`ObjectType`), so the project name and its vocabulary do not diverge. `-Keeper` reads as infrastructure by convention established by Apache ZooKeeper, and carries both holding and guarding.

## Alternatives rejected

### EntityWarden

The leading candidate until registry and prior-art checks were run, and rejected on evidence rather than taste.

"Warden" is heavily contested — Rack authentication (Ruby), a Docker development-environment CLI, health checks, mobile attestation, and .NET process management all ship under that exact name. More seriously, Warden Protocol is an L1 blockchain for AI-agent applications that raised USD 4M in January 2026 and markets itself around "the AI agent economy". That is the same audience and the same keywords with far greater marketing spend, and since users would inevitably shorten `EntityWarden` to "Warden", the collision could not be avoided by the compound.

`entitywarden` was otherwise clear on PyPI, npm, crates.io and as a GitHub organisation.

### StateWarden and StateKeeper

Both attractive because "state" names what is actually guarded. Both rejected: `statewarden` and `statekeeper` are already taken on npm, and unhelpfully so — the former is an Angular state-management alpha, the latter is described as "state transitions". Small abandoned packages, but sitting in this project's exact conceptual territory, so anyone searching would find them first.

### EntityKeeper, EntitySteward

`entitykeeper` and `entitysteward` are held as GitHub accounts. Separately, "data steward" is an established job title in data governance, which would file the project mentally under data catalogues and master-data management rather than application substrate.

### Metaphorical names (Instar, Carapace, Covenant, Custodian)

Considered and set aside once the requirement was stated as *immediately indicating storage and governed access*. They are more distinctive and more ownable, but each needs a sentence of explanation, which the brief ruled out.

### Names implying lifecycle guidance (Conductor, Shepherd, Pilot)

Rejected on design grounds rather than availability. Every agent-noun that carries lifecycle implies *driving the object forward*, which ADR-0012 explicitly rejects — the store gates transitions, it does not propel objects through them. Such a name would advertise behaviour that was deliberately designed out.

## Residual risk

Keeper Security holds "Keeper" as a registered mark for software, in password management. Different product class, and ObjectKeeper is a compound, but this warrants professional advice before any commercial use of the name.

The "object means blob storage" ambiguity from the original working name is reduced but not eliminated; the tagline is what resolves it.

## Evidence

Registry state verified 2026-09-07; registry availability is perishable and should be re-checked before any name is published. At that date `objectkeeper` was unclaimed on PyPI, npm, crates.io and as a GitHub organisation. Domain availability was not verified — it requires a registrar lookup rather than a search.

## Consequences

- Repository: `git@github.com:rxdu/object_keeper.git`.
- The model's vocabulary needs no change: `ObjectType` and "object" were already the terms in use, so the project name and the design documents agree.
- The local working directory retains its original name and is a cosmetic mismatch only.
