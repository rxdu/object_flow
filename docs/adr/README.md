# Architecture Decision Records

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-store-owns-persistence.md) | The store owns persistence and is the sole write path | Accepted |
| [0002](0002-requiredness-attaches-to-transitions.md) | Requiredness attaches to transitions, not to attributes | Accepted |
| [0003](0003-one-state-machine-per-object-type.md) | One state machine per object type; composition rather than parallel regions | Accepted |
| [0004](0004-composite-state-is-gated-not-derived.md) | Composite state is gated by its parts, not derived from them | Accepted |
| [0005](0005-guards-evaluate-over-state-plus-inputs.md) | Guards evaluate over current state plus transition inputs | Accepted · partly superseded by ADR-0047 |
| [0006](0006-controlled-and-free-attributes.md) | Attributes are classified controlled or free | **Superseded by ADR-0042** |
| [0007](0007-decide-and-record-not-compute-or-effect.md) | The store decides and records; it does not compute or cause effects | Accepted |
| [0008](0008-guard-escape-hatch-is-a-named-external-evaluator.md) | The guard escape hatch is a named external evaluator | Accepted (author-confirmed) · refined by ADR-0049 |
| [0009](0009-invariants-declared-at-type-level.md) | Invariants are declared at type level, enforced at transitions | Accepted · partly superseded by ADR-0045 |
| [0010](0010-the-declaration-is-inspectable-at-runtime.md) | The declaration is data, inspectable at runtime | Accepted |
| [0011](0011-project-name-objectkeeper.md) | The project is named ObjectKeeper | Accepted |
| [0012](0012-objectkeeper-does-not-initiate-transitions.md) | ObjectKeeper does not initiate transitions | Accepted |
| [0013](0013-events-are-recorded-to-a-durable-log-in-the-transition-transaction.md) | Events are written to a durable ordered log in the transition's own transaction | Accepted |
| [0014](0014-delivery-is-at-least-once-with-idempotency-keys.md) | Delivery is at-least-once; exactly-once effect comes from idempotency keys | Accepted |
| [0015](0015-first-consumer-and-fresh-build-with-ported-data.md) | The first consumer is the Weston Robot operations platform, rebuilt with its production data ported | Accepted |
| [0016](0016-actions-are-self-transitions.md) | Actions are self-transitions | Accepted |
| [0017](0017-file-attachments-are-content-addressed-references.md) | File attachments are content-addressed references; the bytes are out of scope | Accepted (author-confirmed) |
| [0018](0018-every-object-carries-a-store-assigned-globally-unique-identifier.md) | Every object carries a store-assigned, globally unique identifier | Accepted |
| [0019](0019-outcomes-cascade-across-relationships-atomically.md) | A transition's outcome may cascade transitions and creations across relationships, atomically | Accepted (iteration 1, review pending) · partly superseded by ADR-0038 and ADR-0046 |
| [0020](0020-a-transition-may-be-reachable-only-via-named-parents.md) | A transition may be reachable only via named parent transitions | Accepted (iteration 1, review pending) |
| [0021](0021-derived-attributes-and-the-expression-language.md) | Derived attributes are never stored; the expression language is small and grows only by decision | Accepted (iteration 1, review pending) · refined by ADR-0032, ADR-0047, ADR-0053 |
| [0022](0022-time-is-a-guard-value-and-availability-is-queryable.md) | Time is a guard value; the read surface answers which objects have a transition available | Accepted (iteration 1, review pending) · refined by ADR-0048 |
| [0023](0023-transitions-execute-under-locks-and-may-carry-an-expected-version.md) | One transaction per request, versions, and the `stale` verdict; its locking rules superseded by ADR-0039 | Accepted (iteration 1, review pending) |
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
| [0034](0034-event-ordering-and-subscriptions-as-a-built-in-type.md) | Events are ordered per object and per cause; subscriptions are a built-in type (their shape superseded by ADR-0043) | Accepted (iteration 4, review pending) |
| [0035](0035-approval-is-a-guard-over-recorded-approval-parts.md) | An approval is a recorded part; "needs approval" is a guard over those parts, invalidated by content change | Accepted (iteration 5, review pending) |
| [0036](0036-proposals-are-a-built-in-type-and-delegation-is-supplied.md) | Proposals are a built-in, opt-in type; delegation and attenuation arrive in the actor descriptor | Accepted (iteration 5, review pending) · refined by ADR-0044 |
| [0037](0037-the-read-surface.md) | The read surface: get, query, lookup, availability, check, history, declaration, pull, batch; declared indexes and external identifiers; no hidden projections | Accepted (iteration 6, review pending) · refined by ADR-0048, ADR-0054 |
| [0038](0038-cascades-apply-sequentially.md) | Cascaded transitions apply sequentially; each sees the writes of those before it | Accepted (repair D01, review pending) |
| [0039](0039-transitions-run-at-serialisable-isolation.md) | The transition transaction runs at serialisable isolation | Accepted (repair D02/D04, review pending) |
| [0040](0040-assertion-is-a-declared-capability-gated-transition.md) | The override is an assertion: declared, capability-gated, recorded, and never silent | Accepted (repair D03, review pending) |
| [0041](0041-resolutions-of-six-review-contradictions.md) | Resolutions of six contradictions: only-via listing, idempotent replay, action outcomes, file bytes, constraint compilation, fan-out verdict | Accepted (repair, review pending) |
| [0042](0042-all-attributes-are-controlled-editing-is-an-action.md) | There are no free attributes; every write is a transition and editing is an action | Accepted (repair D09, review pending) |
| [0043](0043-subscription-progress-is-runtime-state-and-lag-is-derived.md) | Subscription progress is runtime state; lag and death are derived, so nothing initiates | Accepted (repair D07, review pending) |
| [0044](0044-proposals-execute-under-the-current-declaration-and-can-be-invalidated.md) | A proposal executes under the current declaration and is invalidated when that becomes impossible | Accepted (repair D08, review pending) |
| [0045](0045-invariant-enforcement-is-dynamic-and-invariants-traverse-declared-inverses.md) | Invariant enforcement is dynamic; invariants may only traverse declared inverses | Accepted (repair D10, review pending) |
| [0046](0046-the-outcome-grammar.md) | The outcome grammar: cascade inputs, named creations, bound iteration, repeat, `this_event`, part re-parenting | Accepted (repair D11-D15/D18/D29, review pending) |
| [0047](0047-expression-semantics.md) | Expression semantics: three-valued logic, explicit binding, aggregates over types, declared inputs and remedy classes, acyclic derivation | Accepted (repair D16-D24, review pending) |
| [0048](0048-the-read-path-must-be-answerable.md) | Sweepable guards, a per-attribute write index, and time-dependent predicates rewritten to their operands | Accepted (repair D25/D26/D30, review pending) |
| [0049](0049-external-evaluators-run-outside-the-transaction.md) | External evaluators run outside the write transaction; their verdict carries an as-of time | Accepted (repair D32, review pending) |
| [0050](0050-a-type-declares-its-tracking-mode.md) | A type declares whether it is serial-tracked or quantity-tracked | Accepted (repair D33, review pending) |
| [0051](0051-erasure-integrity.md) | Personal values may not be copied into non-personal attributes; the redaction marker is absence | Accepted (repair D35, review pending) |
| [0052](0052-grammar-amendments-from-re-expressing-the-case-studies.md) | Grammar amendments found by re-expressing the case studies | Accepted (repair D37-D41, review pending) |
| [0053](0053-a-definite-null-test.md) | `is null` and `is not null` are definite predicates; comparison with null stays unknown | Accepted (repair D42, review pending) |
| [0054](0054-four-semantics-the-repair-left-open.md) | Parent ordering, partial `check`, built-in assertion for import, and discharged admissions | Accepted (repair D43-D46, review pending) |
| [0055](0055-local-invariants-and-set-writes.md) | A third invariant form for single-object properties, and outcome steps that add to and remove from a set | Accepted (repair D48-D49, review pending) |
| [0056](0056-model-amendments-found-by-writing-the-syntax.md) | Ten model amendments found by writing the declaration syntax | Accepted (repair D50-D59, review pending) |
| [0057](0057-changed-since-reaches-parts.md) | `changed_since` may name a part relationship, so an approval is invalidated by an edit to a part | Accepted (repair D60, review pending) |
| [0058](0058-composition-lifetime-and-re-parenting.md) | Every terminal transition disposes of its parts; re-parenting is checked against both wholes | Accepted (repair D61-D62, review pending) |
| [0059](0059-which-end-of-a-relationship-stores-the-value.md) | Both ends of a relationship are declared; cardinality decides which one stores the value | Accepted (repair of the iteration-9 syntax review, review pending) |
| [0060](0060-decisions-from-the-iteration-11-syntax-review.md) | Twelve decisions from the iteration-11 syntax review | Accepted (repair D63-D96, review pending) |
| [0061](0061-decisions-from-the-payments-ledger-review.md) | Ten decisions from the payments-ledger review | Accepted (repair D114-D133, review pending) |
| [0062](0062-a-normative-statement-must-cite-its-check.md) | A normative statement must cite the check that enforces it | Accepted (repair D134-D148, review pending) |
| [0063](0063-the-swap-test-is-a-decision-procedure.md) | The swap test names its normalisations, and check 52's claim is retracted | Accepted (repair D149-D158, review pending) |
