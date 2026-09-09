# Defect register

Findings from every review of this design. It began as the implementation-readiness review of 2026-09-08 and has since absorbed nine further rounds — re-expression, a coherence pass, sixteen review rounds on the declaration syntax, and the corpus audit that follows the author's rulings. Each provenance section below says which round found its entries. Every entry was verified against the file cited; the review ran as four independent passes (one direct, three delegated) and only findings confirmed by reading the source are recorded here.

**A note on citations.** Every `file:line` below was correct when the defect was recorded. The documents have since been repaired and re-expressed, so line numbers have drifted; the quoted text is the reliable locator.

**Two kinds of closure.** *Resolved by* means the design now does the thing. *Refused by* means the design decided not to, and the case is recorded in `edge-cases.md` instead. D15 and D17 are refusals.

**How to read this.** `D37`–`D41` were found on 2026-09-08 by re-expressing the case studies against the repaired grammar, which is the test the repair called for. `D01`–`D10` break the model or a running system and must be resolved before a runtime is built. `D11`–`D26` are things the design cannot express or has no algorithm for. `D27`–`D36` are contradictions and scope errors. `C01`–`C05` are cosmetic. A closure is `Resolved`, `Refused by decision` with the case recorded in `edge-cases.md`, or `Recorded as open question N` for a finding the author later ruled on. Twelve are open — D189 to D200, from the whole-record review of 2026-09-09 — and each says what would close it. The index above is generated from the entries, so it cannot fall behind them again.

**Provenance.** ADR-0019 to ADR-0037 and the five case studies were produced in the autonomous design iterations of 2026-09-07/08. Defect density is highest there, and the case-study notation problem (`D11`–`D18`) originates entirely in that work.

