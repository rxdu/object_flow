# Defect register

Findings from every review of this design. It began as the implementation-readiness review of 2026-09-08 and has since absorbed nine further rounds — re-expression, a coherence pass, sixteen review rounds on the declaration syntax, and the corpus audit that follows the author's rulings. Each provenance section below says which round found its entries. Every entry was verified against the file cited; the review ran as four independent passes (one direct, three delegated) and only findings confirmed by reading the source are recorded here.

**A note on citations.** Every `file:line` below was correct when the defect was recorded. The documents have since been repaired and re-expressed, so line numbers have drifted; the quoted text is the reliable locator.

**Two kinds of closure.** *Resolved by* means the design now does the thing. *Refused by* means the design decided not to, and the case is recorded in `edge-cases.md` instead. D15 and D17 are refusals.

**How to read this.** `D37`–`D41` were found on 2026-09-08 by re-expressing the case studies against the repaired grammar, which is the test the repair called for. `D01`–`D10` break the model or a running system and must be resolved before a runtime is built. `D11`–`D26` are things the design cannot express or has no algorithm for. `D27`–`D36` are contradictions and scope errors. `C01`–`C05` are cosmetic. A closure is `Resolved`, `Refused by decision` with the case recorded in `edge-cases.md`, or `Recorded as open question N` for a finding the author later ruled on. 0 are open — an operations review of the worked example on 2026-09-24 found D289, corrected in place; an audit of the first consumer's production code the same day found D275 to D281, of which D280 and D281 were corrected in place, reviewing the design against the PRD with the unit's journey found D282 and D283, and verifying ADR-0103's first draft found D284 to D288; ADR-0103 resolved D275 to D279, D282 and D284 to D288, and D283 was corrected in place; the design review and design evaluation of 2026-09-23 found D202 to D215, and ADR-0082, ADR-0083, ADR-0085 and ADR-0087 to ADR-0093 resolved them the same day; mapping the design against the PRD found D216 and D217, and ADR-0095 resolved them; an independent review of that mapping found D218 to D229, a second pass over the repairs found D230 to D234, and a third found D235, all resolved by ADR-0096; five reviewers, one per slice of the PRD, the decision record and the documents' agreement, then found D236 to D260, which ADR-0097 to ADR-0100 and in-place corrections resolved; their verification found D261 to D268, and further passes D269 to D274, which ADR-0101 resolved, while the design was iterated until every PRD requirement was covered; the three before them, D190, D193 and D194, were ruled on 2026-09-09 at the author's direction as ADR-0078 to ADR-0080. The index above is generated from the entries, so it cannot fall behind them again.

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
| [D189](#d189) | The idempotency key is scoped to an optional `principal`, and two documents disagree on whether that was decided | Resolved by ADR-0077 |
| [D190](#d190) | Erasure does not reach legacy entries, proposal inputs, or inputs never written to a personal attribute | Resolved by ADR-0078 |
| [D191](#d191) | Two-pass import cannot insert a row whose required reference is `NOT NULL` | Resolved by ADR-0077 |
| [D192](#d192) | The disposition file offers `ADMIT` for guard-class violations nothing can admit | Resolved by ADR-0077 |
| [D193](#d193) | The actor's optional attributes are unreadable, readable and absent, in three documents | Resolved by ADR-0079 |
| [D194](#d194) | "Mirror" names two mechanisms with different write paths, and the model document carries neither | Resolved by ADR-0080 |
| [D195](#d195) | Two version numbers share the name "declaration version" | Resolved by ADR-0077 |
| [D196](#d196) | Six API and schema shapes do not carry what the model says they carry | Resolved by ADR-0077 |
| [D197](#d197) | An action would clear an object's `asserted` marking | Resolved by ADR-0077 |
| [D198](#d198) | The id format is undecided while cascade order and harness reproducibility rest on it | Resolved by ADR-0077 |
| [D199](#d199) | Check 7's stored-end clause is unimplemented and the specification's own example violates it | Resolved |
| [D200](#d200) | Eight summaries are stale again, one day after D176 | Resolved |
| [D201](#d201) | ADR-0075 breaks a cycle by the optionality of its edges, which answers the import question and not the stage question | Resolved |
| [D202](#d202) | `changed_since` over a part is answered from one position per whole, so the specification's own approval is invalidated by its own creation | Resolved by ADR-0082 |
| [D203](#d203) | At serialisable isolation on PostgreSQL a row lock does not make a contended row queue; the waiter blocks and is then aborted | Resolved by ADR-0090 |
| [D204](#d204) | Nothing says how a SQLite transaction begins, and a deferred one turns an ordinary race into a storage fault | Resolved by ADR-0090 |
| [D205](#d205) | Every imported object is listed by `exceptions(type)` until it next changes state, and a terminal one for ever | Resolved by ADR-0083 |
| [D206](#d206) | The stage order counts references only; the legacy system's cross-type writes put the operational core in one stage | Resolved by ADR-0093 |
| [D207](#d207) | Erasing one object's file deletes every other object's identical file | Resolved by ADR-0087 |
| [D208](#d208) | Erasure stops at the object's boundary, across supersession and into a caller's event, where ADR-0078 says it follows | Resolved by ADR-0087 |
| [D209](#d209) | The deterministic racer cannot interleave a synchronous `request()`, and the async question cites the harness the wrong way round | Resolved by ADR-0091 |
| [D210](#d210) | The settled position has no mechanism in the schema | Resolved by ADR-0089 |
| [D211](#d211) | An event row is completed at commit, and the schema says no event row is ever updated | Resolved by ADR-0089 |
| [D212](#d212) | The publish event has no object to belong to | Resolved by ADR-0085 |
| [D213](#d213) | On PostgreSQL the mint's second connection can exhaust the pool it is drawn from | Resolved by ADR-0090 |
| [D214](#d214) | Three verdict and schema shapes cannot carry what the model says | Resolved by ADR-0092 |
| [D215](#d215) | ADR-0049 consults an evaluator before the transaction and never says what happens when its argument depends on state the transaction changes | Resolved by ADR-0092 |
| [D216](#d216) | A read set discloses the ids of objects its reader cannot see | Resolved by ADR-0095 |
| [D217](#d217) | A metric guard's value is not recorded on the event, only its as-of time | Resolved by ADR-0095 |
| [D218](#d218) | An observation kind's personal fields cannot be erased | Resolved by ADR-0096 |
| [D219](#d219) | A correction is not validated, cannot be made on a terminal subject, and cannot be made by an agent | Resolved by ADR-0096 |
| [D220](#d220) | A metric path, an attempt's read set and a refusal's value can reveal what their reader cannot see | Resolved by ADR-0096 |
| [D221](#d221) | A refusal does not carry the value it was decided on, and the attempt log does not record it | Resolved by ADR-0096 |
| [D222](#d222) | Overrides cannot be counted per reason, or with the import's excluded | Resolved by ADR-0096 |
| [D223](#d223) | Cycle time cannot be attributed to the assignee who held the work, or split by who created it | Resolved by ADR-0096 |
| [D224](#d224) | An occurred time has no upper bound | Resolved by ADR-0096 |
| [D225](#d225) | Permanent refusal counts are monthly and nothing reads them | Resolved by ADR-0096 |
| [D226](#d226) | Legacy intervals contradict "the log can rebuild it" | Resolved by ADR-0096 |
| [D227](#d227) | A failed file deletion waits on a sweep that no operation owns | Resolved by ADR-0096 |
| [D228](#d228) | An approval publishes on the dry run's impact report, however stale | Resolved by ADR-0096 |
| [D229](#d229) | Four statements stale after ADR-0095, three underspecified points, and five rows citing no implementation | Resolved by ADR-0096 |
| [D230](#d230) | Two repairs of D220 and D225 still let a metric reveal what its reader cannot see | Resolved by ADR-0096 |
| [D231](#d231) | A result on a finished subject can never be corrected | Resolved by ADR-0096 |
| [D232](#d232) | An erased personal enum survives in the interval index | Resolved by ADR-0096 |
| [D233](#d233) | The standard metrics' "open" and "completion" are undefined | Resolved by ADR-0096 |
| [D234](#d234) | Six gaps left by the first repairs | Resolved by ADR-0096 |
| [D235](#d235) | Withholding a metric group broke M2, leaked dimension values, and three smaller gaps | Resolved by ADR-0096 |
| [D236](#d236) | The flow change had states and no lifecycle, and its authority was named nowhere | Resolved by ADR-0097 |
| [D237](#d237) | A stale flow change could reinstall text that drops a newer guard | Resolved by ADR-0097 |
| [D238](#d238) | The impact report said nothing about changed guards, and the evidence was not recorded | Resolved by ADR-0097 |
| [D239](#d239) | A proposal's actor guards were re-evaluated in the model and not in the decision | Resolved by ADR-0097 |
| [D240](#d240) | The standard metrics were phrases, not definitions, and some could not be written | Resolved by ADR-0098 |
| [D241](#d241) | A reader could not read the value a rule reads | Resolved by ADR-0098 |
| [D242](#d242) | Most sources could not be split by version, and attempts had no actor | Resolved by ADR-0098 |
| [D243](#d243) | Set-valued dimensions, distinct counts and derived attributes in rows were undefined | Resolved by ADR-0098 |
| [D244](#d244) | C2, UC-8 and UC-10 were read in a way that departs from their wording, with no revision proposed | Resolved by ADR-0098 |
| [D245](#d245) | Adding a datapoint kind took two edits | Resolved by ADR-0099 |
| [D246](#d246) | An observation's occurred time had two channels, no column and no verdict | Resolved by ADR-0099 |
| [D247](#d247) | Recording could not be discovered | Resolved by ADR-0099 |
| [D248](#d248) | What recording writes to the subject was unsaid | Resolved by ADR-0099 |
| [D249](#d249) | An optional reference could never be cleared | Resolved by ADR-0099 |
| [D250](#d250) | A publish met live objects without recording what it did to them | Resolved by ADR-0099 |
| [D251](#d251) | The import had no operation and nothing limited it | Resolved by ADR-0100 |
| [D252](#d252) | Pruning and archiving had no operation | Resolved by ADR-0100 |
| [D253](#d253) | Erasure missed six copies of a person | Resolved by ADR-0100 |
| [D254](#d254) | Verdicts named objects their requester could not see | Resolved by ADR-0100 |
| [D255](#d255) | Three execution steps were missing, and one was wrong | Resolved by ADR-0100 |
| [D256](#d256) | The export and subscriptions still assumed positions | Resolved by ADR-0100 |
| [D257](#d257) | Mirror reads had no as-of time, and evaluators were not injected | Resolved by ADR-0100 |
| [D258](#d258) | The port and the cutover assumed more than the first consumer has | Resolved by ADR-0100 |
| [D259](#d259) | Fourteen older decisions were contradicted by the current model with nothing marking it | Resolved by annotation and back-links |
| [D260](#d260) | Smaller drift between the documents | Resolved by correction |
| [D261](#d261) | The import could still write an owned type nobody had created an object of | Resolved by ADR-0101 |
| [D262](#d262) | An approval could install a report its approver never read | Resolved by ADR-0101 |
| [D263](#d263) | The flow change showed what verdicts hide | Resolved by ADR-0101 |
| [D264](#d264) | Pruning had no floor | Resolved by ADR-0101 |
| [D265](#d265) | Migrations could not fill a reference, and their reach was unsaid | Resolved by ADR-0101 |
| [D266](#d266) | A mirror could not be erased | Resolved by ADR-0101 |
| [D267](#d267) | Four standard metrics counted the wrong thing | Resolved by ADR-0101 |
| [D268](#d268) | Residues of the repairs | Resolved by ADR-0101 |
| [D269](#d269) | Porting every type as a mirror contradicted check 53, so wholes, parts, families and datapoints could not be ported | Resolved by ADR-0101 |
| [D270](#d270) | An approval's version was optional, and a publish had no verdict | Resolved by ADR-0101 |
| [D271](#d271) | A value written after an object first closed had a negative duration | Resolved by ADR-0101 |
| [D272](#d272) | A reader's metric collapsed its groups, and the per-object metrics were not aggregates | Resolved by ADR-0101 |
| [D273](#d273) | A publish's migrations were counted as flow | Resolved by ADR-0101 |
| [D274](#d274) | Observations on a mirror contradicted check 53, and four residues | Resolved by ADR-0101 |
| [D275](#d275) | A duration cannot be divided by a duration, so a utilisation cannot be declared | Resolved by ADR-0103 |
| [D276](#d276) | What an unknown filter does inside an aggregate or a loop is unsaid | Resolved by ADR-0103 |
| [D277](#d277) | Idempotency records have no operation that removes them | Resolved by ADR-0103 |
| [D278](#d278) | Check 15 makes a closed state need an exit that §4.2 calls optional | Resolved by ADR-0103 |
| [D279](#d279) | The record says the first consumer keeps no stock level, and it does | Resolved by ADR-0103 |
| [D280](#d280) | The walkthrough has drifted from the grammar and from production | Resolved in place |
| [D281](#d281) | The syntax checker refused two legal forms | Resolved in place |
| [D282](#d282) | The unit's journey broke UC-6: its receipt could not be backdated | Resolved by ADR-0103 |
| [D283](#d283) | The checker counted an action as a way out of a state | Resolved in place |
| [D284](#d284) | ADR-0103's first draft let an unknown filter satisfy an exclusion in a guard, reopening D20 | Resolved by ADR-0103 |
| [D285](#d285) | ADR-0103's first draft withheld who acted more strictly than production, and left the assignment metrics incomplete | Resolved by ADR-0103 |
| [D286](#d286) | ADR-0103's first draft minted serials with no port path and no rule for an absent maker | Resolved by ADR-0103 |
| [D287](#d287) | ADR-0103's first draft declared a monthly utilisation that buckets spans by when they began | Resolved by ADR-0103 |
| [D288](#d288) | Residues of ADR-0103's first draft | Resolved by ADR-0103 |
| [D289](#d289) | A unit bought out of a lease could not be serviced for its buyer | Resolved in place |
| [D290](#d290) | The core kept a push delivery worker, against N6 and UC-20 | Resolved by ADR-0105 |
| [D291](#d291) | Subscription and Proposal had no declared authority, and a proposal no visibility | Resolved by ADR-0105 |
| [D292](#d292) | An empty store could not install its first declaration, and the built-ins were unversioned | Resolved by ADR-0105 |
| [D293](#d293) | A migration was called an assertion, was never checked against the invariants, and "or admitted" had no form | Resolved by ADR-0105 |
| [D294](#d294) | Erasure changed other objects with no event, and a sync could write an erased value back | Resolved by ADR-0105 |
| [D295](#d295) | Only an unsatisfied verdict carried a remedy class | Resolved by ADR-0105 |
| [D296](#d296) | A refusal decided on a request value could not be re-evaluated from its record | Resolved by ADR-0105 |
| [D297](#d297) | An upper-layer application's reason had no field | Resolved by ADR-0105 |
| [D298](#d298) | A late or stopped scheduler misdated time-driven transitions for good | Resolved by ADR-0105 |
| [D299](#d299) | A tracked member added by a publish had no intervals | Resolved by ADR-0106 |
| [D300](#d300) | Gaps in legacy history were promised as "incomplete" with no mechanism, and `.created_at` was undefined for a backdated or ported creation | Resolved by ADR-0106 |
| [D301](#d301) | The port split every current stay in two, and a mirror's refresh lost who and when | Resolved by ADR-0106 |
| [D302](#d302) | A derived attribute reading other objects had no visibility rule under `get` and `query` | Resolved by ADR-0106 |
| [D303](#d303) | Override counts per reason existed only for enum reasons, and admissions were not counted | Resolved by ADR-0106 |
| [D304](#d304) | A metric could read a personal value, so erasure changed counts | Resolved by ADR-0106 |
| [D305](#d305) | A declared combined metric could not be split by version or actor kind, and silent legacy rows took the importer's kind | Resolved by ADR-0106 |
| [D306](#d306) | Cancellations counted as completions, and reopened work was undefined | Resolved by ADR-0106 |
| [D307](#d307) | Percentiles and buckets differed by backend | Resolved by ADR-0106 |
| [D308](#d308) | `.held` on a transition was ambiguous, and "acting" undefined for recordings | Resolved by ADR-0106 |
| [D309](#d309) | `avg` of a duration lost its unit | Resolved by ADR-0106 |
| [D310](#d310) | The unknown-filter rule missed a reader's filter and a row's own collections | Resolved by ADR-0106 |
| [D311](#d311) | The export could not read the permanent refusal counts or labels | Resolved by ADR-0106 |
| [D312](#d312) | A formula over one object's own flow data could be written only as a metric grouped by object | Resolved by ADR-0106 |
| [D313](#d313) | Three limits were unstated: label names, labels' occurred time, multi-choice attributes | Resolved by ADR-0106 |
| [D314](#d314) | Pruning lost who a rule on trial would have refused | Resolved by ADR-0106 |
| [D315](#d315) | The writing transaction was typed TEXT, so a cursor ordered it as text | Resolved in place |
| [D316](#d316) | The refusal rollup could not hold an attempt with no type | Resolved in place |
| [D317](#d317) | The release map contradicted PRD §10 revision 5 | Resolved in place |
| [D318](#d318) | data-driven-engine.md stated superseded design beyond its release map | Resolved in place |
| [D319](#d319) | The harness missed UC-19's fourth route, three failure rows and the erasure exemption | Resolved in place |
| [D320](#d320) | A metric was still defined "across many objects or over time" | Resolved in place |
| [D321](#d321) | DESIGN's dimensions omitted the creator's kind | Resolved in place |
| [D322](#d322) | DESIGN left who rewrites a time-dependent query, so a caller would restate the threshold | Resolved in place |
| [D323](#d323) | Two cosmetic slips: the renderers omitted the p80, and DESIGN dropped "stored" | Resolved in place |
| [D324](#d324) | edge-cases.md stated a database clock, no replay, uncovered grouping and different notes options | Resolved in place |
| [D325](#d325) | N3's measurement before cutover had no owner | Resolved in place |
| [D326](#d326) | DESIGN did not state D7's rule that catalogues are governed | Resolved in place |
| [D327](#d327) | README said the engine does not compete "on flows" | Resolved in place |
| [D328](#d328) | ADR acceptance status disagreed between documents | Resolved in place |
| [D329](#d329) | "Consumer" meant three things, and the glossary lacked the PRD's terms | Resolved in place |
| [D330](#d330) | DESIGN defined a flow more narrowly than the PRD | Resolved in place |
| [D331](#d331) | DESIGN's glossary said a business identifier is consumer-minted | Resolved in place |
| [D332](#d332) | Six examples declared `tracking serial` for records | Resolved in place |
| [D333](#d333) | flow-review gave the store worklists and notifications | Resolved in place |
| [D334](#d334) | G4's proposed revision was tracked nowhere | Resolved in place |
| [D335](#d335) | DESIGN §4 and the harness fixture pointed at the superseded walkthrough | Resolved in place |
| [D336](#d336) | The syntax document's §12 and Q12 still said the counter is unexercised | Resolved in place |
| [D337](#d337) | README and TODO carried stale revisions and counts | Resolved in place |
| [D338](#d338) | TODO said no questions were open | Resolved in place |
| [D339](#d339) | Amendment bullets sat under the wrong headings, and a paragraph split a table | Resolved in place |
| [D340](#d340) | A refused requester was said to read the value a metric guard used | Resolved in place |
| [D341](#d341) | DESIGN did not state L6's per-object order | Resolved in place |
| [D342](#d342) | The impact report omitted what mappings rewrite and the changes a publish supersedes | Resolved in place |
| [D343](#d343) | A list filter on a derived status was recorded as a decision, not a gap in the core | Resolved in place |
| [D344](#d344) | UC-4's subject was stated three ways, and its gate was never written | Resolved in place |
| [D345](#d345) | The opening-stock creation skipped production's serial rule without counting as an override | Resolved in place |
| [D346](#d346) | The first consumer's module omitted the engineer-of-record rule | Resolved in place |
| [D347](#d347) | UC-20's coverage rested on undecided declarations, and the appendix reused version numbers | Resolved in place |
| [D348](#d348) | The walkthrough found low stock by filtering a counter a unit cannot have | Resolved in place |
| [D349](#d349) | Traceability rows were not re-traced after revision 5 | Resolved in place |
| [D350](#d350) | Two case studies described the read surface as it no longer is | Resolved in place |
| [D351](#d351) | Legacy history was said to be preserved while fields were dropped and carrying it left open | Resolved in place |
| [D352](#d352) | L1 was narrowed to exclude labels without a PRD revision | Resolved in place |
| [D353](#d353) | The PRD's proposed C2 said "consumer" | Resolved in place |
| [D354](#d354) | A refusal by an invariant or a cascade could not be re-evaluated from its record | Resolved by ADR-0105 |
| [D355](#d355) | Overrides were counted per free-text reason again, by an `admit` and by an erasure | Resolved by ADR-0105 |
| [D356](#d356) | A gap in coverage could go unreported: an undated creation, and a current stay across a silent span | Resolved by ADR-0106 |
| [D357](#d357) | An `admit` could not yield for an invariant compiled to a constraint | Resolved by ADR-0105 |
| [D358](#d358) | The release map deferred rules the first release needs | Resolved in place |
| [D359](#d359) | `not_erased` had no stated scope | Resolved by ADR-0105 |
| [D360](#d360) | "Every count is the same after an erasure" was false, and the erasure's events had no place in the metrics | Resolved by ADR-0106 |
| [D361](#d361) | Superseded statements survived the repair | Resolved in place |
| [D362](#d362) | D5 was marked covered while labels and overrides do not meet it | Resolved by ADR-0106 |
| [D363](#d363) | Creating a store was a write with no operation and no attribution | Resolved by ADR-0105 |
| [D364](#d364) | Pinning the built-ins was said to pin the language's meaning | Resolved by ADR-0105 |
| [D365](#d365) | ADR-0105 said UC-20 was met only within the backdating bound | Resolved by ADR-0105 |
| [D366](#d366) | An assertion's explanation was said to be personal and marked so nowhere | Resolved by ADR-0106 |
| [D367](#d367) | ADRs still presented push delivery, migration as an assertion, and the first-entry completion rule as current | Resolved in place |
| [D368](#d368) | "Consumer" survived in its three old senses across the implementation documents | Resolved in place |
| [D369](#d369) | Three slips of the first repair: unmarked text, a serial record, a creation without the engineer rule | Resolved in place |
| [D370](#d370) | Counting and structural slips in the repair | Resolved in place |
| [D371](#d371) | The worked example claimed a guard `User.leave` did not declare | Resolved in place |
| [D372](#d372) | Five of the worked example's cases rested on declarations no checked module held | Resolved in place |
| [D373](#d373) | `RobotModel`, which the journey's rules read, was declared nowhere | Resolved in place |
| [D374](#d374) | The syntax checker ignored `use`, so a module was never checked against the closure it imports | Resolved in place |
| [D375](#d375) | Check 19 never resolved a state literal | Resolved in place |
| [D376](#d376) | Check 56 read only the first line of a wrapped guard | Resolved in place |
| [D377](#d377) | The journey's purpose paragraph split its status paragraph | Resolved in place |
| [D378](#d378) | A type's printed rules did not show a cascade declared on another type | Resolved by ADR-0108 |
| [D379](#d379) | Nothing recorded a unit's manufacturer serial or photos after its creation | Resolved by ADR-0109 |
| [D380](#d380) | Check 19 ignored a bare state compared with the object's own state | Resolved by ADR-0109 |
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
`docs/adr/0014-delivery-is-at-least-once-with-idempotency-keys.md:27` — "ObjectFlow refuses a second attempt carrying a key it has already applied." `docs/DESIGN.md:165` — "If the idempotency key has been applied before, return the original result."

Different observable behaviour. `docs/adr/0022-time-is-a-guard-value-and-availability-is-queryable.md:15` depends on the replay reading ("so a repeated sweep is harmless"). ADR-0014 itself warns at `:54` against conflating request replay with transition deduplication.

**Resolved by ADR-0041.** Replay wins: an applied key returns the original result, marked as a replay. ADR-0014's wording corrected in place.

### D07
**The built-in Subscription cannot be operated under its own rules.**
Three defects in one place.
1. `docs/adr/0034-event-ordering-and-subscriptions-as-a-built-in-type.md:20` makes the cursor a controlled attribute and declares only one action, `acknowledge`. Under `docs/DESIGN.md:56` a controlled attribute is written only via transitions, so the cursor has no writer. Yet `docs/adr/0037:24` makes `pull` a read-surface operation, so a read would advance a controlled attribute.
2. `docs/adr/0034:20` advances the `lagging` and `dead-lettered` states "by the delivery worker", which is part of an ObjectFlow deployment (`docs/DESIGN.md:30`). `docs/adr/0012-objectflow-does-not-initiate-transitions.md:17` forbids any path by which an object moves without an external actor. Same problem for `Proposal.expired` (`docs/adr/0036-proposals-are-a-built-in-type-and-delegation-is-supplied.md:17`).
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
**Files are both never touched and deleted.** `docs/DESIGN.md:96` — "ObjectFlow never touches the bytes"; `:200` — erasure "deletes referenced file content"; `docs/adr/0031:14` the same; `docs/adr/0017-file-attachments-are-content-addressed-references.md:20` gives the backend interface as "put, get-URL, exists, delete". Also, an Accepted ADR (0031) depends on a Proposed one (0017), and `docs/DESIGN.md:212` states file porting as settled while `docs/adr/0015:74` leaves it open.

**Resolved by ADR-0041.** Erasure wins: ObjectFlow never reads, streams or serves bytes, and deletes them only during erasure.

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
**The sequence table would have made sequences gapless, which a decision rejected.** `of_sequence` was a row updated inside the request's transaction. A rolled-back creation would then roll the counter back too, so no gap would appear — and ADR-0029 requires the gap, because a gapless sequence serialises every creation in its scope, which is the reason it was rejected as the default.

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

A single-pass read of the whole record by one reviewer — the model, all 75 decisions, the seven design documents, the five case studies, the walkthrough, the edge cases and the status files — after the six documents of 2026-09-09 were written, with three claims probed by running something rather than reading. The findings cluster where `docs/LESSONS.md` predicts: at the seams between the newest documents and the model. Entries are ordered by consequence. D187 and D188 were repaired the same day by ADR-0076 and nine more by ADR-0077; D190, D193 and D194 were ruled on the same day at the author's direction, as ADR-0078 to ADR-0080, pending the author's review; D201, found by the repair, was closed the same day.

### D187
**On SQLite the sequence mint deadlocks against the request that needs it.** `storage-schema.md` §6 and §11 allocate a sequence value "in its OWN transaction on its own connection" and say "on SQLite it is a second connection". SQLite permits one writer per database file. The mint happens inside the request — `declaration-syntax.md` §3.1: "after those writes and before every other outcome step" — and the event's position is allocated when its transition begins applying (ADR-0046 §5), which on this schema is an insert into `of_event`; so, by those two statements, the request's connection has written before the mint and holds the write lock when the second connection tries to update `of_sequence`. **Observed**, SQLite 3.37.2 in WAL mode: an update on a second connection succeeds while the first connection has only read, fails with `database is locked` after the busy timeout once the first has written one row, and succeeds against a separate database file. The document's "what is verified" paragraph names the DDL, a cascade round trip and a refused state value; this claim was reasoned, not run.

**Resolved by ADR-0076.** On SQLite `of_sequence` lives in a separate database file beside the store, opened on its own connection; on PostgreSQL the second connection opens the same database, since it locks rows. `storage-schema.md` §6 and §11 say so, its verified paragraph names the probe, and `scripts/check-schema-doc.py` runs it on every corpus run in both journal modes: the same-file connection fails with `database is locked`, the separate-file one succeeds, and its allocation survives the request's rollback. ADR-0029 rule 1, which still read "inside the creating transaction", is annotated. Related: D181, whose fix this corrects.

### D188
**A retry that carries both its idempotency key and its `expected_version` is refused as `stale` instead of being replayed.** `DESIGN.md` §6 orders the request: step 2 "If an `expected_version` is given and differs, refuse with `stale`", step 3 "If the idempotency key has been applied before, return the original result, marked as a replay". A caller that read version 7, requested with `expected_version = 7` and a key, timed out after the commit raised the version to 8, and retries the identical request meets step 2 first and gets `stale`. That is the case ADR-0041 §2 was written to make harmless, and the case the harness's retrier (`adversarial-harness.md` §2) is scripted to test. ADR-0023 rule 5 requires only that `stale` precede the guards, which a replay check before it still satisfies.

**Resolved by ADR-0076.** `DESIGN.md` §6 now runs visibility, replay, version, guards; a replay returns the recorded verdict whatever `expected_version` the retry carries, and `stale` is possible only for a request that has not been applied. A creation skips steps 1 and 3. `library-api.md` §3 says the key wins, and ADR-0023 and ADR-0041 carry the back-link.

### D189
**The idempotency key is scoped to `principal`, which is optional; and two documents disagree on whether that scope was decided.** `storage-schema.md` §6 declares `of_idempotency` with `principal TEXT NOT NULL` and `PRIMARY KEY (principal, key)`, and §11 lists "An idempotency key is scoped to the principal" under Decided. `library-api.md` §3 has `principal: str | None`, and ADR-0025 makes it "optional: the party on whose behalf an agent or service acts"; a human acting for themselves has none, so a human's keyed request has no row it can write. `adversarial-harness.md` §2, written the same day, says the retrier tests "the boundary of the key's scope, which the record leaves undecided — so the harness is where that decision gets its evidence".

**Resolved by ADR-0077.** The key is scoped to the actor's `id`; `of_idempotency` keys on `(actor_id, key)`, a key reused with a different digest is refused as `KeyReused`, and the harness paragraph names the scope it tests.

### D190
**Erasure does not reach three places a personal value can rest.** `storage-schema.md` §9 lists what erasure does: the object row, past event payloads, the file content, and an `erased` event. It does not touch `of_legacy_entry.payload`, though ADR-0015 says the legacy audit log "carries before/after values, principal, source and IP address" and `DESIGN.md` §11 attaches those entries to the object; for the first consumer's customers that history is where the personal data is. It does not touch `t_proposal.inputs`, so a pending proposal to edit an email keeps the email after the person is erased. And an input carries no `personal` marking: check 10 follows a value from an input into a personal attribute, but an input passed only to an evaluator or a `call` argument is recorded in the event payload (§4: "the writes and the inputs of one transition") with nothing to say it was personal. ADR-0031 §3 promises redaction of "inputs, outcome writes, before-and-after values"; only the attribute-tainted inputs have a mechanism.

**Resolved by ADR-0078,** decided at the author's direction. Erasure redacts the legacy entries attached to the object except the fields the import mapping lists as kept, an allowlist because the field nobody thought about should be the one redacted; redacts the inputs of every proposal targeting the object or carrying a personal value and invalidates the pending ones; and redacts personal inputs from every payload, an input being personal by marking or by flowing into a personal attribute. The first consumer's audit rows carry before-and-after values for customers and the employee's IP address besides. Personal data inside another object's free text stays a recorded limit.

### D191
**Two-pass import cannot insert a row whose required reference is `NOT NULL`.** `publish-and-import.md` §4 says "every object is created before any reference is written" and §8 decides "Import is two passes, not deferred constraints". `storage-schema.md` §3 gives a declared-required attribute `NOT NULL` and shows `model_id TEXT NOT NULL REFERENCES t_robotmodel(id)` on `t_robot`. A robot cannot be created before its model is written into it. `first-consumer-cutover.md` §2 already reasons correctly for the cycle it found — nullable edges break, and "a cycle forces one stage only when it contains a required reference" — but the import document's general rule contradicts it, and a cycle of required references within one stage has no insertion order that satisfies `NOT NULL` plus a foreign key on either backend without deferring.

**Resolved by ADR-0077,** and the recommendation above was wrong in one respect: SQLite does honour `DEFERRABLE INITIALLY DEFERRED`, which `scripts/check-schema-doc.py` now demonstrates. Import is one pass with every id assigned first, a stored end's foreign key is deferred to commit on both backends, and a cycle of required references commits in one transaction. ADR-0075's two-pass sentence and the cutover document are annotated, and the question the annotation leaves is D201.

### D192
**The disposition file offers `ADMIT` for a class of violation nothing can admit.** `publish-and-import.md` §6: a class's `rule` is "an invariant name, or a guard's clause name", and `ADMIT` means "import it violating, and record that", becoming "an admission on the object". ADR-0054 §4 and `of_admission` define an admission as one object and one **invariant**. A structural guard violation — a required attribute absent, a required reference missing — has no admission shape, and `storage-schema.md` §3's `NOT NULL` refuses the row regardless. ADR-0002 and ADR-0015 say the importer *reports* failed structural guards; the disposition file lets a person choose an outcome the importer cannot deliver, and they would discover that mid-import.

**Resolved by ADR-0077.** A class carries its kind; a guard class may be cleaned upstream, supplied by the mapping or excluded, and the dry run refuses `ADMIT` on it. `publish-and-import.md` §6, fifth rule.

### D193
**The actor's optional `attributes` are unreadable, readable and absent, in three documents.** ADR-0025 and `DESIGN.md` §5.8 give the descriptor optional attributes and say a guard reads four members and "the optional attributes are deliberately not readable"; `declaration-syntax.md` §8.1 agrees. ADR-0036 §5 rests attenuation on them — "attenuation is a descriptor attribute a guard reads (`amount <= actor.attributes.approval_limit`)" — and `case-study-approvals-and-bookings.md` §3 repeats the expression; ADR-0030 §1's example reads `actor.teams`. `library-api.md` §3's `Actor` has no attributes field at all. So the delegation answer of ADR-0036 has no spelling, which is the class ADR-0073 records for `clear`: a decision that promised a construct the language never had.

**Resolved by ADR-0079,** decided at the author's direction. The descriptor has no free-form attributes: a small finite fact is a capability, a fact with a value is an object the guard reads, which is what ADR-0036 §6 already gave for governed delegation. The first consumer's API keys carry a permission allowlist and no limit of any kind. ADR-0025's row is struck, ADR-0036 §5 and ADR-0030's example are annotated, and the approvals study's attenuation guard is rewritten over a `Delegation` object.

### D194
**"Mirror" now names two mechanisms with different write paths, and the model document carries neither.** ADR-0069 makes an external system that assigns a value "a mirror: an object here with an external identifier, written by the consumer on observing the event" — an ordinary type, written through transitions by a sync actor. ADR-0075 makes `mirror` a type marking whose objects are "written only by the import path" and may declare no transitions (check 53). `DESIGN.md` uses the word only in the first sense (§5.5); its model block, §5.9 and §11 do not mention the marking or staged cutover. The first consumer's customer is both: legacy-owned until its stage, and Xero-owned by design intent for ever after (ADR-0069; the walkthrough §3.2; `first-consumer-cutover.md` §3, "customers … migrate last"). If a Xero-owned type is a `mirror` in the ADR-0075 sense, it never cuts over and "import" becomes the Xero sync mechanism, which ADR-0069 did not say; if it is a type in the ADR-0069 sense, the customer changes kind at stage 7 from one mirror to the other, which nothing describes.

**Resolved by ADR-0080,** decided at the author's direction. `mirror` is the cutover marking and nothing else; ADR-0069's concept is an externally owned type, an ordinary type with an external identifier whose sync-driven transitions the sync requests. The first consumer's own Xero design decides it: `is_active` stays local, an archive in Xero must never cascade, and a Xero merge is followed by re-pointing and retiring, which is supersession, none of which a type with no transitions can do. The customer is a mirror while legacy owns it and cuts over to the Xero-synced type.

### D195
**Two version numbers share the name "declaration version".** Every type, machine, enum, sequence and evaluator carries its own version in the text (`declaration-syntax.md` §1; check 22). `of_declaration` has one row per publish with a single `version` and a `module`. `of_event.declaration_version` and `t_proposal.submitted_under REFERENCES of_declaration(version)` are publish versions; `t_<type>.type_version` is the type's; `DESIGN.md` §5.9 says "every object and event records the version of its **type**", ADR-0027 §2 says "the declaration version in force", and `library-api.md`'s `Object.declaration_version` and `declaration(type, version?)` do not say which integer they mean. The two are different numbers after the second publish.

**Resolved by ADR-0077.** "Declaration version" is the publish's number and "type version" the number in the text; objects and events record the former, the row column is renamed `declaration_version`, and `DESIGN.md` §5.9 and §14, the syntax document §1 and ADR-0056 §4 say so.

### D196
**Six API and schema shapes do not carry what the model says they carry.** `library-api.md` §6: `check` returns a `Verdict`, and `DESIGN.md` §10 says its verdict "is partial: it names the external guards it did not evaluate", which no `Verdict` member can hold, though `TransitionOffer.unevaluated` exists for the listing. `pull` returns a `Page`, and `storage-schema.md` §7 says `pull` returns the position-window bound a consumer must not acknowledge past. `history` returns `Iterator[Event]`, and §10 says it returns "events with provenance, legacy entries"; an `Event` requires `transition` and `to_state`, which a legacy entry lacks. `Event` has no `declaration_version`, `context` or `taint_version`, all of which `of_event` stores and §7 calls provenance, nor the as-of time of an external verdict that ADR-0049 records on the event; and `Event.actor: Actor` carries `capabilities`, which the event row does not store. `Request.context` is `Mapping[str, Any] | None` where §6 and ADR-0033 say a free-form string and `of_event.context` is `TEXT`. And `t_subscription` has no actor column, though delivery applies visibility "as that actor" (§7, ADR-0034 §6). `scripts/check-api-doc.py` holds the operation set against §10 and cannot see any of these, which is why they survived.

**Resolved by ADR-0077.** `check` returns `Checked`, `pull` returns `EventPage` with the settled position, `history` yields `Event | LegacyEntry`, `Event` carries the provenance the log stores and the as-of times, `Request.context` is a string, and `t_subscription` gains `deliver_as`.

### D197
**An action would clear an object's `asserted` marking.** `storage-schema.md` §5: `state_source` is "the `source` of the event that last set `state`", and "an ordinary transition afterwards sets `state_source` back to `observed`, which is right: an object whose state was asserted and has since moved through the machine normally is no longer an exception". An action is an ordinary transition that does not move through the machine. Read literally, editing a note on an asserted unit drops it out of `exceptions(type)`, which is the control ADR-0040 §6 rests on.

**Resolved by ADR-0077.** `state_source` resets only on an event that changes state; `storage-schema.md` §3 and §5.

### D198
**The id format was deferred to the storage design, the storage design did not decide it, and two guarantees now rest on it.** ADR-0018 leaves random against time-ordered open, "to be confirmed when storage is designed". `storage-schema.md` declares `id TEXT` and its §11 decides only that the directory stays. Meanwhile ADR-0038 §3 and `DESIGN.md` §6 order every cascade over a collection "in ascending object-id order" so that "a request is therefore reproducible", and `adversarial-harness.md` §4 says a run "is reproducible from those three" — seed, declaration, transcript. With random ids, which part a cascade reaches first, and so which object a failing verdict names, differs between two runs of one transcript, and the shortest-failing-prefix reduction is unsound.

**Resolved by ADR-0077.** UUIDv7, in `storage-schema.md` §2; the store takes its id source and clock as injected dependencies, and the harness says its reproducibility rests on that. ADR-0018's open item is annotated as decided.

### D199
**Check 7's stored-end clause is unimplemented, and the specification's own mirror example violates it.** `declaration-syntax.md` §3.1 says marking a stored relationship end `indexed` "is rejected rather than merely redundant (§8.1, check 7)", and check 7 lists "an `indexed` marking on a stored relationship end, which already is one". The Delivery in §2's mirror example declares `ref customer : Customer indexed`. `scripts/check-syntax-doc.py` reports the document clean and lists check 7 among the twenty-eight it does not enforce. This is the class the lesson "a specification's examples must be run against its own rules" is about, arriving in the newest example, because the check that would have caught it is one of the specified-but-unbuilt ones.

**Resolved.** The marking is dropped, and the clause is implemented in `scripts/check-syntax-doc.py` with a fixture; on its first run over the specification it reported exactly that line, which is the evidence a new check is supposed to give before it is believed.

### D200
**Eight summaries are stale again, one day after D176 corrected the last set.** "Fifty-two checks" in `DESIGN.md` line 5, `publish-and-import.md` §1, `declaration-syntax.md` §10 and `TODO.md`'s status table, where the corpus checker reports 53 and the syntax checker "25 of 53"; §10 and TODO say twenty-four implemented. `TODO.md`'s defect-repair section says the register "holds 170 entries" while its own status table says 186. README, `DESIGN.md` and `TODO.md` say "ADR-0019 to ADR-0064 … await review", omitting ADR-0074 (pending review) and the derived half of ADR-0075, and the author review queue does not list the six documents written 2026-09-09. `DESIGN.md` §3's property table still says "the one way to set state without satisfying a guard is a declared, recorded assertion", the sentence D178 corrected in §13 and the README but not here. The model block in §5 does not mention the `mirror` marking or `survives`. `case-study-payments.md` §4 opens "A counter is a non-negative integer", which ADR-0072 §5 reversed, in a paragraph that goes on to say so.

**Resolved.** Each line is corrected, and `scripts/check-corpus.py` now holds the register size, the open count and the check counts in README, `DESIGN.md`, `TODO.md`, `publish-and-import.md`, the syntax document and the register's own header against their sources, so the third recurrence fails the build.

### D201
**ADR-0075 breaks a cycle by the optionality of its edges, which answers the import question and not the stage question.** Found while repairing D191. ADR-0075 says a cycle in the reference graph forces its members into one stage "unless the cycle can be broken", and that "a cycle every one of whose edges is optional can be" by importing with those references absent and writing them in a second pass; `first-consumer-cutover.md` §2 applies this to the one cycle it found and concludes "it is not one atomic stage". But the stage rule rests on **ownership**, not on insertability: a legacy referrer cannot point at rows a migrated referent now owns whether its column is nullable or not, since the business still needs the reference filled. ADR-0077 has since removed the second pass altogether, so the mechanism the argument leaned on no longer exists. The practical plan is unaffected — the cutover document already places the whole cycle in stage 4 — so this is the reasoning, not the order.

**Resolved.** The exception is struck through in ADR-0075 with the reason beside it, in the manner the record uses for a derived statement later withdrawn; the cutover document's §2 is rewritten under the heading "one cycle, and it is one stage", and its §5, TODO.md's stage-order item and ADR-0075's consequences say the same. The stage table was already right; the reasoning now matches it. The author accepted staged cutover and will see the struck paragraph when reviewing ADR-0075.

## Found by the design review of 2026-09-23

One reviewer, reading the spine (`README.md`, `DESIGN.md`, `TODO.md`, `LESSONS.md`), the declaration syntax, the six implementation documents of 2026-09-09, ADR-0017, ADR-0038, ADR-0039, ADR-0054, ADR-0057 and ADR-0074 to ADR-0080 in full, and the case studies, the walkthrough and the remaining decisions only where a finding led. The five checkers were run and all exit clean. Two claims about database behaviour were probed rather than read, and both failed: row locking at serialisable isolation, on PostgreSQL 16.8 in a throwaway container, and how a transaction begins on SQLite 3.37.2. That makes four reasoned claims about the databases in this record, after D187 and D191, that failed the first probe pointed at them. Entries are ordered by consequence. None is repaired here; five need a ruling.

### D202
**`changed_since` over a part relationship is answered from one position per whole, so a whole with two part relationships cannot tell them apart, and the specification's own approval is invalidated by its own creation.** `declaration-syntax.md` §8.3 lets `changed_since` name "a part relationship, which means 'any change to any of those parts'". ADR-0057 answers it from an index "extended to carry, on each whole, the position of the last event on any of its parts" — one position per whole, which `storage-schema.md` §3 stores as the single column `last_part_event` and §5 lists among the indexes a guarantee rests on. An approval is itself a part of the approved object (ADR-0035). So the `Delivery` of `declaration-syntax.md` §3.2 has two part relationships, `checklist_items` and `approvals`, and its `approve` action creates an `Approval` with `at_event := this_event`. That creation is the `approve` event's child, recorded after it (`DESIGN.md` §6 steps 6 and 8), and it stamps the delivery's `last_part_event` with a position later than `a.at_event`. The guard `signed: any(a in approvals where not changed_since([approved_total, checklist_items], a.at_event))` then sees a part change after every approval that exists, including the one just recorded, and `complete_sale` can never be taken. Stamping the parent's position instead only moves the failure: each later approval still invalidates every earlier one, so an N-of-M count on a whole that names any part in its freshness test never reaches N. ADR-0057's own motivating case, a purchase order whose approvals are invalidated by an edit to its `lines`, is this shape, and the `Delivery` is the harness's fixture (`adversarial-harness.md` §3). No runtime exists, so this is derived from the three texts, not observed.

**Resolved by ADR-0082**, accepted 2026-09-23: part positions are kept per relationship in `of_attribute_write`, and `last_part_event` is retired. `DESIGN.md` §5.3 and §7 say so; `storage-schema.md` carries a notice until its tables are amended.

*As recorded when open:* The language names a relationship and the index records the whole, so one of them has to change. The smaller repair keeps the language: record the part position per relationship. `of_attribute_write (object_id, attribute)` already has that shape if a part relationship's name is written as the key, which would retire `last_part_event`. A harness case of "approve, then complete" should fail before the repair and pass after it.

### D203
**At serialisable isolation on PostgreSQL, a row lock does not make a contended row queue: the second request waits for the first to commit and is then aborted.** ADR-0039 decision 3 keeps row locks because "they make hot-row conflicts block instead of aborting and retrying, which is cheaper for the flash-sale case", and its consequences say "a popular product now queues on the row lock rather than aborting and retrying … so the failure mode is latency rather than a retry storm". `storage-schema.md` §7 says "a contended row queues rather than aborting and retrying"; `edge-cases.md` "Hot rows" says the lock "exists so that contention blocks rather than aborts"; `case-study-orders.md` §4 repeats it; and D180 resolved ADR-0039's two answers toward "queues". Observed 2026-09-23 on PostgreSQL 16.8: request A takes `SELECT … FOR UPDATE` on a row, updates it and holds its transaction three seconds; request B, a second later, issues the same `SELECT … FOR UPDATE` as its first statement. At `READ COMMITTED`, B waited and committed, and the row ended at 2. At `SERIALIZABLE`, B waited the full three seconds and then failed with `could not serialize access due to concurrent update`, and the row ended at 1. The PostgreSQL 16 manual says the same of repeatable read, which serializable builds on: a transaction that updates or locks a row modified by another transaction after it started gets a serialisation error (`transaction-iso.html`). So a hot row pays the lock wait **and** the retry, and a flash sale exhausts the retry bound into `stale`, which is the retry storm the documents say the lock prevents. Correctness is unaffected, and nothing oversells.

**Resolved by ADR-0090**, 2026-09-23: the design now says a contended row waits and then retries on PostgreSQL, row locks end the loser sooner, and retries are recorded per event to measure the cost.

*As recorded when open:*  The claim is false in five places, and D180's resolution rests on it. What the lock is for at this isolation level needs deciding: drop it, since it adds a wait and saves no retry, or keep it for a reason that holds. A real queue needs the lock taken before the snapshot exists — for instance a session-level advisory lock on objects known before the transaction opens, which in the orders case includes the stock, an input to `Order.place` — and whether that earns its complexity is a measurement. The probe belongs beside `scripts/check-schema-doc.py` wherever a PostgreSQL is available.

### D204
**Nothing says how the store begins a transaction on SQLite, and under SQLite's default deferred begin a request that reads and then writes fails with a storage fault when another request commits in between.** ADR-0039 says "SQLite serialises writers already, so this costs nothing there"; `storage-schema.md` §7 says "On SQLite, which serialises writers, the window is empty"; `adversarial-harness.md` §5 says "SQLite serialises writers already, so the racer finds almost nothing there". SQLite serialises writes. Whether a request's guard reads are serialised with them depends on how its transaction begins, and no document says. Observed 2026-09-23 with SQLite 3.37.2 through Python's `sqlite3`: A begins and reads a row, which stands for its guards; B begins, updates the row and commits; A updates the row. With `BEGIN DEFERRED`, the driver's default, in WAL mode, B committed and A's write failed with `database is locked`, which no busy timeout resolves because A's snapshot is already stale. In rollback-journal mode the deferred pair also ended with one side refused. With `BEGIN IMMEDIATE`, in both modes, B waited behind A and A committed. `library-api.md` §7 raises `StorageUnavailable` for "a transaction failed for a reason that is not a serialisation conflict", so a runtime that begins deferred turns an ordinary race into an exception rather than a retry and a verdict.

**Resolved by ADR-0090**, 2026-09-23: every SQLite transition transaction begins `IMMEDIATE`, and a busy result past the timeout is a serialisation failure, retried and then `stale`.

*As recorded when open:*  State that every transition transaction on SQLite begins `IMMEDIATE`, which is what makes "serialises writers" true of guards as well as commits, and say how a busy result that still occurs is classified. Keep the four-way probe beside the sequence probe in `scripts/check-schema-doc.py`.

### D205
**Every imported object is listed by `exceptions(type)` until it next changes state, and one imported in a terminal state is listed for ever.** Import writes every object's state through the built-in assertion with provenance `asserted` (`DESIGN.md` §11; `publish-and-import.md` §4). `state_source` holds the source of the event that last changed state and returns to `observed` only on an ordinary state-changing transition (`storage-schema.md` §3 and §5; ADR-0077 §6), and `exceptions(type)` returns every object whose `state_source` is `asserted`. So on the day of cutover the list holds the whole legacy population, and every delivered delivery, expired warranty and retired unit stays on it permanently, since nothing moves a terminal object again. A mirror's objects go back on the list with every refresh, since a mirror "is kept current by repeated import" (`publish-and-import.md` §7). ADR-0080 rejects an alternative on exactly this ground — "every customer would sit in `exceptions(type)` for ever" — without noticing that the adopted import path does it to every type. The list is the control that keeps the escape hatch reviewable (`DESIGN.md` §8 and §13; ADR-0040 §6), and drowned in the legacy population it cannot show the one repair someone made last week. A smaller edge in the same column: a mapping that moves an asserted object out of a removed state sets `state_source` to `migrated`, and the object leaves the list without having moved through the machine.

**Resolved by ADR-0083**, accepted 2026-09-23: `state_source` gains `imported`, and `exceptions(type)` reads `asserted` alone. `DESIGN.md` §8 and §10 say so.

*As recorded when open:* Import and repair are both assertions, but they are reviewed differently. An import is reviewed once, through the committed disposition file (`publish-and-import.md` §6); a repair is reviewed through this list. Recording the import's state change distinctly in `state_source`, while its event keeps the provenance `asserted` that ADR-0015 requires, would keep the list to repairs and admissions. Whether imported objects belong on the list at all is the author's call.

### D206
**The stage order counts reference edges only; the legacy system's own cross-type writes add edges the other way and put most of the operational core in one stage.** ADR-0075's rule is "nothing writes a type it does not own", and it derives the order from references alone: "a type may migrate only after every type that references it has migrated". A legacy transition that writes a second type also constrains the order, because while the first type is legacy-owned the second must be too. In the first consumer, completing a delivery marks its units sold and creates their warranty contracts (`wr:app/core/state_registry.py:771`, side effects `_update_delivery_items_to_sold` and `_create_delivery_warranties`), and cancelling reverts the units and voids the warranties (`:779`, `:788`) — the evidence `TODO.md` already cites for ADR-0019. `first-consumer-cutover.md` §3 puts `warranty_contracts` in stage 3 and `deliveries` in stage 6, so for three stages the legacy completion path would create rows in a type this store owns. The write edge says a warranty contract migrates no earlier than a delivery, and its reference `warranty_contracts.delivery_id` (`wr:app/models/warranty.py:112`) says no later, so they are one stage. The same holds for the unit cycle of §2, which the completion writes, and so for `delivery_configurations`, which lies on the reference path between that cycle and `deliveries` (`wr:app/models/delivery.py:155`, `wr:app/models/delivery_configuration.py:49`). At least seven tables move together: `deliveries`, `delivery_items`, `delivery_configurations`, `robots`, `accessories`, `spare_parts` and `warranty_contracts`. The store's side has the matching constraint: check 53 forbids an owned type's outcome from reaching a mirror, so a migrated type's cascade targets must migrate no later than it. Read from the legacy registry and models at commit `4109939`; the whole graph was not recomputed.

**Resolved by ADR-0093**, 2026-09-23: the stage order counts the legacy system's writes as well as its references; the recomputation over types is the queued mapping work, and the choice of staging stays the author's.

*As recorded when open:*  Recompute the order over references plus writes. The legacy writes can be enumerated from the registry's `side_effects=` lists, and the store's from each declaration's `call` and `create` steps; the recomputation comes after the table-to-type mapping `TODO.md` already queues. The result bears on ADR-0075 itself, which kept big-bang as "the simpler path": if the core is one stage, staging buys less than §3's "only nine of the 42 components are pinned" suggests, and the author should see that before a plan is built on it.

### D207
**Erasing one object's file deletes every other object's identical file.** Files are content-addressed so that "duplicates collapse" (ADR-0017), `of_file` is keyed by the hash alone (`storage-schema.md` §6), and erasure "sets `of_file.erased_at` and deletes the content behind the hash" (`storage-schema.md` §9 step 3). Two objects that hold the same bytes — a label template attached twice, one photo attached to a delivery and to its service, a legacy photo row that was duplicated — share one row and one blob. Erasing a person's copy deletes the other object's evidence and marks it erased, even where the other attribute is not personal at all. Nothing in the record counts references to a hash, and finding them means scanning every `file` column, every set-valued side table and every event payload. Found by reading; a search of the record for deduplication, reference counts or shared files finds nothing.

**Resolved by ADR-0087**, 2026-09-23: a file's content is deleted when its last unerased reference goes, through a reference index per hash, with a pending set a sweep retries.

*As recorded when open:*  One option: erasure deletes bytes only when no unerased reference remains, which ADR-0017's sweep for unreferenced keys nearly is already, provided erasure marks the reference and not the hash. The other: references stop sharing blobs, which gives up the deduplication ADR-0017 wanted. The first keeps the design, and needs the sweep to run soon enough to meet an erasure deadline, a deployment obligation to write down.

### D208
**Erasure stops at the object's own boundary, and a person's data crosses it in two common ways ADR-0078 says erasure follows.** First, supersession. The CRM merge (`case-study-crm.md` §3) closes the losing contact as `merged_into` the survivor, and the loser keeps its own personal values and history. Erasing the survivor, the id a request for erasure will name, reaches its parts (check 39) and not its predecessors, and none of ADR-0031, ADR-0051 or ADR-0078 mentions supersession. Second, arguments. A personal input passed to a `call` or `create` on another object is recorded in the caller's event payload as well as the callee's. ADR-0078 decision 3 says a personal input "is redacted wherever it was recorded", but `storage-schema.md` §9 step 7 applies it "in step 2", to the erased object's own events. So, for instance, a conversion that creates a contact from an input email leaves that email in the converting object's history after the contact is erased. Relatedly, ADR-0078 §2 redacts proposals "whose inputs carry a personal value" without saying whose: read literally, erasing one person invalidates every pending proposal that carries anyone's personal value. ADR-0078 is one of the three decisions taken by delegation, first in the author's review queue.

**Resolved by ADR-0087**, 2026-09-23: erasure follows the supersession chain, reaches the caller's event through the cause chain and the taint analysis, and the proposal clause names whose values.

*As recorded when open:*  Decide whether erasure follows the supersession chain, which the store can walk since `superseded_by` is stored. Decide whether a callee's erasure reaches the caller's event, which it can find through `cause_position` and the flow check 10 already traces. Narrow the proposal clause to the erased person's values, and bring ADR-0078's "wherever" into line with what is decided.

### D209
**The harness's deterministic racer cannot interleave a synchronous `request()`, and the library's open question on async cites the harness the wrong way round.** `adversarial-harness.md` §4 has the racer hold several connections in one process, "no thread races anything", and "make connection B do the same before letting A write", from which "the harness needs the store's API to be synchronous". But those statements are issued inside `Store.request`, which runs to completion on its caller's thread (`library-api.md` §6), and a synchronous call cannot be stopped between two of its statements within one thread. Stopping it needs one of two things. One is a thread per request, with an injected connection whose statements wait on a barrier the harness releases, which is a hook, the thing §4 rejected as invasive, though a small one. The other is a core written as coroutines, where every database call is a suspension point that a deterministic scheduler orders in one thread. `library-api.md` §8 keeps async open and counts the harness for a synchronous core; the second option counts the other way. Either way the store must take its connection source as an injected dependency, and ADR-0077 §7 lists only the id source and the clock. Found by reading.

**Resolved by ADR-0091**, 2026-09-23: the core is synchronous and takes its connection source as an injected dependency; the harness runs each request on its own thread behind a statement barrier and releases one statement at a time.

*As recorded when open:*  Settle the async question with this in view, and add the connection source to the injected dependencies.

### D210
**The settled position has no mechanism.** `DESIGN.md` §7 and `library-api.md`'s `EventPage.settled` promise "the highest position below which no transaction is still in flight". `storage-schema.md` §7 says that what the schema owes for it is "the oldest in-flight transaction's age", and that a consumer "must not advance its acknowledged position past `head - window`", which subtracts a duration from a position. Positions are allocated when a transition begins applying (ADR-0046 §5) and committed later, and on PostgreSQL a reader computing the bound cannot see another transaction's uncommitted rows. `of_event` carries nothing, such as the writing transaction's id, from which the bound could be computed. On SQLite, with writers serialised (D204), the window is empty and the question does not arise.

**Resolved by ADR-0089**, 2026-09-23: the log is read by a settled cursor, (transaction, position) bounded by the reader's snapshot, which a probe on PostgreSQL 16 showed cannot skip a late commit.

*As recorded when open:*  Say how the bound is computed, and add what the schema needs for it. Recording the writing transaction's id on each event and reading below the oldest one still active is a common technique on PostgreSQL, and it bounds by transaction rather than by age.

### D211
**An event row is completed at commit, and the schema says no event row is ever updated.** ADR-0046 §5 says "An event's identity is allocated when its transition begins applying, and its content is completed at commit", and D187's diagnosis rests on that allocation being a write on SQLite. `storage-schema.md` §2 declares `position INTEGER PRIMARY KEY AUTOINCREMENT`, which SQLite assigns only on insert, with `to_state` and `payload` `NOT NULL`, and says "Nothing else ever updates a row of `of_event`, and nothing ever deletes one", erasure excepted. Either the event is inserted with placeholder content and updated at commit, or its position comes from somewhere other than the insert. Neither is written down, and the first makes the append-only sentence false.

**Resolved by ADR-0089**, 2026-09-23: the position is allocated first and the event row inserted once, complete; nothing but erasure updates it.

*As recorded when open:*  Say which, and correct §2 or ADR-0046 to match. It matters beyond wording if a deployment ever enforces append-only on the table.

### D212
**The publish event has no object to belong to.** `publish-and-import.md` §1 records "a publish event that is the cause of every one of those" migration events, and §3 gives each migration event "the publish event as its cause". `of_event.object_id` is `NOT NULL REFERENCES of_object(id)` (`storage-schema.md` §2), but a publish is not an object, `of_declaration` is not in the directory, and the only built-in types are `Subscription` and `Proposal` (`DESIGN.md` §9).

**Resolved by ADR-0085**, accepted 2026-09-23: the built-in `DeclarationChange` is the object a publish event belongs to (`DESIGN.md` §9).

*As recorded when open:* Either a built-in object records publishes, or a migration event's cause is the declaration version it already carries and the publish event is dropped.

### D213
**On PostgreSQL the mint's second connection can exhaust the pool it is drawn from.** `storage-schema.md` §6 and ADR-0076 allocate a sequence value on a second connection while the request's own connection holds its transaction open. If both come from one pool, a burst of concurrent creations can hold every connection, each waiting for a second that none will release, until the pool's timeout fails them all. Inferred, not probed.

**Resolved by ADR-0090**, 2026-09-23: the mint draws from a pool of its own.

*As recorded when open:*  Say that the mint uses a dedicated connection or its own pool, sized independently of the request pool.

### D214
**Three shapes still cannot carry what the model says they carry.** `Stale` is `expected: int, actual: int` (`library-api.md` §4), but `DESIGN.md` §5.5 also returns `stale` when "the transaction exhausted its serialisation retries", where no `expected_version` need have been given. `Unsatisfied` carries one `object_id`, but a `dependent` verdict names "the parts" (`DESIGN.md` §5.3) and a `delegable` one "may name a capability and, if proposable, offer a Proposal" (§5.5), and it has a field for neither. And a tool schema renders a transition's inputs as properties beside `object_id`, `expected_version` and `idempotency_key` (`renderers.md` §3), and nothing in the syntax keeps an input from taking one of those names (§9.2 reserves only `any`, `terminal` and `superseding`, and only for states, categories and transitions), so an input called `object_id` collides with the request's own field.

**Resolved by ADR-0092**, 2026-09-23: `Unsatisfied` names its objects, capability and whether a proposal is accepted; `Stale` carries its cause; a tool schema nests inputs under `inputs`.

*As recorded when open:*  Make `Stale`'s fields optional or give it a cause; give `Unsatisfied` its objects and capability; either reserve the three request field names against inputs or nest inputs under a property of their own.

## Found by the design evaluation of 2026-09-23

Found while evaluating designs against the use cases of `PRD.md`, in writing the rule for metric guards (`data-driven-engine.md` §3.5). Recorded rather than repaired: the evaluation's decisions are all proposed, and this finding is about an accepted decision outside them.

### D215
**ADR-0049 consults an external evaluator before the write transaction and never says what happens when the evaluator's argument depends on state the transaction reads or changes.** Its decision 1: "A request that names external guards consults them first, outside the transaction, then opens the transaction and evaluates everything else." An evaluator's argument is an expression over the object (`xero.invoice_valid(order_id)`), and for a cascaded transition over an object the parent's outcome reaches (`for u in units { call u.sell() }`), the argument is a value read from committed state before the transaction and possibly different inside it — a concurrent request may have changed it, or the request's own parent outcome may have written it. The verdict is then about a different object or value than the guard it is recorded against. ADR-0084 (proposed) closes the same gap for metric guards by resolving arguments before the transaction and again inside it, consulting again within the serialisation retry bound on a mismatch. Found by reading, while writing that rule.

**Resolved by ADR-0092**, 2026-09-23: an evaluator's arguments are resolved before the transaction and again inside it, and one the outcome writes is refused at publish.

*As recorded when open:*  Apply the same rule to external evaluators, and say what an argument that the request's own outcome writes means: most likely that it is refused at publish, since no consultation before the transaction can see a value the transaction has not yet written.

## Found by mapping the design against the PRD, 2026-09-23

### D216
**A read set discloses the ids of objects its reader cannot see.** ADR-0088's read set lists "every object a guard or invariant read, with the version it read", a type-scan's matches included, and `history`, `pull` and `export` return an event under the visibility of the event's own object. A reader permitted to see a delivery therefore learns the ids of the objects its guards read, whether or not they may see those objects. ADR-0030 refuses exactly that: "existence is information". PRD T5: "Visibility rules apply to every read, including datapoints and metrics." Found while mapping T5 in `traceability.md`, by reading `DESIGN.md` §6's read-set paragraph beside §5.8.

**Resolved by ADR-0095**, 2026-09-23: a read filters the read set to what the reader can see and says that something was withheld; an auditor reads as an actor that can see everything.

### D217
**A metric guard's value is not recorded on the event, only its as-of time.** ADR-0084 §6: "The value and its as-of time go on the event beside the guard's name." `storage-schema.md` §4 said the payload "holds that value's as-of time", and `library-api.md`'s `Event` had `as_of` and no field for the value. A decision over a metric could therefore not be re-evaluated from the record (PRD L2). Found while writing ADR-0095's first decision.

**Resolved by ADR-0095**, 2026-09-23: the payload holds each consulted verdict or value beside its as-of time, and `Event.consulted` returns it.

## Found by reviewing the coverage claims against the PRD, 2026-09-23

One reviewer read every passage `traceability.md` cites against every clause of the PRD requirement it claims to cover, and tried to write the use cases in the grammar. Each finding was confirmed by reading the cited text before it was repaired.

### D218
**An observation kind's personal fields cannot be erased.** `declaration-syntax.md` §6.8 allowed "A field may be `personal`", and the expansion gave a kind no `erase`, with no way to declare one. Check 39 requires "a `call` to that type's own `erase`", so a subject with an `erase` and a personal observation field could not publish, and one without could not be erased. `DESIGN.md` §8's "It reaches observations through the subject's `erase`, since they are parts" had no mechanism behind it. PRD D8. Found by the coverage review of 2026-09-23.

**Resolved by ADR-0096**, 2026-09-23: each kind expands with a generated `forget`, which the subject's `erase` runs without a step and which may be requested on one observation.

### D219
**A correction is not validated, cannot be made on a terminal subject, and cannot be made by an agent.** §6.8 defined `corrects` as "an observation of the same kind and subject" and named only the guards `recorded_by` and `subject_open`, so nothing enforced the subject. A recorder allowed on one job could name another job's FAIL and, since a corrected observation is hidden from every read, open that job's gate. `subject_open` refused every correction once the subject was terminal, and the specification's own `ServiceJob` ended in a terminal `DONE`. The observation tool in `renderers.md` §3 had no `corrects` input and `additionalProperties: false`. PRD D4, D12, UC-19.

**Resolved by ADR-0096**, 2026-09-23: a third guard, `corrects_current`; `corrects` in the tool; a correction is exempt from `subject_open` (D231).

### D220
**A metric path, an attempt's read set and a refusal's value can reveal what their reader cannot see.** Metrics applied only "the source type's visibility predicate" (`DESIGN.md` §5.12), while a dimension reaches two hops, so `by x = i.object.engineer.login` read Users the reader may not see. `library-api.md` §6 filtered "an event's `reads`" and not an attempt's. A refusal's metric value, once added, would have gone to a requester who may not see the rows it aggregates. PRD T5. The same review asked how C2's "every consumer … gets the same value" reads beside T5, which no document said.

**Resolved by ADR-0096**, 2026-09-23: a path through a hidden object yields absence, and every result says whether it is complete for its reader; ADR-0095's filter covers attempts and refusals (D230, D235).

### D221
**A refusal does not carry the value it was decided on, and the attempt log does not record it.** UC-10: "the refusal names the value and the threshold". `Unsatisfied` had no value field, and neither `Attempt` nor `of_attempt` recorded a consulted metric value or evaluator verdict, so a refused data-driven decision could not be re-evaluated from the record (PRD L2).

**Resolved by ADR-0096**, 2026-09-23: `consulted` on `Unsatisfied`, `Attempt` and `of_attempt`.

### D222
**Overrides cannot be counted per reason, or with the import's excluded.** UC-3 asks for "override counts per target state and per reason, excluding the objects placed there by the data import". ADR-0084 promised "overrides by target state and reason". `DESIGN.md` §5.12 dropped the reason, and the `.transitions` rows carried neither a reason nor a way to tell the import's events, which keep provenance `asserted`.

**Resolved by ADR-0096**, 2026-09-23: `.reason` for a declared-enum reason, `.imported`, and the standard metric per target state and reason with the import's excluded.

### D223
**Cycle time cannot be attributed to the assignee who held the work, or split by who created it.** UC-18 asks "whether cycle time differs by engineer" as a standard metric, and none was listed. The specification's own example grouped state intervals by `i.object.engineer`, the current assignee, which moves a reassigned job's whole history to its latest engineer. `created_at` was not readable, and UC-16's "created by agents" had no member holding the creator's kind, although `data-driven-engine.md` claimed one.

**Resolved by ADR-0096**, 2026-09-23: `.held(<member>)`, `.created_at` and `.created_by_kind`, and the cycle-time standard metrics.

### D224
**An occurred time has no upper bound.** Backdating was bounded only "no further back" and `occurred within` only "how far back", so a forward-dated occurred time was accepted, which falsified "so no interval is negative" and could shorten a measured lead time. PRD D5, UC-19.

**Resolved by ADR-0096**, 2026-09-23: never later than the time it is recorded.

### D225
**Permanent refusal counts are monthly and nothing reads them.** `of_attempt_rollup` was monthly, while UC-2 counts per week, and no metric source, operation or diagnostic read it: the `.attempts` source was individual rows, which are pruned. Beyond the retention period, UC-2 could not be answered. PRD T3.

**Resolved by ADR-0096**, 2026-09-23: a daily rollup per object with the verdict, written in the prune's transaction, and the `<Type>.attempt_counts` source (D230).

### D226
**Legacy intervals contradict "the log can rebuild it".** `DESIGN.md` §7, ADR-0083 and ADR-0086 said the log rebuilds the interval index and the store keeps "no projection the log cannot rebuild", while `storage-schema.md` §6 said a `legacy = 1` row is "the one kind of row the log cannot rebuild", and the harness failed any "interval the log does not reproduce". The ported reassignments N5 depends on were in conflict with the harness.

**Resolved by ADR-0096**, 2026-09-23: legacy intervals are imported history, not a projection, and the harness checks them for shape (D234).

### D227
**A failed file deletion waits on a sweep that no operation owns.** ADR-0087: "A deletion that fails leaves the hash marked pending. A sweep retries it, and the pending set is readable." Neither exists among the seventeen operations, so the sweep was either a background process that erasure's completeness depends on (PRD N2) or work nobody owned.

**Resolved by ADR-0096**, 2026-09-23: the next erasure request retries pending deletions, and `diagnostics` lists them.

### D228
**An approval publishes on the dry run's impact report, however stale.** `publish-and-import.md` §1: approval "runs step 5", and step 3, which reads the live objects, runs only in the dry run. Objects that began violating a new invariant after the report was attached were never reported to the approver. PRD F7.

**Resolved by ADR-0096**, 2026-09-23: the publish recomputes the report in its transaction and is refused if it changed.

### D229
**Four statements stale after ADR-0095, three underspecified points, and five rows citing no implementation.** Stale: `DESIGN.md` §5.11 and ADR-0082 said corrected observations are hidden "by default"; `data-driven-engine.md` §3.3 kept the superseded backdating bound; `DESIGN.md` §5.7 bound an undeclared `model :=` dimension; `library-api.md` §5 said an `Event` carries only the as-of time. Underspecified: a metric guard leaving a declared dimension unbound; check 54 not reserving `recorded_by_kind`. And the traceability rows for D7, D9, M3, M4 and V5 cited no implementing document, against the map's own definition of covered.

**Resolved by ADR-0096**, 2026-09-23: each corrected; the unbound dimension is aggregated over; the checker now requires a covered row to cite an implementing document.

## Found by the second pass of the coverage review, 2026-09-23

The same reviewer read the repairs of D218 to D229 against the findings they claimed to close, and against the rest of the PRD.

### D230
**Two repairs of D220 and D225 still let a metric reveal what its reader cannot see.** The daily rollup had no object column (`storage-schema.md` §6), so after a prune `<Type>.attempt_counts` could apply no subject's visibility, and the same definition gave a partial reader different values before and after a prune. And a guard's recorded metric value was shown to anyone who could see every current object of the metric's *source* type, while its paths could reach related types the reader cannot see. Found by the second pass of the coverage review; the first leak was introduced by ADR-0096's own first draft.

**Resolved by ADR-0096**, 2026-09-23: the rollup keeps the object; a recorded or consulted value is tested against every type the metric reads. The first remedy here for live metrics, withholding, broke M2 and was replaced (D235).

### D231
**A result on a finished subject can never be corrected.** ADR-0096's first draft kept `subject_open` for corrections, so a wrong result on a terminal subject stayed wrong for good. PRD D4 ("a mistake is corrected by a newer datapoint linked to it") has no such exception, and no revision of it was proposed.

**Resolved by ADR-0096**, 2026-09-23: a correction is exempt from `subject_open`, since it adds no new fact to a settled whole.

### D232
**An erased personal enum survives in the interval index.** `DESIGN.md` §5.2: "Every **enum** attribute … is **tracked**", and nothing forbade marking an enum `personal`. `storage-schema.md` §9 skipped `of_interval` because "an interval over a personal attribute is never kept, since only enums and references are tracked", which does not follow. The erased value stayed readable through `intervals(<member>)` and `.held`. PRD D8, UC-17. Present since ADR-0083; both passes of the first review missed it until the second.

**Resolved by ADR-0096**, 2026-09-23: erasure redacts a personal enum's values in the interval index, as in the events it is rebuilt from. A first remedy, never tracking a personal attribute, narrowed D11 and was replaced (D235).

### D233
**The standard metrics' "open" and "completion" are undefined.** Work in progress, the oldest open work, open work per assignee and cycle time at completion (`DESIGN.md` §5.12, §5.13) all need to know when work is finished. Categories are a consumer's vocabulary, and the only built-in distinction was `terminal`, which means something else: nothing further will be recorded. Yet ADR-0070's report and the observation kind's `RECORDED` state already relied on `closed` having a meaning of its own. PRD M1, M6.

**Resolved by ADR-0096**, 2026-09-23: `closed` is built into every vocabulary: work is open until it enters a `closed` or a terminal state, and completes when it first does (D235).

### D234
**Six gaps left by the first repairs.**
- The no-future bound was in `DESIGN.md` and not in the specification's §4.2, which wins where the two disagree.
- The approval re-check compared counts, so a different set of violating objects of the same size would pass.
- `.created_at` and `.created_by_kind` of a ported object reflected the import, not the legacy creation.
- An ordinary type could still declare a `created_at` of its own, shadowing the built-in.
- The harness checked legacy intervals against "the import's record", which nothing stores.
- `diagnostics(type)` was said to list pending file deletions, which carry no type and which its row did not mention.

Also, UC-3's per-reason split existed only for enum reasons, and every assertion in the specification took a free-text reason.

**Resolved by ADR-0096**, 2026-09-23: each corrected: §4.2's bound; ids compared at approval; legacy creation supplied by the mapping; the built-in names reserved by check 33; legacy intervals checked for shape; pending deletions listed through `of_file_ref`; the machine example's assertion takes an `OverrideReason`.

## Found by the third pass of the coverage review, 2026-09-23

### D235
**Withholding a metric group broke M2, leaked dimension values, and three smaller gaps.** ADR-0096's second draft answered D220 by showing a metric group only to a reader who could see every row it aggregated, and returning any other group `withheld`.
- The standard metrics group by state and week, not by whatever a reader's visibility selects, so a person or agent who could see 99 of 100 objects got nothing from any of them. PRD M2 promises metrics "under the same visibility rules as the objects they are computed from", and UC-11 held only for an agent seeing every purchase order.
- A withheld group kept its dimension values, so a group made wholly of hidden rows disclosed an assignee, a customer or a week of activity (PRD T5).
- `diagnostics` aggregated over the whole type under no visibility rule at all.
- A terminal state outside `closed` left work open forever in the standard metrics, with nothing to flag it.
- Never tracking a personal attribute, the remedy for D232, narrowed PRD D11's "every choice-from-a-list attribute" with no revision proposed.

Found by the third pass, which checked only the second round's repairs.

**Resolved by ADR-0096**, 2026-09-23: a metric is computed over the rows its reader can see, with a path through a hidden object yielding absence, and says whether it is complete; `diagnostics` follows the same rule; a terminal state counts as finished; a personal enum is tracked and redacted at erasure.

## Found by the five-slice review against the PRD, 2026-09-23

Five reviewers read the record independently: the functional and datapoint requirements, the computation and convergence requirements, trust and the first consumer, the decision record, and the documents' agreement with each other. Each finding was confirmed against the files before it was repaired.

### D236
**The flow change had states and no lifecycle, and its authority was named nowhere.** `storage-schema.md` listed `DeclarationChange` states in a `CHECK` constraint, one of them (`submitted`) mentioned by no document, and no transition, guard or capability declared them. "The drafting capability" and "the publish capability" (`DESIGN.md` §9, `publish-and-import.md` §1) were never named, so PRD F7's "who may draft and who may approve are declared" failed. Nothing enforced "a person, when an agent drafted it" beyond a column. `DESIGN.md` said approval publishes while `library-api.md` had `publish()` install an approved change; a refused publish returned the change to no defined state. A reviewer writing UC-15 had to invent four transition names and two capabilities.

**Resolved by ADR-0097**, 2026-09-23: a built-in type with `draft`, `revise`, `submit`, `refresh`, `publish`, `reject`, `withdraw` and `supersede`, `publish` being the approval, and six built-in capabilities.

### D237
**A stale flow change could reinstall text that drops a newer guard.** A change "is superseded when the installed version moves under it" (`DESIGN.md` §9), but nothing wrote `superseded`, and the publish refused only when the live-object findings changed. A change drafted against version 7 and approved after version 8 republished version 7's module and dropped version 8's guards, and the report the approver saw never showed it. PRD T1, F7.

**Resolved by ADR-0097**, 2026-09-23: `publish` requires a change drafted against the installed version, and supersedes every older open change in its transaction.

### D238
**The impact report said nothing about changed guards, and the evidence was not recorded.** Step 3 read invariant violations, removed states and invalidated proposals only (`publish-and-import.md` §1), so a new guard, or removing an `observe` marking (UC-14 to UC-15), changed which live objects could move with nothing reported. The evidence was "links, as JSON" to metrics that are computed on read, so what the approver saw was not in the record. And the drafter/approver separation compared actor ids only, so an agent drafting for Alice could be approved by Alice. PRD F7, V4.

**Resolved by ADR-0097**, 2026-09-23: the report names objects gaining or losing an available transition and those a newly enforced clause refuses; an evidence snapshot; the principal rule.

### D239
**A proposal's actor guards were re-evaluated in the model and not in the decision.** `DESIGN.md` §9: "every guard is re-evaluated at execution … actor guards included"; ADR-0036 decision 3: "Every non-actor guard is re-evaluated at execution", repeated by ADR-0044. No decision recorded the change. The ADR's reading would let an approver execute a transition they could not request themselves. PRD T1. Found by the decision-record review.

**Resolved by ADR-0097**, 2026-09-23: the model's rule is recorded, and ADR-0036 annotated.

### D240
**The standard metrics were phrases, not definitions, and some could not be written.** `DESIGN.md` §5.12 and §5.13 listed them; no document gave a name `metric()` accepts, a source, dimensions or a value, and the renderers could not list them. A refusal rate needs two datasets and a metric had one `from`; "someone other than the assignee acts" needs the actor's id, which no row carried; handoffs per object needs two levels of aggregation. PRD M1, M6.

**Resolved by ADR-0098**, 2026-09-23: the standard metrics are declarations in `declaration-syntax.md` §6.11, with combined metrics, per-object flow collections and `.actor_id`.

### D241
**A reader could not read the value a rule reads.** A guard could bind a metric's dimensions, window it and aggregate over an unbound dimension; `metric(actor, name, filter, cursor)` could do none of these, and its filter's grammar was unstated. A screen could not show the 30-day value a guard refused on. PRD C2, C3, UC-10, UC-11.

**Resolved by ADR-0098**, 2026-09-23: `metric()` takes the same arguments as a guard's reference, and its filter is an expression over the declaration's binder.

### D242
**Most sources could not be split by version, and attempts had no actor.** Rows from a type, an observation kind, labels and `attempt_counts` had no `.declaration_version`, nor did the rollup, so M3's "any metric" failed. `<Type>.attempts` exposed neither actor nor object, so UC-2's example — one agent repeating a refused request — and UC-14's "who would have been refused" could only be answered by export.

**Resolved by ADR-0098**, 2026-09-23: `.declaration_version` on every source and the rollup; `.actor_id` and `.object` on attempt rows.

### D243
**Set-valued dimensions, distinct counts and derived attributes in rows were undefined.** UC-1's "by product configuration" is a path through a set and three hops long; UC-13's "how many jobs carry the label" needs a distinct count; a derived attribute read by a metric row had no visibility rule, so UC-12's low-stock figure either leaked hidden units or varied by reader unmarked.

**Resolved by ADR-0098**, 2026-09-23: a set-valued dimension counts a row once per member, hops count from the row's object, `count(distinct …)`, and derived attributes are read under the reader's visibility and count toward `complete`.

### D244
**C2, UC-8 and UC-10 were read in a way that departs from their wording, with no revision proposed.** `DESIGN.md` §5.12 narrowed C2 to "every reader who gets a complete value" and ruled that T5 prevails over UC-10's "the refusal names the value", while `traceability.md` marked both covered. The PRD's status line says a design that finds the PRD wrong proposes a revision rather than working around it.

**Resolved by ADR-0098**, 2026-09-23: PRD §12 proposes revision 4 for C2, UC-8 and UC-10, and `traceability.md` marks the three rows `revision proposed`, a status its checker allows only for rows PRD §12 names.

### D245
**Adding a datapoint kind took two edits.** Check 54 required the subject to declare the kind's part, so adding a kind edited the subject's text and advanced its version (check 22). PRD D3: "adding a kind of datapoint is one declaration and no code".

**Resolved by ADR-0099**, 2026-09-23: `observation … on <Type> as <collection>`, and the subject is not edited.

### D246
**An observation's occurred time had two channels, no column and no verdict.** The syntax and the tool made `occurred_at` an input of the generated `record`; `library-api.md` made it a request field "only on a backdatable transition"; the observation table had no column for it, while `<Kind>.occurred_at` was a metric member. No clause named the bound, so the refusal UC-19's second route relies on could name no rule or remedy and could not be counted. PRD D5, D2, F4.

**Resolved by ADR-0099**, 2026-09-23: a request field for both, a generated guard `occurred_within`, and an `occurred_at` column on every observation table.

### D247
**Recording could not be discovered.** `availability(id)` listed only an object's own transitions and `check` needed an object id, so nothing said whether a creation — an observation, a label — was permitted. The observation tool nonetheless told agents to call `availability(subject_id)`, and put `subject_id` where `Request` has no field. PRD F3.

**Resolved by ADR-0099**, 2026-09-23: `availability(subject)` offers recording and labelling, `check` accepts a type for a creation, and the tool carries `subject` as an input.

### D248
**What recording writes to the subject was unsaid.** "Recording does not write the subject" (`DESIGN.md` §5.11), yet a later result must invalidate `changed_since([inspections])`, which reads a position stamped per part relationship, and §6 step 7 counts a destination whole as written.

**Resolved by ADR-0099**, 2026-09-23: recording stamps the collection's position and nothing else, and conflicts only with a transition that read the collection.

### D249
**An optional reference could never be cleared.** Check 17 rejected `clear` on a relationship end, and ADR-0073 said an end is cleared "by writing the reference itself" — but there is no `null` to write, and an unsupplied optional input skips its write. An assignee could never be removed, nor a binding released, and D10 and D11 could never record absence after creation.

**Resolved by ADR-0099**, 2026-09-23: `clear` reaches an optional, singular, stored `ref` end.

### D250
**A publish met live objects without recording what it did to them.** A new attribute was added "nullable, or with the declared `default`" by DDL: with a default, every live row changed with no event and replay no longer reproduced it (T3); without one, live objects lacked a required attribute (T1). The mapping forms could not supply values, map a removed enum member, or say what removing a type means, and a field added to an observation kind could never be filled. PRD F5, T1, T3.

**Resolved by ADR-0099**, 2026-09-23: DDL never writes a live row; `backfill` and `removed member` mappings applied as recorded migrations; new observation fields optional; removed types retired.

### D251
**The import had no operation and nothing limited it.** None of the seventeen operations imported, and `Request` could not carry legacy entries, intervals or creation times, so the importer either wrote tables directly — the path `DESIGN.md` §13 says voids the guarantee — or used the built-in assertion through `request`, which any holder of the deployment capability, such as a mirror's recurring refresh, could then use to set state on owned types, recorded as `imported` and hidden from `exceptions()` and the override metrics. `.imported` had no column. PRD N5, T1, T4.

**Resolved by ADR-0100**, 2026-09-23: `import_batch`, limited to mirrors and types no ordinary request has written, and `of_event.imported`.

### D252
**Pruning and archiving had no operation.** Pruning the attempt log with its rollup, and moving events to the archive, were "deployment actions" with no operation, although `DESIGN.md` §13 says anything else reaching the database voids the guarantee. PRD N2.

**Resolved by ADR-0100**, 2026-09-23: `maintain`, which changes nothing any read returns and is never needed for correctness.

### D253
**Erasure missed six copies of a person.** `of_external_id` values of a personal external identifier (ADR-0051) survived, so `lookup` still resolved an erased email; a proposal's own creation event kept its inputs; values the erased object passed down by `call` or `create` into other objects' personal attributes were not reached; corrected observations were outside every read and so outside the part's erasure; legacy payloads carried other objects' fields; and legacy creation fields were not named. PRD D8, T5, UC-17.

**Resolved by ADR-0100**, 2026-09-23: each is reached, and legacy entries keep only their own object's fields.

### D254
**Verdicts named objects their requester could not see.** `Unsatisfied.objects` and `InvariantViolated.objects` come from type-scans, which ignore visibility (ADR-0030), and only read sets were filtered, so a uniqueness conflict returned the id of an object `get` reports as not found. PRD T5.

**Resolved by ADR-0100**, 2026-09-23: a verdict names only visible objects and sets `withheld`.

### D255
**Three execution steps were missing, and one was wrong.** No step of `DESIGN.md` §6 wrote the idempotency record or discharged an admitted violation; nothing stopped a part being re-parented into a whole in a terminal state (a harness failure condition); and step 6's "any failure aborts" made an observing clause on a cascaded transition refuse, which check 57 allows on an `only via` transition. PRD T1, V3.

**Resolved by ADR-0100**, 2026-09-23: each written: the record with the event, discharge in step 7, a generated `whole_open` guard, and observing clauses refuse nothing at any depth.

### D256
**The export and subscriptions still assumed positions.** Attempt rows had no writing transaction, so they could not be paged by the settled cursor; an interval updated when it closed was never re-emitted to a consumer that had paged past it; subscription lag was `log_head - acknowledged`, which a cursor cannot compute, against an INTEGER threshold (ADR-0043). PRD M5, T3.

**Resolved by ADR-0100**, 2026-09-23: `of_attempt.txn`, intervals paged by their last-changing event, and lag as the age of the oldest unacknowledged event with duration thresholds.

### D257
**Mirror reads had no as-of time, and evaluators were not injected.** A guard reading a mirror read whatever the last refresh left, with no as-of time or bound, unlike an evaluator's verdict (`DESIGN.md` §13 listed only evaluators and metrics). The evaluators were not among the store's injected dependencies, so a harness run with an external guard could not be reproduced. PRD N2, N4.

**Resolved by ADR-0100**, 2026-09-23: `.imported_at` on every mirror object, boundable by a guard; an `EvaluatorSource` injected.

### D258
**The port and the cutover assumed more than the first consumer has.** Legacy intervals could not have gaps, though transition audit rows were not written from 2026-02-03 to 2026-07-13 (legacy commit `e675165`) and side-effect status changes were never audited (`wr:app/core/state_registry.py:85-131`); a ported reassignment lost who made it; the cutover's write edges came only from the registry's side-effect lists and missed the service layer's writes (`wr:app/services/intake_batch_service.py:848-851`, `shipment_service.py:281-285`, `service_service.py:928`); and time-driven transitions need a scheduler the legacy system does not have (`wr:app/models/warranty.py:345-372`). PRD N5, N2, UC-18.

**Resolved by ADR-0100**, 2026-09-23: gaps allowed and reported, `of_interval.entered_by`, write edges from every write, and a scheduler as a cutover prerequisite.

### D259
**Fourteen older decisions were contradicted by the current model with nothing marking it, and thirty back-links were missing.** Among them: ADR-0039 still said a hot row queues; ADR-0037 said the store keeps no undeclared projection; ADR-0031 deleted a file on any erasure; ADR-0032 and ADR-0021 put scoring and sums outside the store; ADR-0056 left `erase` optional; ADR-0026 said categories have no fixed member; ADR-0048 and ADR-0057 kept one part position per whole; ADR-0047 cited the wrong ADR; and `scripts/check-corpus.py` accepted any "Refined by" line as a back-link, whichever ADR it named. Found by the decision-record review.

**Resolved**, 2026-09-23: each annotated in place with the decision that changed it, the back-links added, the ADR index's status column regenerated from the headers, and the back-link check made to require the specific ADR.

### D260
**Smaller drift between the documents.** Check 11's observation exemption sat on the iterations table's row 11, not the check table's; §9.4's type production lacked `mirror` and §5.1's head lacked `backdatable within`; `mirror`, `combine`, `as` and others were not reserved; the rendered Delivery said `tracking record` against the syntax's `tracking serial`; `of_attempt.type` could not record a request naming an unknown id; a proposed creation had no target to store; `diagnostics` took no window; `Attempt` lacked the applied event and `Interval` its transition; `ReadSet` could not hold an empty type-scan; `DESIGN.md` counted five versioned kinds and three metric sources; the rendered rule set could not list standard metrics; and `check-api-doc.py` claimed to compare what it did not. Found by the cross-document review.

**Resolved**, 2026-09-23: each corrected in place, and the API checker's message says it compares operation names only.

## Found by verifying the five-slice repairs, 2026-09-23

Four of the five reviewers checked their own findings against the repairs, and wrote the use cases again.

### D261
**The import could still write an owned type nobody had created an object of.** ADR-0100 limited `import_batch` to a mirror "or while no ordinary request has created an object of it", so a cut-over type whose objects were ported and transitioned daily stayed importable until someone created one, and every newly published type until its first use; a refresh's writes were marked `imported` and so excluded from `exceptions()` and the override metrics. `ImportBatch.admitted` carried no reason, which `of_admission.reason` requires. PRD T1, T4. Found by the verification of ADR-0100.

**Resolved by ADR-0101**, 2026-09-23: the import writes only mirrors; every type is ported as one; admissions carry reasons; an erased value is never restored.

### D262
**An approval could install a report its approver never read.** `refresh` replaced the attached report and was open to the drafter, `impact_unchanged` compared against whatever report was attached at commit, and `publish` took no version, so an approver who read one report could install the impact of the next. PRD F7, T1. Found by two verification reviewers independently.

**Resolved by ADR-0101**, 2026-09-23: `publish` names the version of the change the approver read, and `refresh` advances it.

### D263
**The flow change showed what verdicts hide.** `DeclarationChange` declared no visibility, so any actor could read it, and its report's `affected` ids and its evidence's metric values disclosed what ADR-0100 withholds from a verdict. PRD T5.

**Resolved by ADR-0101**, 2026-09-23: visible to drafters and approvers only, with its ids and values filtered like a verdict's.

### D264
**Pruning had no floor.** `MaintenanceTask(name, before)` had no minimum age, so a mistaken cut-off deleted refusal records inside the retention period T3 requires, and `DESIGN.md` claimed `maintain` "changes nothing any read returns" although pruned rows leave the attempt sources. PRD T3.

**Resolved by ADR-0101**, 2026-09-23: the store is built with its attempt retention, and `maintain` refuses a prune inside it; the claim is corrected.

### D265
**Migrations could not fill a reference, and their reach was unsaid.** `backfill` covered attributes only, so a new required reference or part had no form; nothing said whether a migration reaches a terminal object, which admits no further transitions, or an observation, which D4 says is never edited — and the specification's own `removed member` example could only match terminal robots; and a `backfill` copying a personal value escaped check 10. PRD F5, D4, T1, D8.

**Resolved by ADR-0101**, 2026-09-23: migrations reach terminal objects and never observations; `backfill` fills references; a new required part on live objects is refused; check 10 covers `backfill`.

### D266
**A mirror could not be erased.** A mirror declares no transitions, `erase` included, while erasure happens only where a type declares one, so the first consumer's customer — a mirror until its cutover — could not be erased here, although the specification promised erasure "redacts this store's copy". PRD D8, UC-17. Present since ADR-0075; found by the verification of the first consumer's use cases.

**Resolved by ADR-0101**, 2026-09-23: a mirror may declare `erase`, and the import never rewrites what it erased.

### D267
**Four standard metrics counted the wrong thing.** Most counted the import's own events, so every ported closed job completed in cutover week and the importer counted as a non-assignee; a span still open on a finished object — an engineer named on a closed job — grew with `now` for ever, and so did time in a final state; no standard metric named the jobs that changed hands more than once (UC-18); and most declared neither version nor actor kind as a dimension, while a combined metric had no binder to filter by (M3, M4). Also `.returns` counted an `act` as rework, `avg`'s type was unstated, and whether an unwritten reference has an interval from creation was unsaid. PRD M1, M6, M3, M4, UC-16, UC-18.

**Resolved by ADR-0101**, 2026-09-23: the import's events are excluded, finished spans clipped, every standard metric carries `version` and `actor_kind`, two per-object metrics name the objects, and the definitions are tightened.

### D268
**Residues of the repairs.** `whole_open` on every transition writing an owner would have refused an observation's correction on a finished subject; `of_migration` could hold neither new mapping; the state was a tracked member in §6.9 and not in §8.3, on which `oldest_open` depended; `.open` and `.imported_at` had no spelling or reservation; §6.11 said its definitions were checked when no checker read them; `TransitionOffer` could not tell two kinds' `record` offers apart; `check` could not test an occurred time; `draft` had no inputs for its evidence; `Event` lacked `imported` and `Attempt.type` could not be absent; legacy intervals reused a shape needing fields a port cannot know; a label's note survived in its own event; and several statements and headers had drifted. Found by the verification reviewers.

**Resolved by ADR-0101**, 2026-09-23: each corrected; the standard metrics are instantiated as a checked block.

## Found by verifying ADR-0101's first draft, 2026-09-23

### D269
**Porting every type as a mirror contradicted check 53, so wholes, parts, families and datapoints could not be ported.** ADR-0101's first draft limited the import to mirrors and said every type is ported as one, while check 53 refused a mirror that takes part in `extends` or composes with a `part`/`owner`, and an observation kind cannot be a mirror. So no delivery with its checklist items and approvals, no family member, and no legacy datapoint could be ported by any path. PRD N5, D6. Found independently by three verification reviewers.

**Resolved by ADR-0101**, 2026-09-23: mirrors may compose with and extend each other, check 53 refuses only a mix, and the import writes the observations and labels on a mirror's objects.

### D270
**An approval's version was optional, and a publish had no verdict.** `publish(…, expected_version: int | None = None, …)` let an approver omit the version and meet the race it was added to prevent, and `publish` returned only a `PublishReport`, which could carry neither the `stale` refusal nor the named refusal clauses nor a `withheld` flag. PRD F7.

**Resolved by ADR-0101**, 2026-09-23: the version is required for any publish but a dry run, and `publish` returns a `PublishResult` with the verdict beside a report that carries `withheld`.

### D271
**A value written after an object first closed had a negative duration.** A current span on a finished object ran to the object's first completion, so on an object kept in a `closed`, non-terminal state for later records — as the specification advises — or reopened and closed again, a span beginning after that point ended before it began. PRD D11.

**Resolved by ADR-0101**, 2026-09-23: a current span on an object now in a `closed` or terminal state runs to its entry into that state only if it began before it.

### D272
**A reader's metric collapsed its groups, and the per-object metrics were not aggregates.** `metric()` aggregated over every dimension the reader did not bind, so `handoffs_by_object` returned one value unless the reader already knew each job's id, and UC-1's "which states hold work longest" took a call per state. The two per-object metrics' values were row expressions, not aggregates over the group. PRD UC-18, UC-1.

**Resolved by ADR-0101**, 2026-09-23: a reader keeps every declared dimension by default and may name fewer with `keep`; only a guard aggregates over what it leaves unbound; the values are aggregates.

### D273
**A publish's migrations were counted as flow.** Migration events carry provenance `migrated` with the publisher as actor, and ADR-0101 lets them reach terminal objects, but no row member marked them and the standard metrics excluded only the import, so a state mapping or a `backfill` inflated throughput, rework and cycle time and counted the publisher as a non-assignee. Also: stale "single carve-out" and "no transitions" statements about erasure and mirrors, "tracked from its first write", an interval row opened only on a write, a `LegacyInterval` with no actor kind, an evidence reference with no shape, the live-offer example without `creates`, and a misplaced clause about `avg`. PRD M1, M6.

**Resolved by ADR-0101**, 2026-09-23: `.migrated` is a row member and every standard metric over transitions excludes it; the statements and shapes are corrected.

### D274
**Observations on a mirror contradicted check 53, and four residues.** ADR-0101 let the import write the observations and labels on a mirror's objects, but an observation kind expands into an owned type whose `owner subject` pairs with the mirror, which check 53 as worded refuses; and nothing said whether an ordinary request may record or label on a mirror's object, which would give the mirror's collection two writers. Also: a span beginning after an object closed still accrued time; §2 still said a mirror "neither extends nor is extended"; the standard metrics' `version` and `actor_kind` were said to be left unbound by a reader, which now keeps every dimension; ADR-0098 §3 still described a read as aggregating unbound dimensions; a ported observation had no occurred time; and the `publish` bullet omitted the `may` clause. Found by the fourth verification pass, three reviewers naming the first independently.

**Resolved by ADR-0101**, 2026-09-23: an observation kind's collection on a mirror is exempt from check 53, and a generated `subject_owned` guard refuses a `record` or `label` request on a mirror's object; no span accrues on a finished object; the wording, annotation and shapes are corrected.

## Found by auditing production against the design, 2026-09-24

Four read-only audits of the first consumer's production code, one per slice, compared every rule they found with the design ([`first-consumer-audit.md`](first-consumer-audit.md)); writing the unit's full lifecycle as a module ([`unit-journey.md`](unit-journey.md)) found the rest. Each entry below was re-verified against both repositories before it was recorded.

### D275
**A duration cannot be divided by a duration, so a utilisation cannot be declared.** The first consumer's leasing decision asks first how much the pool was used (`wr:docs/adr/0002-unit-engagement-and-leasing-model.md`, "Rejected alternatives": "how much did we use it?"). Both halves are declarable as metrics — the time units spent on loan and the time they spent in the pool (`unit-journey.md` §2) — but not their ratio: `declaration-syntax.md` §8.3 allows `/` only with a scalar on the right and names no conversion from a duration to a number, and a combined metric's value is arithmetic over its parts. Every share of time — utilisation, the fraction of a job's life spent waiting, on-time share by duration — meets the same wall.

**Resolved by ADR-0103**, 2026-09-24: a quotient of two durations, or of two amounts of one currency, is a `decimal(19,6)`; `unit-journey.md` declares `pool_utilisation` per model over the whole history. Per month it is not declarable, since a metric buckets a span by when it began (D287, `edge-cases.md`).

### D276
**What an unknown filter does inside an aggregate or a loop is unsaid.** `declaration-syntax.md` §8.2 defines `unknown` for operators and for the four places a whole expression is evaluated — a guard, an invariant, a visibility predicate, a derivation — but not for the `where` of an aggregate or of a `for`, which is evaluated once per element. So `none(s in slots where s.role == Role.PRIMARY and not s.filled)` with an absent `role` either counts the slot or does not, and nothing says which. Production decides it one way: a slot with no role never blocks completion (`wr:app/core/state_registry.py:544-545`).

**Resolved by ADR-0103**, 2026-09-24: in a metric's filter or an outcome's `for`, an element whose filter is unknown is not selected, as SQL's `WHERE` leaves out a row; inside a guard, an invariant, a visibility predicate or a derivation it leaves the aggregate unknown, so a guard fails closed (D284). Production's slot rule is written with a presence test, and the publish report lists every guard and filter that can be unknown through an optional it does not test.

### D277
**Idempotency records have no operation that removes them.** ADR-0100 §2 holds that nothing writes the database but the nineteen operations, and `maintain` offers two tasks, pruning attempt rows and archiving events. The idempotency record written with every keyed request (DESIGN.md §6 step 2) has neither a task nor a retention the store is constructed with; `storage-schema.md` ("Still open") leaves its retention a number to measure, which nothing could then apply. Production has the same hole and a script for it: its keys "accumulate forever unless pruned" (`wr:app/core/idempotency.py:55-58`), and `wr:scripts/maintenance/prune_idempotency_keys.py` prunes them when someone runs it.

**Resolved by ADR-0103**, 2026-09-24: `maintain` gains `prune_idempotency(before)`, refused inside the idempotency retention the store is built with; a retry with a pruned key is a new request.

### D278
**Check 15 makes a closed state need an exit that §4.2 calls optional.** `declaration-syntax.md` §4.2 advises an object that is finished but must still accept records to sit in "a `closed` state that is **not** terminal, with a genuinely final state after it if one is needed". Check 15 refuses any non-terminal state with no outgoing `do`, so the final state is never optional. Production lets a retired unit be edited and relabelled with no way out of `RETIRED` (`wr:app/services/inventory_item_service.py:480-536`; `wr:app/services/label_print_service.py:174-197`); here that needs an invented exit, or `RETIRED` stays terminal and loses the edits, as `unit-journey.md` chose.

**Resolved by ADR-0103**, 2026-09-24: check 15 exempts a `closed` state, which is finished work and may end a lifecycle; `unit-journey.md`'s `RETIRED` is closed and takes relabels, as in production.

### D279
**The record says the first consumer keeps no stock level, and it does.** `edge-cases.md` ("A consumer that maintains a counter") says the first consumer "stores no stock level at all", and `declaration-syntax.md` §11 question 10 defers the counter as "unexercised by the consumer the design was drawn from". Production decrements `Accessory.quantity` when a service consumes accessories and clamps it at zero (`wr:app/services/service_service.py:751-773`), on a serial-tracked row, which check 31 refuses a `counter` on.

**Resolved by ADR-0103**, 2026-09-24: `edge-cases.md`, question 10 and every other statement that the first consumer stores no stock level are corrected; the design would model a bulk lot as a `tracking quantity` type with a counter and `nonneg`, declared by the table-to-type mapping, and production's clamp at zero would become a refusal.

### D280
**The walkthrough has drifted from the grammar and from production.** `first-consumer-walkthrough.md` §2.3 guards `retire` on `actor.role == ADMIN`, which ADR-0079 replaced by a capability; its §5 has `flag_missing` leave a unit in `PROCUREMENT` and a commit loop that fills pegs, where production cancels a missing unit (`wr:app/services/shipment_service.py:985-1055`) and has retired the commit-time fill (`wr:docs/design/explicit-delivery-assignment.md`; `wr:app/services/intake_batch_service.py:912-934`).

**Resolved in place**, 2026-09-24: the guard reads `actor.has(ADMIN)`, and §2.1 and §5 point to `unit-journey.md`, which is read against production as it now is.

### D281
**The syntax checker refused two legal forms.** A machine's `clear` of a reference it `requires` was reported as an undeclared name (check 19), since the check looked for the name among the machine's own members, which a machine has none of; and a dataset row's `.object` was counted as a dimension hop (check 56), where §6.9 says it is not one. Neither form appears in the specification's examples, so its clean runs never exercised them. `unit-journey.md` uses both.

**Resolved in place**, 2026-09-24: a machine's `clear` is held to its requirement being an optional, singular reference or attribute (a new fixture for check 17), each binder's end still being checked when the machine's transitions are analysed as the binder's; `.object` on a dataset row is not counted. Both were probed in each direction.

## Found by reviewing the design against the PRD with the unit's journey, 2026-09-24

### D282
**The unit's journey broke UC-6: its receipt could not be backdated.** The specification's unit had `receive … backdatable within 2 days`. `unit-journey.md` made `receive` `only via Shipment.receive_unit`, as production reaches it only through the shipment, and check 58 rightly forbids the marking on an `only via` transition. The parent `receive_unit` was not backdatable either, so a receipt recorded the morning after ended the unit's time in `PROCUREMENT` when it was recorded, which UC-6's acceptance refuses.

**Resolved by ADR-0103**, 2026-09-24: `Shipment.receive_unit` is `backdatable within 2 days`, and the unit's `receive` records its parent's occurred time (ADR-0095).

### D283
**The checker counted an action as a way out of a state.** Check 15 says "an `act` is a self-transition and does not count as outgoing, and being an `assert` target does not count as reached". `scripts/check-syntax-doc.py` added every `act`'s state to both the outgoing and the reached sets, treated `at any` as a from-state list, and counted an assertion's targets as reached, so a live state with only an action, or reached only by an override, passed. Every document stayed clean once it was corrected, so no example had relied on it; a live state with only an action, and a state reached only by an assertion, are now check 15 fixtures.

**Resolved in place**, 2026-09-24.

## Found by verifying ADR-0103's first draft, 2026-09-24

An independent reader checked the first draft against the PRD, the rest of the record and production's code; each finding below was re-read at its cited lines before it was recorded.

### D284
**An unknown filter satisfied an exclusion in a guard.** The first draft made every element whose filter is unknown unselected, in guards as elsewhere. D20 was resolved by making a guard over an erased value fail (ADR-0047): `none(a in approvals where a.approver == actor.id)` over an erased approver is unknown and refuses. Under the draft, that element would be skipped and the guard would pass, restoring self-approval by an erased person. The production rule it was written for — a slot with no role never blocks completion — did not need it, since a presence test makes the filter definite.

**Resolved by ADR-0103**, 2026-09-24: an unknown filter selects nothing only in a metric's filter and an outcome's `for`; inside a guard, an invariant, a visibility predicate or a derivation it leaves the aggregate unknown.

### D285
**The draft withheld who acted more strictly than production, and left the assignment metrics incomplete.** It declared `actors visible when actor.has(AUDIT_VIEW)`, on the audit's reading that production shows its audit trail only to administrators. Production shows each record's activity, with usernames, to anyone who may view the record (`wr:app/api/deliveries.py:966-1035` under `DELIVERY_VIEW`, which every human role holds, `wr:app/core/permissions.py:23-30`), and `created_by` on every record (`wr:app/schemas/base.py:27-36`); `AUDIT_VIEW` gates only the cross-record trail (`wr:app/api/audit.py:25-290`). The rule would also have made `acted_by_non_assignee` and every metric comparing identities incomplete for ordinary readers, against M2 and M6, and still left identities readable through intervals, legacy payloads, subscriptions and attributes copying an actor's id.

**Resolved by ADR-0103**, 2026-09-24: withdrawn. Who acted is shown to every reader of the object, as production shows it, and the cross-record trail is the export and `pull`, served from routes the consumer authorises.

### D286
**The draft's serial had no port path and no rule for an absent maker.** Nothing advanced the store's sequence past production's, so the first serial minted after cutover would collide with a ported one and be refused by `unique`. Production drops the maker's segment where a model has no manufacturer, which is nullable (`wr:app/services/serial_number_service.py:150-155`; `wr:app/models/robot_models.py:26`), and the draft said nothing about an absent placeholder or where `maker_code` comes from.

**Resolved by ADR-0103**, 2026-09-24: an import batch carries each sequence's high-water mark, which the store raises the sequence to; `[ … ]` omits a segment whose placeholder is absent, and an optional attribute may be named only inside one (check 44); `RobotModel.maker_code` is optional and filled by the port with production's derivation.

### D287
**The draft's monthly utilisation divided the wrong numbers.** Both of its metrics bucketed a span by `month(i.entered_at)`, so a unit that joined the pool in January put all its pool time in January, and a loan in March was divided by March's pool time, which could be zero or smaller than the loan.

**Resolved by ADR-0103**, 2026-09-24: `pool_utilisation` is per model over the whole history, and apportioning a span across buckets is recorded as a known limit.

### D288
**Residues of the draft.** It did not say how `requests by` treats a proposal's approval, the request an approval executes, `publish`, the agent's tool schemas, or `check` and `availability`; it claimed the rule matched production, which asks for the key only on an agent's POSTs; it asserted a bulk-lot type the record does not declare; it called a shipment past its eta production's "procurement overdue", which is an order past its expected arrival; the new vocabulary had no check; the promised report of unknown filters, §8.3's division bullet, the maintenance prose, the key's terminology, three ADR back-links, three statements that the first consumer keeps no stock, ADR-0102's context and several counts were not brought into line; `length` in an indexed derivation needed an indexed attribute, and NUL would have counted differently on the two backends.

**Resolved by ADR-0103**, 2026-09-24, and in the documents it names.

## Found by an operations review of the worked example, 2026-09-24

### D289
**A unit bought out of a lease could not be serviced for its buyer.** `ServiceJob.open` in `unit-journey.md` accepted a sold unit only if `robot.binding.customer` was the job's customer. A lease-to-own conversion leaves the unit bound to the internal delivery that put it in the pool, so the buyer of a converted unit would have been refused, `theirs`, when asking for a repair on their own robot. Found by a reader with no context, briefed as an operations manager, who asked whether a lease buyer could get a warranty repair ([`flow-review.md`](flow-review.md)).

**Resolved in place**, 2026-09-24: the unit records its buyer in `sold_to` on every route into `SOLD` and clears it on every route out, and `theirs` compares against it.

## Found by reviewing the whole record against PRD revision 5, 2026-09-24

Five readers, one slice each — flows and trust; data, formulas and metrics; logic, convergence and the non-functional requirements; every use case end to end; and the record's coherence as one text — read the whole record against PRD revision 5 after it was made to agree with itself. They reported ninety-one findings; after deduplication, and after each was re-read at its cited lines, these sixty-four stand. The two that depend on production's behaviour, D345 and D346, were confirmed in its code at `4109939`.

### D290
**The core kept a push delivery worker, against N6 and UC-20.** `DESIGN.md` §2 said "a deployment becomes a service when it adds a delivery worker for push subscriptions", and §7 drew the worker posting to a subscription's endpoint; ADR-0043 called it "part of an ObjectFlow deployment". ADR-0104, accepted by the author, says the engine contains "no notifier … no reaction to its own events", and UC-20's acceptance says it "sends nothing". The worker also wrote `last_error` and `last_error_at` (`storage-schema.md` §6) through none of the six operations `DESIGN.md` §13 calls the only writes. Found by slice 1 (flows and trust).

**Resolved by ADR-0105**, 2026-09-24: the core delivers by `pull` alone; posting to an endpoint is an upper-layer relay; the subscription loses its endpoint and `deliver_as`, and its position the error columns.

### D291
**Subscription and Proposal had no declared authority, and a proposal no visibility.** The built-in capabilities (`DESIGN.md` §9) gated no subscription creation, `acknowledge` or proposal rejection; `deliver_as` let a subscription be delivered as any descriptor; and `t_proposal` showed its target's id and inputs to readers who cannot see the target, the defect D263 repaired for `DeclarationChange` alone. Anyone could therefore acknowledge another's cursor past events it had not handled, against L6. Found by slice 1.

**Resolved by ADR-0105**, 2026-09-24: `OF_SUBSCRIBE` gates creating a subscription, which names the one reader who may pull and acknowledge it; a proposal is visible to its proposer and to whoever can see its target, its proposer may withdraw it, and whoever passes the proposed transition's actor guards may reject it.

### D292
**An empty store could not install its first declaration, and the built-ins were unversioned.** A publish is a request on a `DeclarationChange` "drafted against the installed version" (`DESIGN.md` §9), and `base_version` references `of_declaration` (`storage-schema.md` §6), which a new store had no row in; `DeclarationError` meant "none installed" (`library-api.md` §7). Separately, the standard metrics, the `label` kind and the built-in capabilities belonged to no declaration version, so an engine release could change what a recorded event's metrics meant with no publish, against PRD §5's flow, "versioned together". Searched for "version zero", "bootstrap" and "first publish": nothing defined it. Found by slices 1 and 3.

**Resolved by ADR-0105**, 2026-09-24: a store is created at declaration version 0 holding the built-ins; the first change is drafted against it; each version records the built-in module it was published with.

### D293
**A migration was called an assertion, was never checked against the invariants, and "or admitted" had no form.** `DESIGN.md` §3, §8 and §13 and the harness called migration a use of the built-in assertion, while the schema recorded it `migrated`, without a reason, and `override_counts` excluded it (`declaration-syntax.md` §6.11). PRD F2 names a flow change's mapping as a route of its own. Nothing said whether a mapping's writes are checked against the invariants, so a `backfill` onto a `unique` attribute would either fail at publish, unlike its dry run, or break T1. `DESIGN.md` §5.9 said a new invariant is "resolved by a mapping or admitted", and no mapping form admitted anything. And `state_source` listed `migrated`, which the rule that a migration leaves it alone meant was never written. Found by slice 1.

**Resolved by ADR-0105**, 2026-09-24: a migration is F2's third route, authorised by the publish's approval; the dry run and the publish check every invariant over what mappings write; `admit <Type>.<invariant> because "…"` is the admission form, an override; `.overrides` marks every override; `migrated` leaves `state_source`.

### D294
**Erasure changed other objects with no event, and a sync could write an erased value back.** `DESIGN.md` §8 and `storage-schema.md` §9 step 12 said erasure "erases what flowed from it into other objects' personal attributes, in those events and on those objects", with no event on those objects and no version advance, so a subscriber never saw the change and a stale `expected_version` passed; and `DESIGN.md` said an erasure's admission "exists only where a type declares `erase`", which those objects' types need not. Separately, only the import was barred from writing an erased value (ADR-0101); the first consumer's Xero sync, an externally owned type's transition (ADR-0080), could restore one. Found by slices 1 and 4.

**Resolved by ADR-0105**, 2026-09-24: every object an erasure redacts gets an `erased` event and a version advance; the generated guard `not_erased` refuses any request writing a personal attribute of an object whose erasure is recorded.

### D295
**Only an unsatisfied verdict carried a remedy class.** `library-api.md` §4 gave `Stale`, `NotFound`, `NotRequestable`, `OverLimit` and `InvariantViolated` no remedy, and `DESIGN.md` §5.5 said "an unsatisfied verdict's remedy class"; PRD F4, a Must, asks every refusal to say what to do next, and attempt rows for those verdicts carried none. Found by slice 1.

**Resolved by ADR-0105**, 2026-09-24: every verdict carries a remedy class, fixed per kind of verdict, and `of_attempt.remedy` is always set.

### D296
**A refusal decided on a request value could not be re-evaluated from its record.** An attempt row held the failing clause's read set "and never the inputs" (`DESIGN.md` §6). The read set records identities and versions, so a refusal whose clause read a scalar input or the request's `occurred_at` could not be re-evaluated, which PRD L2 requires of every decision "for as long as that record is kept", and which ADR-0088's own rejected alternative names: "the inputs are what is not recorded". Found by slice 3 (logic and non-functional).

**Resolved by ADR-0105**, 2026-09-24: an attempt records the non-personal inputs and the occurred time its failing clause read, and names personal ones as withheld; the harness fails a refusal that does not re-evaluate.

### D297
**An upper-layer application's reason had no field.** UC-20 and ADR-0104 record an application's actions "with its reason"; a request carried only `context`, "naming the route it came by" (`DESIGN.md` §6), and ordinary transitions take no reason input. Found by slices 1 and 4.

**Resolved by ADR-0105**, 2026-09-24: `context` is the caller's statement of why and by which route, recorded on the event and the attempt row.

### D298
**A late or stopped scheduler misdated time-driven transitions for good.** A time-driven transition such as a warranty's `expire` is requested by a scheduler above the store (`DESIGN.md` §7), and no such transition was backdatable, so while the scheduler was late or stopped its occurred time was when it finally ran, and time in the earlier state was overstated permanently. `first-consumer-cutover.md` §4 said so for warranties; UC-20 says stopping the application "loses nothing from the record". Found by slices 3 and 4.

**Resolved by ADR-0105**, 2026-09-24: a time-driven transition may be `backdatable within` the tolerated delay, and the scheduler supplies the deadline it read as the occurred time.

### D299
**A tracked member added by a publish had no intervals.** `storage-schema.md` §10 said a new tracked member needs "no DDL; its intervals are rebuilt from the log", while `DESIGN.md` §5.2 said every tracked member has an interval from its object's creation; the publish wrote no interval rows and no operation rebuilt them, so `entered_at(member)` was unknown for every live object and ageing queries never found them. Found by slice 3.

**Resolved by ADR-0106**, 2026-09-24: a publish that makes a member tracked opens its current interval for every live object at the publish event, which the fold reproduces; time before it is reported as a gap.

### D300
**Gaps in legacy history were promised as "incomplete" with no mechanism, and `.created_at` was undefined for a backdated or ported creation.** `DESIGN.md` §11, `publish-and-import.md` §4, `first-consumer-cutover.md` §4 and ADR-0100 promised that a metric covering a gap "counts it as unknown time and reports as incomplete"; `complete` was defined by visibility alone (`DESIGN.md` §5.12, `library-api.md` `MetricPage`), and a gap is an absent row, so a reader who sees everything was told `complete` over the first consumer's five-month audit hole. `.created_at` was "taken from its creation event", which carries two times, while standard metrics subtract it from occurred times. Found independently by slices 2, 3 and 4.

**Resolved by ADR-0106**, 2026-09-24: each object records `recorded_from`; the mapping declares silent spans; every metric row reports `gaps`, and the page `history_complete`; `.created_at` is the creation's occurred time.

### D301
**The port split every current stay in two, and a mirror's refresh lost who and when.** The import wrote the last legacy interval ending at the import and the recorded one beginning there (`DESIGN.md` §7, §11; `library-api.md` `ImportedObject` had no current entry time), so `entered_at(state)` returned the import time for every ported object, `oldest_open` restarted at cutover, and `handoffs_by_object` and `returns_by_object`, which count intervals, gave every ported job an extra handoff. A refresh of a mirror had no rule, so a reassignment made in the legacy system while the type was a mirror was dated at the refresh and attributed to the importer. Found by slice 4 (use cases).

**Resolved by ADR-0106**, 2026-09-24: an imported object carries `entered` per dimension, and the import's event opens the current interval then; a refresh carries the same and the intermediate values; a write that leaves a tracked value unchanged opens no interval.

### D302
**A derived attribute reading other objects had no visibility rule under `get` and `query`.** `DESIGN.md` §5.12 and `declaration-syntax.md` §6.9 said a derivation a metric row reads is evaluated under the reader's visibility; nothing covered `get` or `query`, and `Object` carried no completeness mark, so a per-model stock count either counted hidden units (T5) or varied by reader unmarked (C2). Found by slice 2 (data and metrics).

**Resolved by ADR-0106**, 2026-09-24: a reader reads such a derivation over what it can see, and `Object.partial` names it where that is not everything; a guard reads it over everything.

### D303
**Override counts per reason existed only for enum reasons, and admissions were not counted.** `declaration-syntax.md` §6.9: "`.reason` is an assertion's `reason` input where it is a declared enum"; check 29 required only that a reason exist. PRD M1, revision 5, asks for override counts per reason without declaring any. And `DESIGN.md` §8 called erasure's admissions overrides while `override_counts` counted `t.asserted` only. Found by slices 1 and 2.

**Resolved by ADR-0106**, 2026-09-24: check 29 requires an enum reason, with a fixture; `.overrides` counts every admission as well (ADR-0105).

### D304
**A metric could read a personal value, so erasure changed counts.** A personal value "may be counted" (`declaration-syntax.md` §6.9, `DESIGN.md` §5.12), and nothing forbade one in a filter or `count(where …)`: `count(where l.note is not null)` falls when `forget` redacts a note. UC-17 says metrics built from erased datapoints "keep their counts". Found by slice 4.

**Resolved by ADR-0106**, 2026-09-24: a metric reads no personal value anywhere; check 56 enforces it, with a fixture.

### D305
**A declared combined metric could not be split by version or actor kind, and silent legacy rows took the importer's kind.** Only the standard metrics had `version` and `actor_kind` (`declaration-syntax.md` §6.11), and ADR-0101 noted "a combined metric has no binder to filter by", so `delivery_refusal_rate` could not be split as UC-16 asks of every metric. A legacy row whose record named no actor took the importer's kind, filing legacy work under `service`. Found by slice 4.

**Resolved by ADR-0106**, 2026-09-24: every metric has both dimensions from its rows, passed through by `combine`; a silent legacy row has the kind `unknown`.

### D306
**Cancellations counted as completions, and reopened work was undefined.** `.completes` marked "the transition that first" enters a closed or terminal state, so `throughput` and `cycle_time_by_assignee` counted `ServiceJob.cancel` into the closed `CANCELLED`; `.open` was "true until" such an entry, undefined after `reopen` or `Robot.accept_return` left a closed state, and a second completion was never counted. Found by slice 4.

**Resolved by ADR-0106**, 2026-09-24: `.open` is read from the current state; every entry from an open state into a closed or terminal one completes; throughput and cycle time carry the state entered.

### D307
**Percentiles and buckets differed by backend.** `data-driven-engine.md` §3.4: nearest rank on SQLite, "and PostgreSQL has `percentile_cont`", which interpolates; a metric guard near its threshold could pass on one backend and refuse on the other, and SQLite is the harness's reference for four behaviours. The start of a week and a bucket's time zone were left to the database. Found by slice 3.

**Resolved by ADR-0106**, 2026-09-24: nearest-rank on both, `percentile_disc` on PostgreSQL; median is the fiftieth percentile; ISO weeks from Monday; every bucket in UTC; a probe of both backends is recorded as owed.

### D308
**`.held` on a transition was ambiguous, and "acting" undefined for recordings.** `.held(<member>)` was "the value … when … the transition happened" (`declaration-syntax.md` §6.9), which does not say before or after the transition's own write, so a reassignment could be attributed to either engineer; and whether recording an observation counts as acting on the object was unstated. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: `.held` is the value before the transition's writes; acting means requesting a transition; a recording is counted by its kind's recorder.

### D309
**`avg` of a duration lost its unit.** `declaration-syntax.md` §6.9: "`avg` yields a `decimal`", so an average duration compared against `10 days` would not type. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: `avg` keeps its body's type.

### D310
**The unknown-filter rule missed a reader's filter and a row's own collections.** ADR-0103's rule, that an unknown filter selects nothing, covered a metric's `from` and `count(where …)` (`declaration-syntax.md` §8.2) and not a reader's `filter` to `metric()` or an aggregate over a row's own collections inside a value. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: both are added to §8.2's table.

### D311
**The export could not read the permanent refusal counts or labels.** `export`'s sources were the log, a type's intervals, transitions and attempts, and observation kinds (`DESIGN.md` §10, `library-api.md` §6), so after a prune the refusal history could not leave the store (PRD M5), and labels were not named. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: `<Type>.attempt_counts` and `<Type>.labels` are export sources.

### D312
**A formula over one object's own flow data could be written only as a metric grouped by object.** A derivation could read only `entered_at` and `time_in` of its own intervals (`DESIGN.md` §5.7, check 58), so handoffs on one job existed only as `handoffs_by_object`, a metric grouped by object. PRD C1 calls a formula over one object a derived datapoint. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: a derivation may aggregate over `this.intervals(…)` and `this.transitions`, is never indexed, and no invariant may read one that reads the clock.

### D313
**Three limits were unstated: label names, labels' occurred time, multi-choice attributes.** A label's `name` is user-typed free text that erasure does not reach (`declaration-syntax.md` §6.8, `storage-schema.md` `t_label`), not among `DESIGN.md` §8's known limits; "a label's occurred time is its recorded time" was stated bare, though D5 asks every datapoint to distinguish the two; and a set of enum values is not tracked (`storage-schema.md` `of_interval` holds one value) while `DESIGN.md` §13 named only sets of references. Found by slice 2.

**Resolved by ADR-0106**, 2026-09-24: each is stated with its reason in `DESIGN.md` §5.11 and §13, and PRD §12 proposes D11's wording for the third.

### D314
**Pruning lost who a rule on trial would have refused.** The rollup keeps the actor's kind and not its id (`storage-schema.md` §6), and `maintain(prune_attempts)` ignored whether an observing clause was still on trial, so UC-14's "who would have been refused" was lost at the first prune during a trial. Found by slice 3.

**Resolved by ADR-0106**, 2026-09-24: the prune keeps a would-be refusal while its clause is still observing.

### D315
**The writing transaction was typed TEXT, so a cursor ordered it as text.** `of_event.txn` and `of_attempt.txn` were `TEXT` (`storage-schema.md` §2, §6), and the settled cursor orders by `(txn, position)`; as text `'10'` sorts before `'9'`, so a cursor would skip or reorder events once the counter crossed a power of ten, breaking L6 and M5's export. Found by slice 3.

**Resolved in place**, 2026-09-24: `INTEGER` on SQLite, `xid8` on PostgreSQL, and the reason stated beside the column.

### D316
**The refusal rollup could not hold an attempt with no type.** D260 made `of_attempt.type` nullable for a request naming an unknown id, but `of_attempt_rollup.type` stayed `NOT NULL` with no default and in the primary key (`storage-schema.md` §6), so pruning a day that held one would fail or drop its count, against "a count is never lost". Found by slice 2.

**Resolved in place**, 2026-09-24: `type` defaults to `''`, and such a count is visible only to a reader who sees every object.

### D317
**The release map contradicted PRD §10 revision 5.** `data-driven-engine.md` §9 put L2, C3, C6, M3 and M4 in release 2 and `DeclarationChange` and all of `diagnostics` in release 3, while PRD §10 revision 5 schedules L2 for recorded datapoints, L6, and C3, C6, M3 and M4 as the standard metrics use them in release 1; the standard metrics are written in the metric form, so the form cannot wait; release 1 had no publish path, though `DESIGN.md` §5.9 makes every flow change a `DeclarationChange`; and first-release erasure relied on `diagnostics`. Its header said revisions 4 and 5 changed none of its evaluations. Found by slices 2, 3 and 5.

**Resolved in place**, 2026-09-24: §9 is rewritten against revision 5, and the header says what changed.

### D318
**data-driven-engine.md stated superseded design beyond its release map.** The subject declaring `part inspections …` and a kind with no `as` (pre-ADR-0099, which check 54 refuses); the kind declared on `ConfiguredRobot`; dimensions without the creator's kind; the M6 list without cycle time per assignee; "every one can be split by the kind of actor who made the assignment", which §6.11 contradicts for five metrics; D204, D208 and D215 listed as untouched though resolved; out-of-date operation signatures; the use-case table without UC-20 or UC-19's fourth route; "fifty-two requirements". Found by slices 2, 3, 4 and 5.

**Resolved in place**, 2026-09-24: each marked inline with the document's amendment convention.

### D319
**The harness missed UC-19's fourth route, three failure rows and the erasure exemption.** `adversarial-harness.md` still said "UC-19's three routes", and its route-tester had no route retiring a catalogue entry, which PRD revision 5 added; its failure table had no row for a refusal missing from the attempt log, one whose record does not re-evaluate, or a pull that skips or misorders an event, though the text claims all three are tested; and it failed any event whose rules re-evaluate differently, though ADR-0088 says a re-evaluation after an erasure sees absence, so a correct store failed its own test. Found by slices 1, 2, 3, 4 and 5.

**Resolved in place**, 2026-09-24: the fourth route, the three rows, the erasure exemption, and a fixture gate to try the route against.

### D320
**A metric was still defined "across many objects or over time".** `DESIGN.md` §5, §5.12 and §14 and `declaration-syntax.md` §6.9; PRD revision 5 removed "or over time" because it overlapped derived datapoints. Found by slices 2 and 5.

**Resolved in place**, 2026-09-24: "across many objects", with a formula over one object a derived attribute, in all four places.

### D321
**DESIGN's dimensions omitted the creator's kind.** `DESIGN.md` §5.12 listed "the kind of actor that requested a transition or recorded a datapoint"; PRD M4 revision 5 adds the kind that created an object, which `.created_by_kind` already provides. Found by slice 2.

**Resolved in place**, 2026-09-24: the dimension list names all three.

### D322
**DESIGN left who rewrites a time-dependent query, so a caller would restate the threshold.** `DESIGN.md` §10 let `query` filter on "derived attributes that are themselves indexable" and said time-dependent ones "are queried by filtering the stored operand"; `declaration-syntax.md` §8.3 showed the caller writing the whole condition, threshold included. ADR-0048 says the store rewrites the derivation, and PRD M7 needs the condition declared once. Found by slice 2.

**Resolved in place**, 2026-09-24: `query` accepts a queryable derivation by name and the store rewrites it, stated in `DESIGN.md` §5.12, §10 and the syntax §8.3.

### D323
**Two cosmetic slips: the renderers omitted the p80, and DESIGN dropped "stored".** `renderers.md` §2 listed nine standard metrics where §6.11 declares ten; `DESIGN.md` §5.13 said "every singular reference is tracked" where the rule is every singular stored reference. Found by slice 2.

**Resolved in place**, 2026-09-24: both corrected.

### D324
**edge-cases.md stated a database clock, no replay, uncovered grouping and different notes options.** It said `now` is "the database's transaction time … evaluated at commit", which the injected clock replaced; "no tool will replay a historical request", which ADR-0088's read set made possible; grouped aggregation "not covered", which ADR-0084's metric form covers; and it listed the notes' options differently from the cutover plan. Found by slices 3 and 5.

**Resolved in place**, 2026-09-24: each entry corrected visibly, and the notes' options stated once, in `first-consumer-cutover.md` §1.

### D325
**N3's measurement before cutover had no owner.** PRD N3 requires event and datapoint rates "measured before cutover"; only `data-driven-engine.md` §7 listed it, the cutover steps did not, `DESIGN.md` never stated N3, and its traceability row cited sections that do not cover it. Found by slice 3.

**Resolved in place**, 2026-09-24: a cutover step, a paragraph in `DESIGN.md` §2, and a corrected row.

### D326
**DESIGN did not state D7's rule that catalogues are governed.** PRD D7 revision 5: a catalogue's entries change "only by their own recorded transitions … so a rule that reads a catalogue is loosened only through a recorded, attributed change"; `DESIGN.md` §5.11 said only that catalogues are data. Found by slices 2 and 5.

**Resolved in place**, 2026-09-24: §5.11 states it.

### D327
**README said the engine does not compete "on flows".** `README.md`: "does not compete with full metadata-driven platforms on flows, screens or automation"; PRD §1 and ADR-0104's correction say flows are the product and the line is automation, scheduling and screens. Found by slice 5 (coherence).

**Resolved in place**, 2026-09-24: reworded as ADR-0104 §5 is.

### D328
**ADR acceptance status disagreed between documents.** The ADR files and index read "Accepted" for ADR-0097 to ADR-0101 and ADR-0103, meaning decided at the author's direction, while `DESIGN.md`, `TODO.md` and `README.md` said they "await acceptance" without saying the two meant different things; ADR-0104, accepted by the author, and PRD revisions 4 and 5 were missing from the status lines; `DESIGN.md` said it was amended for ADR-0081 to ADR-0101 only. Found by slice 5.

**Resolved in place**, 2026-09-24: the status lines name the two kinds of acceptance and list each decision under one.

### D329
**"Consumer" meant three things, and the glossary lacked the PRD's terms.** `DESIGN.md`, `README.md` and several design documents used "consumer" for a deployment, an upper-layer application and a reader, which PRD revision 5 separated; `DESIGN.md` §14 had no entry for engine, deployment, upper-layer application, reader, governed state or override; §1 listed "who holds what" for N6's "what waits and on whom"; §2's diagram put workflow orchestration beside the store. Found by slices 1 and 5.

**Resolved in place**, 2026-09-24: each use replaced, §14 points to PRD §5 and adds the design's own terms, and the diagram puts orchestration above.

### D330
**DESIGN defined a flow more narrowly than the PRD.** `DESIGN.md` §1: "a set of object types with their lifecycles, transitions and rules"; PRD §5 includes datapoint kinds, formulas and conditions, versioned together. Found by slice 5.

**Resolved in place**, 2026-09-24: §1 and §14 state PRD §5's definition.

### D331
**DESIGN's glossary said a business identifier is consumer-minted.** `DESIGN.md` §14: "a consumer-minted meaningful value such as a serial", beside a row saying a sequence mints business identifiers; ADR-0029 withdrew consumer minting. Found by slice 5.

**Resolved in place**, 2026-09-24: "supplied as input or minted from a sequence".

### D332
**Six examples declared `tracking serial` for records.** The `Delivery` examples in `declaration-syntax.md` §3.2 and `renderers.md` §2 declared `tracking serial`, as did `ExpenseClaim`, `ChecklistItem`, `Approval`, `Note` and `Order`; ADR-0067 and `DESIGN.md` §5.10 say a delivery or an audit entry is a `record`. D260 had aligned the renderer to the syntax document's wrong side. Found by slice 5, which named the delivery; the other five were found fixing it.

**Resolved in place**, 2026-09-24: all six are `tracking record`.

### D333
**flow-review gave the store worklists and notifications.** `flow-review.md` §2.1 listed "the worklists" among the store's questions, and §2.2 said a missing unit "should notify the order's owner" without saying which layer; PRD N6 says the core presents no worklist, "though it answers by query what such a worklist would list". Found by slice 5.

**Resolved in place**, 2026-09-24: the application builds its worklists from the queries, and it notifies.

### D334
**G4's proposed revision was tracked nowhere.** PRD §12 proposes G4 and says traceability marks each affected row; goals are not rows, and `README.md` and `TODO.md` named only C2, UC-8 and UC-10. Found by slice 5.

**Resolved in place**, 2026-09-24: traceability says G4 is met through C2's row; README and TODO name it.

### D335
**DESIGN §4 and the harness fixture pointed at the superseded walkthrough.** `DESIGN.md` §4 named only the walkthrough, and `adversarial-harness.md` §3 took "a unit with a nine-state lifecycle" from it, though `unit-journey.md` superseded that unit (ADR-0102) with eleven states. Found by slice 5.

**Resolved in place**, 2026-09-24: both point to the unit's journey.

### D336
**The syntax document's §12 and Q12 still said the counter is unexercised.** `declaration-syntax.md` §12: "nothing in the record exercises the counter at all" and "no quantity-tracked type in the first consumer"; the Q12 row said none is quantity-tracked. D279 corrected Q10 for the bulk-accessory lot and left these. Found by slices 1 and 5.

**Resolved in place**, 2026-09-24: both bullets rewritten and the row annotated.

### D337
**README and TODO carried stale revisions and counts.** `README.md` gave the PRD as revision 3 and omitted `unit-journey.md`, `flow-review.md` and `first-consumer-audit.md` from its documents table; `TODO.md` said nineteen use cases, left "PRD revision 4 proposed" unannotated beside the positioning's revision 4, and had no revision-5 row. Found by slice 5.

**Resolved in place**, 2026-09-24: updated.

### D338
**TODO said no questions were open.** `TODO.md`'s status table said "Open questions | none" while PRD §10's two questions and flow-review §7's decisions await the author. Found by slice 5.

**Resolved in place**, 2026-09-24: the row names what is open, and where.

### D339
**Amendment bullets sat under the wrong headings, and a paragraph split a table.** In `storage-schema.md` and `adversarial-harness.md` the bullets of the 2026-09-23 amendment for ADR-0082 onwards sat under a later amendment's line, misattributing them; `storage-schema.md` §10 had a paragraph inside its table, so two rows fell outside it. Found by slices 3 and 5.

**Resolved in place**, 2026-09-24: the bullets are back under their headings and the table is whole.

### D340
**A refused requester was said to read the value a metric guard used.** `declaration-syntax.md` §6.9: "the person a guard refused can read the value it was decided on", against the same section's rule that the value goes to a requester whose view is complete "and otherwise withheld", and against T5; `DESIGN.md` §5.12 dropped "over the same data". Found by slice 5.

**Resolved in place**, 2026-09-24: a refused requester reads the value where their view is complete, and otherwise the value over their own rows, marked partial.

### D341
**DESIGN did not state L6's per-object order.** `DESIGN.md` §7 stated the log's per-object order and at-least-once delivery; the argument that per-object order survives the settled cursor was only in `storage-schema.md` §7 and ADR-0089. Found by slice 1.

**Resolved in place**, 2026-09-24: §7 states it, citing L6.

### D342
**The impact report omitted what mappings rewrite and the changes a publish supersedes.** PRD F7 asks what a change "affects among live objects and pending work"; `DESIGN.md` §9 and `publish-and-import.md` §1 named neither the objects each mapping rewrites nor the open changes a publish supersedes, and the two lists differed. Found by slice 1.

**Resolved in place**, 2026-09-24: both lists name both, and agree.

### D343
**A list filter on a derived status was recorded as a decision, not a gap in the core.** `DESIGN.md` §13 and `edge-cases.md` said filtering a list on a status derived from other objects stays with the first consumer "by decision", met by the reader filtering the page. ADR-0104 §4, accepted by the author: "a need an upper layer meets by scanning … is a gap in the core, and is recorded as one". None of N6's or UC-20's questions needs it, so N6 stays covered. Found by slice 1.

**Resolved in place**, 2026-09-24: recorded as a gap in `TODO.md`, and `available` named as the answer where the status gates a transition.

### D344
**UC-4's subject was stated three ways, and its gate was never written.** ADR-0082 and `data-driven-engine.md` put the inspection kind on `ConfiguredRobot`, `declaration-syntax.md` §6.8 advised the delivery, and its checked example was on `ServiceJob`; no document wrote UC-4's gate, a result for every check of the configuration's checklist, and the catalogue had no configuration. Found by slice 4.

**Resolved in place**, 2026-09-24: a checked example in §6.8 — `Configuration`, `PdiCheck`, `PdiResult` on `Handover`, and the gate `all_checked` — and ADR-0082 and data-driven-engine annotated.

### D345
**The opening-stock creation skipped production's serial rule without counting as an override.** `unit-journey.md`'s `add_opening_stock` creates a unit in `AVAILABLE` checking only the label, while production refuses opening stock with no manufacturer serial where the model requires one (`wr:app/services/inventory_item_service.py:277-293`, read at `4109939`); an `ASSERT` holder could therefore bypass `inventorize`'s serial rule through a creation that no override count sees. The flow review's appendix had the same gap. Found by slice 4.

**Resolved in place**, 2026-09-24: `mfr_serial` on the creation, in the journey and the appendix.

**The general question it raised**, whether the engine should say when a creation lands past a lifecycle's first state with rules of the path it skips, was decided on 2026-09-24 by ADR-0109. The label and the serial are invariants of every unit on offer, which bind the creation as they bind every route, so the copied guard is gone; the photo, which production checks only at intake, stays intake's; and publishing reports what each such creation skips.

### D346
**The first consumer's module omitted the engineer-of-record rule.** Production requires an engineer to be an active user whose role is ADMIN or ENGINEERING, on create and on update (`wr:app/services/service_service.py:488-520`, called at `:361` and `:544`, read at `4109939`), which is the rule PRD UC-18 cites, that an agent may not be engineer-of-record; `ServiceJob.open` validated no engineer, `reassign` checked only `ACTIVE`, and `User` had no role. Found by slice 4.

**Resolved in place**, 2026-09-24: `User` gains `role`, and `open` and `reassign` require an active ADMIN or ENGINEERING user, in the syntax example, the journey and the appendix.

### D347
**UC-20's coverage rested on undecided declarations, and the appendix reused version numbers.** Two of UC-20's five questions need `procured_for` and `mark_ready`, which only the flow review's appendix declares and the author has not decided; the traceability row cited no declaration. The appendix reused the journey's version numbers for six declarations whose content differs, which check 22 would refuse if it were published after the journey. Found by slice 4.

**Resolved in place**, 2026-09-24: the row cites the review and says what is conditional; the six declarations advance.

### D348
**The walkthrough found low stock by filtering a counter a unit cannot have.** `first-consumer-walkthrough.md`: "low stock by filtering the counter"; a robot is `tracking serial`, and a counter on a serial type is refused (check 31). Found by slice 4.

**Resolved in place**, 2026-09-24: corrected visibly.

### D349
**Traceability rows were not re-traced after revision 5.** UC-13 cited nothing holding revision 5's mapping clause, UC-7 nothing saying a summary is recorded as an observation, and the history recorded only L6 for revision 5. Found by slice 4.

**Resolved in place**, 2026-09-24: every row re-traced and the citations added.

### D350
**Two case studies described the read surface as it no longer is.** `case-study-approvals-and-bookings.md`: `check` "takes an object id … this question has no answer", which ADR-0099 answered; `case-study-orders.md`: "analytics exports from the log, it does not query the store", which ADR-0081 and ADR-0084 changed. Found by slice 4.

**Resolved in place**, 2026-09-24: both annotated.

### D351
**Legacy history was said to be preserved while fields were dropped and carrying it left open.** `DESIGN.md` §11 said legacy entries "carry their original payload" while §8 and `publish-and-import.md` §4 keep only the entry's own object's fields and leave behind the employee's IP address and user agent; `publish-and-import.md` §8 left open whether every entry is carried. PRD N5 is a Must: legacy history "ported and preserved". Found by slice 3.

**Resolved in place**, 2026-09-24: every entry is carried, fields about another ported object move to its history, and PRD §12 proposes that N5 say what is not ported.

### D352
**L1 was narrowed to exclude labels without a PRD revision.** PRD §5 counts a label as a recorded datapoint and L1 lets rules read recorded datapoints; `DESIGN.md` §5.11 and check 55 forbid a rule to read a label, for a sound reason, and the traceability row said covered. The PRD's own rule is to propose a revision rather than work around it. Found by slice 3.

**Resolved in place**, 2026-09-24: PRD §12 proposes L1's wording, and the row says revision proposed.

### D353
**The PRD's proposed C2 said "consumer".** PRD §12's C2 wording said "every consumer … a consumer that may see only part", which §5, since revision 5, calls a reader. Found by slice 5.

**Resolved in place**, 2026-09-24: reworded.

## Found by verifying ADR-0105 and ADR-0106, 2026-09-24

An independent reader checked both decisions against the PRD, ADR-0104 and the whole working tree, confirmed D290 to D353 at their sites, and searched the record for superseded wording; each finding below was re-read at its cited lines before it was recorded. D371 was found preparing the worked example's update.

### D354
**A refusal by an invariant or a cascade could not be re-evaluated from its record.** ADR-0105's first draft recorded on an attempt row only "the values its failing clause read from the request", and the harness failed a refusal that did not re-evaluate over them. An invariant (DESIGN.md §6 step 7) and a cascaded guard ("against the state produced so far") refuse over versions the request itself produced and rolled back, which are never in the log, and read no input directly, so nothing was recorded and a correct store would have failed its own harness row, against L2. Found by the verification of ADR-0105 and ADR-0106, inferred and confirmed against DESIGN.md §6.

**Resolved by ADR-0105**, 2026-09-24: an attempt records every non-personal input and the occurred time of the request, and the read set of everything the request had read; a refusal is re-evaluated by replaying the request, and the harness row says so.

### D355
**Overrides were counted per free-text reason again, by an `admit` and by an erasure.** The first draft's `admit … because "<reason>"` took a string literal, and an erasure's reason is `input reason : string`, while `override_counts` groups by `.reason` and ADR-0106 rejects counting per free-text reason, which may name a person (UC-17). Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: an `admit` names a member of a declared enum (`OverrideReason.LEGACY_DATA` in the examples), and an erasure's events carry the fixed reason `erasure`.

### D356
**A gap in coverage could go unreported: an undated creation, and a current stay across a silent span.** The first draft's gap test was "`recorded_from` later than `.created_at`", which fails for a port with no legacy creation, where both are the port, so short cycles went unflagged; and where an object's current value was entered before the 2026 audit hole, its current interval ran across the silent span and no gap condition fired, so `oldest_open` vouched for a stay the record cannot vouch for. An entry time earlier than an unknown creation could also make `time_to_first_assignment` negative. Found by the verification, inferred and confirmed against ADR-0106 §1 and §2.

**Resolved by ADR-0106**, 2026-09-24: one gap rule with four cases — a window before `recorded_from`, an interval that began before it, an undated `.created_at`, a member read before its tracking began; an undated creation is the earliest time the port knows of, and an entry time or legacy interval earlier than a supplied creation is refused.

### D357
**An `admit` could not yield for an invariant compiled to a constraint.** `storage-schema.md` §10 gave an `admit` "no DDL", and §8 leaves uncompiled only invariants an assertion may admit and new ones an `admit` covers; a `backfill` or `removed state` mapping violating an existing `UNIQUE` or partial index would be refused by the database after the runtime allowed it, the failure ADR-0074 exists to prevent. Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: an `admit` of a compiled invariant drops its constraint in the publish's transaction, before the migration writes, and it stays uncompiled while any admission of it stands.

### D358
**The release map deferred rules the first release needs.** `data-driven-engine.md` §9 scheduled "derivations over other objects, read under visibility" for release 2 while release 1 carries T5 and already has derivations that count other objects (the low-stock count, the journey's `on_loan`); and ADR-0106 §12, derivations over their own object's flow data, was in no release. Found by the verification.

**Resolved in place**, 2026-09-24: both are in release 1, the first as T5 needs it and the second as the derived datapoints for one object.

### D359
**`not_erased` had no stated scope.** It refused writes to "an object whose erasure is recorded", beside a rule that every object the taint analysis reaches receives an `erased` event, so a delivery whose contact was copied from an erased customer could never be given a new contact. Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: `not_erased` applies to an object whose own erasure ran — through its type's `erase`, or as a member of an erased supersession chain — and an object reached by taint may be given a new value.

### D360
**"Every count is the same after an erasure" was false, and the erasure's events had no place in the metrics.** DESIGN.md §5.12, `declaration-syntax.md` §6.9 and ADR-0106 said every count is the same before and after an erasure; the `erase` request is itself a transition row, and ADR-0105's new events on redacted objects were rows no one had placed. Found by the verification.

**Resolved by ADR-0106**, 2026-09-24: restated as "no row a metric reads is removed and no value it reads changed"; the events on redacted objects are `.redacted`, left out of every standard metric but the override counts.

### D361
**Superseded statements survived the repair.** Six sites still said the attempt log holds no inputs (`DESIGN.md` §7, §8, §14; `storage-schema.md` §6; `declaration-syntax.md` §6.9; `data-driven-engine.md` §4); two still let a metric count a personal value (`declaration-syntax.md` §6.9, `data-driven-engine.md` §3.4); and `storage-schema.md` §6 still opened an interval on a write that changes nothing. Found by the verification.

**Resolved in place**, 2026-09-24: each corrected.

### D362
**D5 was marked covered while labels and overrides do not meet it.** `traceability.md` marked D5 covered, while ADR-0106 §14 says it is not met for labels, and an assertion is dated when the engine was told; the map's own rule forbids a covered row with a remainder. Found by the verification.

**Resolved by ADR-0106**, 2026-09-24: PRD §12 proposes D5's revision, and the row says `revision proposed`.

### D363
**Creating a store was a write with no operation and no attribution.** Version 0 "is written when the store is created" against `DESIGN.md`'s "nothing else writes the database" and its list of six operations, with `published_by` "the store's construction" and no defined `report`. Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: creating the store is named as the one write that precedes every operation, attributed to the engine release, with the built-in module's check report as its report.

### D364
**Pinning the built-ins was said to pin the language's meaning.** ADR-0105 §3 said a declaration version "fixes the standard metrics as it fixes everything else"; what `.completes` marks and how a percentile is computed are the engine's, and ADR-0106 had just changed both. Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: narrowed: the version fixes the metrics' definitions, and a change to the language's meaning is a decision that says so.

### D365
**ADR-0105 said UC-20 was met only within the backdating bound.** Its §9 said stopping the application loses nothing "within the bound", which confused the record with a measure over it; beyond the bound the record still holds the late change and the deadline, and only time in state is overstated. Found by the verification.

**Resolved by ADR-0105**, 2026-09-24: reworded: the record loses nothing either way, and the measure's limit is in `DESIGN.md` §13.

### D366
**An assertion's explanation was said to be personal and marked so nowhere.** ADR-0106 said `detail` "is free text, and treated as personal", while an input is personal only by marking or by flowing into a personal attribute, and every example declared `input detail : string?` unmarked. Found by the verification.

**Resolved by ADR-0106**, 2026-09-24: `detail : string? personal` in the syntax example, the journey and the flow review's appendix.

### D367
**ADRs still presented push delivery, migration as an assertion, and the first-entry completion rule as current.** ADR-0007, ADR-0030, ADR-0034 and ADR-0077 described push delivery and `deliver_as`; ADR-0040 described migration as a use of the built-in assertion, as did `TODO.md`; ADR-0026 stated that work completes when it first enters a closed state. None pointed to the decision that changed it. Found by the verification.

**Resolved in place**, 2026-09-24: each carries a back-link to ADR-0105 or ADR-0106 in its header and index row, and ADR-0105 and ADR-0106 list them.

### D368
**"Consumer" survived in its three old senses across the implementation documents.** `library-api.md`, `declaration-syntax.md`, `storage-schema.md`, `adversarial-harness.md`, `edge-cases.md`, `data-driven-engine.md` and `TODO.md` still said consumer for a deployment, an upper-layer application or a reader, which D329 was to have removed. Found by the verification.

**Resolved in place**, 2026-09-24: each replaced; the dated case studies and the walkthrough carry a note on their vocabulary instead, since they are records of their day; a quoted older passage in `data-driven-engine.md` keeps its words.

### D369
**Three slips of the first repair: unmarked text, a serial record, a creation without the engineer rule.** `data-driven-engine.md` still stated three datasets per type, counted personal values and said attempts hold no inputs without marking them, and named only D208's resolution beside D204's; `Whole`, the base of a ticket family, declared `tracking serial`; and the syntax example's `ServiceJob.open` lacked the engineer-of-record rule D346 claimed for it. Found by the verification.

**Resolved in place**, 2026-09-24: each corrected.

### D370
**Counting and structural slips in the repair.** `TODO.md` said the decisions resolved eighteen defects where they resolved twenty-five; `DESIGN.md` §5.2 had the file-attribute paragraph inside a bullet, §5.12 the guard paragraph inside another and the standard-metric list without its lead-in, and its impact list omitted objects holding a removed enum member; PRD §12's L1 row cited a decision that decides nothing about L1. Found by the verification, the second §5.12 slip while fixing the first.

**Resolved in place**, 2026-09-24: each corrected.

### D371
**The worked example claimed a guard `User.leave` did not declare.** The page's case "An engineer leaves with open jobs" said `User.leave` refuses while service jobs are assigned to the user, naming them as dependent; the declaration in `declaration-syntax.md` §6.10 required only `SERVICE_ASSIGN`. Slice 4 had suggested the guard. Found preparing the page's update.

**Resolved in place**, 2026-09-24: `User.leave` declares `no_open_jobs`, a type-scan over open service jobs, remedy `dependent`, so the case is what the record says.

## Found writing the worked example's cases into a checked module, 2026-09-24

The author asked for the declarations behind the page's newer cases to be checked like the journey. Writing them, and probing the result with planted mistakes, found these.

### D372
**Five of the worked example's cases rested on declarations no checked module held.** The page's cases on a flow with no rules, a label promoted to a state, a governed reorder point, a count a partial reader sees and a rule reading a metric described a `Return` type, `RobotModel.reorder_point`, `available_units` and a metric guard that no document declared, so the checker had verified none of them. Found by the author's request to hold them to the journey's standard.

**Resolved in place**, 2026-09-24: `returns-module.md` holds two checked publishes of a `Return` flow — the first with no rules, the second with a promoted state, a visibility rule, `prior_returns` and the metric guard `second_eye` — and the journey declares `RobotModel` with its reorder point; the page's cases now describe exactly those declarations.

### D373
**`RobotModel`, which the journey's rules read, was declared nowhere.** The journey imported `RobotModel` from `inventory`, and its serial format and guards read `model.maker_code`, `model.manufacturer_serial_required` and `model.label_photo_required`, but no document declared the type. Found writing the reorder point onto it.

**Resolved in place**, 2026-09-24: the journey declares it from production's `robot_models` columns, with the reorder point and its two derivations marked as additions made to test L4 and M7; the flow review's appendix carries the same declaration.

### D374
**The syntax checker ignored `use`, so a module was never checked against the closure it imports.** `scripts/check-syntax-doc.py` checked each document's declarations as a closed world, although syntax §1 says publishing takes a module with the closure of its `use` imports; a module referencing another document's types in a metric source or a type-scan was checked against nothing. Found writing the returns module, which imports the journey's `Robot`.

**Resolved in place**, 2026-09-24: the checker resolves each `use` against the document that declares the module, bringing an imported type's machine and base with it, with an explicit home for a module declared in two documents (the journey, over the flow review's variant).

### D375
**Check 19 never resolved a state literal.** Check 19's row names a state among the names that must resolve, but the checker did not test `<Type>.<STATE>`: a planted `Robot.AVAILABEL` in a derivation passed. Found by probing the new `RobotModel` with planted mistakes.

**Resolved in place**, 2026-09-24: every `<Type>.<STATE>` is resolved against the type's states or its machine's, with a fixture; no document in the record tripped it.

### D376
**Check 56 read only the first line of a wrapped guard.** The checker took each `require` clause's first line, so a metric reference on a continuation line — which §9.1 makes part of the clause — escaped check 56: a planted binding of an undeclared dimension in `second_eye` passed. Found by probing the returns module with planted mistakes.

**Resolved in place**, 2026-09-24: clauses are read whole under §9.1's continuation rule, with a fixture that references a metric on a continuation line.

### D377
**The journey's purpose paragraph split its status paragraph.** The purpose statement added on 2026-09-24 was inserted mid-paragraph, leaving the status paragraph's closing sentences — what the module supersedes and which decisions record it — under the purpose heading. Found reading the journey to add `RobotModel`.

**Resolved in place**, 2026-09-24: the sentences are back in the status paragraph.

## Found checking the author's principle on cascades, 2026-09-24

The author said that transition cascading should be supported, as long as the rule is declared clearly enough that the transition does not appear as a surprise. The design was read against that condition, from each reader's side: the requester, the history, and the reviewer of each type.

### D378
**A type's printed rules did not show a cascade declared on another type.** `renderers.md` §2 printed a cascade only under the `then` of the transition that causes it. An `only via` transition names its parents in its own declaration, but a requestable transition names nothing, and any transition may `call` it (`declaration-syntax.md` §5.2). In the unit's journey, `Shipment.commit` inventorises every unit in `INTAKE` (`unit-journey.md` §2), while the unit's rules show `inventorize` only as a transition holders of `EDIT` request, so a reviewer of the unit could not learn from them that a shipment's commit puts units on offer.

**Resolved by ADR-0108**, 2026-09-24: the rule set prints every cause of a transition on its own type — `caused by` for an `only via` transition's parents, `also caused by` for other transitions that cascade to a requestable one — derived from the call graph publishing already builds; PRD revision 6 states the condition as F8, tested by UC-21.

## Found deciding D345's question, 2026-09-24

The author accepted the recommendation to state a condition of a state as an invariant and to report what a creation skips (ADR-0109). Writing the journey's invariants, and probing the checker with a planted mistake, found these.

### D379
**Nothing recorded a unit's manufacturer serial or photos after its creation.** In `unit-journey.md` only `add_to_intake` and `add_opening_stock` accepted `manufacturer_serial`, and nothing wrote `photos` at all. So a unit ordered through `request`, whose model requires a serial or a photo, could never pass `inventorize`'s guards: a unit stranded in `INTAKE`. Production edits both on the intake item before commit (`wr:app/services/intake_batch_service.py:470-520`), carries the photos onto the unit at commit (`:874-880`), and lets an existing robot's serial be edited (`wr:app/schemas/inventory.py:59-63`). Found writing the invariant `mfr_serial`, which asked how a unit on offer without a required serial could be given one.

**Resolved by ADR-0109**, 2026-09-24: `record_manufacturer_serial`, an act at any non-terminal state, and `add_photo`, an act at `INTAKE`, in the journey and the appendix.

### D380
**Check 19 ignored a bare state compared with the object's own state.** It resolved `<Type>.<STATE>` literals only, so `state != AVAILABL` in an invariant passed as clean. Found by planting that misspelling in the journey's new invariant `labelled`.

**Resolved by ADR-0109**, 2026-09-24: check 19 also resolves a bare state compared with `state` inside a type or machine, against that type's states or its machine's, proven by a fixture; the planted misspelling is now reported.
