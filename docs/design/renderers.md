# Renderers

Draft, 2026-09-09, amended 2026-09-23. Three projections of one declaration: text a person reads, tool schemas an agent is given, and the field-level hints a form needs. They exist because the declaration is inspectable at runtime (ADR-0010), and that property is worth nothing until something renders it.

**Amended 2026-09-23** for ADR-0082 to ADR-0086 and ADR-0092: the rule set prints observations, metrics, the assignee and observing clauses; each observation kind and the metrics get tools; a tool's transition inputs are nested under `inputs`, so no input can collide with a request field; and the live offer's verdict names what its remedy points at.

**Amended again 2026-09-23** for ADR-0097 to ADR-0101, from a review of the whole record against the PRD; each change cites the decision it carries.

**Amended 2026-09-24** for ADR-0103: the rule set prints the module's request rule first, and an agent's tool schemas list the fields it requires.

**Amended 2026-09-24** for ADR-0105 and ADR-0106: the built-in types print with their authority — `OK_SUBSCRIBE`, a subscription's reader, and who may end a proposal — and a delivery prints as the record it is.

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

The constructs of ADR-0082 to ADR-0086 render in the same deterministic order, and each says what it is rather than leaving a reader to infer it:

```
ServiceJob — version 2
  tracking record
  assignee           engineer : User       responsible for the job; its history is tracked

  Observations
    inspections      InspectionResult      recorded by holders of PDI_RECORD;
                                           value in V; may be recorded up to 7 days late

  finish             WORKING → DONE
    requires
      mine           the engineer is the one acting           delegable
      none_failed    no inspection failed                     dependent
      photo_taken    a photo is attached        OBSERVING — NOT ENFORCED   self-serviceable
    then
      photo is the one supplied, if any

  Metrics
    time_working     median time in WORKING, by engineer, by month (UTC)
                     flag slow when over 10 days
    standard         time_in_state, time_in_state_p80, throughput,
                     work_in_progress, oldest_open,
                     transition_counts, refusals, refusal_rate, override_counts,
                     rework; for engineer: open_work, time_unassigned,
                     time_to_first_assignment, time_with_assignee,
                     cycle_time_by_assignee, time_in_state_by_holder, handoffs,
                     reassigned_back, acted_by_non_assignee, handoffs_by_object,
                     returns_by_object (declaration-syntax §6.11)
```

An **observing clause** is printed as not enforced, on the same line as the rule, so no reader can mistake a trial for a guarantee (ADR-0085). A **metric** is printed as its definition rendered in words, exactly as a guard is, because the rule set is the one place a definition is meant to be read as the rule; everywhere else points at `metric()` (§3). The **assignee** line says which reference is the responsibility, since that is what the assignment metrics are over (ADR-0086). The **standard metrics** are listed by name under every type, since each is a declaration a reader can look up (ADR-0098). The **built-in types** — `DeclarationChange`, `Proposal`, `Subscription` and the `label` kind — are rendered like any type, with the authority that gates each, so who may draft, approve, label, import or subscribe is on the page (ADR-0097, ADR-0105). `Subscription` prints `OK_SUBSCRIBE` and that only its reader may pull and acknowledge it; `Proposal` prints that its proposer may withdraw it and that whoever passes the proposed transition's actor guards may reject it. They print under declaration version 0 and every version after it, with the built-in module the version was published with.

Guard descriptions are rendered from the expression, not written by hand. `none(c in checklist_items where not c.checked)` becomes "every checklist item is checked". That rendering is mechanical and lossy on purpose — the expression is beside it in the machine-readable form, and a person reading a rule set wants the sentence.

**The module's request rule prints first**, before any type, since it binds every type in the closure (ADR-0103):

```
Requests by agent require an expected version and an idempotency key
  (refused as versioned or keyed, self_serviceable)
```

An agent's tool schemas follow it: where a module requires fields of agents' requests, each tool lists `expected_version` (for a tool naming an existing object) and `idempotency_key` among its `required` properties, so an agent reading the schema knows before it asks (PRD F3).

## 3. Tool schemas

An agent is given one tool per **requestable** transition. Only-via transitions are not tools; they are not requestable, and offering them would be offering a call that always returns `not requestable`.

