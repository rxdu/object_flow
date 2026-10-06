# ADR-0161: Scenario versions, query fields, empty label names and unresolved imports

- **Status:** Decided by the author, 2026-10-06 ("go with your recommendations on 2 to 5"), on recommendations stated with the questions.
- **Date:** 2026-10-06
- **Refines:** ADR-0127 (what `query` reads), ADR-0106 (labels), ADR-0155 (what a publish checks)
- **Relates to:** `DESIGN.md` §5.9, §10; `design/metric-scenarios.md` §1, §3.4; `design/library-api.md` §6; `design/declaration-syntax.md` §6.8; `design/publish-and-import.md` §1

## Context

Four questions the core's replay of `metric-scenarios.md` and its slices raised, each searched for in the record first.

- The scenario numbers declaration versions from the journey, as version 1, and §1 says the store imported the legacy objects "while every type was a mirror", a declaration of its own. The core's import writes only into mirror types (ADR-0100, ADR-0101), so its port publishes that declaration first and the journey becomes version 2; every version the scenario names is then one less than the store's, and its one read split by version could not match.
- `metric-scenarios.md` §3.4's query table quotes each delivery's entry time into its state, the order key, and a `gaps` column. A `Page` holds items with the fields asked for, and gaps are a metric row's (`DESIGN.md` §5.12); `query`'s `fields` named attributes, derived values and a few members, and not `entered_at(…)`, which its `order` takes (ADR-0127).
- `declaration-syntax.md` §6.8 normalises a label's name to lower case, trimmed, and says nothing of a name that is only spaces, which the core recorded as the empty name.
- A change's source is the whole version, so a module it leaves out is removed with its types, which are then retired (`declaration-syntax.md` §6.6), and a remaining module importing from it passed with a notice, its references then naming retired types.

## Decision

1. **The scenario numbers versions as the store does.** The port's all-mirror declaration is version 1 and the journey, which cut it over, version 2; every version `metric-scenarios.md` names is one more than it was, and its transcription counts from 2.
2. **A query page carries the fields asked for and nothing of its own.** `fields` may name `entered_at(<tracked member>)`, the entry time of the object's current interval of that member, as `order` may; `recorded_from`, which `fields` already names, says where an object's record begins, so a reader sees an entry before it. §3.4's table reads both as fields and has no `gaps` column; gaps stay a metric row's.
3. **A label whose name is empty once normalised is refused as `invalid input`, naming `name`.**
4. **A published version resolves its imports among its own modules.** An import from a module the version does not carry is a fatal finding of the publish, as an unresolved reference is, where it was a notice.

## Alternatives rejected

- **Counting the scenario's port as version 0.** Version 0 is the built-ins a store is created with (ADR-0105), and the mirror declaration is a publish like any other.
- **Keeping the scenario's numbers and mapping them in the replay.** A metric split by version reads the store's numbers, and a document whose numbers differ from them cannot be read against the store.
- **Every item carrying its order key's value.** A page would carry what nobody asked for; a reader who wants it asks for it.
- **A `gaps` column on query pages.** Gaps are a metric's account of rows read from before the record; a query reads current values, and `recorded_from` gives a reader the same fact.
- **Recording the empty name.** A label nobody can name or count under a name is noise in every label metric.
- **Refusing it as `unsatisfied`.** No guard refuses it; the input is malformed, which `invalid input` names.
- **Leaving an unresolved import a notice.** The version would publish references to types that no longer exist.

## Consequences

- `metric-scenarios.md` §1 names versions 1 and 2, every later version is renumbered, and §3.4's table reads `entered_at(state)` and `recorded_from`; ADR-0129 and `TODO.md`'s closed entry citing version 14 are annotated.
- The core's `query` takes `entered_at(<tracked member>)` as a field, refuses a label name empty once normalised, and the publish checker's unresolved import is fatal, in the core and in the Python checker it is held to.
- `library-api.md` §6, `DESIGN.md` §10, `declaration-syntax.md` §6.8 and `publish-and-import.md` §1 step 1 state the rules.