| ID | Defect | Closure |
|---|---|---|
| [D01](#d01) | Cascaded transitions cannot see each other's writes | Resolved by ADR-0038 |
| [D02](#d02) | Reads outside the lock set are unisolated | Resolved by ADR-0039 |
| [D03](#d03) | The guarantee is false as written, and the override path is undeclared | Resolved by ADR-0040 |
| [D04](#d04) | The lock set cannot be computed when the spec says | Resolved by ADR-0039 |
| [D05](#d05) | Availability both does and does not list only-via transitions | Resolved by ADR-0041 |
| [D06](#d06) | Idempotent replay is specified as both refuse and replay | Resolved by ADR-0041 |
| [D07](#d07) | The built-in Subscription cannot be operated under its own rules | Resolved by ADR-0043 |
| [D08](#d08) | Proposals across declaration versions are undefined | Resolved by ADR-0044 |
| [D09](#d09) | Free attributes have no write path | Resolved by ADR-0042 |
| [D10](#d10) | Two different invariant-enforcement specifications | Resolved by ADR-0045 |
| [D11](#d11) | Cascaded transitions cannot take inputs | Resolved by ADR-0046 |
| [D12](#d12) | An outcome cannot name an object it creates | Resolved by ADR-0046 |
| [D13](#d13) | `this_event` is undefined and chronologically impossible | Resolved by ADR-0046 |
| [D14](#d14) | Repeat-N creation is not expressible | Resolved by ADR-0046 |
| [D15](#d15) | Map-typed inputs and dynamic writes are not expressible | Resolved by ADR-0046 |
| [D16](#d16) | `sum` over a type is required but not granted | Resolved by ADR-0047 |
| [D17](#d17) | Grouped aggregation is used but undefined | Resolved by ADR-0047 |
| [D18](#d18) | There is no event-reference attribute type | Resolved by ADR-0046 |
| [D19](#d19) | Division by zero yields a verdict, not a value | Resolved by ADR-0047 |
| [D20](#d20) | Null semantics are unstated | Resolved by ADR-0047 |
| [D21](#d21) | Derived attributes are not required to be acyclic | Resolved by ADR-0047 |
| [D22](#d22) | Expression scoping and name resolution are unstated | Resolved by ADR-0047 |
| [D23](#d23) | Parameter-schema derivation has no algorithm | Resolved by ADR-0047 |
| [D24](#d24) | Remedy-class assignment has no rule | Resolved by ADR-0047 |
| [D25](#d25) | The availability query has unbounded cost | Resolved by ADR-0048 |
| [D26](#d26) | `changed_since` has no index story | Resolved by ADR-0048 |
| [D27](#d27) | The spec's definition of an action is the option ADR-0016 rejected | Resolved by ADR-0041 |
| [D28](#d28) | Files are both never touched and deleted | Resolved by ADR-0041 |
| [D29](#d29) | Splitting is both uncovered and expressible | Resolved by ADR-0046 |
| [D30](#d30) | Derived attributes cannot be queried | Resolved by ADR-0048 |
| [D31](#d31) | Constraint compilation is mis-cited and backend-dependent | Resolved by ADR-0041 |
| [D32](#d32) | External evaluators have no slot in the execution sequence | Resolved by ADR-0049 |
| [D33](#d33) | Quantity and serial tracking are incompatible | Resolved by ADR-0050 |
| [D34](#d34) | Three Proposed ADRs are load-bearing | Resolved |
| [D35](#d35) | Erasure has no taint rule and may collide with uniqueness | Resolved by ADR-0051 |
| [D36](#d36) | `self-serviceable` is misused for the fan-out cap | Resolved by ADR-0041 |
| [D37](#d37) | Iteration cannot take a collection-valued expression | Resolved by ADR-0052 |
| [D38](#d38) | A creation does not name which creation transition it uses | Resolved by ADR-0052 |
| [D39](#d39) | A write from an unsupplied optional input clears the field | Resolved by ADR-0052 |
| [D40](#d40) | `this` is not stated to be available in a creation outcome | Resolved by ADR-0052 |
| [D41](#d41) | Type-scan invariants have no affected-set rule | Resolved by ADR-0052 |
| [D42](#d42) | Three-valued logic left no way to test for absence | Resolved by ADR-0053 |
| [D43](#d43) | ADR-0038 never says when the parent's own outcome applies | Resolved by ADR-0054 |
| [D44](#d44) | `check` both simulates the real verdict and skips evaluators | Resolved by ADR-0054 |
| [D45](#d45) | A type without a declared assertion cannot be imported or migrated | Resolved by ADR-0054 |
| [D46](#d46) | An admitted invariant violation freezes the object | Resolved by ADR-0054 |
| [D47](#d47) | Superseded rules restated as current across the record | Resolved by the coherence pass of 2026-09-08 |
| [D48](#d48) | Single-object invariants are rejected by the model | Resolved by ADR-0055 |
| [D49](#d49) | A set-valued attribute can only be replaced wholly | Resolved by ADR-0055 |
| [D50](#d50) | Runtime-maintained inverses were a second write path | Resolved by ADR-0056 |
| [D51](#d51) | `supersede` was missing from the outcome grammar | Resolved by ADR-0056 |
| [D52](#d52) | A composition's delete cascade had no declaration site | Resolved by ADR-0056 |
| [D53](#d53) | Versions did not propagate from a machine to its binders | Resolved by ADR-0056 |
| [D54](#d54) | A machine could not state what it requires of a binding type | Resolved by ADR-0056 |
| [D55](#d55) | A deletion guard had to hand-enumerate every referencing type | Resolved by ADR-0056 |
| [D56](#d56) | Erasure could not run on a deleted object | Resolved by ADR-0056 |
| [D57](#d57) | The conditional `? :` collided with `?` as optionality | Resolved by ADR-0056 |
| [D58](#d58) | Implication `→` collided with the to-state arrow in ASCII | Resolved by ADR-0056 |
| [D59](#d59) | Remedy class names contain hyphens, which lex as subtraction | Resolved by ADR-0056 |
| [D60](#d60) | An approval could not be invalidated by an edit to a part | Resolved by ADR-0057 |
| [D61](#d61) | A part is orphaned by any terminal transition its cascade does not name | Resolved by ADR-0058 |
| [D62](#d62) | Re-parenting escapes the source whole's invariants | Resolved by ADR-0058 |
| [D63](#d63) | `accepts` is documented in two mutually exclusive positions | Resolved by ADR-0060 decision 2 |
| [D64](#d64) | The line-continuation rule does not admit a trailing comma | Resolved by ADR-0060 decision 1 |
| [D65](#d65) | Seven continuation lines begin with an operator after a line that ends in an identifier | Resolved by ADR-0060 decision 1 |
| [D66](#d66) | Braces mean three things and the cascade group is written two ways | Resolved |
| [D67](#d67) | A bare state literal is indistinguishable from an attribute of the same name | Resolved |
| [D68](#d68) | A state may legally be named `any`, which collides with the wildcard | Resolved |
| [D69](#d69) | Operator associativity is never stated | Resolved |
| [D70](#d70) | Header clause order is contradicted by adjacent examples | Resolved |
| [D71](#d71) | The compilation unit is undefined | Resolved by ADR-0060 decision 12 |
| [D72](#d72) | There is no syntax for an enum member literal | Resolved by ADR-0060 decision 10 |
| [D73](#d73) | `.state` and `.category` are read in five guards and granted by no member rule | Resolved by ADR-0060 decisions 9 and 10 |
| [D74](#d74) | Three-valued evaluation is absent from §8 | Resolved by ADR-0060 decision 4 |
| [D75](#d75) | Duration units, currencies and decimal precision are given by example only | Resolved |
| [D76](#d76) | `this.id` and `identity` arguments to an evaluator are unstated | Resolved |
| [D77](#d77) | `referrers` has a heterogeneous element type on which `.state` is read | Resolved |
| [D78](#d78) | `counter` has no marking slot in the grammar and no stated relation to `attr` | Resolved by ADR-0060 decision 8 |
| [D79](#d79) | `owner` has no marking slot, and nothing says whether the state and stored reference ends are indexed | Resolved by ADR-0060 decision 9 |
| [D80](#d80) | A part declared in an abstract base cannot declare its cascades | Resolved by ADR-0060 decision 7 |
| [D81](#d81) | `survives` is all-or-nothing where the need is per-transition | Resolved by ADR-0060 decision 7 |
| [D82](#d82) | A cascade clause is repeated once per terminal transition | Resolved by ADR-0060 decision 7 |
| [D83](#d83) | `terminal` and `closed` pull in opposite directions with no warning | Resolved |
| [D84](#d84) | No personal attribute can be required, and no invariant can assert one is present | Resolved by ADR-0060 decision 5 |
| [D85](#d85) | `may admit` cannot name the invariant an assertion actually breaches | Resolved by ADR-0060 decision 6 |
| [D86](#d86) | `accepts` plus `default` silently resets a value on a non-creation transition | Resolved by ADR-0060 decision 3 |
| [D87](#d87) | `visible when` has no typing, traversal or indexing rules | Resolved |
| [D88](#d88) | `sweepable` is reported by the publish report and defined nowhere | Resolved |
| [D89](#d89) | The identifier rule contradicts itself on scope | Resolved |
| [D90](#d90) | Mandatory `limit` produces invented numbers and an unactionable report | Recorded as evidence for open question 4, and resolved by the ruling |
| [D91](#d91) | Confidentiality is per-object, and the common need is per-attribute | Recorded as open question 6, and refused by the ruling |
| [D92](#d92) | An evaluator cannot return a value, and the common integration assigns one | Recorded as open question 8, and refused by the ruling |
| [D93](#d93) | The §10 preamble's decidability claim is wrong in three ways | Resolved |
| [D94](#d94) | Fourteen checks are ambiguous, fire on the document's own examples, or cannot be built | Resolved |
| [D95](#d95) | Eleven terms are used by §10 and defined nowhere in §1–9 | Resolved by ADR-0060 |
| [D96](#d96) | Seventeen rules stated in §1–9 have no check | Resolved |
| [D97](#d97) | The new indentation rule does not parse the document's own compressed bodies | Resolved |
| [D98](#d98) | A sentence surviving the cascade rewrite contradicted the example six lines above it | Resolved |
| [D99](#d99) | A part with an abstract `owner` could be declared, given cascades, and never created | Resolved |
| [D100](#d100) | The worked example teaching the unknown trap used the enum syntax the same iteration made mandatory | Resolved |
| [D101](#d101) | Three stored relationship ends carried `indexed` after the same iteration said stored ends are never marked | Resolved |
| [D102](#d102) | `survives on { … }` was justified by an example its own rules reject, and had no subtype form | Resolved |
| [D103](#d103) | Check 50 was filed among the checks needing the previous declaration | Resolved |
| [D104](#d104) | DESIGN.md contradicted the syntax document on the rule the unknown fix depends on | Resolved |
| [D105](#d105) | The unknown rule was stated for two of the four contexts | Resolved |
| [D106](#d106) | Decimal accumulation could never publish | Resolved |
| [D107](#d107) | The list of clauses taking bare comma-separated lists omitted four that the examples use | Resolved |
| [D108](#d108) | The literal list omitted numbers | Resolved |
| [D109](#d109) | Check 33 put transitions in one namespace with attributes | Resolved |
| [D110](#d110) | Whether a cascade skips a part whose guard fails was answered differently in two documents | Resolved |
| [D111](#d111) | Nine checks retained gaps after the §10 rewrite | Resolved |
| [D112](#d112) | A claimed check clause was not demonstrated by any fixture | Resolved |
| [D113](#d113) | The new-invariant scan was named in the §10 preamble and absent from the report list | Resolved |
| [D114](#d114) | The abstract-base part pattern had no spelling that publishes | Resolved |
| [D115](#d115) | A wrapped `provides capability` line was read as declaring only its first line | Resolved |
| [D116](#d116) | `money` had no scale and no rounding rule | Resolved |
| [D117](#d117) | `money` fixes its currency at declaration, so a multi-currency ledger cannot be typed | Recorded as open question 9, and refused by the ruling |
| [D118](#d118) | Idempotency, an advertised runtime capability, had no declarable form | Resolved |
| [D119](#d119) | The negative of an external verdict did not type | Resolved |
| [D120](#d120) | A required reference or singular part was unchecked at creation | Resolved |
| [D121](#d121) | A `ref` with only one end declared had no stored-end rule | Resolved |
| [D122](#d122) | Whether a family member satisfies a base-typed input was never stated | Resolved |
| [D123](#d123) | The cascade skip rule did not say whether it governs a `call` | Resolved |
| [D124](#d124) | "Terminal transition" was never defined | Resolved |
| [D125](#d125) | A sequence could be scoped only by a reference | Resolved |
| [D126](#d126) | `tracking` was not inherited and an abstract type had to declare it | Resolved |
| [D127](#d127) | No remedy class distinguished a window that has not opened from one that has closed | Resolved |
| [D128](#d128) | A single two-valued branch multiplied into four transitions | Resolved |
| [D129](#d129) | A money balance cannot be a counter | Recorded as open question 10, and deferred by the ruling |
| [D130](#d130) | `only via` on a shared child scales as binders times triggers | Recorded as open question 11, and agreed but not built |
| [D131](#d131) | `serial` and `quantity` fit neither a payment, a posting nor an account | Recorded as open question 12, and resolved by the ruling |
| [D132](#d132) | Whether an `enum` body may wrap was unclear | Resolved |
| [D133](#d133) | The prohibition on marking a stored end `indexed` lived only in §8.1 and a check | Resolved |
| [D134](#d134) | Check 21 rejected the construct §8.3 had just been rewritten to permit | Resolved |
| [D135](#d135) | The symmetric-shape whitelist excluded the commonest type-scan there is | Resolved |
| [D136](#d136) | The booking case study still carried the `id` exclusion the same day's rule made an error | Resolved |
| [D137](#d137) | Check 8's new singular-`part` clause was unsatisfiable | Resolved |
| [D138](#d138) | A type-scan `count` silently changed meaning | Resolved |
| [D139](#d139) | `tracking` inheritance was stated twice and implemented nowhere | Resolved |
| [D140](#d140) | A wrapped `enum` body was legal by §9.3 and rejected by check 51 | Resolved |
| [D141](#d141) | The terminal-transition gloss reasoned from the wrong type | Resolved |
| [D142](#d142) | Four defects across three iterations were a rule changed without its check, and nothing linked the two | Resolved by check 52 |
| [D143](#d143) | Five smaller gaps from the same audit | Resolved |
| [D144](#d144) | A decimal product could never be assigned | Resolved |
| [D145](#d145) | Time-gated was a static report line with a runtime condition | Resolved |
| [D146](#d146) | A cascade's target was checked for existence and not for from-state coverage | Resolved as a publish report |
| [D147](#d147) | The abstract-base worked example had one concrete whole | Resolved |
| [D148](#d148) | Two wording residues | Resolved |
| [D149](#d149) | The swap test rejected the two shapes it claimed to accept | Resolved |
| [D150](#d150) | Check 52 claimed to catch the defects it was built for, and catches one of four | Resolved by retraction in the document |
| [D151](#d151) | Check 52's detector recognised seven phrasings and missed seventeen normative statements in this document | Resolved |
| [D152](#d152) | Two normative rules had no check to cite | Resolved |
| [D153](#d153) | A scale mismatch on a write cited a check about expressions that type | Resolved |
| [D154](#d154) | The cascade-coverage check's verdict on a legitimate cascade could not be predicted | Resolved |
| [D155](#d155) | The design document's outcome grammar had gone stale in three of six lines | Resolved |
| [D156](#d156) | Check 8's two clauses used different quantifiers, and the weaker one was silent | Resolved |
| [D157](#d157) | Two paragraphs each claimed to state the only skip rule and contradicted each other on `call` | Resolved |
| [D158](#d158) | A clause left behind by the swap-test rewrite gave a false reason | Resolved |
| [D159](#d159) | A binder's own `create` did not replace the machine's, so the document's own remedy could not be spelled | Resolved |
| [D160](#d160) | Check 52's proximity window gave a live false pass | Resolved |
| [D161](#d161) | A check widened to satisfy a citation came out forbidding more than its rule did | Resolved |
| [D162](#d162) | The overlap illustration defending the normalisation table was wrong on its own terms | Resolved |
| [D163](#d163) | Check 8's two clauses still stated their quantifier with different precision | Resolved |
| [D164](#d164) | Two smaller items | Resolved |
| [D165](#d165) | The creation-replacement rule broke its own motivating example | Resolved |
| [D166](#d166) | Two checks were not updated alongside the two that were | Resolved |
| [D167](#d167) | Inserting an open question renumbered four others and invalidated six references | Resolved |
| [D168](#d168) | A rename invented a new word for an established concept and redefined it | Resolved |
| [D169](#d169) | Twenty-five findings across the five older case studies and the walkthrough, none of which had ever been checked | Resolved |
| [D170](#d170) | `DESIGN.md` promised a construct the language did not have | Resolved by ADR-0073 |
| [D171](#d171) | A scripted edit put four answer paragraphs in the wrong lists | Resolved |
| [D172](#d172) | Fifty refinement links were one-way, over twenty-six records | Resolved |
| [D173](#d173) | Five decisions were reversed on one side only | Resolved |
| [D174](#d174) | Six of thirty-two rules stated in both the model document and the specification had drifted to the superseded version | Resolved |
| [D175](#d175) | The defect index listed 62 of 170 entries | Resolved |
| [D176](#d176) | Most counts in the status documents were wrong | Resolved |
| [D177](#d177) | Seven register entries read "Not resolved" after the author had ruled on the question each was carried to | Resolved |
| [D178](#d178) | The mediated guarantee named one exception and there are two | Resolved |
| [D179](#d179) | Nine case-study statements described the model as it no longer is | Resolved |
| [D180](#d180) | Two pairs of documents contradicted each other on substance | Resolved |
| [D181](#d181) | The sequence table would have made sequences gapless, which a decision rejected | Resolved |
| [D182](#d182) | Four tables were missing a column a decision requires | Resolved |
| [D183](#d183) | A compiled constraint cannot yield to an admission, and nothing said so | Resolved |
| [D184](#d184) | Three rules about absence and identity were unstated | Resolved |
| [D185](#d185) | The blob store's lifecycle expiry must be disabled and the schema did not say so | Resolved |
| [D186](#d186) | Four routes to a writable mirror | Resolved |
| [D187](#d187) | On SQLite the sequence mint deadlocks against the request that needs it | Resolved by ADR-0076 |
| [D188](#d188) | A retry carrying both its idempotency key and its `expected_version` is refused as `stale`, not replayed | Resolved by ADR-0076 |
| [D189](#d189) | The idempotency key is scoped to an optional `principal`, and two documents disagree on whether that was decided | Open |
| [D190](#d190) | Erasure does not reach legacy entries, proposal inputs, or inputs never written to a personal attribute | Open |
| [D191](#d191) | Two-pass import cannot insert a row whose required reference is `NOT NULL` | Open |
| [D192](#d192) | The disposition file offers `ADMIT` for guard-class violations nothing can admit | Open |
| [D193](#d193) | The actor's optional attributes are unreadable, readable and absent, in three documents | Open — needs the author |
| [D194](#d194) | "Mirror" names two mechanisms with different write paths, and the model document carries neither | Open — needs the author |
| [D195](#d195) | Two version numbers share the name "declaration version" | Open |
| [D196](#d196) | Six API and schema shapes do not carry what the model says they carry | Open |
| [D197](#d197) | An action would clear an object's `asserted` marking | Open |
| [D198](#d198) | The id format is undecided while cascade order and harness reproducibility rest on it | Open |
| [D199](#d199) | Check 7's stored-end clause is unimplemented and the specification's own example violates it | Open |
| [D200](#d200) | Eight summaries are stale again, one day after D176 | Open |
---

## Severity 1: breaks the model or a running system

### D01
**Cascaded transitions cannot see each other's writes.**
`docs/adr/0019-outcomes-cascade-across-relationships-atomically.md:14` requires "All guards first ... evaluated before anything is written", and `docs/DESIGN.md:167`/`:169` separate guard evaluation (step 5) from writing (step 7).

Breaks three things:
1. `IntakeBatch.commit` cascades `inventorize` then `reserve` (`docs/design/first-consumer-walkthrough.md:191`). Reserve requires the unit `AVAILABLE`, which only the first cascade produces. Against a pre-write snapshot the guard always fails. This is ADR-0019's own motivating example (`0019:8`) and the scenario that closes the double-booking window.
2. Order placement cascades `product.reserve(qty)` per line (`docs/design/case-study-orders.md:16`) guarded on `on_hand - reserved >= inputs.qty` (`:20`). Two lines on one product both read the same snapshot, both pass, both apply. One order oversells itself.
3. `reserved := reserved + qty` applied twice from one snapshot is either 12 or a lost update to 6. No rule states which.

**Resolution required:** a decision on sequential versus simultaneous cascade application, and on repeated writes to one attribute.

**Resolved by ADR-0038.** Sequential application: each cascade's guards see the writes of those before it; failure aborts before commit; iteration in ascending id order; writes are read-modify-write.

### D02
**Reads outside the lock set are unisolated.**
`docs/adr/0023-transitions-execute-under-locks-and-may-carry-an-expected-version.md:13` locks only what is written; `:14` states "Objects that are only read are not locked" and grants serialisable isolation to *invariants* alone. No document states the isolation level of the transition transaction.

Defeats ADR-0024 by construction. Its delete guard is `none(<referencing type> where <reference> == this and state not terminal)` (`docs/adr/0024-deletion-is-a-terminal-transition-gated-on-live-references.md:13`). The reference lives on the referencing row, so `Customer.delete` and `Delivery.create` lock disjoint rows, share no conflict, and both commit. The result is a deleted customer with a live delivery, which is the failure ADR-0024 exists to prevent.

Applies to every cross-object guard, including `none(Service where unit == this and state != CANCELLED)` (`docs/DESIGN.md:140`).

**Resolution required:** the isolation level, and what the store does about reads the lock set does not cover.

**Resolved by ADR-0039.** The transition transaction runs at serialisable isolation, so guard reads outside the lock set are protected; serialisation failures retry then refuse with `stale`.

### D03
**The guarantee is false as written, and the override path is undeclared.**
`docs/DESIGN.md:229` states the guarantee absolutely: "no state change bypassed the guards." Against that:
- `docs/DESIGN.md:212` — imported state is "asserted through the override path".
- `docs/adr/0015-first-consumer-and-fresh-build-with-ported-data.md:56` — "none of that state passed through a guard."
- `docs/adr/0027-declarations-are-versioned-and-removals-require-a-mapping.md:14` — invariant violations may be "admitted with a recorded flag, by a person."

So the store can hold objects that failed their guards and objects that violate their invariants. Separately, the override is named in `DESIGN.md:198`, `:212`, `:231` and `docs/adr/0024:16` but declared nowhere: it appears in no model block, has no guards, no stated authority and no limit on which states it may assert. `docs/adr/0024:16` calls it "itself a transition with guards", but it must assert states the type's guards forbid, so its guards are not the type's.

It also collides with `docs/adr/0016-actions-are-self-transitions.md:18` ("Transitions are addressed by name, never by target state"), since the override and ADR-0027's migration mapping both address a target state.

**Resolution required:** declare the override as a first-class element, and qualify the guarantee.

**Resolved by ADR-0040.** The override is declared as an asserting transition: named, capability-gated, reason required, invariants admitted explicitly, and the guarantee in §13 is restated truthfully.

### D04
**The lock set cannot be computed when the spec says.**
`docs/DESIGN.md:166` locks "every object the outcome will write" before guards run; `docs/adr/0023:17` requires locks in id order. The cascade target set is a filtered iteration (`docs/adr/0019:22`), so it is discovered by reading, and `0023:14` does not lock what is read. Three rules cannot all hold.

Two further problems: a created object has no id to sort by, and `docs/adr/0018-every-object-carries-a-store-assigned-globally-unique-identifier.md:57` leaves time-ordering undecided, so deadlock freedom rests on an open storage choice. Sequence values are taken atomically inside the transaction (`docs/adr/0029-named-sequences-mint-business-identifiers.md:12`) but sequences are not objects and sit outside the id-ordered discipline.

**Resolved by ADR-0039.** The lock set is no longer computed in advance. Locks are taken as objects are reached; ADR-0023's id-ordering rule is withdrawn and deadlock is handled by retry.

### D05
**Availability both does and does not list only-via transitions.**
`docs/adr/0020-a-transition-may-be-reachable-only-via-named-parents.md:14` — an only-via transition "does not appear in any caller's availability list". `docs/adr/0037-the-read-surface.md:19` and `docs/DESIGN.md:208` — availability returns "every transition of the object's type". `docs/DESIGN.md:116` makes "not requestable" a verdict, implying such transitions do appear.

This decides whether an agent is shown `unit.sell`, which is the hazard ADR-0020 names at `0020:25`: "an agent will call whatever is listed."

**Resolved by ADR-0041.** ADR-0020 wins: only-via transitions are not listed; `not requestable` is returned when one is named explicitly.

### D06
**Idempotent replay is specified as both refuse and replay.**
`docs/adr/0014-delivery-is-at-least-once-with-idempotency-keys.md:27` — "ObjectKeeper refuses a second attempt carrying a key it has already applied." `docs/DESIGN.md:165` — "If the idempotency key has been applied before, return the original result."

Different observable behaviour. `docs/adr/0022-time-is-a-guard-value-and-availability-is-queryable.md:15` depends on the replay reading ("so a repeated sweep is harmless"). ADR-0014 itself warns at `:54` against conflating request replay with transition deduplication.

**Resolved by ADR-0041.** Replay wins: an applied key returns the original result, marked as a replay. ADR-0014's wording corrected in place.

### D07
**The built-in Subscription cannot be operated under its own rules.**
Three defects in one place.
1. `docs/adr/0034-event-ordering-and-subscriptions-as-a-built-in-type.md:20` makes the cursor a controlled attribute and declares only one action, `acknowledge`. Under `docs/DESIGN.md:56` a controlled attribute is written only via transitions, so the cursor has no writer. Yet `docs/adr/0037:24` makes `pull` a read-surface operation, so a read would advance a controlled attribute.
2. `docs/adr/0034:20` advances the `lagging` and `dead-lettered` states "by the delivery worker", which is part of an ObjectKeeper deployment (`docs/DESIGN.md:30`). `docs/adr/0012-objectkeeper-does-not-initiate-transitions.md:17` forbids any path by which an object moves without an external actor. Same problem for `Proposal.expired` (`docs/adr/0036-proposals-are-a-built-in-type-and-delegation-is-supplied.md:17`).
3. Every acknowledgement is an action, so it records an event, and the log is never pruned (`docs/adr/0033-current-state-is-stored-and-the-log-is-permanent-history.md:15`). Subscription acknowledgements become permanent history at delivery rate.

**Resolved by ADR-0043.** Progress is runtime state outside the object model; lag and death are derived, so nothing initiates and acknowledgements never enter the log.

### D08
**Proposals across declaration versions are undefined.**
`docs/adr/0036:16` re-evaluates guards at execution but never says under which declaration version. Consequences unhandled: a version that adds a required input makes the stored request unexecutable, and `0036:14` offers no terminal state for it, so it pends forever; `docs/adr/0027:15` says removing a transition affects no object, but it strands a proposal naming it; a migration can move the target out of the proposal's from-state.

**Resolved by ADR-0044.** Execution uses the current declaration; a new terminal state `invalidated` catches requests the declaration can no longer express.

### D09
**Free attributes have no write path.**
`docs/DESIGN.md:61`, `:96` and `docs/adr/0006-controlled-and-free-attributes.md:15` assert free attributes are written by any permitted actor and recorded. Nothing defines how.
- No operation exists. `docs/adr/0037:15-25` enumerates the whole API and contains no write; `docs/DESIGN.md:161-169` defines execution for transitions only.
- "Permitted" is undefined. `docs/adr/0025-the-actor-is-a-value-supplied-by-the-consumer.md:44` reduces permissions to guards over `actor.*`, and guards attach to transitions.
- No provenance class fits. `docs/adr/0033:14` fixes a closed set whose `observed` means "a transition produced it".
- The event shape assumes a transition: `changes_state` is undefined and the subscription filter keys on transition names (`docs/adr/0034:21`).

ADR-0006 is still Proposed and says at `:38` "Revisit before it becomes load-bearing." It is now load-bearing in ADR-0016, 0017, 0021, 0031, 0033, 0034 and 0035.

**Resolved by ADR-0042.** The free class is removed. Every attribute is controlled, and editing is a declared action, which costs one line and buys a guard, an actor and an event.

### D10
**Two different invariant-enforcement specifications.**
`docs/adr/0009-invariants-declared-at-type-level.md:12` is static: "The runtime determines which transitions could violate them and enforces there." `docs/DESIGN.md:168` is dynamic: "Check every invariant the written objects could violate." Different algorithms, different cost, different coverage. Neither is marked normative, and `0009:27` concedes the static analysis is "a real piece of machinery" without specifying it.
**Resolved by ADR-0045.** Dynamic enforcement is normative; the static analysis becomes a publish-time report; invariants may traverse only declared inverses.

---

## Severity 2: cannot be expressed, or has no algorithm


### D11
**Cascaded transitions cannot take inputs.** Used in every case study (`walkthrough:135`, `:191`, `:198`; `crm:46-48`; `orders:16`). Neither `docs/DESIGN.md:108` nor `docs/adr/0019:14` permits it, and ADR-0019's only example passes none.

**Resolved by ADR-0046.** Resolved by the outcome grammar.

### D12
**An outcome cannot name an object it creates.** `docs/adr/0019:15` shows `create WarrantyContract(...)` with no binding. This makes `docs/adr/0028-supersession-an-object-may-end-by-naming-a-successor.md` self-contradictory: `:1` takes the successor as an input object id, `:2` permits the transition to cascade its creation. Both cannot hold for move and convert, which is supersession's headline use. It also blocks split (see D29).

**Resolved by ADR-0046.** Creations may be bound to a name, which also resolves ADR-0028's internal contradiction.

### D13
**`this_event` is undefined and chronologically impossible.** Used once, at `docs/design/case-study-approvals-and-bookings.md:40`, and defined nowhere. Every guard in ADR-0035 keys on the approval carrying it. Events are written at `docs/DESIGN.md:169` step 7, after the outcome that would read it.

**Resolved by ADR-0046.** An event's identity is allocated before its outcome runs, so `this_event` is well defined.

### D14
**Repeat-N creation is not expressible.** `docs/design/first-consumer-walkthrough.md:187` raises a procurement order "cascading `create Unit(...)` ×N". `docs/adr/0019:22` permits iterating a relationship only, and `:46` forbids iterating a query over a type. An integer input has nothing to iterate.

**Resolved by ADR-0046.** Resolved by the outcome grammar.

### D15
**Map-typed inputs and dynamic writes are not expressible.** `docs/design/case-study-crm.md:45` writes `<each attribute in values> := inputs.values.<attribute>`. `docs/DESIGN.md:96` gives a closed attribute-type list with no map, and `:108` fixes outcome writes as static `attribute := expression`. The merge is that study's centrepiece.

**Resolved by ADR-0046.** Refused by decision: a transition writes the attributes it names; the merge declares its fields.

### D16
**`sum` over a type is required but not granted.** `docs/adr/0032-expression-language-version-2-adds-arithmetic.md:18` and `docs/DESIGN.md:141` grant `sum`/`min`/`max` over a relationship. `docs/adr/0032:37` states as its own consequence that available-to-promise needs "a type-scan sum over inbound units".

**Resolved by ADR-0047.** Resolved by the expression semantics.

### D17
**Grouped aggregation is used but undefined.** `docs/design/case-study-orders.md:24` declares the invariant `sum(shipments.qty for a line) <= line.qty`. No `for a <group>` construct exists in ADR-0021 or ADR-0032.

**Resolved by ADR-0047.** Refused by decision: grouping belongs to the read surface; declare a relationship aggregate on the other end.

### D18
**There is no event-reference attribute type.** `docs/adr/0035-approval-is-a-guard-over-recorded-approval-parts.md:12` requires an Approval to carry "the event that recorded it", and `docs/DESIGN.md:143` reads `approval.event`. The closed list at `docs/DESIGN.md:96` has no event reference, and `docs/DESIGN.md:204` makes only Subscription and Proposal built in, so Approval must be consumer-declared. It cannot be.

**Resolved by ADR-0046.** Resolved by the outcome grammar.

### D19
**Division by zero yields a verdict, not a value.** `docs/adr/0032:16` — "division by zero is a guard failure, never a value." Expression evaluation is therefore not compositional: the rule is undefined inside `none(...)`, inside an implication's antecedent, and inside a derived attribute the read surface returns.

**Resolved by ADR-0047.** Resolved by the expression semantics.

### D20
**Null semantics are unstated.** `docs/adr/0021-derived-attributes-and-the-expression-language.md:19` has a null test and `docs/DESIGN.md:144` compares against null, but nothing says whether comparison with null is three-valued. `docs/adr/0031-erasure-redacts-declared-personal-attributes-across-history.md:17` makes the redaction marker read as null. Under a two-valued reading, erasing a person makes `actor.id != requested_by.id` and `none(approvals where approver == actor.id)` true (`docs/design/case-study-approvals-and-bookings.md:17`), granting the erased person self-approval and repeat-approval. `docs/design/edge-cases.md:31` claims "Erasure never silently satisfies a guard", which is false for exclusionary guards.

**Resolved by ADR-0047.** Three-valued logic; a guard evaluating to unknown fails, which closes the erasure/separation-of-duties hole.

### D21
**Derived attributes are not required to be acyclic.** `docs/adr/0019:23` and `docs/DESIGN.md:171` require the cascade graph to be acyclic. `docs/adr/0021:12` imposes nothing, so `a := b + 1; b := a + 1` is a legal declaration and non-terminating.

**Resolved by ADR-0047.** Resolved by the expression semantics.

### D22
**Expression scoping and name resolution are unstated.** `docs/adr/0035:14` defines `changed_since` over "the named attributes of `this`", `docs/DESIGN.md:143` qualifies the element (`approval.event`), and `docs/design/case-study-approvals-and-bookings.md:37`,`:42` use a bare `event` inside `none(approvals where ...)` while `relevant` is declared on the parent. Whether `this` rebinds inside a predicate is never stated, and the two readings give opposite results.

**Resolved by ADR-0047.** Resolved by the expression semantics.

### D23
**Parameter-schema derivation has no algorithm.** `docs/adr/0005-guards-evaluate-over-state-plus-inputs.md:23` asserts a transition's parameter list is derived from its guards, and `docs/adr/0037:19` returns it. Deriving input constraints from an arbitrary boolean guard such as `inputs.qty <= on_hand - reserved` is abduction, not extraction. ADR-0010's headline claim that tool schemas and validation cannot drift rests on this. Related inconsistency: `docs/DESIGN.md:76` says a transition declares `inputs` while `:116` says the schema is derived, and under the derived reading an input used only in an outcome write is invisible to callers.

**Resolved by ADR-0047.** Inputs are declared, not derived; ADR-0005's derived-schema claim is withdrawn and anti-drift is preserved by publish-time checks.

### D24
**Remedy-class assignment has no rule.** `docs/DESIGN.md:120-124` defines five classes. `docs/adr/0022:12` implies inference from expression shape, but `end_date <= now` is `temporal` when stored and `self-serviceable` when supplied, and a conjunction of clauses with different classes has no stated rule. Whether the class is declared per guard or inferred is never said.

**Resolved by ADR-0047.** Resolved by the expression semantics.

### D25
**The availability query has unbounded cost.** `docs/adr/0037:20` returns objects for which a transition is available. Guards may contain type scans, aggregates and `changed_since`, so cost is O(candidates × unbounded). Only *deferred* external evaluators are excluded (`0037:20`, `0022:14`), so an eager one (`docs/adr/0008-guard-escape-hatch-is-a-named-external-evaluator.md:14`) means a third-party round trip per object. Visibility is an arbitrary actor predicate and cannot generally be indexed. Cursor pagination over a computed predicate needs a stable total order that is not specified. `docs/adr/0037:51` calls the operation cheap.

**Resolved by ADR-0048.** A transition is sweepable or `available` refuses it; publishing reports which; external evaluators are never called in a sweep.

### D26
**`changed_since` has no index story.** `docs/adr/0035:14` reads the log. Every indexing statement in the record is about the object row (`docs/adr/0033:12`, `docs/adr/0037:27`). Answering "which event last wrote attribute X of object O" requires per-attribute writes to be queryable in the log, which the storage schema does not yet exist to say.
**Resolved by ADR-0048.** A per-attribute last-written index, maintained in the writing transaction, makes `changed_since` a constant-time comparison.

---

## Severity 3: contradictions and scope errors


### D27
**The spec's definition of an action is the option ADR-0016 rejected.** `docs/DESIGN.md:106` and `:242` say an action "writes controlled attributes without changing lifecycle state", which is the shape of ADR-0016's rejected option B (`docs/adr/0016:45`, "an outcome restricted to attribute writes"). ADR-0035 requires actions to create Approvals (`0035:12`) and the CRM merge requires an action to cascade (`crm:47`).

**Resolved by ADR-0041.** ADR-0016 wins: an action's outcome is unrestricted. DESIGN.md §5.4 and the terminology entry corrected.

### D28
**Files are both never touched and deleted.** `docs/DESIGN.md:96` — "ObjectKeeper never touches the bytes"; `:200` — erasure "deletes referenced file content"; `docs/adr/0031:14` the same; `docs/adr/0017-file-attachments-are-content-addressed-references.md:20` gives the backend interface as "put, get-URL, exists, delete". Also, an Accepted ADR (0031) depends on a Proposed one (0017), and `docs/DESIGN.md:212` states file porting as settled while `docs/adr/0015:74` leaves it open.

**Resolved by ADR-0041.** Erasure wins: ObjectKeeper never reads, streams or serves bytes, and deletes them only during erasure.

### D29
**Splitting is both uncovered and expressible.** Recorded as not covered at `docs/design/edge-cases.md:14`, `:22` and `docs/DESIGN.md:233`; asserted as expressible at `docs/design/first-consumer-walkthrough.md:201`. It appeared in three of five case studies, it is the one-to-many mirror of supersession, and it needs exactly D12 plus a declared write path for a composition part's owner.

**Resolved by ADR-0046.** Part re-parenting makes split expressible; the edge-case entries are updated.

### D30
**Derived attributes cannot be queried.** `docs/adr/0037:17` restricts query filters to indexed attributes, state and category; `docs/adr/0021:12` makes derived attributes never stored; `now`-dependent ones can never be indexed because the value changes with no write. ADR-0022's consequences say the automation layer polls such predicates "through the ordinary read surface". Overdue and low-stock alerts therefore have no path. The same unsupported claim appears at `case-study-tickets.md:40` and `case-study-approvals-and-bookings.md:25`.

**Resolved by ADR-0048.** Time-dependent derived attributes are queried by filtering the stored operand; static ones over indexed attributes are themselves indexable.

### D31
**Constraint compilation is mis-cited and backend-dependent.** `docs/DESIGN.md:130` and `docs/adr/0001-store-owns-persistence.md:34` attribute a three-shape list to ADR-0023, which names only "uniqueness, a partial unique index" (`0023:14`). "Uniqueness per external source" comes from `docs/adr/0037:29`; "interval exclusion per key" appears only in `case-study-approvals-and-bookings.md:70`, which itself mis-cites ADR-0023. SQLite, a stated target (`docs/adr/0001:16`), has no exclusion constraint, so the same declaration gives different guarantees per backend with no caveat. `docs/adr/0002-requiredness-attaches-to-transitions.md:35` still asserts unannotated that "the database is not a safety net".

**Resolved by ADR-0041.** Serialisable isolation (ADR-0039) makes correctness backend-independent; compilation is an optimisation, reported at publish.

### D32
**External evaluators have no slot in the execution sequence.** `docs/DESIGN.md:161-169` never mentions them. A deferred guard is "checked only at execution" (`docs/adr/0008:14`), which places a third-party network call inside the write transaction while holding row locks, the cost ADR-0014 rejected in-process handlers for (`0014:45`).

**Resolved by ADR-0049.** Evaluators are consulted before the transaction opens; the verdict carries an as-of time and an optional freshness bound.

### D33
**Quantity and serial tracking are incompatible.** `docs/design/edge-cases.md:17` defers quantity-tracked consumables to iteration 4; iteration 4 modelled quantity as counters on a Product (`orders:14`, `:20`) and never met a serialised unit. `docs/DESIGN.md:44` names SparePart among the types the model must carry and `walkthrough:9` serialises it. Nothing says how a delivery slot binds to a quantity, or how `reserve` means two things per tracking mode.

**Resolved by ADR-0050.** A type declares serial or quantity tracking, and a slot declares its fill form, so one kit may carry both.

### D34
**Three Proposed ADRs are load-bearing.** ADR-0006, ADR-0008 and ADR-0017 are Proposed. `docs/DESIGN.md` states their content in the model block, the language table and the import path, marked "proposed" only in §5.2 and §5.5 prose. `docs/adr/0037:49` and `TODO.md:48` claim no open model questions remain while `TODO.md:105-108` still lists confirming all three.

**Resolved.** ADR-0006 is superseded by ADR-0042 rather than confirmed, and the author confirmed ADR-0008 and ADR-0017 on 2026-09-08. No ADR is now Proposed while being depended on. ADR-0008 is depended on by ADR-0021, ADR-0032, ADR-0022, ADR-0037, ADR-0048 and ADR-0049; ADR-0017 by ADR-0031, ADR-0051 and the import path. The claim in ADR-0037 and TODO.md that no open model questions remain is now true.

### D35
**Erasure has no taint rule and may collide with uniqueness.** Nothing forbids `display_name := contact.email` where the source is personal and the target is not; erasure then misses the copy, in the row and in the event that wrote it. Separately, `docs/adr/0031:17` defines the redaction marker only at expression level, while `docs/adr/0037:29` makes external-identifier uniqueness an automatic invariant compiled to a constraint. Erasing two records whose personal attribute is also an external identifier writes the marker twice, and a sentinel would make a legally mandated operation impossible.

**Resolved by ADR-0051.** Personal values are tainted and the taint is checked at publish; the redaction marker is absence, so uniqueness constraints do not collide.

### D36
**`self-serviceable` is misused for the fan-out cap.** `docs/design/edge-cases.md:39` refuses an oversized cascade with `self-serviceable`, defined at `docs/DESIGN.md:120` as "Satisfiable by a transition argument". A fan-out cap is satisfiable by no argument, and for order placement "split the request" means creating two orders, a different business fact.
**Resolved by ADR-0041.** A new `over-limit` verdict replaces the misuse of `self-serviceable`.

---

## Cosmetic

- **C01** `docs/adr/0013-events-are-recorded-to-a-durable-log-in-the-transition-transaction.md:34` diagram still reads "consumer holds a cursor", which ADR-0034 explicitly rejected at `0034:35`. **Resolved: diagram corrected.**
- **C02** `docs/DESIGN.md:83` lays out `actions` as a sibling of `transitions`, the visual shape of ADR-0016's rejected option B, though the text is correct. **Resolved: `actions` is now indented under `transitions` in the model block, so the layout says what the text says.**
- **C03** `docs/adr/0011-project-name-objectkeeper.md:42` attributes "gates rather than propels" to ADR-0004; it is ADR-0012's decision. **Resolved: citation corrected to ADR-0012.**
- **C04** `docs/adr/0004-composite-state-is-gated-not-derived.md:26` says a cascade-close "proposes" terminal transitions; since ADR-0036 "propose" is a reserved concept. **Resolved: reworded to "cascades".**
- **C05** The iteration-4 clarifications (outcomes may iterate an input's relationships; declared indexes; the fan-out cap) and the request `context` field live only in DESIGN.md and TODO.md with no ADR, against the convention that decisions live in ADRs. **Resolved: each has a home — iterating an input's relationship is ADR-0046, declared indexes are ADR-0048, the fan-out cap is ADR-0041 and now ADR-0071, and the request `context` field is ADR-0033.**


---

## Found by re-expressing the case studies

These five were found by writing the case studies' declarations in the grammar of ADR-0046 and ADR-0047, rather than by inspection. Each blocked a declaration a study needs.

### D37
**Iteration cannot take a collection-valued expression.** ADR-0046 allowed `for <name> in <relationship>` and `for <name> in 1..<expr>`. Splitting a delivery iterates a set-valued input naming the slots to move, and the CRM merge iterates a relationship of an input. Neither is a relationship of `this`, so neither was expressible.

**Resolved by ADR-0052.** The iteration source is any expression yielding a collection.

### D38
**A creation does not name which creation transition it uses.** `create Unit(...)` is ambiguous for the first consumer's unit type, which is born `REQUESTED` when procured, `INTAKE` when added by hand and `AVAILABLE` as opening stock. Creation is a transition, so the creation form has to name one.

**Resolved by ADR-0052.** `create <Type>.<transition>(...)`, with the bare form legal only where the type has one creation transition.

### D39
**A write from an unsupplied optional input clears the field.** `email := inputs.email` where the input is optional and absent writes absence under three-valued semantics, silently emptying the attribute. Every partial update has this shape, including the merge of the CRM study and the edit actions ADR-0042 requires of every type.

**Resolved by ADR-0052.** Such a write is skipped, which is a rule about optionality rather than a branch.

### D40
**`this` is not stated to be available in a creation outcome.** ADR-0046 allocated an event's identity before its outcome runs, so `this_event` works, but said nothing about the object. Without the same rule, an order cannot create its own lines and no creation can pass itself to a cascade.

**Resolved by ADR-0052.** The new object's id is allocated before its creation outcome runs.

### D41
**Type-scan invariants have no affected-set rule.** ADR-0045 restricted invariants to relationships with declared inverses so the affected set is computable by reverse traversal. The booking overlap invariant scans a type and traverses no relationship, so the restriction does not reach it, and reversing an arbitrary scan predicate is not generally possible.

**Resolved by ADR-0052.** A type-scan invariant must be symmetric, which the recognised shapes are and which makes the reverse the same predicate.

---

## Found by the coherence review

### D42
**Three-valued logic left no way to test for absence.** ADR-0047 made comparison with an absent value yield unknown, which was right and closed a real hole. It also broke every null test: `reason != null` in ADR-0021, and `s.unit != null`, `s.warranty_product != null` and `inputs.fix_version != null` in the re-expressed case studies, are all unknown whether or not the value is present, so any guard containing one fails permanently. ADR-0047's own rejected alternatives name the problem and leave no replacement.

**Resolved by ADR-0053.** `is null` and `is not null` are definite predicates; comparing against a `null` literal is a publish error.

*The first fix was incomplete, and instructively so. It swept fenced declaration blocks and corrected four lines; a coherence review then found nine more in guard tables and prose, plus four in the ADRs themselves. Scoping a fix to one syntactic form is how the same defect survives a repair — see `LESSONS.md`.*

### D43
**ADR-0038 never says when the parent's own outcome applies.** It sequenced the cascades against each other and left the parent's state change and attribute writes unplaced, which decides whether a cascade reads the parent before or after its transition. ADR-0020's rejected alternative also rested on the answer.

**Resolved by ADR-0054.** The parent's outcome applies in full before the first cascade's guards, and ADR-0020's rationale is restated on the argument that survives.

### D44
**`check` both simulates the real verdict and skips evaluators.** ADR-0038 said `check` returns the verdict a real request would receive; ADR-0049 said it calls no external evaluator. For any transition with an external guard both cannot hold.

**Resolved by ADR-0054.** `check` returns an explicitly partial verdict naming the guards it did not evaluate, and ADR-0037's claim that it is cheap is withdrawn.

### D45
**A type without a declared assertion cannot be imported or migrated.** ADR-0040 rule 1 says a type with no asserting transition cannot be overridden at all; rule 7 makes import and migration bulk assertion. Together a type author could make their type unimportable by omission, discovered at cutover.

**Resolved by ADR-0054.** Import and migration use a built-in assertion gated on a deployment capability, distinct from the declared one that governs repair.

### D46
**An admitted invariant violation freezes the object.** ADR-0027 and ADR-0040 both admit violations; neither says what the next ordinary transition does, and ADR-0045 checks every invariant the written objects could violate, so the next transition would refuse. Admission would let data in and then trap it.

**Resolved by ADR-0054.** An admission names an object and an invariant, suppresses that check for that object, and is discharged automatically when the invariant holds again.

### D47
**Superseded rules restated as current across the record.** The repair annotated the ADRs it changed and rewrote the fenced declaration blocks, and left the same superseded rules standing in guard tables, mapping tables, prose, "what held without change" sections and the specification's own model block and glossary. Instances included the all-guards-first rule and the pre-computed lock set presented as current in the walkthrough, the free-attribute class in a mapping table, the three-state subscription lifecycle, and the pre-repair Mediated property in the README.

**Resolved by the coherence pass of 2026-09-08**, which rewrote DESIGN.md from scratch rather than patching it further, revisited every table and prose section in the five case studies, and corrected twenty statements across the ADR set. The lesson is recorded in `LESSONS.md`.

---

## Found by writing the declaration syntax

Both were invisible while the model was described in prose, and became obvious the moment someone tried to write a real type in it.

### D48
**Single-object invariants are rejected by the model.** DESIGN.md §5.6 admitted exactly two forms, traversal and type scan, each with a rule that makes the affected set computable, and said publishing rejects anything else. The commonest invariant of all reads only the object's own attributes — "a unit in `RESERVED` has exactly one binding" — and is neither. It was overlooked precisely because it needs no rule: the affected set is the object just written.

**Resolved by ADR-0055.** A third, local form, which publishing classifies and reports like the others.

### D49
**A set-valued attribute can only be replaced wholly.** The outcome grammar had one write step, `attribute := expression`, and the expression language has no set operators by design. So attaching an intake photo, adding a watcher or adding a typed link cannot be written. Replacing the whole set makes a concurrent second attachment silently discard the first, which is the lost update the design's concurrency work exists to prevent.

**Resolved by ADR-0055.** `add` and `remove` outcome steps, read-modify-write like any other write, with no set algebra added to the expression language.

---

## Found by writing the declaration syntax, rounds 2 to 5

### D50
**Runtime-maintained inverses were a second write path.** Iteration 3 of the syntax had the runtime write both ends of a declared relationship. The far object's event carried no transition name, so no subscription could filter it, `changes_state` was undefined for it, and it stamped the far object's last-written index, so binding a unit invalidated every approval on the delivery. Only one end is now stored; the inverse is a derived view.

**Resolved by ADR-0056.**

### D51
**`supersede` was missing from the outcome grammar.** DESIGN.md §5.4 listed the outcome steps without it, while §8 required supersession and ADR-0028 defined it as an outcome operation.

**Resolved by ADR-0056.**

### D52
**A composition's delete cascade had no declaration site.** "Parts cascade" was asserted in the model and declared nowhere, so the cascade named no transition, carried no bound, and was invisible to the acyclicity check.

**Resolved by ADR-0056.**

### D53
**Versions did not propagate from a machine to its binders.** An object records its type's version, but a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses. A recorded version that did not move when those moved identified nothing.

**Resolved by ADR-0056.**

### D54
**A machine could not state what it requires of a binding type.** Transitions live in a machine; the attributes, references and invariants they read live on the type. With two types binding one machine, nothing said what the second must provide, and checks over a machine body were undecidable.

**Resolved by ADR-0056.**

### D55
**A deletion guard had to hand-enumerate every referencing type.** In other modules, forever, which is the cascade failure the deletion rule exists to prevent reintroduced as a maintenance burden. `referrers` replaces it and reaches references with no declared inverse, which previously escaped the guard entirely.

**Resolved by ADR-0056.**

### D56
**Erasure could not run on a deleted object.** A deleted object admits no further transitions, and erasure requests arrive precisely for closed accounts and retired units. Erasure is now the one carve-out.

**Resolved by ADR-0056.**

### D57
**The conditional `? :` collided with `?` as optionality.** Two meanings for one symbol in a language whose lexical section exists to prevent them.

**Resolved by ADR-0056.**

### D58
**Implication `→` collided with the to-state arrow in ASCII.** And cannot be typed. `implies` replaces it.

**Resolved by ADR-0056.**

### D59
**Remedy class names contain hyphens, which lex as subtraction.** `because unreachable-from-here` lexed as three identifiers and two subtractions. Renamed to single tokens and swept across seventeen sites.

**Resolved by ADR-0056.**

### D60
**An approval could not be invalidated by an edit to a part.** `changed_since` read attributes of `this` only, and ADR-0056 removed the runtime-maintained inverse that used to stamp a whole when a part changed. So editing a purchase order's line left the order untouched and its approvals standing, and the only workaround was a timestamp attribute, an action to touch it, and a cascade from every line-editing transition — the per-transition duplication ADR-0035 exists to remove, in its own headline guard.

**Resolved by ADR-0057.** `changed_since` accepts a part relationship name, answered from an index over the log rather than a write.

### D61
**A part is orphaned by any terminal transition its cascade does not name.** ADR-0056 gave a composition a cascade trigger, and a whole with two terminal transitions names one. Its parts survive the other, alive under a closed whole, which is what "lifetime bounded by the whole" forbids.

**Resolved by ADR-0058.** Every terminal transition must dispose of the parts or the part must be explicitly marked as surviving.

### D62
**Re-parenting escapes the source whole's invariants.** Writing a part's owner is what makes splitting expressible, and after the write the part points only at the destination, so reverse traversal never re-checks the source. An invariant such as "a shipment has at least one line" could be broken by moving the last line away.

**Resolved by ADR-0058.** A re-parent is treated as writing both wholes for the purposes of invariants and the part-event index, though neither whole's guards run.

## Found by the iteration-10 review of the declaration syntax

Two independent reviews on 2026-09-08, both returning a blocking verdict. One wrote a clinical-trial model in the syntax as a first-time user; the other audited every check in §10 for decidability against the grammar in §1–9. Line citations are `docs/design/declaration-syntax.md` at iteration 10.

The dominant finding is a recurrence of the defect fixed the same day as D48: **a term that exists only in the check list**. Eleven of them, listed under D95. `docs/LESSONS.md` records the pattern; it did not prevent the repeat because the earlier fix was scoped to the one term found rather than to the class.

### D63
**`accepts` is documented in two mutually exclusive positions.** `:426` places it in the transition body, in a stated clause order. All seven examples place it in the head, before the `{`, where `:407` says only markings go. Two of them (`:144`, `:264`) additionally put the head `accepts` before a body `input`, reversing the stated order. An implementer must reject either the rule or every example.

**Resolved by ADR-0060 decision 2.** `accepts` and `only via` are the transition head; inputs, guards and outcomes are the body.

### D64
**The line-continuation rule does not admit a trailing comma.** `:619` continues a line only when it "ends in an operator or an open bracket". Five sites end on a comma and continue: `:34`, `:152`, `:198`, `:268`. Read literally, `do delete ACTIVE -> DELETED only via Delivery.cancel,` is a complete declaration.

**Resolved by ADR-0060 decision 1.** A clause continues when the next line is indented more deeply.

### D65
**Seven continuation lines begin with an operator after a line that ends in an identifier.** `:62`, `:73`, `:239`, `:240`, `:279`, `:334`, `:530`. `derive leasable = state == DEVELOPMENT` / `and none(…)` parses as two clauses under the stated rule, the second beginning with a reserved word, which `:623` forbids and check 21 flags. The examples need a look-ahead rule the document does not state.

**Resolved by ADR-0060 decision 1.** The same rule; indentation is what the examples were already doing.

### D66
**Braces mean three things and the cascade group is written two ways.** `:190` writes the cascade group inside `{ … }` as metasyntactic repetition; the example at `:201` writes the same construct with no braces. `:629` claims a collection literal and a block "never occupy the same position", which is false for `assert <name> -> { S } { … }` and `act <name> at { A } { … }`, where they are adjacent. The meta-notation itself is never introduced.

**Resolved.** §9.3 gives the rule for adjacent groups: the first is the collection, the second the body, and a body is always written. §9.4 introduces the meta-notation. The cascade grammar now matches the example.

### D67
**A bare state literal is indistinguishable from an attribute of the same name.** `:605` gives `<Type>.<STATE>` for another type's states and nothing for the bare case, while no casing convention is mandated. A type with `states ACTIVE` and `attr ACTIVE bool` makes `require g: ACTIVE` ambiguous. The same holds for a bare category name against an attribute named `closed`. The partial checker resolves it with an undocumented upper-case regex, which is the invention the document forces.

**Resolved.** §9.2 gives a resolution order, and check 33 rejects a state name colliding with a member name, so no declaration reaches the ambiguous case.

### D68
**A state may legally be named `any`, which collides with the wildcard.** `:623` permits any reserved word as a state name. `states any category live` plus `act poke at any` has two readings, and checks 15, 19 and 40 give different answers. The same shape applies to a category named `terminal`.

**Resolved.** §9.2 forbids `any`, `terminal` and `superseding` as state, category or transition names — the three genuine collisions, rather than a blanket reservation.

### D69
**Operator associativity is never stated.** `:619` gives precedence only. `closes_at - grace - opened_at` types left-associatively and fails right-associatively, so check 24's verdict on a legal guard rests on a rule the document does not carry.

**Resolved.** §8.3 states left associativity, `implies` right, and comparison non-chaining.

### D70
**Header clause order is contradicted by adjacent examples.** `:551` writes `type Bug version 1 abstract`; `:552` writes `type BugInPlatform extends Bug version 1`. Whether `abstract` precedes the version, whether `extends` follows it, and whether marking order on `ref` and `state` is free are all unstated.

**Resolved.** §2 fixes the header as `type <Name> [extends <Base>] version <n> [abstract]` and makes marking order free elsewhere.

### D71
**The compilation unit is undefined.** `:28` calls a file a module and `:32` imports across modules. Eight checks need types from other modules. Whether publishing takes a module or a whole-model closure, and whether the capability and category vocabularies are module-scoped or global, is stated nowhere.

**Resolved by ADR-0060 decision 12.** Publishing takes a module and the closure of its `use` imports; capabilities and categories are one vocabulary across it.

### D72
**There is no syntax for an enum member literal.** `:605` gives the rule for a state literal and explains why a bare name is ambiguous; it gives no analogous rule for enums, and no example ever reads an enum-typed attribute. `severity == AESeverity.SEVERE` cannot be written from the document. Check 19 nonetheless verifies "enum member" references. This is the second-most-common guard shape after a capability test.

**Resolved by ADR-0060 decision 10.** An enum member is `<Enum>.<MEMBER>`.

### D73
**`.state` and `.category` are read in five guards and granted by no member rule.** `:605` gives a reference "`.id` and its declared members", and `state` is declared on no type. Yet `l.engagement.state` (`:73`), `r.state.category` (`:251`), `inputs.slot.state` (`:339`) and bare `state` (`:72`) all depend on it. `.category` yields a value whose type §8 does not list, so `r.state.category != closed` cannot be typed under `:611`.

**Resolved by ADR-0060 decisions 9 and 10.** §8.1 gives every object `.state`, of type `state`, carrying `.category` of type `category`.

### D74
**Three-valued evaluation is absent from §8.** The section claims the checker types every expression and never mentions absence, unknown, or that an unknown guard fails. A first-time reader wrote `derive serious = severity == SEVERE or ae_outcome == FATAL` where `ae_outcome` is absent early in the lifecycle, making a later `not serious` guard permanently unsatisfiable. No check catches it, and the rule exists only in `docs/DESIGN.md` §5.7 and one incidental clause at `:441`.

**Resolved by ADR-0060 decision 4.** §8.2 states absence, unknown propagation, the three exceptions, and the asymmetry that guards fail and invariants hold. The publish report names guards that can be unknown through an optional they never test.

### D75
**Duration units, currencies and decimal precision are given by example only.** `:619` shows `30 min`, `2 h`, `14 days` with no closed unit set, no singular/plural rule and no seconds or weeks. `money(ccy)` names no currency set. `:609` gives a scale rule for `+`/`-` on decimals and no precision rule.

**Resolved.** §8.3 closes the duration units at `s min h days weeks`, requires ISO 4217 currencies, and gives the decimal precision rule.

### D76
**`this.id` and `identity` arguments to an evaluator are unstated.** `:605` grants `.id` to every reference; `this` is not obviously a reference. `:497` passes a `string` to an evaluator, and whether `identity` is a legal argument type is not said. Passing an object to an external system is the ordinary case.

**Resolved.** §8.1 gives `this.id` and makes `identity` an ordinary scalar an evaluator may take.

### D77
**`referrers` has a heterogeneous element type on which `.state` is read.** `:534` defines it as every object holding a live reference, across types, and the guard at `:529` reads `r.state.category`. The element type is unstated, so check 24 cannot type the document's own deletion guard.

**Resolved.** §8.1 gives a `referrers` element `.id` and `.state` and nothing else, which types the deletion guard and no more. Filtering by type was open question 2, answered by ADR-0072 §1: an element gains a comparable `.type`.

### D78
**`counter` has no marking slot in the grammar and no stated relation to `attr`.** `:163` is bare `counter <name>`; the example at `:579` writes `counter on_hand indexed`. Whether a counter is an "attribute" for the purposes of checks 7, 8, 10, 17, 19 and 33 is never said, and one reading makes check 17 fire on the document's own `set reserved := reserved + inputs.qty` at `:590`. The partial checker special-cases counters, which is evidence the ambiguity is real.

**Resolved by ADR-0060 decision 8.** A counter is an attribute of type `int` and takes markings.

### D79
**`owner` has no marking slot, and nothing says whether the state and stored reference ends are indexed.** A type-scan invariant over a composed type reads its `owner`, and check 7 fails a type-scan invariant reading an unindexed attribute. There is no syntax to index an `owner`, so the invariant cannot be written and cannot be fixed.

**Resolved by ADR-0060 decision 9.** `.state` and stored relationship ends are always available to a filter and are never marked.

### D80
**A part declared in an abstract base cannot declare its cascades.** `:414` blesses an abstract `owner` as the way one part type serves two wholes, and §4.1 points users there. The base has no transitions, so a `cascade on` clause in it can name nothing, and nothing says a subtype may supply the clauses. Check 11 also fires wrongly on this shape, since `only via` then names a subtype rather than "its whole".

**Resolved by ADR-0060 decision 7.** A part in an abstract base carries no cascade clauses; each subtype supplies its own with the top-level `cascade <part> on …` form, and check 11 resolves the whole to each concrete declarer.

### D81
**`survives` is all-or-nothing where the need is per-transition.** A part may have to outlive one terminal transition and be disposed of by another: an adverse event follows a withdrawn trial subject but must be closed before the record is archived. `:189` and `:273` offer only the whole-part choice, so the constraint moves into a guard and the failure changes from an automatic cascade to a blocked transition someone must chase.

**Resolved by ADR-0060 decision 7.** `survives on { … }` names the terminal transitions a part outlives; the rest must still be covered.

### D82
**A cascade clause is repeated once per terminal transition.** Four identical lines per part in the document's own Delivery, and a reviewer's model reached sixteen. The repetition is mechanical and invites the omission check 37 exists to catch.

**Resolved by ADR-0060 decision 7.** `cascade on { a, b } to T.x limit n` takes a set of triggers.

### D83
**`terminal` and `closed` pull in opposite directions with no warning.** Check 40 forbids an `act` at a terminal state, so any object that must still accept recorded activity after it closes cannot mark its closing state terminal. A reviewer discovered this only after writing the lifecycle and had to insert an extra state and transition and re-issue every cascade clause.

**Resolved.** §4.2 states that `terminal` is not `closed`, and that an object still accepting recorded activity belongs in a closed non-terminal state.

### D84
**No personal attribute can be required, and no invariant can assert one is present.** `:181` forbids a required `personal` attribute because erasure writes absence. The consequence is that a consent signature and a subject's initials are both optionally absent in a model where they are legally mandatory, and the natural repair, an invariant asserting presence, would then be violated by erasure. Whether invariants are suspended during erasure is not stated.

**Resolved by ADR-0060 decision 5.** An erasure admits every invariant reading what it erased, and records the admission.

### D85
**`may admit` cannot name the invariant an assertion actually breaches.** Check 27 restricts admission to invariants the binding type declares. The invariant at risk is typically on the related type: asserting a subject back into an enrolled state breaches the site's enrolment cap. There is no admission path, so the assertion is simply blocked.

**Resolved by ADR-0060 decision 6.** `may admit <Type>.<invariant>` reaches invariants on types reachable by a declared inverse.

### D86
**`accepts` plus `default` silently resets a value on a non-creation transition.** `:183` expands a default into a creation only; `:441` says without restriction that an unsupplied accepted attribute takes the default. Under the second, a partial edit writes the default over a value someone set. No check covers it.

**Resolved by ADR-0060 decision 3.** A `default` applies at creation only.

### D87
**`visible when` has no typing, traversal or indexing rules.** `:550` shows one example, `:605` lists `actor.principal` with no type, `:182` says visibility consumes indexes, and check 7 does not cover visibility. Whether a predicate may traverse a relationship is unstated, so scoping a reader to their own site cannot be written with confidence.

**Resolved.** §6.7 types the predicate, allows one step of traversal over stored ends, requires indexed reads, and forbids evaluators, unindexed derivations and set-valued traversal. Check 7 now covers visibility.

### D88
**`sweepable` is reported by the publish report and defined nowhere.** `:681`. The word appears once in the document. The operational question behind it, finding the objects whose time-gated transition is now due, has no answer: `now` is forbidden in an invariant, a derive reading a clock cannot be indexed, and the store never initiates.

**Resolved.** The report line names **time-gated** transitions, meaning every guard that can currently fail is `temporal`, which is the list a consumer polls.

### D89
**The identifier rule contradicts itself on scope.** `:178` mints an identifier "at creation before the outcome runs" and permits a scope naming "only a reference the creation writes". The creation writes that reference in the outcome, which by the first clause has not run. The document's own `Robot.serial scoped by model` has this shape.

**Resolved.** §3.1 states that the mint happens after the creation's writes to the scoping reference and before every other outcome step.

### D90
**Mandatory `limit` produces invented numbers and an unactionable report.** A reviewer's model reached a reported worst-case fan-out around thirty-three thousand from limits they acknowledged inventing. Open question 4 already asks whether mandatory `limit` is worth its friction; this is the first evidence.

**Recorded as evidence for open question 4, and resolved by the ruling.** ADR-0071 kept mandatory bounds and dropped the reported worst-case product, which is the half this defect was about.

### D91
**Confidentiality is per-object, and the common need is per-attribute.** Hiding a treatment allocation from an investigator while the rest of the subject stays visible required splitting one string into its own type with its own lifecycle and four cascade clauses. The document does not name this cost.

**Recorded as open question 6, and refused by the ruling.** ADR-0072 §4 keeps confidentiality per object; the survey found no field-level restriction in the first consumer and no cost or margin column to protect. The cost of the workaround is recorded in `edge-cases.md`.

### D92
**An evaluator cannot return a value, and the common integration assigns one.** `:490` returns only a verdict. A randomisation service assigns the arm; the model must accept the arm from the caller and separately ask whether an allocation exists, so the store cannot check that the supplied value is the one the external system chose.

**Recorded as open question 8, and refused by the ruling.** ADR-0069 keeps evaluators verdict-only; an external system that assigns is a mirror, which is what the first consumer's own Xero design intent proposes.

### D93
**The §10 preamble's decidability claim is wrong in three ways.** `:635`. Check 23's rename clause is undecidable in principle, since a rename with no mapping is textually identical to a drop plus an add. The report list at `:681` does not contain the new-invariant scan the preamble places in it. "Which invariants compile to a database constraint on this backend" needs the backend configuration, a third external input the preamble does not mention.

**Resolved.** The §10 preamble now names the four external inputs correctly and claims decidability only from the closure.

### D94
**Fourteen checks are ambiguous, fire on the document's own examples, or cannot be built.** Checks 2, 7, 8, 10, 11, 12, 13, 15, 16, 17, 18, 19, 21, 22, 26, 33, 34, 35, 36 and 39 each need an invented decision; the audit gives one per check. Three fire on the document's own text: check 19 on `summary serial, model, state` at `:59`, check 13's "a name that never calls it" on `ChecklistItem.add` at `:144`, check 17 on the counter write at `:590`. Check 20 cannot be built at all: the document gives no inference procedure, and the only one in the repository rejects the document's own guard at `:589`.

**Resolved.** §10 was rewritten. Every check names the section defining its terms, the three that fired on the document's own examples no longer do, and check 20 no longer claims an inference that contradicts a declared class.

### D95
**Eleven terms are used by §10 and defined nowhere in §1–9.** traversal invariant, type-scan invariant, symmetric, "the three forms", family, analysable, actor guard, cascade argument, remedy-class inference, scope for duplicate names, and sweepable. Seven of them are defined in `docs/DESIGN.md` or an ADR, so the syntax document is not self-contained while presenting itself as the artefact a declaration author reads. One, `analysable` in check 36, is defined nowhere in the repository, including in the ADR that introduced it.

**Resolved by ADR-0060.** All eleven are now defined in the syntax document: the invariant forms and symmetry in §3.4, family in §2, actor guard in §4.2, remedy classes and their inference in §5.1, cascade edges and taint in §10, scope in check 33, and `analysable` replaced by a decidable statement about an abstract owner in check 36.

### D96
**Seventeen rules stated in §1–9 have no check.** Mandatory `tracking` (`:80`); the presence of a version on every declaration (`:50`); three stated clause orders (`:82`, `:407`, `:426`); the `extends` base being declared, abstract and acyclic; the identifier scope rule (`:178`); `unique in scope` presupposing a `scoped by`; what a `default` expression may read; "any larger expression containing an unsupplied optional input is a publish error" (`:466`), which names itself a publish error; `limit` on a cascade; `eager`/`deferred` appearing only on an evaluator guard; the `supersede` operand form; the rule that an indexed `derive` reads no clock (`:599`); the `visible when` predicate; a body-less `sum`; `<Binder>.<transition>` rather than `<Machine>.<transition>`; the remedy-class vocabulary, where a misspelling is silent; and `use` imports.


**Resolved.** Checks 42 to 50 close sixteen of the seventeen. The seventeenth, type-body clause order, is demoted to a readability convention, since a declaration with shuffled clauses is no less well defined.
## Found by the iteration-11 review

The same two reviewers re-read their own findings against the repair. Both returned blocking again. Nine of the sixteen entries below are defects the **repair itself introduced**, which is the useful result: rewriting three sections wholesale fixed the class of defect that prompted it and created a new crop in the examples the rewrite did not reach.

### D97
**The new indentation rule does not parse the document's own compressed bodies.** `:153` — `act tick at ACTIVE { require may: … because delegable` opens at indent 2 and its continuation `set checked := true }` sits at indent 25, so §9.1 reads the write as continuing the guard. Two sites, both introduced before iteration 11 and both missed by the rule written to replace the one that broke on them.

**Resolved.** A body that does not close on its opening line begins its clauses on the next line, which is the one layout §9.1's examples never showed. The two examples are reformatted.

### D98
**A sentence surviving the cascade rewrite contradicted the example six lines above it.** `:299` still read "which is why the delivery above repeats four" after the delivery was rewritten to a single grouped clause.

**Resolved.** Rewritten to describe the grouped form.

### D99
**A part with an abstract `owner` could be declared, given cascades, and never created.** Check 36 failed any transition writing an `owner` of abstract type; check 18 failed a part creation that never writes its `owner`. Between them no such part was constructible, while check 11, §3.2 and §4.2 were all rewritten in the same iteration to enable exactly that construct. ADR-0058's decision concerned re-parenting, which has a source whole; a creation has none.

**Resolved.** Check 36 applies to a re-parent, not to a creation.

### D100
**The worked example teaching the unknown trap used the enum syntax the same iteration made mandatory.** `:685` wrote `severity == SEVERE`, four times, while `:669` requires `<Enum>.<MEMBER>` and §9.2's resolution order does not reach enum members.

**Resolved.** Qualified in the example.

### D101
**Three stored relationship ends carried `indexed` after the same iteration said stored ends are never marked.** `:71`, `:72`, `:626`.

**Resolved.** Stripped, and check 7 now rejects the redundant marking rather than leaving it undecided.

### D102
**`survives on { … }` was justified by an example its own rules reject, and had no subtype form.** The justification, an adverse event outliving a subject's withdrawal, requires the withdrawal to be terminal, which check 15 then forbids the later archive from leaving. Separately, §3.2 gave a subtype form for `cascade` and none for `survives`, so an inherited part could be cascaded by a subtype and not survived by one.

**Resolved.** Justified instead by a delivery's audit photographs outliving its deletion, which is genuinely terminal, and the subtype `survives <part> …` form is added.

### D103
**Check 50 was filed among the checks needing the previous declaration.** `:766`. Both its clauses are decidable from the current closure.

**Resolved.**

### D104
**DESIGN.md contradicted the syntax document on the rule the unknown fix depends on.** DESIGN.md §5.7 said flatly "unknown propagates" while §8.2 gives the two Kleene exceptions, without which the prescribed presence test does not work. The verdict table also had no way to report a clause as unknown rather than false, which §8.2 promises.

**Resolved.** Both corrected in DESIGN.md.

### D105
**The unknown rule was stated for two of the four contexts.** Guards and invariants had answers; a `visible when` predicate and a derivation did not, and visibility is reachable with an unknown because `actor.principal` is optional on the descriptor.

**Resolved.** All four are now in one table: guards fail, invariants hold, visibility fails closed, a derivation reads as absent.

### D106
**Decimal accumulation could never publish.** `decimal(p1,s) + decimal(p2,s)` yielded `decimal(max+1, s)` and a `set` whose result did not fit the target's precision was a publish error, so `set total := total + inputs.amount` on a `decimal(8,2)` was rejected. Introduced in iteration 11 and hit on the first decimal counter a reviewer wrote. `int` and `money` were unaffected, so it bit exactly the type used for doses, weights and lab values.

**Resolved.** Addition keeps the wider precision, scale is the publish-time rule, and precision overflow refuses the request at runtime.

### D107
**The list of clauses taking bare comma-separated lists omitted four that the examples use.** `:750` named five; `states`, `only via`, `provides capability` and `requires capability` also take them, and `may admit` was undecidable.

**Resolved.**

### D108
**The literal list omitted numbers.** `:669` enumerated durations, money, strings, booleans and collections, and not the bare integers and decimals used throughout.

**Resolved**, with the rule that a bare number without a point is an `int`.

### D109
**Check 33 put transitions in one namespace with attributes.** A machine declaring `requires attr answer` and `do answer` — the natural shape for a review flow — was a duplicate.

**Resolved.** The scopes are separate per kind.

### D110
**Whether a cascade skips a part whose guard fails was answered differently in two documents.** §3.2 said a cascade drives its transition "on each part for which it is available", which reads as a guard test; ADR-0038 and DESIGN.md say any failure aborts the request. Silent skipping is the failure mode the model forbids by name.

**Resolved.** The terminal-state skip is the only skip; a failing guard aborts. The related finding, that a cascade cannot pass arguments and a reviewer declared three transitions solely to work around it, is recorded as open question 7.

### D111
**Nine checks retained gaps after the §10 rewrite.** Check 8 named a write with no syntax; check 19 cited a resolution order that covers only expression positions; check 21 needed a grammar that sixteen forms did not have; check 24 left multiplication of two scalars, integer division and division by zero untyped; check 30 named a mechanism that does not match the one §3.2 describes; check 39 defined reaching over the relationship graph, which inverts it; check 42's version clause did not say which forms are versioned; check 44 said "the creation" for a type that may have several; and check 29 used "capability guard" for what §4.2 defines as an actor guard.

**Resolved.** All nine reworded, and §9.4 gains productions for the sixteen forms that had none.

### D112
**A claimed check clause was not demonstrated by any fixture.** Check 42's version clause was in the table and not in the checker, and the fixture mechanism allowed only one fixture per check, so a multi-clause check could not prove more than one clause.

**Resolved.** A check may now carry several fixtures, the version clause is implemented, and the self-test reports fixtures rather than checks so the difference is visible.

### D113
**The new-invariant scan was named in the §10 preamble and absent from the report list.** Reported against iteration 10 and survived the rewrite of §10 verbatim.

**Resolved.** The report now lists how many live objects a new invariant would violate.

## Found by modelling a payments ledger

The third review, on the first domain that is high-volume, short-lived and money-carrying. Two of the findings are the prose and the checker disagreeing about the same construct, which is a worse state than either being wrong alone.

### D114
**The abstract-base part pattern had no spelling that publishes.** §3.2 and §4.2 both recommend an abstract `owner` for a part serving two wholes, and the checker rejected both spellings: `only via <Subtype>.<transition>` failed check 11 as "not its whole", and `only via <Base>.<transition>` failed check 13 because an abstract base declares no transitions. Eighteen of the reviewer's twenty-two findings were this. Separately, the top-level `cascade <part> on …` form was not counted as a call site although §3.2 says a cascade clause is one, so the part's disposal transition looked unreachable.

**Resolved.** Check 11 resolves an abstract whole to any concrete type in its family; the top-level cascade clause is parsed and counted. A worked example is now in §3.2, so the checker regression-tests the pattern on every run.

### D115
**A wrapped `provides capability` line was read as declaring only its first line.** Four spurious check-16 findings on a legal declaration. The parser accumulated continuation lines for relationships and not for `provides`.

**Resolved.** It follows §9.1 like everything else.

### D116
**`money` had no scale and no rounding rule.** §8.3 stated that `int / int` truncates *because* money is the trap it would create, and then left money's own arithmetic undefined: nothing said that `money(JPY)` has no minor unit, or what `money(USD) * decimal(5,4)` does with a fractional cent.

**Resolved.** The scale is the currency's minor unit; multiplication and division by a scalar round half to even, stated rather than deferred to a backend. Exact allocation of an amount into parts is named as not expressible.

### D117
**`money` fixes its currency at declaration, so a multi-currency ledger cannot be typed.** The alternatives are one attribute per currency, or a `decimal` beside a currency string, which discards every check the type exists to provide.

**Recorded as open question 9, and refused by the ruling.** ADR-0068 keeps the currency in the declaration, so a mismatch is a publish error; a multi-currency model declares an account per currency.

### D118
**Idempotency, an advertised runtime capability, had no declarable form.** `unique` was global, sequence-scoped or partial, with no compound key. Written as a type-scan, nothing said whether the scanned set includes the object being written; read the obvious way it matched itself, so no object could ever be created. Adding `p.id != this.id` fixed that and violated check 6, since an inequality is not one of the symmetric shapes.

**Resolved.** A type-scan ranges over every *other* object of the type, so no exclusion clause is needed or permitted, and `unique with <attr>, …` gives the compound form directly.

### D119
**The negative of an external verdict did not type.** §8.3 prescribes writing a conditional external check as two transitions with opposite guards, and a verdict was usable only as a whole guard clause, so the second transition could not be written. A decline could be gated on nobody having asked the authority, never on the authority having said no.

**Resolved.** A guard clause may be a negated evaluator call. A stale verdict is still refused as `temporal`, since `not stale` is not `satisfied`.

### D120
**A required reference or singular part was unchecked at creation.** Check 8 covered attributes only, so a type could declare a mandatory `ref` and a creation that never wrote it.

**Resolved.** Check 8 covers all three.

### D121
**A `ref` with only one end declared had no stored-end rule.** §3.3's table had rows for one-singular-one-set, two-singular and two-set, and none for the common case of a reference with no `inverse`, which §3.2 explicitly permits.

**Resolved.** That end stores it and must be singular; a set-valued reference with no inverse is rejected.

### D122
**Whether a family member satisfies a base-typed input was never stated.** A machine shared by two types passes `this` to something declared over their common base, which is the ordinary shape and rested on an assumption.

**Resolved.** Stated in §8.1, in one direction only.

### D123
**The cascade skip rule did not say whether it governs a `call`.** "Skips a part already in a terminal state … that is the only skip" left an erasure's `call c.forget(…)` ambiguous, and under one reading an erasure silently left personal data in place while check 39 still passed.

**Resolved.** The skip belongs to a `cascade` clause alone; a `call` never skips.

### D124
**"Terminal transition" was never defined**, though the cascade coverage rule turns on it. A type whose objects are born final has creations into a terminal state and no `do` at all, and its wholes could not tell whether coverage applied.

**Resolved.** A terminal transition is a `do` whose to-state is terminal; a creation is not one.

### D125
**A sequence could be scoped only by a reference.** A tenant identifier mirrored from another system is an indexed attribute, so a per-tenant reference number required inventing a type solely to be scoped by.

**Resolved.** A scope names a reference or an indexed attribute the creation writes.

### D126
**`tracking` was not inherited and an abstract type had to declare it.** So a family stated it twice with nothing checking that the two agreed, on a type that has no objects.

**Resolved.** It is inherited like everything else, and an abstract type needs none.

### D127
**No remedy class distinguished a window that has not opened from one that has closed.** Both are a comparison against `now`, both inferred `temporal`, and the publish report lists a transition as time-gated on that basis. A chargeback window that had expired would sit on a consumer's polling list forever.

**Resolved.** `temporal` is defined as "wait, and it will pass"; a closed window is `unreachable_from_here`; no inference can separate them, so the class must be declared on any deadline guard, and the time-gated report uses the declared class only.

### D128
**A single two-valued branch multiplied into four transitions.** Debit and credit differ by a sign, and with `if` confined to derivations the branch split the writing transition, then the transition that called it, then every `only via` list naming them.

**Resolved.** `if` is allowed in the value expression of an outcome step, and still not in a guard, where a conditional would hide which clause failed.

### D129
**A money balance cannot be a counter.** DESIGN.md offers a counter and a maintaining cascade as the remedy when an invariant's scan is too slow, and a counter is a non-negative `int`, so the remedy is closed to every quantity that is money.

**Recorded as open question 10, and deferred by the ruling.** ADR-0072 §5 found the counter mechanism unexercised by the first consumer, which stores no stock level at all. Non-negativity did move out of `counter` and into an invariant.

### D130
**`only via` on a shared child scales as binders times triggers.** Eight parents for one disposal transition, duplicated in each subtype's cascade clauses, with check 13 making both directions an error so three lists must agree exactly.

**Recorded as open question 11, and agreed but not built.** ADR-0072 §6: no family in the first consumer needs it, and when it is built the family must be named explicitly.

### D131
**`serial` and `quantity` fit neither a payment, a posting nor an account.** Each declares `serial` because the alternative demands a counter, so the declaration states something untrue about the domain.

**Recorded as open question 12, and resolved by the ruling.** ADR-0067 adds a third tracking mode, `record`, amending ADR-0050. Twelve or more of the first consumer's entities are records.

### D132
**Whether an `enum` body may wrap was unclear**, since §9.3's list of comma-taking clauses omitted it and check 51 now makes a wrong guess a publish error.

**Resolved.** Listed, and it wraps by §9.1 like everything else.

### D133
**The prohibition on marking a stored end `indexed` lived only in §8.1 and a check.** §3.1's marking table and §3.2's grammar both still invited it, and the document's own examples carried it until iteration 12.

**Resolved.** Stated in the marking table where a reader meets it, and check 7 rejects it rather than leaving it undecided.

## Found by the iteration-13 review

Three reviewers re-read their own findings against the repair. Two returned non-blocking. Almost every remaining defect is one shape, which the decidability audit named: **a rule changed in §1 to §9 with its check in §10 left alone.** It had happened four times across three iterations, and nothing linked the two halves.

### D134
**Check 21 rejected the construct §8.3 had just been rewritten to permit.** `if` in an outcome value expression was argued for at length in the body and still listed as an error in the check list. Following the document produced a declaration that does not publish.

**Resolved**, and the class of defect is now checked: see D142.

### D135
**The symmetric-shape whitelist excluded the commonest type-scan there is.** "Overlap of two ranges, equality on a shared key, and conjunctions of those" left out "at most one open X per Y", which DESIGN.md names as canonical. Three documents gave three answers, and check 5 reports which form it decided only after a publish.

**Resolved.** Symmetry is decided by a **swap test** — exchange the two objects and require the same predicate back — and the named shapes become examples rather than the whitelist. The consequence worth having is that a status filter must be applied to both objects, and the test names the member that appears on one side only.

### D136
**The booking case study still carried the `id` exclusion the same day's rule made an error.** `docs/design/case-study-approvals-and-bookings.md:94`, the exact example ADR-0052's symmetry rule was derived from, unedited by ADR-0061.

**Resolved.** Rewritten to the swap-symmetric form with the filter on both sides.

### D137
**Check 8's new singular-`part` clause was unsatisfiable.** It required a creation to write a required singular part while check 17 forbids writing a `part` end, which is derived. Separately a machine could not require one at all, since `requires part` was set-valued only, so a binder needing a part from birth had no legal declaration.

**Resolved.** A creation **fills** a part with a `create` step for the child, which is how the model already worked and was nowhere stated, and `requires part` admits a singular type.

### D138
**A type-scan `count` silently changed meaning.** Self-exclusion is right for uniqueness and wrong for cardinality: `count(v in Version where …) <= 10` now counts every other object and permits eleven. The prohibition on the old exclusion idiom had no check behind it.

**Resolved.** The counting rule is stated with a worked example, a traversal is recommended for any cardinality bound over related objects, and check 6 covers the prohibition.

### D139
**`tracking` inheritance was stated twice and implemented nowhere.** The body said a family states it once on the base and check 42 said "inherited or declared"; the checker walked no base chain, so a family written the way the document prescribes failed to publish, naming a clause the author had satisfied.

**Resolved.** The checker walks the chain, and the §3.2 worked example now inherits its tracking, so the document's own text is the regression test.

### D140
**A wrapped `enum` body was legal by §9.3 and rejected by check 51.** §9.3 listed it among the wrapping forms in the same iteration that check 51 began rejecting a brace line carrying a clause.

**Resolved.** An enum body is a braced list rather than a block of clauses, so §9.1's layout rule does not reach it, in the document and in the checker.

### D141
**The terminal-transition gloss reasoned from the wrong type.** It concluded that a born-final part asks nothing of its wholes, when the coverage rule is about the whole's terminal transitions. A journal of posted entries has a terminal transition, no cascade is available because the child has no such transition, so `survives` is forced — and the text called that a modelling error.

**Resolved.** Named as the one shape where `survives` is the only correct answer, with the journal, statement and audit-log cases given.

### D142
**Four defects across three iterations were a rule changed without its check, and nothing linked the two.** Twelve normative statements in §1 to §9 cited no check at all.

**Resolved by check 52**, the first check on the document rather than on a declaration: a statement that something is rejected or is a publish error must cite the check enforcing it. Twelve citations added, and the check was confirmed by removing one and watching it fire.

### D143
**Five smaller gaps from the same audit.** `/` did not say which side takes the scalar, so `int / money` had two answers; a bare decimal literal's scale was ambiguous in a comparison; check 44 did not cover the attribute scope §3.1 had just permitted; whether a type-scan **guard** excludes the object being transitioned was undefined, which changes runtime meaning rather than publish behaviour; and a `call` through an absent optional part had no rule, which an erasure needs.

**Resolved**, each in the section that defines it.

### D144
**A decimal product could never be assigned.** Multiplication widened the scale and a scale mismatch on `set` was a publish error, with no rounding or cast, so no decimal product was writable to a decimal attribute.

**Resolved.** A product or quotient rounds half to even to the target's scale, as money does; addition of mismatched scales stays an error, so rounding is permitted exactly where it is unavoidable.

### D145
**Time-gated was a static report line with a runtime condition.** "Every guard that can currently fail" cannot be evaluated at publish, and read literally no transition carrying an actor guard could ever appear, which is nearly all of them.

**Resolved.** Computed from the declaration: at least one guard declared `temporal` and none declared `unreachable_from_here`.

### D146
**A cascade's target was checked for existence and not for from-state coverage.** Since a failing cascade now aborts the whole request, a mis-specified one is a production abort rather than a publish error, and the coverage is decidable from the text.

**Resolved as a publish report.** Iteration 15 found the condition undecidable as first written — see D154 — so it names the uncovered states without failing.

### D147
**The abstract-base worked example had one concrete whole.** It demonstrated the mechanism the checker verifies rather than the case the paragraph is about, a part serving *two* wholes with different transition names.

**Resolved.** A second member with its own transition names and its own cascade clause.

### D148
**Two wording residues.** Check 11 said the whole "declares" the part where the example inherits it, and open question 8's rationale had been left attached to question 12 when the two were split.

**Resolved.**

## Found by the iteration-14 review

Two of three reviewers non-blocking; the payments declaration that could not be written four rounds ago now publishes clean. The round's result is that **the instrument added in iteration 14 was itself the least-checked thing in the document**, and both reviewers went at it directly.

### D149
**The swap test rejected the two shapes it claimed to accept.** §3.4 defined symmetry as the swapped predicate being "the same up to the order of its conjuncts", and asserted that overlap and shared-key equality pass. Swapping `b.start < end` yields `start < b.end`, which is the same comparison only once direction is discounted, and swapping `b.resource == resource` reverses its operands. Under the stated relation both fail, including §3.4's own worked example. The relation was therefore either too weak, rejecting what it blessed, or it meant semantic equivalence, which is unbounded and gives an implementer nothing.

**Resolved.** Four normalisations are named — conjuncts as a set, `==`/`!=` modulo operand order, the four inequalities modulo direction, and a member of `this` modulo its spelling — and nothing else is normalised, so the test is a decision procedure rather than an equivalence to argue about.

### D150
**Check 52 claimed to catch the defects it was built for, and catches one of four.** A reviewer ran the implemented predicate over the four historical rule-and-check divergences: it fires on one, misses two for want of a trigger phrase, and cannot see the fourth at all, since that was a contradiction between two checks rather than between a rule and a check. The two it misses are the two that produced blocking defects.

**Resolved by retraction in the document**, not by softening. Check 52's row now states what it does — enforce the discipline going forward for the phrasings it recognises — and names its two mechanical bounds: one citation shields every statement within 260 characters, and the phrasing list is fixed.

### D151
**Check 52's detector recognised seven phrasings and missed seventeen normative statements in this document.** Rules stated as "publishing enforces", "is mandatory", "may not be", "must reach", "is not allowed" and "are publish errors" were invisible to it, so the document did not pass the check it had just introduced.

**Resolved.** The detector covers twenty phrasings, runtime refusals are excluded as not being publish-time rules, and all seventeen statements now cite their check.

### D152
**Two normative rules had no check to cite.** A `visible when` predicate may not call an evaluator, read an unindexed derivation or traverse a set-valued end; and a state, category or transition may not be named `any`, `terminal` or `superseding`. Both were stated as rules and enforced by nothing.

**Resolved.** The first is check 7, the second check 33. This is check 52's most useful result: run properly, the first thing it found was rules that were never enforced at all.

### D153
**A scale mismatch on a write cited a check about expressions that type.** The expression types perfectly well; it is the assignment that mismatches, and no check named assignment compatibility. The rule was load-bearing, having repaired D106.

**Resolved.** Check 17 covers a write whose value does not fit its target, and the citation points there.

### D154
**The cascade-coverage check's verdict on a legitimate cascade could not be predicted.** "Every non-terminal state its part can be in when the trigger fires" was carrying the decision and was undefined. The cheap reading rejects a correct model; the expensive one requires propagating a guard on a different transition two states earlier, which is not decidable from the text.

**Resolved.** The strict form, reported without failing, naming the uncovered states. It moves out of the check table into the report, since a report is what an undecidable-in-general risk deserves.

### D155
**The design document's outcome grammar had gone stale in three of six lines**, still showing forms the syntax document had replaced, and its straight-line rule contradicted the syntax document's conditional outcome value.

**Resolved.** The syntax document owns the grammar and the design document says so; the straight-line rule now distinguishes a conditional *step*, which is forbidden, from a conditional *value*, which is not.

### D156
**Check 8's two clauses used different quantifiers, and the weaker one was silent.** A required attribute was checked per creation and a required singular part per type, so a binder whose machine-supplied creation did not fill its part published clean and failed soft at runtime.

**Resolved.** Both are per creation, and §2.1 now names references and parts alongside attributes as things a machine-supplied creation cannot provide.

### D157
**Two paragraphs each claimed to state the only skip rule and contradicted each other on `call`.** Both cited a check, so check 52 passed them: the defect was disagreement between two statements, which no citation rule detects.

**Resolved.** One is a skip on account of state and belongs to a cascade; the other is a skip because a path does not resolve. Each now names the other.

### D158
**A clause left behind by the swap-test rewrite gave a false reason.** It said the `id` exclusion "is not one of the symmetric shapes below" when there are no longer shapes below and the clause passes the swap test.

**Resolved.** The ground is redundancy, which is what check 6 rejects it on.

## Found by the iteration-15 and iteration-17 reviews

All three reviewers non-blocking, and one wrote "the document is ready for the author". These are their residuals.

### D159
**A binder's own `create` did not replace the machine's, so the document's own remedy could not be spelled.** §2.1 offered three ways for a binder to hold a required attribute, reference or part that a machine-supplied creation cannot provide: make it optional, give it a `default`, or declare its own `create`. The third does not work, because a binder's transitions are additional, so the machine's generic creation remains available and still produces an object without the required thing. The document's named example, a card payment obliged to hold its card details from birth, had no legal spelling.

**Resolved.** A `create` is the exception to the additive rule: a binder's creations replace the machine's. Birth is where a type's obligations are established, and a type that says how it is born says so completely.

### D160
**Check 52's proximity window gave a live false pass.** A citation 219 characters before an uncited rule satisfied the check, and the citation named a different check than the one enforcing it, so a reader following it was sent to the wrong place.

**Resolved.** The citation must be in the same sentence as the statement. Scoping it that way immediately found four more uncited statements, including the one the window had been shielding.

### D161
**A check widened to satisfy a citation came out forbidding more than its rule did.** §9.2 says a state or category may not be named `any`, `terminal` or `superseding`, and a transition may not be named `any` — only `any`, for a stated reason. Check 33 flattened the three bullets into one clause and so rejected `do terminal S -> D`, which §9.2 permits.

**Resolved.** This is the rule-and-check divergence class arriving inside the repair built for it, and it is invisible to check 52 in both directions: the check cites its rule and would pass any presence test. It was caught the way all four earlier instances were, by reading the two texts side by side.

### D162
**The overlap illustration defending the normalisation table was wrong on its own terms.** It said swapping `b.start < end` gives the same comparison once direction is discounted. It does not: the swapped conjunct matches the *other* original conjunct, not the one it came from.

**Resolved.** The corrected illustration shows both normalisations doing work, which is what the sentence was there to argue.

### D163
**Check 8's two clauses still stated their quantifier with different precision**, the attribute clause saying "a creation never writes" where the part clause had been tightened to "every creation".

**Resolved**, and the part clause is now implemented: a creation that does not fill a required singular part is reported, per creation, which was the silent gap D156 named and did not close in the tool.

### D164
**Two smaller items.** `.state` is described as sitting "alongside" an object's declared members while the swap test and the resolution order both need it to count as one; and a dangling reference to the cascade-coverage check survived its demotion to a report, in a file check 52 does not read.

**Resolved.**

### D165
**The creation-replacement rule broke its own motivating example.** Replacing a machine's creation orphans the state that creation reached, which is the ordinary shape of an initial state, so check 15 rejected the card payment ADR-0064 was written to make expressible. Reachability is checked per binder, and before the rule every binder inherited the creation, so the case could not previously arise.

**Resolved.** A state only a replaced creation reached is reported, not failed. For that binder it is a state the type carries and can never occupy, which is worth knowing and is not an error; failing would make a machine unshareable by anyone needing to be born in a different state.

### D166
**Two checks were not updated alongside the two that were.** Check 33's transition scope still counted machine transitions the binder no longer has, so reusing the replaced creation's name — the natural way to write a replacement — tripped the duplicate rule. Check 44 said "the type's creations" without saying which set, where check 8 had been made explicit.

**Resolved.** Both carry the clause check 8 got, and check 13 now reports a replaced parent in those words rather than as one that does not exist.

### D167
**Inserting an open question renumbered four others and invalidated six references** across the ADRs and the defect register, which is the lesson about scoping a fix already recorded in `docs/LESSONS.md` arriving in the fix for something else.

**Resolved.** The numbering is restored, the new question is appended, and §11 says the questions are addressed by number so a new one goes at the end.

### D168
**A rename invented a new word for an established concept and redefined it.** D88 reported that the publish report named `sweepable` and defined it nowhere in the syntax document. The repair renamed it to *time-gated* and defined that instead. But `sweepable` is ADR-0048's term, it appears in thirteen files, and it names a different property: guards decomposing into an indexable prefilter, which is what makes `available` able to find objects at all. *Time-gated* named only the temporal-guard subset. So the syntax document reported a different thing under a different name than the rest of the record, and the correct repair for "defined nowhere here" was to define it, not to replace it.

**Resolved.** The report names `sweepable` with ADR-0048's definition, and the temporal-guard refinement is stated as which of the sweepable transitions are worth polling, which is the part that keeps a closed window off a scheduler.

## Found by pointing the checker at the other documents

### D169
**Twenty-five findings across the five older case studies and the walkthrough, none of which had ever been checked.** The payments study was born in the syntax and was checked from its first commit. The checker was hardcoded to the specification, so every other document's declarations were verified by reading alone. Pointed at them it reported 2 to 8 findings each.

The cause is worse than drift. Those blocks were never written in the declaration syntax at all: they use a pre-syntax pseudo-notation with `guards:` and `outcome:` labels, a unicode arrow, bracketed remedy classes, `for x in y:` with no bound, and a bare `<path>.<transition>(…)` where `call` is now required. The "re-express the case studies in the new grammar" pass of September rewrote the tables and the prose and left the code blocks in the old notation, which is the same scoping failure `docs/LESSONS.md` records — and it survived because nothing could see them.

**Resolved.** The five that needed it are rewritten in the current syntax, and all six documents are clean. The checker takes a path, and `scripts/check-corpus.py` runs it over every design document.

### D170
**`DESIGN.md` promised a construct the language did not have.** §5.4 said "clearing a value deliberately is a separate input or a separate action". An unsupplied optional input skips its write rather than clearing, there is no assignable `null`, and iteration 15 had removed a check-8 clause for naming exactly this unwritable write. Six iterations passed without notice because no example needed it.

`Bug.reopen` in the ticket study needs it: a bug moved back to in-progress that still reads `resolution = FIXED` is wrong in the way the model exists to prevent. It had been written as `resolution := null` in the unchecked notation, which is how the gap survived.

**Resolved by ADR-0073.** A `clear` step writes absence to an optional attribute, and is a write like any other so that it invalidates an approval that read the attribute.

## Found by the corpus coherence review

Four parallel audits on 2026-09-08 after the author's rulings: the decision set against itself, the model against the language, the case studies against the model, and the status documents against the filesystem. Sixty-one findings.

### D171
**A scripted edit put four answer paragraphs in the wrong lists.** The script that recorded a recommendation under each open question matched a numbered item anywhere in the document. One landed inside design goal 1, which lost its explanation; three split the five-step name-resolution order that check 19 depends on. Every line stayed valid, both checkers reported clean, and it survived four commits.

**Resolved.** Removed, and the corpus checker verifies where an answer paragraph may be. It deliberately does not attempt the general case: an inserted indented paragraph between two list items is structurally identical to a legitimate continuation. The real correction is in `docs/LESSONS.md` — a scripted edit must assert where it matched.

### D172
**Fifty refinement links were one-way, over twenty-six records.** The corpus checker verified `Amends` and not `Refines` or `Supersedes`, so a reader of ADR-0052 learned nothing of the five later decisions that refine it, including the one that superseded its symmetric-shape whitelist. The decision index had also stopped recording refinements after ADR-0059.

**Resolved.** Every back-link is generated from the refining record's own title; thirty-seven index rows say what refined them; the checker covers all three relations.

### D173
**Five decisions were reversed on one side only.** A ruling moved a counter's non-negativity into an invariant while the pending decision still said counters are never negative. ADR-0052's whitelist was superseded in substance by two later decisions and carried nothing. ADR-0034 rejected consumer-managed cursors and ADR-0043 substantially adopted them. ADR-0023's verdict taxonomy predates two additions. ADR-0028 said in one paragraph that a duplicate is not superseded and in the next that merge is the same mechanism.

**Resolved.** Each annotated where the reader will meet it.

### D174
**Six of thirty-two rules stated in both the model document and the specification had drifted to the superseded version.** Whether a bound machine is whole, what a composition's cascade covers, how symmetry is decided, where a conditional may appear, whether a reference is an attribute type, and whether a bound is optional. Two further claims were false about the repository itself: that the declaration syntax does not exist, and that a payments case study is still needed.

**Resolved**, and the division of ownership is now stated at the top of the model document, since assuming it has failed twice.

### D175
**The defect index listed 62 of 170 entries.** The register's most-skimmed artefact was 64 per cent incomplete, which is exactly what `docs/LESSONS.md` predicts of a summary maintained beside its body.

**Resolved.** Generated from the entries, with a corpus check that fails when one is missing.

### D176
**Most counts in the status documents were wrong.** The register held 47 entries in one place and 170 in another, the readiness review was 42 defects and found 41, the syntax was eleven iterations and is eighteen, five case studies were re-expressed and it was four and the walkthrough, and the thirteen rulings tallied as 5/3/1/4 and are 5/4/1/1/1/1. Three completed tasks were still open, including the licence, which is Apache-2.0 in the tree and undecided in two records.

**Resolved.** Each verified by counting rather than reading.

### D177
**Seven register entries read "Not resolved" after the author had ruled on the question each was carried to**, and the README's claim that all 170 findings were resolved rested on them.

**Resolved.** Each says what the ruling was.

### D178
**The mediated guarantee named one exception and there are two.** The README and the model document both said the only way to set state without satisfying a guard is a declared assertion; import and migration use a built-in one, gated on a deployment capability. This is the absolute-claim lesson recurring in the sentence that lesson was written about.

**Resolved.** Both carve-outs are named in the same sentence as the guarantee.

### D179
**Nine case-study statements described the model as it no longer is**, of which one was factually false about the production system and two would have produced a declaration that does not publish. The catalogue of edge cases had gone stale in both directions and was missing eight publish refusals added since it was last touched.

**Resolved.** Listed in the commit; the audit's citations are the record.

### D180
**Two pairs of documents contradicted each other on substance.** Whether a contended row queues or retries, which ADR-0039 itself answered both ways; and whether an input may be a set of objects, which the specification's own assert example uses and its grammar line omitted.

**Resolved.** The decision rule wins over its own consequence in the first; the grammar gains the form in the second, and the real limit is restated as a set of anonymous structures, which no input can be.

## Found by surveying the record for storage requirements

A sweep of DESIGN.md and all 73 decision records for what they oblige storage to do, run while the schema was being written and reported after it was committed.

### D181
**The sequence table would have made sequences gapless, which a decision rejected.** `ok_sequence` was a row updated inside the request's transaction. A rolled-back creation would then roll the counter back too, so no gap would appear — and ADR-0029 requires the gap, because a gapless sequence serialises every creation in its scope, which is the reason it was rejected as the default.

**Resolved.** The allocation is its own transaction on its own connection, committed before the request continues, so it survives a rollback. On PostgreSQL a native `SEQUENCE` already behaves this way; on SQLite it is a second connection *(to a separate database file — a second connection to the store's own file blocks behind the request's write lock; D187, ADR-0076)*. It is now the one thing in the schema written outside the request's transaction, and the document says so.

### D182
**Four tables were missing a column a decision requires.** No `superseded_by`, so `get(id, follow)` had nothing to walk; no discharge on an admission, though ADR-0054 discharges one automatically the first time the invariant holds again and records it; a proposal kept neither the verdict that left it pending nor the version it was submitted under; and the subscription states were invented rather than taken from ADR-0043's `active → revoked`.

**Resolved.**

### D183
**A compiled constraint cannot yield to an admission, and nothing said so.** An admission suppresses one invariant for one object. A database constraint has no per-row exemption, so an invariant an assertion may admit must not be compiled, or the database refuses what the runtime allowed. The design record is silent on this; the schema now states it as a constraint on which invariants publishing compiles.

**Resolved** in the schema, and worth an ADR if the author agrees with the reasoning.

### D184
**Three rules about absence and identity were unstated.** That absence is SQL `NULL` and never a sentinel, which is why uniqueness over a personal or external attribute is partial. That an event's position is its identity as well as its order, so nothing else needs an event id. And that erasure does not touch the per-attribute write index, since redacting a value does not change which event last wrote it — which follows from what the index means and is nowhere written.

**Resolved.**

### D185
**The blob store's lifecycle expiry must be disabled and the schema did not say so.** The log is permanent and a file reference outlives any expiry policy, so a bucket rule deleting after ninety days silently breaks history (ADR-0017).

**Resolved.**

### D186
**Four routes to a writable mirror.** ADR-0075 added the `mirror` marking and check 53 to keep such a type read-only, and the first version of the check guarded only outcomes and compositions. Probing found four more: a mirror binding a machine, which hands it every transition that machine supplies and defeats the marking completely; a mirror declaring transitions of its own; a mirror extending a type; and an ordinary type extending a mirror, which lets a type this store owns inherit the shape of one it does not.

**Resolved.** All four are refused by check 53, each confirmed by probe, and the specification carries a worked mirror example so the checker verifies the pattern on every run rather than the prose merely describing it.

## Found by the whole-record review of 2026-09-09

A single-pass read of the whole record by one reviewer — the model, all 75 decisions, the seven design documents, the five case studies, the walkthrough, the edge cases and the status files — after the six documents of 2026-09-09 were written, with three claims probed by running something rather than reading. The findings cluster where `docs/LESSONS.md` predicts: at the seams between the newest documents and the model. Entries are ordered by consequence. D187 and D188 were repaired the same day by ADR-0076; the other twelve are **open**, each says what would close it, and the three that need the author's ruling say so.

### D187
**On SQLite the sequence mint deadlocks against the request that needs it.** `storage-schema.md` §6 and §11 allocate a sequence value "in its OWN transaction on its own connection" and say "on SQLite it is a second connection". SQLite permits one writer per database file. The mint happens inside the request — `declaration-syntax.md` §3.1: "after those writes and before every other outcome step" — and the event's position is allocated when its transition begins applying (ADR-0046 §5), which on this schema is an insert into `ok_event`; so, by those two statements, the request's connection has written before the mint and holds the write lock when the second connection tries to update `ok_sequence`. **Observed**, SQLite 3.37.2 in WAL mode: an update on a second connection succeeds while the first connection has only read, fails with `database is locked` after the busy timeout once the first has written one row, and succeeds against a separate database file. The document's "what is verified" paragraph names the DDL, a cascade round trip and a refused state value; this claim was reasoned, not run.

**Resolved by ADR-0076.** On SQLite `ok_sequence` lives in a separate database file beside the store, opened on its own connection; on PostgreSQL the second connection opens the same database, since it locks rows. `storage-schema.md` §6 and §11 say so, its verified paragraph names the probe, and `scripts/check-schema-doc.py` runs it on every corpus run in both journal modes: the same-file connection fails with `database is locked`, the separate-file one succeeds, and its allocation survives the request's rollback. ADR-0029 rule 1, which still read "inside the creating transaction", is annotated. Related: D181, whose fix this corrects.

### D188
**A retry that carries both its idempotency key and its `expected_version` is refused as `stale` instead of being replayed.** `DESIGN.md` §6 orders the request: step 2 "If an `expected_version` is given and differs, refuse with `stale`", step 3 "If the idempotency key has been applied before, return the original result, marked as a replay". A caller that read version 7, requested with `expected_version = 7` and a key, timed out after the commit raised the version to 8, and retries the identical request meets step 2 first and gets `stale`. That is the case ADR-0041 §2 was written to make harmless, and the case the harness's retrier (`adversarial-harness.md` §2) is scripted to test. ADR-0023 rule 5 requires only that `stale` precede the guards, which a replay check before it still satisfies.

**Resolved by ADR-0076.** `DESIGN.md` §6 now runs visibility, replay, version, guards; a replay returns the recorded verdict whatever `expected_version` the retry carries, and `stale` is possible only for a request that has not been applied. A creation skips steps 1 and 3. `library-api.md` §3 says the key wins, and ADR-0023 and ADR-0041 carry the back-link.

### D189
**The idempotency key is scoped to `principal`, which is optional; and two documents disagree on whether that scope was decided.** `storage-schema.md` §6 declares `ok_idempotency` with `principal TEXT NOT NULL` and `PRIMARY KEY (principal, key)`, and §11 lists "An idempotency key is scoped to the principal" under Decided. `library-api.md` §3 has `principal: str | None`, and ADR-0025 makes it "optional: the party on whose behalf an agent or service acts"; a human acting for themselves has none, so a human's keyed request has no row it can write. `adversarial-harness.md` §2, written the same day, says the retrier tests "the boundary of the key's scope, which the record leaves undecided — so the harness is where that decision gets its evidence".

**Open.** Scope the key to the actor's `id`, or to `principal` falling back to `id`, and say which in both documents; the harness paragraph then names the decision it tests. The first consumer scopes per principal because its principals are never absent (ADR-0014, Evidence).

### D190
**Erasure does not reach three places a personal value can rest.** `storage-schema.md` §9 lists what erasure does: the object row, past event payloads, the file content, and an `erased` event. It does not touch `ok_legacy_entry.payload`, though ADR-0015 says the legacy audit log "carries before/after values, principal, source and IP address" and `DESIGN.md` §11 attaches those entries to the object; for the first consumer's customers that history is where the personal data is. It does not touch `t_proposal.inputs`, so a pending proposal to edit an email keeps the email after the person is erased. And an input carries no `personal` marking: check 10 follows a value from an input into a personal attribute, but an input passed only to an evaluator or a `call` argument is recorded in the event payload (§4: "the writes and the inputs of one transition") with nothing to say it was personal. ADR-0031 §3 promises redaction of "inputs, outcome writes, before-and-after values"; only the attribute-tainted inputs have a mechanism.

**Open.** Erasure redacts the legacy entries attached to the object, whole or by fields the import mapping marks personal; invalidates or redacts pending proposals whose inputs carry a tainted value; and either inputs gain a `personal` marking or the payload records only inputs that were written. **Needs the author** for the legacy-entry choice, since redacting whole legacy payloads loses non-personal history.

### D191
**Two-pass import cannot insert a row whose required reference is `NOT NULL`.** `publish-and-import.md` §4 says "every object is created before any reference is written" and §8 decides "Import is two passes, not deferred constraints". `storage-schema.md` §3 gives a declared-required attribute `NOT NULL` and shows `model_id TEXT NOT NULL REFERENCES t_robotmodel(id)` on `t_robot`. A robot cannot be created before its model is written into it. `first-consumer-cutover.md` §2 already reasons correctly for the cycle it found — nullable edges break, and "a cycle forces one stage only when it contains a required reference" — but the import document's general rule contradicts it, and a cycle of required references within one stage has no insertion order that satisfies `NOT NULL` plus a foreign key on either backend without deferring.

**Open.** State the rule: pass one inserts in topological order of required references with those references present; only optional references are written in pass two; a cycle of required references needs deferrable constraints on PostgreSQL and is not importable on SQLite with foreign keys enforced, which the dry run should report against the mapping. The first consumer has no such cycle, so this does not block its cutover.

### D192
**The disposition file offers `ADMIT` for a class of violation nothing can admit.** `publish-and-import.md` §6: a class's `rule` is "an invariant name, or a guard's clause name", and `ADMIT` means "import it violating, and record that", becoming "an admission on the object". ADR-0054 §4 and `ok_admission` define an admission as one object and one **invariant**. A structural guard violation — a required attribute absent, a required reference missing — has no admission shape, and `storage-schema.md` §3's `NOT NULL` refuses the row regardless. ADR-0002 and ADR-0015 say the importer *reports* failed structural guards; the disposition file lets a person choose an outcome the importer cannot deliver, and they would discover that mid-import.

**Open.** Per class kind, name the dispositions available: an invariant class may be admitted; a guard class may only be cleaned upstream or excluded, or the mapping supplies a value. The dry run should refuse `ADMIT` on a guard class rather than the import failing on it.

### D193
**The actor's optional `attributes` are unreadable, readable and absent, in three documents.** ADR-0025 and `DESIGN.md` §5.8 give the descriptor optional attributes and say a guard reads four members and "the optional attributes are deliberately not readable"; `declaration-syntax.md` §8.1 agrees. ADR-0036 §5 rests attenuation on them — "attenuation is a descriptor attribute a guard reads (`amount <= actor.attributes.approval_limit`)" — and `case-study-approvals-and-bookings.md` §3 repeats the expression; ADR-0030 §1's example reads `actor.teams`. `library-api.md` §3's `Actor` has no attributes field at all. So the delegation answer of ADR-0036 has no spelling, which is the class ADR-0073 records for `clear`: a decision that promised a construct the language never had.

**Open. Needs the author.** Either the module declares typed actor attributes (`actor attr approval_limit money(SGD)`) so a guard over one can be checked at publish and the descriptor's shape is validated against the declaration, or ADR-0036 §5 is amended to say attenuation is a capability or an object and the two examples are rewritten.

### D194
**"Mirror" now names two mechanisms with different write paths, and the model document carries neither.** ADR-0069 makes an external system that assigns a value "a mirror: an object here with an external identifier, written by the consumer on observing the event" — an ordinary type, written through transitions by a sync actor. ADR-0075 makes `mirror` a type marking whose objects are "written only by the import path" and may declare no transitions (check 53). `DESIGN.md` uses the word only in the first sense (§5.5); its model block, §5.9 and §11 do not mention the marking or staged cutover. The first consumer's customer is both: legacy-owned until its stage, and Xero-owned by design intent for ever after (ADR-0069; the walkthrough §3.2; `first-consumer-cutover.md` §3, "customers … migrate last"). If a Xero-owned type is a `mirror` in the ADR-0075 sense, it never cuts over and "import" becomes the Xero sync mechanism, which ADR-0069 did not say; if it is a type in the ADR-0069 sense, the customer changes kind at stage 7 from one mirror to the other, which nothing describes.

**Open. Needs the author.** Decide whether a permanently external-owned type is the `mirror` kind, and if so that repeated import is the sync path and each refresh is an asserted event; then `DESIGN.md` §5, §5.9 and §11 carry the marking and the stage rule, and ADR-0069's "written by the consumer" is reconciled or annotated.

### D195
**Two version numbers share the name "declaration version".** Every type, machine, enum, sequence and evaluator carries its own version in the text (`declaration-syntax.md` §1; check 22). `ok_declaration` has one row per publish with a single `version` and a `module`. `ok_event.declaration_version` and `t_proposal.submitted_under REFERENCES ok_declaration(version)` are publish versions; `t_<type>.type_version` is the type's; `DESIGN.md` §5.9 says "every object and event records the version of its **type**", ADR-0027 §2 says "the declaration version in force", and `library-api.md`'s `Object.declaration_version` and `declaration(type, version?)` do not say which integer they mean. The two are different numbers after the second publish.

**Open.** Name them — publish version, per store, monotonic; type version, in the text — and state that an event records the publish version, which determines every type's version at that instant, so the object needs at most the publish version too. Check 22 is about type versions and stays as it is.

### D196
**Six API and schema shapes do not carry what the model says they carry.** `library-api.md` §6: `check` returns a `Verdict`, and `DESIGN.md` §10 says its verdict "is partial: it names the external guards it did not evaluate", which no `Verdict` member can hold, though `TransitionOffer.unevaluated` exists for the listing. `pull` returns a `Page`, and `storage-schema.md` §7 says `pull` returns the position-window bound a consumer must not acknowledge past. `history` returns `Iterator[Event]`, and §10 says it returns "events with provenance, legacy entries"; an `Event` requires `transition` and `to_state`, which a legacy entry lacks. `Event` has no `declaration_version`, `context` or `taint_version`, all of which `ok_event` stores and §7 calls provenance, nor the as-of time of an external verdict that ADR-0049 records on the event; and `Event.actor: Actor` carries `capabilities`, which the event row does not store. `Request.context` is `Mapping[str, Any] | None` where §6 and ADR-0033 say a free-form string and `ok_event.context` is `TEXT`. And `t_subscription` has no actor column, though delivery applies visibility "as that actor" (§7, ADR-0034 §6). `scripts/check-api-doc.py` holds the operation set against §10 and cannot see any of these, which is why they survived.

**Open.** Each is a one-line shape change; the one worth thinking about is `check`, which wants either a wrapper carrying the verdict and the unevaluated guards or an `unevaluated` field on every verdict.

### D197
**An action would clear an object's `asserted` marking.** `storage-schema.md` §5: `state_source` is "the `source` of the event that last set `state`", and "an ordinary transition afterwards sets `state_source` back to `observed`, which is right: an object whose state was asserted and has since moved through the machine normally is no longer an exception". An action is an ordinary transition that does not move through the machine. Read literally, editing a note on an asserted unit drops it out of `exceptions(type)`, which is the control ADR-0040 §6 rests on.

**Open.** Reset `state_source` only when the event has `changes_state` true. One sentence.

### D198
**The id format was deferred to the storage design, the storage design did not decide it, and two guarantees now rest on it.** ADR-0018 leaves random against time-ordered open, "to be confirmed when storage is designed". `storage-schema.md` declares `id TEXT` and its §11 decides only that the directory stays. Meanwhile ADR-0038 §3 and `DESIGN.md` §6 order every cascade over a collection "in ascending object-id order" so that "a request is therefore reproducible", and `adversarial-harness.md` §4 says a run "is reproducible from those three" — seed, declaration, transcript. With random ids, which part a cascade reaches first, and so which object a failing verdict names, differs between two runs of one transcript, and the shortest-failing-prefix reduction is unsound.

**Open.** Decide time-ordered (UUIDv7 or ULID) in `storage-schema.md` §2, and note that the harness needs an injectable id source and clock, since even a time-ordered id is not reproducible from a seed without one.

### D199
**Check 7's stored-end clause is unimplemented, and the specification's own mirror example violates it.** `declaration-syntax.md` §3.1 says marking a stored relationship end `indexed` "is rejected rather than merely redundant (§8.1, check 7)", and check 7 lists "an `indexed` marking on a stored relationship end, which already is one". The Delivery in §2's mirror example declares `ref customer : Customer indexed`. `scripts/check-syntax-doc.py` reports the document clean and lists check 7 among the twenty-eight it does not enforce. This is the class the lesson "a specification's examples must be run against its own rules" is about, arriving in the newest example, because the check that would have caught it is one of the specified-but-unbuilt ones.

**Open.** Drop `indexed` from that line; implementing the clause is a few lines and a fixture, and worth doing since the example was written by someone who had just read the rule.

### D200
**Eight summaries are stale again, one day after D176 corrected the last set.** "Fifty-two checks" in `DESIGN.md` line 5, `publish-and-import.md` §1, `declaration-syntax.md` §10 and `TODO.md`'s status table, where the corpus checker reports 53 and the syntax checker "25 of 53"; §10 and TODO say twenty-four implemented. `TODO.md`'s defect-repair section says the register "holds 170 entries" while its own status table says 186. README, `DESIGN.md` and `TODO.md` say "ADR-0019 to ADR-0064 … await review", omitting ADR-0074 (pending review) and the derived half of ADR-0075, and the author review queue does not list the six documents written 2026-09-09. `DESIGN.md` §3's property table still says "the one way to set state without satisfying a guard is a declared, recorded assertion", the sentence D178 corrected in §13 and the README but not here. The model block in §5 does not mention the `mirror` marking or `survives`. `case-study-payments.md` §4 opens "A counter is a non-negative integer", which ADR-0072 §5 reversed, in a paragraph that goes on to say so.

**Open.** Each is a line. The recurrence is the finding: `scripts/check-corpus.py` could hold the check count and the register count against their sources the way it holds the defect index, since these are the two numbers that have now been wrong twice.
