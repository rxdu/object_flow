# ADR-0205: The audit's decisions

- **Status:** **Decided** 2026-10-14 by the author ("go with your recommendations on the decisions"), each the recommendation stated here
- **Date:** 2026-10-14
- **Refines:** ADR-0204 decisions 9.3 and 9.4; ADR-0201 decision 6 (the idempotency digest); ADR-0202 decision 8 and ADR-0203 decision 1 (identities); ADR-0191 decision 3 and ADR-0192 decision 7 (a derived time past the calendar, refused where it was answered null); `declaration-syntax.md` §8.1 to §8.3
- **Relates to:** ADR-0204 §6 (the audit); its six inventories, `design/audit/personal-data.md`, `write-path.md`, `values.md`, `cost.md`, `durability.md` and `guides.md`, summarised in `design/production-review.md` §21; `TODO.md`

## Context

The audit ADR-0204 §6 set out ran on 2026-10-14 at `698034b`: six reviewers, one area each, each returning its whole inventory graded by ADR-0204 §4.
- **Personal data** found erased identities surviving in sets, proposals, consulted keys, attempts, another case and the published version's approver, and reads that let a personal value be inferred.
- **The write path** found:
  - a label's note erased with no event;
  - `type` beside an id ignored for proposals and changes;
  - an identity rewritable after creation;
  - rules held only on a direct request;
  - gaps in the idempotency key.
- **Values** found:
  - an integer written to a decimal stored in its units;
  - a quotient rounded at the dividend's scale;
  - a value of another type written to any column;
  - failed arithmetic answered as unknown;
  - `any`, `none` and `count` ignoring their body.
- **Cost** found:
  - one-object rules reading a set that grows with the store, outside any pool;
  - per-person metric reads past PRD C5;
  - the urgent `-wal` cut stopping every read;
  - writes saturating at a month-end peak.
- **Durability** found:
  - a restore handing ids, positions and sequence numbers out again;
  - writes acknowledged after the service found its store bad;
  - an intervals export that never retracts;
  - subscriptions matching nothing.
- **The guides** found a printed restore instruction that undoes the restore, and examples of identities a later type can shadow.

Several findings needed a decision, not only a fix. The author accepted the recommendations below.

## Decision

### Values

