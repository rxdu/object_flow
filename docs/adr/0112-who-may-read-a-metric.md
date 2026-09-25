# ADR-0112: Who may read a metric, with splits by person restricted by default

- **Status:** **Superseded by ADR-0114** (2026-09-25). Previously: Accepted by the author, 2026-09-25 ("yes to all four, go ahead and write them"), with the restricted default the recommendation named, against `docs/PRD.md` revision 9: T7, M2, M6 and UC-24.
- **Date:** 2026-09-25
- **Superseded by:** ADR-0114, the same day — who may read a metric is the upper layer's; the engine marks the dimensions that name a person. This record keeps the audience rule as it was decided.
- **Refines:** ADR-0030 (visibility), ADR-0084 (metrics), ADR-0096 (a partial reader), ADR-0098 (the standard metrics)
- **Relates to:** ADR-0110 (actor kinds declared with their types), `design/first-consumer-prd-check.md` §3.3

## Context

T5 restricts a metric by the objects it counts: a reader gets the metric over the rows they may see. Nothing restricted who may read a metric at all. The case study keeps per-person performance behind a separate door, and has a recorded decision that the team never sees per-person numbers. Its team view still shows "current open work per person" openly. Since every caller reaches the engine through one interface (N7), a door kept only in the platform's dashboards would be bypassed by any caller's agent asking the engine directly. The author accepted an audience rule as a general mechanism, with splits by person restricted by default (PRD T7).

## Decision

1. **A metric may declare who may read it**, with `visible when <expression over actor>` (check 63). For a metric a deployment does not declare, such as a standard one, a module line `visible <metric>, … when <expression>` sets the audience instead.
2. **A split by person is restricted by default.** A read keeping a dimension whose value names an actor is readable only by holders of the built-in capability `OF_PERSON_METRICS`, unless the metric's own audience says otherwise. The value names an actor when it is an object of a type that marks an `actor` identity (ADR-0110), or an actor's id. So `cycle_time_by_assignee` per engineer is behind the door until a deployment opens it. A deployment whose team view shows open work per person opens `open_work` to the team with one module line.
3. **Outside the audience, a reader gets no rows** and a refusal naming `audience`, remedy `delegable`, with the capability. The same metric read without the person dimension, through `keep`, stays readable under the objects' visibility. A diagnostic whose subject names an actor, such as a reassignment loop or work done by someone other than the assignee, is behind the same door, since it is a per-person figure by another route.
4. **Rules are unaffected.** A guard reads a metric over every row whoever asks, since a verdict may not depend on the requester (ADR-0084). Its value is withheld from a requester outside the audience, as from a partial reader (ADR-0096).
5. **The door covers aggregates, not attribution.** Who acted on an object, and who held it, stay visible to whoever may see the object (DESIGN §5.8), and a reader who sees the objects could count by hand. A deployment that must hide attribution uses visibility on the objects. This is recorded as a known limit.

## Alternatives rejected

- **The door only in upper-layer applications.** Any caller reaching the engine directly, an agent among them, would bypass it (N7).
- **Every metric open by default, restricted only where declared.** A per-person figure exposed by accident is hard to take back, and the author chose the restricted default.
- **Every split by person closed with no way to open it.** The case study's team view shows open work per person openly, so a deployment must be able to open a split, the standard ones included.
- **Hiding attribution as well.** That is visibility on the objects, which already exists; hiding it by default would break the history's use to the team.

## Consequences

- `DESIGN.md` §5.12 states the audience, §9 adds `OF_PERSON_METRICS` to the built-in capabilities, and §13 records the limit.
- `declaration-syntax.md` §6.9 adds `visible when` on a metric and the module line, and check 63. `time_working` shows it: team leads who assign work may read it per engineer.
- `library-api.md`: `MetricPage.refused`. `renderers.md`: the rule set prints each metric's audience.
