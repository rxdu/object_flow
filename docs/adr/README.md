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
| [0012](0012-objectkeeper-does-not-initiate-transitions.md) | ObjectKeeper does not initiate transitions | Accepted |
| [0013](0013-events-are-recorded-to-a-durable-log-in-the-transition-transaction.md) | Events are written to a durable ordered log in the transition's own transaction | Accepted |
| [0014](0014-delivery-is-at-least-once-with-idempotency-keys.md) | Delivery is at-least-once; exactly-once effect comes from idempotency keys | Accepted |
| [0015](0015-first-consumer-and-fresh-build-with-ported-data.md) | The first consumer is the Weston Robot operations platform, rebuilt with its production data ported | Accepted |
| [0016](0016-actions-are-self-transitions.md) | Actions are self-transitions | Accepted |
| [0017](0017-file-attachments-are-content-addressed-references.md) | File attachments are content-addressed references; the bytes are out of scope | Proposed |
| [0018](0018-every-object-carries-a-store-assigned-globally-unique-identifier.md) | Every object carries a store-assigned, globally unique identifier | Accepted |
| [0019](0019-outcomes-cascade-across-relationships-atomically.md) | A transition's outcome may cascade transitions and creations across relationships, atomically | Accepted (iteration 1, review pending) |
| [0020](0020-a-transition-may-be-reachable-only-via-named-parents.md) | A transition may be reachable only via named parent transitions | Accepted (iteration 1, review pending) |
| [0021](0021-derived-attributes-and-the-expression-language.md) | Derived attributes are never stored; the expression language is small and grows only by decision | Accepted (iteration 1, review pending) |
| [0022](0022-time-is-a-guard-value-and-availability-is-queryable.md) | Time is a guard value; the read surface answers which objects have a transition available | Accepted (iteration 1, review pending) |
| [0023](0023-transitions-execute-under-locks-and-may-carry-an-expected-version.md) | Transitions execute under locks; a request may carry an expected version | Accepted (iteration 1, review pending) |
| [0024](0024-deletion-is-a-terminal-transition-gated-on-live-references.md) | Deletion is a terminal transition gated on live references; parts cascade, references block | Accepted (iteration 1, review pending) |
| [0025](0025-the-actor-is-a-value-supplied-by-the-consumer.md) | The actor is a value supplied by the consumer; ObjectKeeper does not authenticate | Accepted (iteration 1, review pending) |
| [0026](0026-declarations-compose-by-extends-and-bind-a-named-state-machine.md) | Declarations compose by `extends` and bind a named state machine; states carry a category | Accepted (iteration 2, review pending) |
| [0027](0027-declarations-are-versioned-and-removals-require-a-mapping.md) | Declarations are versioned; removals require a mapping applied as recorded migrations | Accepted (iteration 2, review pending) |
| [0028](0028-supersession-an-object-may-end-by-naming-a-successor.md) | Supersession: an object may end in a terminal state that names its successor | Accepted (iteration 2, review pending) |
| [0029](0029-named-sequences-mint-business-identifiers.md) | Named, scoped sequences mint business identifiers at creation | Accepted (iteration 2, review pending) |
| [0030](0030-read-visibility-is-a-declared-predicate.md) | Read visibility is a declared predicate over actor and object; an invisible object is not found | Accepted (iteration 3, review pending) |
| [0031](0031-erasure-redacts-declared-personal-attributes-across-history.md) | Erasure redacts declared personal attributes across history; recorded, irreversible, not deletion | Accepted (iteration 3, review pending) |
| [0032](0032-expression-language-version-2-adds-arithmetic.md) | The expression language gains arithmetic, durations and sum/min/max; domain formulas stay outside | Accepted (iteration 4, review pending) |
| [0033](0033-current-state-is-stored-and-the-log-is-permanent-history.md) | Current state is stored; the log is permanent history, never pruned; provenance attaches to events | Accepted (iteration 4, review pending) |
| [0034](0034-event-ordering-and-subscriptions-as-a-built-in-type.md) | Events are ordered per object and per cause; subscriptions are a built-in type with a filter and lifecycle | Accepted (iteration 4, review pending) |
| [0035](0035-approval-is-a-guard-over-recorded-approval-parts.md) | An approval is a recorded part; "needs approval" is a guard over those parts, invalidated by content change | Accepted (iteration 5, review pending) |
| [0036](0036-proposals-are-a-built-in-type-and-delegation-is-supplied.md) | Proposals are a built-in, opt-in type; delegation and attenuation arrive in the actor descriptor | Accepted (iteration 5, review pending) |