1. **A quotient written to a decimal is computed exactly and rounded once, half to even, at the target's scale**, as `declaration-syntax.md` §8.3 says and the reference runner does. A decimal literal takes its other operand's scale (§8.1). An integer written to a decimal is converted to its scale before the write, and the store refuses a value whose kind or scale is not its column's.
2. **Arithmetic past its type's bound is never unknown.** That covers an integer past 64 bits, a decimal or an amount past its digits, a time or a date past the calendar, and a date moved by part of a day. Unknown is absent data, and a division by zero, as ADR-0047 and §8.3 define it.
   - A read whose derived value or filter is past its bound is refused `value_in_range`, naming the object and no value.
   - An invariant, a guard or a loop's filter past its bound refuses its request `value_in_range`.
   - A metric whose filter or value is past its bound refuses the read.
   - The side of an `and`, an `or` or an `implies` that does not decide refuses nothing.
   - §8.2's table and `wire.md` are rewritten to say so.

   *(Corrected 2026-10-14, while building it: as first written, this said "Unknown is absent data alone", which would have refused a read of `done / total` with `total` 0, and every object it is derived on. The audit's findings, and the recommendation the author accepted, were of arithmetic past its bound; ADR-0047 keeps a division by zero unknown.)*

### Identities and the record

3. **An idempotency key keeps two digests.** One is masked; the other covers the personal values, and an erasure deletes it. Before an erasure, the same request with another personal value is `key_reused`; after it, the key confirms no guess. This replaces ADR-0201 decision 6's trade-off.
4. **A publish that adds or renames a type whose ids would name an identity the store holds is refused**, naming the actors that hold them by their objects (D669). Every `<word>-<number>` identity is not refused, since employee numbers take that shape. The guides' examples change, and say why. *(Corrected 2026-10-14, while building it: this said the finding names the identities, which would keep a personal identity in a change's report past the actor's erasure.)*
5. **An actor's identity attribute is written only at its creation**, since the record names actors by it.
6. **An erasure replaces an erased identity in every case of its ASCII letters**, as identities compare:
   - in sets of identities;
   - in proposals' inputs and events;
   - in attempts;
   - in corrections' records;
   - in a published version's approver.

   A consulted key masks an `identity` argument or binding where an actor's identity is personal.

### Durability and events

7. **A restore advances, past a margin, every counter the store hands out from** (decision 9.4, widened): object numbers, event positions, transactions and the flows' sequences. It refuses every cursor that falls in the gap.
8. **A store the service finds bad latches.** Once the store is unreadable, replaced or overwritten in place, or its `-wal` is gone, writes are refused and the stop's checkpoint is skipped until a restart.
9. **The intervals export carries a tombstone for every interval a later commit deleted or renamed.** This needs a migration in the 1.0.0 release entry.
10. **A subscription is checked as it is made.** Its type and transitions must exist and its `dead_after` must be positive. The check sits in the engine's input check, so the built-in types, and the fingerprint 0.2.0 stores upgrade by, stay as they are.

### Personal reads

11. **Decision 9.3's guard covers every read that lets a personal value be inferred:**
    - a query's filter and order;
    - `available`'s selection by a guard that reads a personal member;
    - `lookup` by a personal external identifier;
    - a metric's filter and its binding of a person dimension;
    - `check` and `availability`;
    - a refusal that names a holder.

    Without the request's opt-in, each is refused `personal`. `consulted` values and `actor_id`, which a read answers rather than selects by, stay answered for the gateway to filter, as the guides say.

### Cost

12. **Every per-object construct that reads a set growing with the store reads it by an index, and is classified as growing**, so its reads wait in a pool. The constructs are an object's transitions, its attempts, its declaration's version, an observation collection and a proposal's last verdict.
13. **Metric reads are made to meet C5.** They read by index and in batches, and any read that cannot is brought to the author by name (ADR-0204 decision 9.6).
14. **The urgent `-wal` cut stops only the reads of pools that did not drain.**
15. **A metric's filter takes the query filter's bounds.** Attempt counts are kept by key and checked against their deadline.
16. **A read stops when its client goes** (D656), so retrying clients cannot fill a pool with reads nobody waits for.
17. **The writer's cost per commit is reduced.** About 140 KB of `-wal` per write is measured first; a month-end peak that still misses C5 is brought to the author by name.

## Alternatives rejected

- **Rounding a quotient at its dividend's scale** (decision 1). It contradicts §8.3 and the runner, and loses digits a wider target holds.
- **Failed arithmetic as unknown** (decision 2). An invariant then holds on an overflow and admits the write it should refuse.
- **One masked digest** (decision 3). A key then replays another request that differs in a personal value.
- **Refusing every identity shaped `<word>-<number>`** (decision 4). It refuses employee numbers, and the shadowing it prevents comes only from a type added later, which the publish can check.
- **Renaming the record when an identity changes** (decision 5). It rewrites history for what opaque identities avoid.
- **Guarding `consulted` and `actor_id` too** (decision 11). They answer, rather than select by, and an integration needs them; the gateway filters them (PRD N7).

## Consequences

- Many fixes refuse what 0.2.0 accepted (ADR-0204 §11), each listed in `TODO.md` and landing before 1.0.0.
- The fixes land area by area: values; identities, the record and personal reads; durability and events; cost; the guides. Each class is closed whole and held by a kept check, with the gates after each area.
- Stores 0.2.0 wrote may hold values decision 1's cause stored in their units. The 1.0.0 upgrade cannot tell those from correct ones, so `CHANGELOG.md` says so.
