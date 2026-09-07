# Architecture Decision Records

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-store-owns-persistence.md) | The store owns persistence and is the sole write path | Accepted |
| [0002](0002-requiredness-attaches-to-transitions.md) | Requiredness attaches to transitions, not to attributes | Accepted |
| [0003](0003-one-state-machine-per-object-type.md) | One state machine per object type; composition rather than parallel regions | Accepted |
| [0004](0004-composite-state-is-gated-not-derived.md) | Composite state is gated by its parts, not derived from them | Accepted |
| [0005](0005-guards-evaluate-over-state-plus-inputs.md) | Guards evaluate over current state plus transition inputs | Accepted |
| [0006](0006-controlled-and-free-attributes.md) | Attributes are classified controlled or free | Proposed |
| [0007](0007-decide-and-record-not-compute-or-effect.md) | The store decides and records; it does not compute or cause effects | Accepted |
| [0008](0008-guard-escape-hatch-is-a-named-external-evaluator.md) | The guard escape hatch is a named external evaluator | Proposed |
| [0009](0009-invariants-declared-at-type-level.md) | Invariants are declared at type level, enforced at transitions | Accepted |
| [0010](0010-the-declaration-is-inspectable-at-runtime.md) | The declaration is data, inspectable at runtime | Accepted |
| [0011](0011-project-name-objectkeeper.md) | The project is named ObjectKeeper | Accepted |