```json
{
  "name": "delivery_complete_sale",
  "description": "Complete a delivery as a sale. Call availability(id) first: it evaluates the real guards against the real data, and its verdict says whether this transition is available now, which guard refuses it if not, and what can be done about it.",
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
      },
      "inputs": {
        "type": "object",
        "properties": {},
        "additionalProperties": false
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

A transition with inputs renders them as properties **of `inputs`**, from their declared types, with `accepts` attributes and `input` declarations treated alike, since §5.1 of the syntax makes them the same thing. Nesting them keeps the request's own fields — `object_id`, `expected_version`, `idempotency_key` — at the top level, so no input name can collide with one, now or when a request field is added (ADR-0092).

**Each observation kind is a tool** (ADR-0082), since recording one is a creation request. The subject, the fields and the observation a correction replaces are its inputs, as the syntax declares them; the occurred time is a request field beside the idempotency key, never an input, and a time outside the kind's bound is refused naming `occurred_within` (ADR-0096, ADR-0099):

```json
{
  "name": "record_inspection_result",
  "description": "Record one inspection result on a service job. Who may record it, and how late, are rules the store checks; availability(subject) offers recording beside the service job's own transitions, and says whether it is available now.",
  "input_schema": {
    "type": "object",
    "properties": {
      "idempotency_key": {
        "type": "string"
      },
      "occurred_at": {"type": "string", "format": "date-time", "description": "when it happened, if earlier than now; how far back is a rule the store checks"},
      "inputs": {
        "type": "object",
        "properties": {
          "subject": {"type": "string", "description": "the ServiceJob"},
          "check": {"type": "string", "description": "the InspectionCheck"},
          "outcome": {"type": "string", "enum": ["PASS", "FAIL", "NOT_APPLICABLE"]},
          "value": {"type": "number", "description": "in V"},
          "note": {"type": "string"},
          "corrects": {"type": "string", "description": "the InspectionResult on this service job that this one replaces"}
        },
        "required": ["subject", "check", "outcome"],
        "additionalProperties": false
      }
    },
    "required": ["inputs"],
    "additionalProperties": false
  }
}
```

**Metrics are one tool**, naming every metric the reader may see by name — the declared ones and the standard ones of the syntax's §6.11 — and nothing more (ADR-0084, ADR-0098). It takes the same arguments a guard's reference does: dimension values to bind, a window, and a filter. The description does not restate a definition, for the reason a transition's description does not restate a guard: `metric()` evaluates the real definition against the real data.

```json
{
  "name": "read_metric",
  "description": "Read a declared or standard metric, as rows of dimensions, value and the flags that hold, in UTC calendar time, computed over the rows you may see; complete says whether those are all the rows there are. The definitions are in declaration(type); this tool does not restate them.",
  "input_schema": {
    "type": "object",
    "properties": {
      "name": {"type": "string", "enum": ["time_working", "inspection_pass_rate", "ServiceJob.time_in_state", "ServiceJob.work_in_progress", "ServiceJob.open_work.engineer", "ServiceJob.cycle_time_by_assignee.engineer"]},
      "bind": {"type": "object", "description": "dimension values to fix"},
      "keep": {"type": "array", "items": {"type": "string"}, "description": "the dimensions to group by; all by default, and any left out is aggregated over"},
      "over": {"type": "string", "description": "a window back from now, such as 30 days"},
      "filter": {"type": "string"},
      "cursor": {"type": "string"}
    },
    "required": ["name"],
    "additionalProperties": false
  }
}
```

## 4. The live offer

`availability(id)` is the second half, and it is what makes the first half honest.

```json
[
  {
    "transition": "complete_sale",
    "creates": null,
    "availability": "unavailable",
    "verdict": {
      "kind": "unsatisfied",
      "clause": "settled",
      "remedy": "dependent",
      "unknown": false,
      "objects": ["chk_41c0", "chk_41c7"],
      "capability": null,
      "proposable": false,
      "consulted": {},
      "withheld": false
    },
    "inputs": {},
    "unevaluated": []
  },
  {
    "transition": "cancel",
    "creates": null,
    "availability": "available",
    "verdict": null,
    "inputs": {},
    "unevaluated": []
  },
  {
    "transition": "add_checklist_item",
    "creates": null,
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

`remedy` is what an agent should branch on rather than on the clause name: `delegable` means find someone with the authority, `temporal` means wait, `dependent` means change something else first, `self_serviceable` means fix the request, and `unreachable_from_here` means stop. The verdict also says **what the remedy points at** (ADR-0092): `objects` are the checklist items still unchecked, so an agent works on them rather than guessing; `capability` would name the authority a `delegable` clause asks for; `proposable` says whether filing a proposal would be accepted.

## 5. Form hints

The third projection, and the one ADR-0010 names as the motivating case: a form knows which fields to require because the transition's guards say so, and a button is disabled with the real reason rather than a guessed one.

It is the same two calls. `declaration(type)` gives the input schema, its types and its optionality, which is the form's shape. `availability(id)` gives the button's state and, when it is disabled, the sentence to put beside it — the failing clause's name and its remedy class, rendered as §2 renders them.

Nothing new is needed for this, which is the point worth making: if a third renderer had needed a new operation, the read surface would have been wrong.

## 6. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **Localisation: the renderer emits a structure and English is one rendering of it.** A guard sentence comes out as its clause name, its operator and its operands. A deployment substitutes its own without the renderer knowing any language, and a guard that changes changes its sentence in every language at once.
- **Tool inputs are nested under `inputs`** (ADR-0092), each observation kind is a tool, and metrics are one tool that names them without restating a definition.
- **An observing clause prints as not enforced**, on the line of the rule it trials (ADR-0085).
- **A rule-set diff is semantic, and it is not new work.** Publishing already compares the incoming declaration with the installed one to decide what needs a mapping, so the diff is a rendering of that comparison rather than a second mechanism that could disagree with it.

**Still open.**

- **Which tool-schema dialect.** The example is shaped for a tool-use API that takes JSON Schema, and there are several that differ in small ways. This is a question about the agent frameworks the first consumer actually uses.
- **How much of a rule set is worth rendering to a model.** The rule set is for people. Whether an agent is better served by the whole declaration or by tools alone is empirical, and the adversarial harness is where it can be answered.
