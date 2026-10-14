# ADR-0202: The fifteenth review's findings, decided

- **Status:** **Decided** 2026-10-14 on the author's standing instruction, "keep the bar and continue the loop", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-14
- **Refined by:** ADR-0203 — an erased identity replaced by value in every attribute and input of type identity, which contains decision 1's set.
- **Refines:** ADR-0192 decision 4 (where an erased actor's identity is replaced); ADR-0201 decisions 3 and 4 (the build's SQLite, an agent's personal query); ADR-0192 decision 6 (an identity's shape)
- **Relates to:** `design/production-review.md` §19; `guides/operate.md`; `guides/agents.md`; `guides/wire.md`; `guides/errors.md`

## Context

The fifteenth production review (`production-review.md` §19) found no blocker. Its operator found no major and gave GO, having measured every major of the fourteenth fixed. Its adversary found one major, so the bar is not met.

**M1.** An erasure of an actor whose identity is personal replaces the identity with the actor's object id where the engine records actors: events, attempts, intervals, idempotency records, and the built-in types' recorded actors. A flow's own attribute set to `actor.id`, the README's `approved_by` pattern, kept the erased person's address. A `get` answered it, a query found the memo by it, and the store's file held it after the `-wal` was cut. operate.md promised the identity replaced "everywhere the store keeps it".

**The operator's minors:**
- ADR-0201 decision 3 lived in the build's environment. A build from the repository's root with `--manifest-path core/Cargo.toml` did not read `core/.cargo/config.toml`, and brought back the collapse exactly: four filters 11.3 s each.
- A query by an actor the store does not know passed the personal guard, which read only an agent's kind.
- A `metric` read bound to one claimant on the wire read the whole type: 0.35 s on six years, where the guard computing the same value took 20 ms.
- The guides lacked:
  - the first publish's socket, which only the next section's unit makes;
  - a prune unit;
  - a local filesystem for the store;
  - that metric names are not promised;
  - that `/health` stays `200` while the main listener is full.
- agents.md named `malformed_filter` where the service answers `personal`.
- A busy `--metrics-bind` port was refused without the flag's name.

**The adversary's minors:**
- A date moved by part of a day answered `501 not_run`.
- An identity shaped as an object's id in another case, `Staff-2`, was taken, while `staff-2` named the object.
- A type added by a later version turned an existing identity, `widget-66`, into an object's id, and the publish said nothing.
- `check`, which records nothing, revealed a unique value's holder and, by halving a guess in 30 calls, a personal salary.
- A per-person metric's value travels in each guarded event's `consulted`.
- The tools offer `occurred_at`.
- After a crash, the start's message about the `-wal` read as advice to delete it.
- A restore brings back what was erased since the backup, and the guide did not say so.

## Decision

1. **An erasure of an actor whose identity is personal also replaces it where an effect wrote it**:
   - in every attribute a transition's effect sets to `actor.id` or `actor.principal`;
   - in every input a `create` or a `call` passes either, in the column it fills and in the payload of the actor's own events that took it.

   Such a value is always written by the actor's own event, whose index finds it. A value computed from the identity is not found by equality, and the guide says to assign the identity itself. Both backends. (M1, D663)
2. **The repository's `.cargo/config.toml` builds SQLite without its shared page cache, for a build run anywhere inside the repository**, and `objectflow-core`'s build script stops a build without the flag, saying how to give it. (D664)
3. **The personal guard on a query applies to every reader but actors the store knows as people or services**, the operator, a person, among them. An agent not yet registered is one it does not know. (D665)
4. **A metric read bound on the wire to one of its item's stored attributes is narrowed as a guard's is**, the binding decoded as the attribute holds it. One that does not decode reads the type, the binding then deciding. (D666)
5. **A date moved by part of a day has no value**, in both backends, as one moved past the calendar has none. An effect writing it is refused `value_in_range`, and a guard reads it as unknown. (D667)
6. **An identity shaped as an object's id in any case of its letters is `malformed`**, in both backends. (D668)
7. **The guides:**
   - the agents' gateway refuses `check`, and sets `occurred_at` (D670);
   - `consulted` carries a per-person metric's value, filtered as the metric is (D671);
   - a crash's `-wal` is kept, and only a restore's is removed (D672);
   - erasures are asked again after a restore, from a record kept outside the store (D673);
   - the first publish runs once the unit serves, there is a prune script and its units, verified against a service, the store is on a local filesystem, metric names are checked after each upgrade, and `/health` is watched with the metrics (D674);
   - agents.md's refusal code (D675);
   - the flag in a busy `--metrics-bind`'s refusal (D676).
8. **Unchanged, or recorded for later:**
   - A version adding a type may make an existing identity name an object (D669, open): the publish is to say so, recorded for later.
   - A read runs on after its client has gone (D656, open).
   - An integer divided by a decimal, and a duration literal past 64 bits, answer `not_run` where the review read them, untriggered: the first needs the rounding both backends share, the second a checker's refusal.
   - The start's quick-check refusal was reported printed twice; it was not reproduced, and is recorded.
   - A heavy metric or aggregate sweep costs about half as much again with four at once on a busy six-core machine, measured by the operator.

## Alternatives rejected

- **Rewriting every text column holding the identity** (decision 1). It would scan every text column of every type, and every event's payload, under the writer, for each erasure. An identity reaches an attribute through `actor.id` or `actor.principal` alone, unless a client sends it, and an input a client sends is the flow's to mark `personal`.
- **Refusing `actor.id` written to an attribute not marked personal** (decision 1). The attribute is the memo's, not the person's: marked personal, it would be erased with the memo, never with the person, which is what the person's erasure asks.
- **Writing an actor's object id for `actor.id` where its identity is personal** (decision 1). A guard comparing `actor.id` with an identity attribute would then compare an id with an identity.
- **A health state for a full listener** (D674). An orchestrator restarting a service whose connections are all busy would drop them all; the refused connections' counter says it.
- **Refusing an agent's `check` in the engine** (D670). The generated tools offer it to no agent, and an agent's wire is closed by the gateway, as for the operations that govern the store (`agents.md`); `check` writes nothing, so what it reveals is the gateway's to keep from an agent, as a read's is.

## Consequences

- An identity like `Staff-2`, taken before, is refused where it is given again; one held stays held.
- A query by an actor the store does not know is refused `personal` where its filter or order reads a personal member, which it answered.
- A build from outside the repository stops unless `LIBSQLITE3_FLAGS` holds the flag.
- Each fix is held by a test that fails without it (`defects.md` D663 to D676), each mutated, but the build script, which a build without the flag shows stopping.
