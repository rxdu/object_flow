# The flow description format

Status: **draft 2, 2026-09-26, under trial** (`authoring-flows.md` §3). Draft 2 adopts the established state-machine terms (ADR-0115, Appendix A). This document is the normative definition of the structured YAML form in which an ObjectFlow flow is described. Its machine-readable half is [`yaml-trial/flow.schema.json`](yaml-trial/flow.schema.json), and [`scripts/check-yaml-trial.py`](../../scripts/check-yaml-trial.py) implements every rule stated here; each rule names the step and the finding code that enforce it. Where this document and the implementation disagree, both are wrong until one is corrected, and `scripts/check-flow-format-doc.py` holds them to each other. The meaning of every form is the meaning of the text-language declaration it converts to, defined in [`declaration-syntax.md`](declaration-syntax.md); this document does not restate the engine's semantics.

## 1. Scope and conformance

A **flow description** is one YAML file declaring one **module**: its state categories, its enumerations and its types, and for each type its attributes, states, conditions, transitions and metrics. A description is **valid** when all four steps of §7 accept it with no fatal finding. Only a valid description can be proposed for publication.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119 and RFC 8174, when written in capitals.

Every word in a description is one of two kinds, and the examples in this document and in the walkthrough colour them apart:
- **Reserved**: a key or value this format defines, such as `transitions`, `kind`, `external`, `guards` or `deny`, and the words of the expression language. The full list is §10. A reserved word means the same thing in every description.
- **Named by the author**: a name the description itself declares, such as a module, a type, a state, an attribute, a condition, a transition, a metric, an enumeration or one of its values, or a category, and every later use of that name. Descriptions and expressions are the author's text.

## 2. The file

1. A file MUST be UTF-8 YAML holding exactly one document, which is one module. (step 1, `yaml`)
2. Only `true` and `false` are booleans. `yes`, `no`, `on` and `off` are ordinary words, so a value named `NO` stays a name. (step 1)
3. A mapping MUST NOT contain the same key twice. A YAML loader would otherwise keep the last one silently. (step 1, `yaml`)
4. Inside an inline list `[…]` or an inline mapping `{…}`, a value containing a comma, a question mark, a colon followed by a space, a bracket, a brace or `#` MUST be quoted, as YAML requires; for example `{ type: "decimal(10,3)" }`. Unquoted, such a value either fails to parse (step 1, `yaml`) or parses into the wrong structure (step 2, `schema`).
5. Comments begin with `#` and are ignored.
6. Nothing in a description has a default: every required key is written, including `tracking` and every condition's `remedy`. (step 2, `schema`)

## 3. Names

| What is named | Form | Example |
|---|---|---|
| module | lower case with underscores, starting with a letter | `inventory` |
| type, enumeration, observation kind | upper camel case | `InventoryItem`, `Condition` |
| state, enumeration value | upper case with underscores | `RESERVED`, `DAMAGED` |
| category, attribute, observation, invariant, condition, transition, metric, flag | lower case with underscores | `reserve`, `not_damaged` |

(step 2, `schema`)

