# ADR-0168: The scenarios' port declares every type a valid mirror

- **Status:** Decided by the author, 2026-10-08 ("go with your recommendation on the scenarios' port"). The question had been put without a recommendation; the recommendation below was formed when the author asked for it, and stated with the decision.
- **Date:** 2026-10-08
- **Refines:** none; it states what `metric-scenarios.md` §1 left unsaid.
- **Relates to:** `design/metric-scenarios.md` §1; `design/publish-and-import.md` §7; `design/declaration-syntax.md` §2 (`mirror`); `design/flow-format.md` §4.15; ADR-0154 (the core's import); ADR-0167 (the runner's replay stops at the port)

## Context

`metric-scenarios.md` §1 says the store imported the legacy system's objects "while every type was a mirror, version 1", and the publish of the journey, version 2, cut them over. It did not say what version 1 declared. The replay (`scripts/replay-scenarios.py`) built it by marking each type of the journey `mirror: true` and nothing else, and the core's service, which loads a sandbox without the checks, took it. Running the replay through the reference runner (ADR-0166's R4), whose sandbox loads as the studio does, through the flow checker, found that version refused: `User`, `Agent` and `Service` were mirrors declaring no external identifier (`flow-format.md` §4.15), `Robot` a mirror binding a state machine, and every mirror kept transitions other than an erasure (`declaration-syntax.md` §2). The replay's own copy of each module had also lost every cascade's `on`, read by YAML 1.1 as `true`, which the core's unchecked load passed over.

The design already says what such a version is. `publish-and-import.md` §7: a type "arrives as a `mirror` in one [publish], is kept current by repeated import while it is one, and is cut over by a second, a version advance that removes the marking and declares its transitions". And the scenarios themselves declare one valid mirror, `Customer`, in `operations_shared`: its attributes, an external identifier `legacy_key` marked `external: legacy`, its states, and no transition.

## Decision

1. **Version 1 declared every type of the journey as a mirror, as `Customer` is declared.** Each keeps its attributes, its invariants, derived attributes, observations and metrics, and its states, a bound machine's states copied; it declares the external identifier `legacy_key`, `{ type: string, external: legacy, indexed: true }`, where it declares no external identifier of its own, and the import fills it with each object's legacy key where the record gives no other; and it declares no transition but an erasure, no condition but one an erasure reads, no composite end's `cascade` or `survives`, no minted identifier, since nothing creates or mints while a type is a mirror, and the module no machine, since no type binds one. `metric-scenarios.md` §1 says so.

2. **The replay derives version 1 from the journey by that rule, and both backends load it checked.** `scripts/replay-scenarios.py` (`port_version`) reads each module as the checker and the core read it, writes version 1, and has the flow checker accept it before either backend loads it; the reference runner's sandbox loads it as it loads any module, through the checker, so the unchecked load built for the port is withdrawn.

## Alternatives rejected

- **A port's first version exempt from the mirror rules.** The rules are what make a mirror mean something, nothing here writing it but the import (check 53), and a deployment's real port is held to them; exempting the scenarios' port would make the design's own worked example the one version no deployment may write.
- **Version 1 kept as written, loaded unchecked.** The design corpus would hold a version the checker refuses, the core's tolerance of it would go untested, and the corruption of each cascade's `on` showed what an unchecked load lets pass.
- **Mirroring only the types the port imports.** The imported types reference nearly every other type of the journey, `Robot` six of them, and a mirror stores references only to mirrors (ADR-0154 decision 1), so the closure is all but `Agent` and `Service`; one rule for every type, as §1 already said, is the simpler statement.

## Consequences

- `metric-scenarios.md` §1 states what version 1 declared; the replay derives it and loads it checked, on both backends. The core's replay matches every read it matched before, 123, the history being the same after the cutover.
- `TODO.md`'s question of the scenarios' port is decided and done.
- Building it found that check 44 refuses a minted identifier on any type with no creation, counting what no creation writes as nothing rather than as everything; an abstract type is exempt by name, a mirror is not. Whether a mirror may declare a minted identifier, and whether the check is wrong on a type with no creation, is recorded in `TODO.md`; version 1 declares none.
