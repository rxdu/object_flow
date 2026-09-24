# ADR-0098: The standard metrics are declared definitions, a reader reads a metric as a guard does, and the PRD's C2 and UC-10 are proposed for revision

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` M1, M3, M4, M6, C2, C3, C1, T5, UC-1, UC-2, UC-10, UC-11, UC-13, UC-16, UC-18; repairs D240 to D244
- **Date:** 2026-09-23
- **Refined by:** ADR-0101 — the standard metrics exclude the import's events, clip finished spans, carry version and actor kind, and include two per-object metrics; they are checked as an instantiated block. ADR-0105 — `override_counts` counts every override, admissions included. ADR-0106 — every metric, declared or standard, has the version and actor-kind dimensions.
- **Refines:** ADR-0084, ADR-0086, ADR-0096

## Context

A review of the record against the PRD tried to write the use cases as metric declarations and calls, and found five gaps (`design/defects.md`).

- **D240.** The standard metrics M1 and M6 require were a list of phrases in `DESIGN.md` §5.12 and §5.13. None had a name to pass to `metric()`, a source, dimensions or a value, and the renderers could not list them. Several could not be written in the §6.9 grammar at all:
  - a refusal *rate* needs two datasets, and a metric has one `from`;
  - "someone other than the assignee acts" needs the actor's id, which no row carried;
  - handoffs per object needs a per-object count summarised across objects, which is two levels of aggregation.
- **D241.** A guard could bind a metric's dimensions, apply a window and aggregate over a dimension it left unbound. `metric()` could do none of these, so a screen could not read the value a rule reads (C2, UC-10, UC-11).
- **D242.** Several sources had no `.declaration_version`, so M3's "any metric" could not be split by version. `<Type>.attempts` had no actor or object. The daily rollup had no version.
- **D243.** A set-valued dimension ("by product configuration", UC-1) and a distinct count ("how many jobs carry the label", UC-13) could not be written. A derived attribute read by a metric row had no stated visibility.
- **D244.** The design read C2 and UC-10 in a way that departs from their wording, without proposing the revision the PRD's own status line requires ("a design that finds the PRD wrong proposes a revision rather than working around it").

## Decision

### 1. The standard metrics are declared, in a built-in module

`declaration-syntax.md` §6.11 gives each standard metric as a `metric` declaration over a type parameter. Each has a name:
- `<Type>.<name>` for a type's metrics;
- `<Type>.<name>.<reference>` for the metrics of one assignee reference.

They are checked like any declaration, printed with the type in the rule set, and offered by name in the metrics tool. M1 and M6 are those definitions, not a list of phrases.

### 2. The grammar gains what the definitions need

- **A combined metric:** `combine <name> = <metric>, …` in place of `from`, and a `value` over those names. A combined metric that lacks one of the composite's dimensions contributes its value to every group that differs only in that dimension. A group it has no rows for reads as its value over no rows: zero for `count` and `sum`, `unknown` otherwise. That is how a refusal rate divides refusals per clause by the transition's requests.
- **Per-object flow collections:** in a row of `<Type>`, `o.intervals`, `o.intervals(<member>)`, `o.transitions` and `o.attempts` are collections an aggregate in the row may range over. A per-object measure can then be summarised across objects.
- **Row members:**
  - `o.open`, which is true until the object enters a `closed` or terminal state;
  - `o.entered_at(…)` and `o.time_in(…)`, as §8.3 defines them for `this`;
  - `t.completes` and `t.returns` on a transition row: the object's first entry into a `closed` or terminal state, and an entry into a state it held before;
  - `.actor_id` on transition and attempt rows;
  - `.object` on attempt rows;
  - `.declaration_version` on every source's rows;
  - `<reference>.actor_id` on a reference whose target marks an `actor` identity.
- **`count(distinct <binder> in <collection> [where …]: <body>)`**, and in a metric's value `count(distinct <body>)`.
- **A set-valued dimension** counts a row once under each member. A path's hops are counted from the row's object, so `.object` on a dataset row is not a hop.

### 3. A reader reads a metric as a guard does

`metric(actor, name, bind, over, filter, cursor)` takes the same things a guard's reference takes: dimension values to bind, an `over last` window, and a `filter`, which is a boolean expression over the declaration's own binder. A declared dimension left unbound is aggregated over. *(Refined by ADR-0101 §7: that is a guard's reading; a reader keeps every declared dimension by default, one row per group, and names fewer with `keep`.)* A screen, an agent and a rule therefore read one value from one definition (C2), and a guard's 30-day value is readable by the person the guard refused (UC-10, UC-11).

### 4. A derived attribute in a metric row is read under the reader's visibility

Its aggregates range over objects the reader can see, and the types it reads count toward the result's `complete` flag (ADR-0096 §3). A metric over a derived stock count therefore neither counts hidden units nor claims to be complete when it does not see them.

### 5. The rollup and the attempt rows carry what the splits need

`of_attempt_rollup` gains `declaration_version`. `<Type>.attempts` rows expose `.actor_id` and `.object`, within the retention period in which the rows exist. The permanent daily counts split by actor kind, which is what UC-2 counts by.

### 6. C2 and UC-10 are proposed for revision, and marked so

C2 as written — "every consumer … gets the same value" — cannot hold beside T5 for a consumer who may see only part of the data. UC-10's "the refusal names the value" cannot hold for a requester who may not see what the value aggregates.

`docs/PRD.md` §12 therefore proposes revision 4 *(now PRD §12's proposed revision, unnumbered since 2026-09-24, after the author's positioning decision and a coherence review took revisions 4 and 5)*, for the author to accept or refuse:
- C2 gets the same value from the same data, with a partial reader's value marked partial.
- UC-10's refusal names the value where the requester may see what it is computed from.
- UC-8, which tests C2, gets the same wording as C2.

Until the author decides, `traceability.md` marks the three rows **revision proposed**, a status its checker allows only for requirements the PRD lists there. The design is otherwise unchanged by the proposal: it already behaves as the revised wording says (ADR-0096 §3).

## Alternatives rejected

- **Leave the standard metrics as prose and compute them in code.** They would be the one set of metrics nobody could read the definition of, against C2, and a reader could not tell which intervals a figure counts.
- **Allow several `from` sources with a join.** A join over two datasets needs a key, and the rows of intervals, transitions and attempts share only the object. Combining metrics by their dimensions says what a rate means without a join language.
- **Let `metric()` return only the declared grouping.** A reader who wants the value a rule reads would need a second definition, and the two could drift.
- **Read C2 and UC-10 as the design needs, without proposing a revision.** The PRD's status line forbids exactly that, and the previous iteration did it.

## Consequences

- `declaration-syntax.md` §6.9 gains the constructs and members, and §6.11 the standard metrics.
- `DESIGN.md` §5.12, §5.13 and §10 point at the definitions, and `metric` takes the guard's arguments.
- `library-api.md`'s `metric` signature changes, and `renderers.md`'s metrics tool offers the standard names and the arguments.
- `storage-schema.md`'s rollup gains its version.
- `docs/PRD.md` gains §12, the proposed revision 4, and `traceability.md` gains the `revision proposed` status.
- D240 to D244 are resolved.
