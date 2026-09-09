# Renderers

Draft, 2026-09-09. Three projections of one declaration: text a person reads, tool schemas an agent is given, and the field-level hints a form needs. They exist because the declaration is inspectable at runtime (ADR-0010), and that property is worth nothing until something renders it.

**What is verified.** `scripts/check-renderers-doc.py` parses every JSON example, checks that the tool schema is a schema a validator accepts, and confirms it rejects a call missing a required field and a call carrying an unknown one. The rule set of §2 is prose and is not checked; nothing generates it yet.

## 1. What a renderer may not do

One rule, and it decides most of the rest.

**A renderer never restates a guard's logic anywhere a person or a model will read it as the rule.** The five-copies problem ADR-0010 exists to end is a business rule written as a storage constraint, as backend validation, as frontend form state, as error text and now as a line in an agent's prompt. A tool description that says "only call this when the delivery has no unchecked items" is the sixth copy, and it goes stale exactly as the other five did.

What a renderer does instead is **name the operation and point at the live answer**. `availability(id)` evaluates the real guards against the real data and returns a verdict with a remedy class. That is the projection. A rendering that duplicates the guard is a rendering that can lie.

The exception is the printable rule set of §2, whose entire purpose is to state the rules to a person — and which is generated per version, so it cannot drift from the version it names.

## 2. The rule set

A deterministic rendering of one declaration version, for a person to read and review.

Determinism matters more than prettiness: two renders of one version must be byte-identical, because the artefact people actually use is the diff between two versions. Attributes in declaration order, transitions in declaration order, guards in the order they are evaluated — which is the order that decides which one a verdict names.

```
Delivery — version 1
  tracking record

  Attributes
    approved_total   money(SGD), optional
    internal         bool, default false

  Parts
    checklist_items  ChecklistItem[]   cascades on cancel, complete_sale,
                                       complete_internal, delete → delete (≤500)
    approvals        Approval[]        cascades on the same four → discard (≤50)

  References
    units            Robot[]           the far end of Robot.binding

  States
    PREPARATION      live
    DELIVERED        closed, terminal
    CANCELLED        closed
    DELETED          closed, terminal

  complete_sale      PREPARATION → DELIVERED
    requires
      not_internal   the delivery is not internal            unreachable from here
      settled        every checklist item is checked         dependent
      signed         an approval that no later edit invalidated   delegable
      may            the actor holds DELIVERY_COMPLETE       delegable
    then
      for each of up to 500 bound units: sell it
```

The guard column on the right is the **remedy class**, which is the one thing a reader most wants: it says whether a failure is theirs to fix, someone else's, a matter of waiting, or impossible from here.

Guard descriptions are rendered from the expression, not written by hand. `none(c in checklist_items where not c.checked)` becomes "every checklist item is checked". That rendering is mechanical and lossy on purpose — the expression is beside it in the machine-readable form, and a person reading a rule set wants the sentence.

## 3. Tool schemas

An agent is given one tool per **requestable** transition. Only-via transitions are not tools; they are not requestable, and offering them would be offering a call that always returns `not requestable`.

```json
{
  "name": "delivery_complete_sale",
  "description": "Complete a delivery as a sale. Call availability(id) first: this transition is refused unless the delivery is in PREPARATION and its guards pass, and the verdict says which guard failed and what can be done about it.",
  "input_schema": {
    "type": "object",
    "properties": {
      "object_id": {
        "type": "string",
        "description": "the Delivery"
      },
      "expected_version": {
        "type": "integer"
      },
      "idempotency_key": {
        "type": "string"
      }
    },
    "required": [
      "object_id"
    ],
    "additionalProperties": false
  }
}
```

The description names the operation and directs the caller to `availability`. It does not enumerate the guards. An agent that wants to know whether it can complete a delivery asks; it does not reason from a copy of the rules that was true when the prompt was written.

A transition with inputs renders them as schema properties from their declared types, with `accepts` attributes and `input` declarations treated alike, since §5.1 of the syntax makes them the same thing.

## 4. The live offer

`availability(id)` is the second half, and it is what makes the first half honest.

```json
[
  {
    "transition": "complete_sale",
    "availability": "unavailable",
    "verdict": {
      "kind": "unsatisfied",
      "clause": "settled",
      "remedy": "dependent",
      "unknown": false,
      "object": "dlv_8f2a"
    },
    "inputs": {},
    "unevaluated": []
  },
  {
    "transition": "cancel",
    "availability": "available",
    "verdict": null,
    "inputs": {},
    "unevaluated": []
  },
  {
    "transition": "add_checklist_item",
    "availability": "available_with_input",
    "verdict": null,
    "inputs": {
      "label": "string"
    },
    "unevaluated": []
  }
]
```

Three-way availability, not two. `available_with_input` is the case a form cares about: the transition is permitted and the caller has not supplied everything yet, which is different from being refused.

`unevaluated` is what keeps this honest about external evaluators. `availability` never calls one (ADR-0048, ADR-0049), so a transition gated on an external fact reports the guards it could not evaluate, and a caller that treats the offer as a promise has been told not to.

`remedy` is what an agent should branch on rather than on the clause name: `delegable` means find someone with the authority, `temporal` means wait, `dependent` means change something else first, `self_serviceable` means fix the request, and `unreachable_from_here` means stop.

## 5. Form hints

The third projection, and the one ADR-0010 names as the motivating case: a form knows which fields to require because the transition's guards say so, and a button is disabled with the real reason rather than a guessed one.

It is the same two calls. `declaration(type)` gives the input schema, its types and its optionality, which is the form's shape. `availability(id)` gives the button's state and, when it is disabled, the sentence to put beside it — the failing clause's name and its remedy class, rendered as §2 renders them.

Nothing new is needed for this, which is the point worth making: if a third renderer had needed a new operation, the read surface would have been wrong.

## 6. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **Localisation: the renderer emits a structure and English is one rendering of it.** A guard sentence comes out as its clause name, its operator and its operands. A deployment substitutes its own without the renderer knowing any language, and a guard that changes changes its sentence in every language at once.
- **A rule-set diff is semantic, and it is not new work.** Publishing already compares the incoming declaration with the installed one to decide what needs a mapping, so the diff is a rendering of that comparison rather than a second mechanism that could disagree with it.

**Still open.**

- **Which tool-schema dialect.** The example is shaped for a tool-use API that takes JSON Schema, and there are several that differ in small ways. This is a question about the agent frameworks the first consumer actually uses.
- **How much of a rule set is worth rendering to a model.** The rule set is for people. Whether an agent is better served by the whole declaration or by tools alone is empirical, and the adversarial harness is where it can be answered.
