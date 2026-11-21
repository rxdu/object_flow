# ADR-0290: An act creates an object for each entry it is given

- **Status:** **Decided** by the author 2026-11-21 ("supervise a session to address the issue with your recommendations"), as proposed the same day for issue #102 under the author's direction ("review open issues and address properly"), its direction and each of its points put with a recommendation. The author skipped the trace put with its direction. To be built for 0.32.0, with ADR-0289.
- **Date:** 2026-11-21
- **Refines:** ADR-0271 decision 5 (a field that differs by element is named by the step); the `create` step of `flow-format.md` §4.8
- **Relates to:** ADR-0254 decision 9 (a value's later slices); ADR-0275 (a derived set defaults to the empty set, and a derivation carries through); ADR-0180 decision 2 (what `/v1` may add); ADR-0228 (a batch); `storage-schema.md` §9 (erasure); `docs/research/deriving-an-acts-inputs-from-the-record-it-makes.md`, its section on many records; issue #102

## Context

Issue #102, filed 2026-11-20 at 0.30.0, asked that one act take a list of entries, each naming an object with fields of its own. Its case is an item admitted with its lines in one act, which also puts the item on sale, after which no line can be added to it. Each line names a model, how many units, and whether the customer may decline it. The act can take the models as a set of references and loop over them, but cannot take a quantity or a flag beside each.

A field may differ by element only where a loop's collection holds it. `rest: inputs` gives a creation the request's inputs, the same for each element, and a field that differs by element is named by the step (ADR-0271 decision 5). A set cannot carry the entries: on the wire it repeats no element, and a loop runs over it in its own order. A value cannot carry them either: its fields hold no reference, and a declared input holds one value, never a set of them (ADR-0254 decision 9). Both were refused at the 0.31.0 release build (`TODO.md`, *Issues filed on GitHub*).

The issue asked for a declared input holding a list of values whose fields may be references. The line's fields are already declared once, as the inputs of the line type's initial transition, with that type's guards and invariants holding them. Rails, Django and Odoo take each entry as the related record's own fields, as an ordered list, bounded, and Rails saves the records in the parent's transaction (the research note).

What reads a request's inputs today reads each by its name at the top of `inputs`: erasure nulls `inputs.<name>` in an event's payload (`store.rs`), export keeps or drops each name, a batch resolves a local name in a value or an array element (`request.rs`, `lids_in`), and an offer's `inputs` maps each name to a type string (`wire.md`). A list of entries is a new kind of input, which each of them must learn.

## Decision

1. **A `create` step may take `each: <name>` and a `limit`.** The act then takes an input `<name>`, a list of entries, and the step creates one object for each entry, in the order given, by its target transition. The step derives the input, as `rest: inputs` derives its inputs: the act declares no input of that name.
   ```yaml
   effect:
     - create: { type: Line, transition: add, inputs: { item: this }, each: lines, limit: 50 }
   ```
2. **Each entry holds the target's inputs the step does not name**, each as the target declares it: of the same type, read in the target's module, with its `not_blank` and its description, required where the target requires it and gives no default, and optional otherwise. A set field is never optional, and defaults to the empty set where the target need not be given it (ADR-0275). A target taking the rest of its own inputs carries them into the entry (ADR-0275 decision 3). The inputs the step names are the same for every entry. `subject` and `corrects` are never an entry's.
3. **A personal field may be an entry's, and erasure reaches it.** Each object an entry creates records its entry's place, `entry`, in its event's payload. Erasure redacts `inputs.<name>[<n>].<field>` in the act's event wherever it would redact that field as a top-level input, reading each event's version, so a field a later version makes personal is redacted in earlier events too, as `storage-schema.md` §9 promises of every input. Export drops it as it drops a personal input.
4. **The list is never optional, and defaults to the empty list**, which creates nothing, as a derived set defaults to the empty set (ADR-0275). An entry may repeat another, and each makes its own object. More entries than the `limit` are refused `over_limit`, its `loop` naming `each <name>`, whole and never truncated. The count is checked with the inputs, before any rule runs, since it is the request's.
5. **The list on the wire, in the event, in `declaration` and in offers.**
   - A request gives it as an array of objects, each holding the entry's fields as the target's inputs are written, and an example as the request does.
   - The act's event records it as given.
   - `declaration` describes it with `entries`, each field's description as an input's, its `limit`, its `default` of `[]` and `derived_from` naming the target, and no `type` or `reference`.
   - An offer's `inputs` gives it the type `entries(<Type>.<transition>)`, a form reading the fields from `declaration`.
6. **A refusal names the entry in fields of its own**, so a name in `inputs` keeps its meaning (ADR-0180 decision 2).
   - An entry wrong before any rule runs is refused `invalid_input`, `inputs` naming the list and `entries` giving each fault as `{entry, input, reason}`, `entry` counted from 0.
   - A creation refused for an entry is `call_refused`, with `entry` beside `step`.
   - The act is all or nothing, as every request is (PRD F2). Numbers drawn for the entries before a refused one stay drawn, as for any refused request.
7. **This slice takes less than the target may declare**, each refused at step 3 or at a publish, naming the step.
   - An entry holds no file.
   - A target with its own `each` is not an `each`'s target.
   - No expression reads the entries.
   - A step with `each` takes no `rest: inputs` and no `result`, and stands in no loop.
   - `<name>` is no attribute the act takes and no input it declares or derives, and no two steps read one list.
   - An entry's field takes no batch local name, and a batch never reads an entry as one.
   - A version renaming an input an `each` derives is refused, since `renamed` maps the names at the top of an event's `inputs` alone, and an earlier event's entries would keep the old names unmapped.
8. **The loader expands it**, as it expands `rest: inputs` (ADR-0271 decision 8), into a new kind of declared input that every reader of inputs learns: the checkers and runners, the publish check that invalidates pending proposals, the agent tools and `declaration`.
9. **A declared list of values whose fields may be references**, as the issue asked, stays unbuilt until a case needs entries that make something other than one object each.

## Alternatives rejected

- **A declared list of values whose fields may be references**, as the issue asked and as GraphQL declares an input type apart. The line would be declared twice, as the value's fields and as the target's inputs, and the step would still pass each field into its `create`, so a field added to the line takes three edits. It needs a reference held in a value, ADR-0254 decision 9's later slice, and a collection of values held by an input. It meets the same questions of erasure, offers and refusals as this one.
- **A set of values as the input.** On the wire a set repeats no element, and a loop runs over it in its own order, not the one the caller gave.
- **A `foreach` over the list, each creation taking the rest of its inputs from the entry.** The list would be an input of its own, naming its target and the inputs the step fills, while the step names the target again and fills those inputs, two places for one shape. It would let other steps and guards read the entries, which no case needs yet.
- **An entry holding no personal field.** A field a later version makes personal could then never be redacted in the events recorded before, which `storage-schema.md` §9 promises of every input.
- **A path in `invalid_input`'s `inputs`, as `lines[1].quantity`.** It changes what a name there means, so a client marking a form's field by it marks nothing.
- **A batch of requests from the application**, as the issue considered. It is atomic, but it is no transition: `availability` cannot offer it, and a form rendered from one transition cannot render it.
- **Each line added after the act.** The act puts the item on sale, after which no act changes it, by the flow's design.

## Consequences

- **The loaders**: `check-flows.py`'s, and the core's `check/view.rs` and `model.rs`, expanding `each` beside `rest: inputs`.
- **The runners**: the core's request and the reference runner, `flowrun.py`, and the Flow Studio's sandbox, create one object per entry, record its place, and name an entry in a refusal; the core's erasure and export, and the sandbox's, reach an entry's field.
- **The readers of inputs**: the publish check that invalidates pending proposals (`publish.rs`); the agent tools (`tools.rs`, `flowtools.py`, `renderers.md` §3); the sandbox's `declaration`, which lists the enumerations an entry's fields read.
- **The schema and the docs**: `flow.schema.json`'s `create`; `flow-format.md` §4.8 and the format's card; the examples format; `wire.md`'s notation, offers and `declaration`; `errors.md`'s `invalid_input`, `call_refused` and `over_limit`; `library-api.md` §6; `storage-schema.md` §9.
- **Tests**, at step 3 and on both backends, for each of decisions 1 to 7, an erasure reaching an entry's field among them, and planted defects in each checker and runner.
- **What a flow meets**: a module without the key reads as before, so the release refuses nothing it took before. `/v1` gains an input kind, an offer's type, and the answer fields `entries` and `entry`.