1. A name MUST be unique among the names of its section. A repeated key is refused by step 1.
2. **Every name MUST be defined before it is used.** A module declares, in this order, `module`, `imports`, `categories`, `enumerations` and `types`; a type declares, in this order, `description`, `tracking`, `attributes`, `observations`, `states`, `invariants`, `conditions`, `transitions` and `metrics`. A type that another type references MUST be declared before it in the module, or imported. (step 3, `order`)
3. Reserved words of the text language MAY be used as names, as `declaration-syntax.md` §9.2 allows, with these exceptions (step 3, `names`):
   - a category MUST NOT be named `any`, `terminal` or `superseding`, and a transition MUST NOT be named `any` (the text language's check 33);
   - an attribute, including an observation's, MUST NOT be named `state`, `inputs`, `actor`, `this`, `now`, `referrers` or `this_event`, since an expression resolves those words before any attribute;
   - a condition MUST NOT be named `<attribute>_provided`, and an invariant MUST NOT be named `<state>_invariant` with the state in lower case, since those are the names of generated guards and invariants (§6).

## 4. Structure

### 4.1 The module

| Key | Required | Value |
|---|---|---|
| `module` | yes | the module's name |
| `imports` | no | a mapping from another module's name to the list of its types this module uses |
| `categories` | yes | the list of state categories the module's states use; `closed` has the meaning `declaration-syntax.md` §1 gives it |
| `enumerations` | no | a mapping from an enumeration's name to its list of values |
| `types` | yes | a mapping from a type's name to its declaration (§4.2) |

### 4.2 A type

| Key | Required | Value |
|---|---|---|
| `description` | yes | one or more sentences saying what an object of the type is |
| `tracking` | yes | `record`, `serial` or `quantity` (`declaration-syntax.md` §2) |
| `attributes` | no | §4.3 |
| `observations` | no | §4.4 |
| `states` | yes | §4.5 |
| `invariants` | no | §4.6 |
| `conditions` | no | §4.7 |
| `transitions` | yes | §4.8 |
| `metrics` | no | §4.9 |

### 4.3 Attributes

Each attribute is a mapping with exactly one of `type` and `reference`. (step 2, `schema`)

| Key | Value |
|---|---|
| `type` | a built-in type (`string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `duration`, `identity`, `file`) or an enumeration's name |
| `reference` | the name of a type declared earlier or imported |
| `optional` | `true` if the attribute may be without a value; otherwise it always has one |
| `unique` | `true` if no two objects of the type may hold the same value |
| `indexed`, `personal` | as `declaration-syntax.md` §3.1 defines them |
| `actor_kind` | `human`, `agent` or `service`, on the `identity` attribute of a type whose objects make requests |
| `assignee` | `true` on a reference that names who is responsible for the object |
| `unit` | the unit of a measured value |
| `description` | one sentence, where the name alone does not say what the attribute holds |

`unit` applies only to an observation's attribute, and `unique`, `indexed`, `actor_kind` and `assignee` only to a type's attribute, since the text language has no form for them elsewhere. (step 3, `names`)

### 4.4 Observations

An observation is a datapoint recorded about an object and never changed afterwards (`declaration-syntax.md` §6.8). Each has a `kind` (a type name), a `description`, its `attributes` (§4.3), and MAY have a `max_recording_delay`, a duration such as `7 days`, and `invariants` (§4.6). Conditions MAY read a type's observations by the observation's name.

### 4.5 States

| Key | Required | Value |
|---|---|---|
| `category` | yes | a category the module declares (step 3, `names`) |
| `description` | no | what it means for an object to be in the state |
| `final` | no | `true` if no transition leaves the state: a final state (UML `FinalState`) |
| `required_attributes` | no | attributes that have a value whenever an object is in the state; this is the state's invariant (UML `State::stateInvariant`) |

Every transition into a state MUST set each attribute the state requires: by requiring it as an input, by assigning it a value that is provably present, or by leaving it in place from source states that all require it. A value is provably present when it is `now`, a literal, a qualified enumeration value, a required input (`inputs.<attribute>`), or an attribute that every source state requires or that is not optional; any other expression is not counted, even when it would have a value. A transition MUST NOT clear an attribute its target requires. (step 3, `required`)

### 4.6 Invariants

An invariant is a condition that holds for every object of the type at all times. Each has a `description` and an `expression` (§5). A state's `required_attributes` generate invariants of their own (§6); `invariants` is for any other.

### 4.7 Conditions

A condition is a named test that transitions use as guards.

| Key | Required | Value |
|---|---|---|
| `description` | yes | the condition, stated as what holds when it is satisfied |
| `expression` | yes | §5 |
| `remedy` | yes | what a caller can do when it fails: `self_serviceable` (supply an input), `delegable` (another actor must act), `temporal` (only time will satisfy it), `dependent` (another object must change first) or `unreachable_from_here` (a different transition must come first); `DESIGN.md` §5.5 |

Every condition MUST be used by at least one transition. (step 3, `names`)

### 4.8 Transitions

A transition changes an object, and every change to an object is one. A transition's name is its **trigger** (UML `Transition::trigger`): a request names the transition it asks for. Each transition has a `kind`, one of UML's:

| `kind` | Means | `from` | `to` |
|---|---|---|---|
| `initial` | a new object comes into being in a state: the transition from UML's initial pseudostate | MUST be absent | the state |
| `external` | the object moves from a state to another (UML `TransitionKind::external`) | one state, or a list of states | the state it moves to |
| `internal` | the object changes without changing state (UML `TransitionKind::internal`) | the states it may be taken in | MUST be absent |

(step 2, `schema`; every state named MUST be declared, step 3, `names`)

| Key | Required | Value |
|---|---|---|
| `kind`, `from`, `to` | as above | |
| `description` | no | what the transition does, in one sentence |
| `required_inputs` | no | attributes the caller MUST supply; each is written to the attribute of the same name |
| `optional_inputs` | no | attributes the caller MAY supply; each is written if supplied. An attribute that is not optional is always required |
| `guards` | no | a mapping from a condition's name to its enforcement, in the order the guards are evaluated |
| `effect` | no | the list of writes the transition makes, in order (UML `Transition::effect`) |
| `backdating_limit` | no | how far in the past a caller may say the change happened, a duration such as `2 days` |

A guard's **enforcement** is one of the three validation actions of Kubernetes admission policies:
- `deny`: the transition is refused when the condition fails;
- `audit`: the condition is evaluated and its failures are recorded, and it refuses nothing (step 3 notice `audit`);
- `warn`: a failure is reported to the caller with the result, and the transition goes through (step 3 notice `warn`).

An **effect step** is one of:
- `assign: { location: <attribute>, expr: <expression> }`, which writes the value of the expression (§5) to the attribute, as SCXML's `<assign>` does;
- `clear: [<attribute>, …]`, which removes the values of the listed attributes, which MUST be optional (step 4, `check 17`).

A transition MUST NOT both write and clear one attribute. (step 3, `names`)

### 4.9 Metrics

A metric is a figure computed from the record of many objects (`declaration-syntax.md` §6.9).

| Key | Required | Value |
|---|---|---|
| `description` | yes | what the figure is |
| `measure` | yes | `median_time_in_state`, the median time objects spend in `state`; or `transition_count`, the number of times `transition` is taken |
| `state` or `transition` | as `measure` requires | a declared state or transition (step 2, `schema`; step 3, `names`) |
| `group_by` | no | dimensions: `month`, `week`, `actor` (the one who took the transition, only for `transition_count`), or an attribute of the type |
| `flag_when` | no | a mapping from a flag's name to an expression over `value`, such as `"value > 60 days"` |

A `transition_count` counts the transitions along its transition's source and target states, so it MUST NOT name a transition that shares a source and target with another. (step 3, `names`)

## 5. Expressions

An `expression` is written in the expression language of `declaration-syntax.md` §8, which this format does not change. Within it:
- an attribute of the object is read by its name, and a value the caller supplies is read as `inputs.<attribute>`;
- `state` is the object's current state, and `now` is the time of the request;
- an enumeration value and a state of another type are written qualified, `<Enumeration>.<VALUE>` and `<Type>.<STATE>`; a value of an enumeration or of a type the module declares MUST exist, and a bare upper-case name MUST be a state of the type (step 3, `names`);
- absence is tested with `is null` and `is not null`; a comparison with an absent value is unknown, and a guard that is unknown refuses (`declaration-syntax.md` §8.2);
- an expression MUST NOT read `actor`: who may act is decided outside the flow (ADR-0114; step 4, `check 64`).

## 6. What a description declares

Each form converts to the text language as follows, and means what that declaration means.

| Description | Text language |
|---|---|
| `kind: initial`, `to: S` | `create <name> -> S` |
| `kind: external`, `from: [A, B]`, `to: C` | `do <name> { A, B } -> C` |
| `kind: internal`, `from: [A, B]` | `act <name> at { A, B }` |
| `required_inputs: [a]`, `optional_inputs: [b]` | `accepts a, b` |
| `required_inputs: [a]` where `a` is optional | also the guard `require a_provided: inputs.a is not null because self_serviceable`, evaluated before the transition's other guards |
| `guards: { g: deny }` | `require g: <expression> because <remedy>` |
| `guards: { g: audit }`, `{ g: warn }` | the same, marked `observe` or `flag` |
| `assign: { location: b, expr: e }` | `set b := e` |
| `clear: [a]` | `clear a` |
| `final: true` on state `S` | `state S category … terminal` |
| `required_attributes: [a, b]` on state `S` | `invariant s_invariant: state != S or (a is not null and b is not null)` |
| `backdating_limit: 2 days` | `backdatable within 2 days` |
| `measure: median_time_in_state` | a metric over the type's intervals in the state, valued `median(i.duration)` |
| `measure: transition_count` | a metric over the type's transitions along the transition's states, valued `count()` |

The generated guards and invariants take the names shown, which §3 reserves. The text language keeps its own keywords for these forms (`create`, `do`, `act`, `terminal`, `observe`, `flag`) until the author decides whether this format replaces it (ADR-0115).

## 7. Validity

A description is checked in four steps, and each stops the check if it finds anything fatal. Every finding names the file, the line and the construct.

| Step | Code | Refuses |
|---|---|---|
| 1. strict loading | `yaml` | a file YAML cannot parse, including a `?` unquoted inside an inline collection, or a key repeated in a mapping |
| 2. structure | `schema` | a missing or unknown key, a value of the wrong form, a name in the wrong case, a transition kind without the `from` and `to` it requires, a metric without the `state` or `transition` its measure requires |
| 3. names and order | `order` | sections out of order, a type used before it is declared |
| | `names` | a name that is not declared (a state, condition, attribute, category, transition or value), a reserved name (§3), an unused condition, a transition that both writes and clears an attribute, an optional input on an attribute that is not optional, an attribute key the attribute's kind has no form for (§4.3), a `transition_count` that cannot be told apart |
| | `required` | a transition into a state that does not set, or that clears, an attribute the state requires |
| 4. publish checks | `check N` | anything the text language's implemented publish checks refuse, reported at the line of the description it came from |

Step 3 also reports two notices, which are not fatal: `audit`, for each guard that audits, and `warn`, for each guard that warns.

## 8. A minimal valid description

```yaml
module: documents
categories: [live, closed]

types:
  Document:
    description: A document that is drafted and then published.
    tracking: record

    attributes:
      title:        { type: string }
      published_at: { type: timestamp, optional: true }

    states:
      DRAFT:
        description: Being written.
        category: live
      PUBLISHED:
        description: Released to its readers.
        category: closed
        final: true
        required_attributes: [published_at]

    transitions:
      start:
        kind: initial
        to: DRAFT
        required_inputs: [title]
      publish:
        kind: external
        from: DRAFT
        to: PUBLISHED
        effect:
          - assign: { location: published_at, expr: now }
```

A full description is [`yaml-trial/inventory.yaml`](yaml-trial/inventory.yaml); [`yaml-trial/service.yaml`](yaml-trial/service.yaml) adds an import, an assignee and observations.

## 9. Open questions

These are open in `authoring-flows.md` §3 and §7, and an answer would change this document:
1. Whether this form replaces the text language as the written form of every flow.
2. Whether version numbers are computed at publication, which this format assumes by having none.
3. Whether the generated guards and invariants should carry written descriptions.
4. Whether a transition whose source state does not require an attribute its target requires should be refused, as now, even when the attribute happens to be kept.
5. How two types that reference each other are ordered, since every name is defined before it is used.

## 10. Reserved vocabulary

These words are reserved by the format. `scripts/check-flow-format-doc.py` holds this list equal to the keys and values `flow.schema.json` and the checker define.

**Keys:** `module`, `imports`, `categories`, `enumerations`, `types`, `description`, `tracking`, `attributes`, `observations`, `states`, `invariants`, `conditions`, `transitions`, `metrics`, `category`, `final`, `required_attributes`, `type`, `reference`, `optional`, `unique`, `indexed`, `personal`, `actor_kind`, `assignee`, `unit`, `kind`, `max_recording_delay`, `expression`, `remedy`, `from`, `to`, `required_inputs`, `optional_inputs`, `guards`, `effect`, `assign`, `location`, `expr`, `clear`, `backdating_limit`, `measure`, `state`, `transition`, `group_by`, `flag_when`.

**Values:** `true`, `false`, `record`, `serial`, `quantity`, `human`, `agent`, `service`, `initial`, `external`, `internal`, `deny`, `audit`, `warn`, `self_serviceable`, `delegable`, `temporal`, `dependent`, `unreachable_from_here`, `median_time_in_state`, `transition_count`, `month`, `week`, `actor`.

**In expressions:** the reserved words of the text language, `declaration-syntax.md` §9.5.

## Appendix A. Terminology against established conventions

A review of 2026-09-26, at the author's request that this format use existing conventions where they exist rather than invent terms. Every source below was read in its primary form: UML 2.5.1 (OMG formal/2017-12-05, https://www.omg.org/spec/UML/2.5.1/PDF), W3C SCXML (Recommendation, 1 September 2015, https://www.w3.org/TR/scxml/), XState v5 (https://stately.ai/docs), the Symfony Workflow component (https://symfony.com/doc/current/workflow.html), AASM (https://github.com/aasm/aasm), django-fsm 2.8.2 (https://github.com/viewflow/django-fsm/blob/2.8.2/README.rst), Kubernetes ValidatingAdmissionPolicy (https://kubernetes.io/docs/reference/access-authn-authz/validating-admission-policy/), OPA Gatekeeper (https://open-policy-agent.github.io/gatekeeper/website/docs/violations/) and Atlassian Jira (https://support.atlassian.com/jira-cloud-administration/docs/what-is-a-workflow-status/). The author adopted the established terms the same day (ADR-0115); §A.3 records what changed.

### A.1 The terms

| This format | Established term | Source | Assessment |
|---|---|---|---|
| `states`, `transitions` | state, transition | UML §14; SCXML §3.3, §3.5; XState | standard |
| `from`, `to` | `source`, `target` | UML §14.5.11.6; SCXML `target` | both are established: UML's model says source and target, the configuration languages of Symfony and AASM say `from` and `to` |
| `guards` | guard | UML `Transition::guard`; XState `guard`; AASM `guard:`; Symfony `guard` | standard |
| `conditions` | `cond`; conditions; named guards | SCXML `<transition cond>`; django-fsm 2.x `conditions=[…]`; XState `setup({ guards })` | established |
| a transition's name, which a request names | trigger or event | UML `Transition::trigger`; AASM `event`; Symfony applies a transition by name | standard in substance; §4.8 says so |
| `kind: action` (draft 1) | internal transition; targetless transition | UML `TransitionKind::internal`, §14.5.12.3: it "occurs without exiting or entering the source State (i.e., it does not cause a state change)"; SCXML Appendix D; XState "targetless self-transition" | **conflicted**: in UML, SCXML and XState an action is behaviour a transition executes, not a kind of transition; now `internal` |
| `kind: state_change` (draft 1) | external transition | UML `TransitionKind::external`, the default | invented where UML has a term; now `external` |
| `kind: creation` (draft 1) | initial transition, from the initial pseudostate | UML §14.2.4.6; SCXML `<initial>`; XState `initial` | invented; now `initial`, although an initial transition here is named, takes inputs and may be one of several |
| `terminal` (draft 1) | final state | UML `FinalState`; SCXML `<final>`; XState `type: 'final'` | the design's own word; now `final` |
| `required_attributes` | state invariant | UML `State::stateInvariant`, §14.5.9: "conditions that are always true when this State is the current State" | a special case of a standard concept, which §4.5 now names; the generated invariant is `<state>_invariant` |
| `invariants` | invariant | UML and OCL class invariant | standard |
| `outcome` (draft 1) | effect; actions | UML `Transition::effect`, §14.5.11.6; XState `actions`; SCXML executable content | ADR-0046's term; now `effect` |
| `copy` (draft 1) | assign | SCXML `<assign location expr>`, §5.4; XState `assign` | invented; now `assign`, taking an expression |
| `clear` | none | | no standard term; an assignment of no value |
| `enforced`, `observed`, `flagged` (draft 1) | `Deny`, `Audit`, `Warn`; `deny`, `dryrun`, `warn` | Kubernetes `ValidatingAdmissionPolicyBinding.validationActions`; OPA Gatekeeper `enforcementAction` | no state-machine standard has these; Kubernetes' three match the three meanings, Deny refusing, Audit recording the failure, Warn reporting it to the client; now `deny`, `audit`, `warn` |
| `category` | status category | Jira: statuses "must belong to one of three status categories – To do, In progress, or Done" | established outside the state-machine standards |
| `required_inputs`, `optional_inputs` | parameters; event data | UML operation parameters; SCXML `_event.data` | the design's `inputs`, which expressions read as `inputs.<attribute>`; consistent with the expression language |

**The model as a whole** is closest to a UML protocol state machine, which "specifies which BehavioralFeatures of that Classifier can be invoked in a given protocol state and under what conditions" (§14.4.3.1). It departs in one respect: a protocol transition has no effect (§14.4.3.2), and a transition here writes attributes. It is therefore a behavioural state machine whose triggers are named requests.

### A.2 The diagram

UML's notation (§14.2.4) against the walkthrough's diagram as it was drawn for draft 1; §A.3 says what the diagram now does:
- the initial pseudostate is "a small solid filled circle" (§14.2.4.6), as drawn;
- a final state is "a circle surrounding a small solid filled circle" (§14.2.4.5); the double border drawn on `RETIRED` is the automata-theory convention for an accepting state, "denoted graphically by a double circle" (Wikipedia, Deterministic finite automaton);
- an internal transition is listed in the state's internal-transitions compartment (§14.2.4.4) and is "not shown explicitly" as an arrow (§14.2.4.9); a loop drawn outside a state is an external self-transition, which exits and re-enters the state, so the loops drawn for `regrade` read as the wrong kind;
- a transition's label is `trigger [guard] / behavior-expression`, every part optional (§14.2.4.8), so a label of the name alone conforms;
- a frame drawn around states is, in UML, a composite state, which implies a hierarchy this format does not have; the `DELIVERED` frame is also drawn from nothing the description declares, which departs from the rule that the diagram maps onto the description.

### A.3 Adopted

On 2026-09-26 the author answered "adopt standards and update our specs" (ADR-0115):
1. The transition kinds are UML's: `initial`, `external` and `internal`. ADR-0016 carries a note that its "self-transition" is UML's internal transition, and `DESIGN.md` says internal transition where it said action.
2. The walkthrough's diagram lists internal transitions inside the states they occur in, draws a final state as UML does, and replaces the frame around the delivered states with a UML comment anchored to them.
3. `terminal` is `final`, `outcome` is `effect`, and `copy` is `assign`, which takes an expression.
4. The guard enforcements are Kubernetes' `deny`, `audit` and `warn`.
5. §4.5 names `required_attributes` as the state's invariant, generated as `<state>_invariant`, and §4.8 states that a transition's name is its trigger.
6. The delivered grouping is drawn as a comment, which declares nothing, rather than as a frame, which in UML declares a composite state.

The text language keeps its own keywords until the author decides whether this format replaces it.
