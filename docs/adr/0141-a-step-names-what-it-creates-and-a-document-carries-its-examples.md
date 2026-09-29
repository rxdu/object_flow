# ADR-0141: A step names the objects its request creates, and a design document carries its modules' examples

- **Status:** Accepted by the author in advance, 2026-09-29: "for all similar items that you have confidence in an recommendation, just accept", the standing direction under which ADR-0139 and ADR-0140 were accepted.
- **Date:** 2026-09-29
- **Refines:** ADR-0136 (examples, built by requests from an empty store), ADR-0140 decision 3 (the design's own examples all run), ADR-0121 (the design's YAML is checked where it stands)
- **Relates to:** `flow-format.md` §11.3; `scripts/check-flow-docs.py`; `jira-flows.md`; `flow-format/examples/delivery.examples.yaml`

## Context

Writing examples for the Jira catalogue's flows, which the reference runner was not written against, found two things the examples could not do.

1. **A later step could not name an object an effect created.** A step's `as` names the object its request creates, and an effect's creations have no name. A part created only through its whole could therefore never act afterwards: a task's subtask, which only `Task.add_subtask` creates, could not be done, so an example could not show a task done once its subtasks are. The delivery's examples had already worked around this. Every one that completes a delivery opens it with an empty checklist, since no step could tick an item. That also hid how the checklist and the sign-off interact: ticking an item is a change to a part of the delivery, and `signed_off` reads `changed_since([price, checklist_items], …)`. So a sign-off given before the ticking goes stale.
2. **A module written in a design document had nowhere to carry its examples.** An examples file sits beside its description (ADR-0136 decision 1). The catalogue's modules are blocks in `jira-flows.md`, checked where they stand (ADR-0121).

## Decision

1. **A step's request may name, as `creates`, the objects its request creates other than its own**, in the order it creates them, depth-first as the effect runs. They are of one type. Step 3 gives each alias the one type the transition's effect creates, through its loops and the creations it makes, and refuses `creates` on a transition whose effect creates no type or more than one. A step naming more objects than its request creates fails the example. The request an example tests names none, since no step follows it.
2. **A design document carries a module's examples in a yaml block beginning `examples_for: <module>`**, after the module in the same document. The block is the examples of the last version of that module begun before it. `scripts/check-flow-docs.py` checks and runs it with the module. An example there that the runner does not run is a finding, as ADR-0140 decision 3 has it for the format's corpus, since these too are the design's own examples.

## Alternatives rejected

- **Finding a created object by a query**, such as its type and a value. It needs a filter syntax the examples do not otherwise have, and two objects that match are ambiguous.
- **Letting a setup request a part's `only_via` creation directly.** A setup would then build a state no caller can reach, which ADR-0136 decision 2 exists to prevent.
- **Naming a created object by the effect's `result`.** Only a creation that declares a result could be named, and not one made in a loop.
- **Writing each created object's type beside its alias.** The effect already declares the type, and the writer would repeat it.
- **Examples of a document's modules in files of their own.** The catalogue's reader would no longer see what a flow does beside the flow, and ADR-0121 checks the design's YAML where it stands.

## Consequences

- `flow-format.md` §11.3 and §10, and `examples.schema.json`, gain `creates`. The flow checker types it at step 3, and the reference runner records each creation and names it.
- The delivery's examples name its checklist items. They show a completion with the checklist ticked, a completion held by an open item, a sign-off made stale by ticking after it, and a sign-off given again.
- `jira-flows.md` carries examples for its eight flows with rules: sprints, versions, the web development workflow, task and process management, and three of the service flows. Its flows without guards or effects carry none, since beyond the arrows their diagrams already show there is nothing for an example to prove.
