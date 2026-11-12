# ADR-0271: An act's inputs are derived from the record it makes

- **Status:** **Decided** by the author 2026-11-12 ("proceed with all 3"), as proposed the same day for issue #88, its direction decided before it ("proceed with your recommendations"), on the recommendation and its four points put that day. To be built for 0.24.0; built the same day, the marker served as `derived_from` (`library-api.md` §6).
- **Date:** 2026-11-12
- **Refines:** ADR-0270 (an effect records an observation); the `create` step of `flow-format.md` §4.8
- **Relates to:** ADR-0254 decision 9 (a value's later slices); ADR-0176 (`taken_by`, sugar the loader expands); ADR-0052 (an optional input left out skips its write); ADR-0078 and the model's check 60 (a personal input held needs an erasure); `docs/research/deriving-an-acts-inputs-from-the-record-it-makes.md`; issue #88

## Context

Issue #88, filed 2026-11-12 at 0.23.0, asked that a value hold references, sets, files and `personal` fields (ADR-0254 decision 9's later slices), so that a group of inputs about fifteen acts take could be declared once. Its case is a custody record: an observation kind of twelve fields, which each act that changes a unit's hands records from its effect, filling what it knows and taking the rest, about nine fields, as its own inputs. Each act declares those inputs again and passes each into its `create`. An optional field added to the kind is passed by no act until each is edited.

The author directed on 2026-11-12 that the design serve ObjectFlow's long term and maintenance, not only the mechanism asked for. The record is already declared once: an observation kind's fields may be references, `personal` texts, sets of references, files and sets of files, with the kind's `invariants`. A kind with each of these passed the reference checker clean that day. What is declared again is only the act's inputs.

The research note finds the pattern in Django's `ModelForm`, which derives a form's fields from its model. The form leaves out by name what the code fills, and runs the model's validation from its own. TypeScript's `Omit` derives one type from another less some keys. Rails nests the associated record's attributes under one key. GraphQL declares a separate input type, for a reason that does not hold here: its output types hold fields an input cannot.

## Decision

1. **A `create` may take the rest of its target's inputs from the act's**: `rest: inputs`. For each input of the target transition the step does not name, the act takes an input of the same name, and the step passes it on.
   ```yaml
   effect:
     - create: { type: Custody, transition: record, inputs: { subject: unit, given_by: customer }, rest: inputs }
   ```
2. **Each derived input is the target's, as declared there.** It has the same type, read in the target's module, so the act's module imports the target alone. It is required where the target requires it and gives no default, and optional otherwise. It carries its `personal` marking, its `media_types` and its description. It also carries a marker naming the target, `Custody.record`, so a form or an agent's tool may show the derived inputs as one group.
3. **`subject` and `corrects` are never derived.** A recording names its subject. A correction stays a request of its own, never a field of every act.
4. **A derived input is the act's own on the wire, in its event, in its offers and in its examples.** An optional one left out skips its write, as any optional input does (ADR-0052). A personal one makes the act's type hold a personal input, so it needs an erasure (check 60).
5. **Within a loop as well.** A derived input is the request's, the same for each element; a field that differs by element is named by the step.
6. **A name is derived once.** Two steps deriving one name with different declarations are refused, and a derived name the act declares itself is refused (step 3, `names`). In each case the step names that input instead.
7. **Any `create`**: an observation kind's `record`, or a type's initial transition. A `call` takes none.
8. **The loader expands it**, as it expands `taken_by` (ADR-0176), so each checker and runner reads the derived inputs as declared ones.
9. **ADR-0254 decision 9's slices for references, sets, files and `personal` fields stay unbuilt** until a value held on a type needs them.

## Alternatives rejected

- **A value holding the group, as #88 asked.** The group would be declared twice, as the kind's fields and as the value's, and each act would still pass each field into its `create`. Storing a reference, a set or a file in a holder's columns needs integrity, `referrers`, index rows and erasure on both backends, for which no case exists yet.
- **One nested input, `handing: { record: Custody }`**, as Rails nests. It needs a nested shape on the wire, in an event's payload and in the redaction of a personal field. Flat inputs reuse each as it stands, and the marker keeps the group for a form.
- **A shared internal transition each act calls.** Each caller still declares every input it passes (issue #65's answer).
- **A transition-level key naming the record**, such as `records: Custody`. It hides which step makes the record, and an act making two records could not say which takes which inputs.

## Consequences

- **The loaders**: `check-flows.py`'s, the core's `check/view.rs` and `model.rs`, expanding `rest: inputs` after `taken_by`'s.
- **The schema and the docs**: `flow.schema.json`'s `create`; `flow-format.md` §4.8 and the format's card.
- **Tests**, at step 3 and on both backends, for each of decisions 2 to 7, and planted defects in each checker and runner.
- **What a flow meets**: a module without the key reads as before.
