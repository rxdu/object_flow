# The core's checker

Draft, 2026-10-05. How the Rust core checks a flow description before it publishes one (ADR-0156), and where each of the sixty-four checks of [`declaration-syntax.md`](declaration-syntax.md) §10 stands. The reference is the Python checker, [`scripts/check-flows.py`](../../scripts/check-flows.py), with the text checker it runs over the internal form, [`scripts/check-syntax-doc.py`](../../scripts/check-syntax-doc.py); the core is held to both by the oracle of ADR-0156 decision 4, `core/crates/objectflow-core/tests/checker_oracle.rs`.

## 1. The steps, and the core's slices

| Step of `check-flows.py` | What it reads | Codes | The core |
|---|---|---|---|
| 1, 2: the strict load and the schemas | each file as written | `yaml`, `schema`, `syntax` | C1, done |
| 3: imports, names and relationships, migrations, notices | each module through its view, machines and bases merged in | `imports`, `names`, `order`, `required`, `migration`, `metric`, `audit`, `warn`, `self_serviceable`, `unknown`, `before` | C1, done |
| the names an examples file uses | each examples file against its module | `names`, `coverage` | C4 |
| 4: the sixty-four checks | every module converted to the internal form, one text | `check <n>` | C2, done |
| 5: contradictions within one type | each type of the view | `contradiction`, `neverpasses` | C3 |
| 6: the examples | each examples file, run | `example`, `notrun` | C4 |

Step 4 runs only when no file has a fatal finding from steps 1 to 3, and the core keeps that order. A check that step 3 and the text checker both enforce is therefore reported by step 3 wherever both would fire, and reaches step 4 only where step 3 misses it.

## 2. The sixty-four checks

Each check is one of four kinds. A *model* check refuses a declaration whatever notation wrote it, and the core runs it on its own reading of the module (ADR-0156 decision 1). A *notation* check is about the internal text form alone, such as a version written in it or a line's indentation, and retires with that form. A *document* check reads the design's documents, not a module. A *withdrawn* check is kept for its number.

*Today* says which part of the reference enforces it: step 3 of `check-flows.py`, the text checker over the converted text, the schema, or neither. Where neither does, the core has nothing to be held to, and the check waits for both; ADR-0116 recorded that thirty-six checks were implemented when the text form was retired as a written form.

| # | What it refuses, in short | Kind | Today | The core |
|---|---|---|---|---|
| 1 | a guard or step reading an input its transition does not take | model | step 3 | C1 |
| 2 | a call or create with an unknown or a missing argument; one of the wrong type | model | step 3; the text checker, a missing one; neither, the type | C1; C2 for the text checker's |
| 3 | derived attributes in a cycle | model | step 3 | C1 |
| 4 | a traversal invariant through an undeclared far end | model | neither | — |
| 5 | an invariant of none of the three forms | model | neither | — |
| 6 | a type-scan invariant failing the swap test | model | neither | — |
| 7 | a scan reading what is not indexed; `indexed` on a stored end | model | step 3; the text checker, the marking | C1; C2 |
| 8 | a creation that never writes a required attribute or fills a required part | model | the text checker | C2 |
| 9 | a required attribute marked personal | model | step 3 | C1 |
| 10 | a personal value reaching a non-personal one | model | neither | — |
| 11 | a create of a part that is not only via its whole | model | the text checker | C2 |
| 12 | a cascade cycle | model | neither | — |
| 13 | an `only via` whose parent does not exist or never reaches it | model | step 3, a missing transition; the text checker, the rest | C1; C2 |
| 14 | `only via` with `proposable` | model | step 3 | C1 |
| 15 | a state with no way out, one nothing reaches, a type with no final state | model | the text checker | C2 |
| 16 | a binder missing what its machine requires; a type with both or neither of states and a machine | model | step 3; the schema; the text checker | C1; C2 |
| 17 | a write or clear through a path, of a counter, of a required or a non-stored end | model | step 3, in part; the text checker | C1; C2 |
| 18 | a create of a part that never writes its owner | model | the text checker | C2 |
| 19 | a name that resolves to nothing | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 20 | a default on an optional input; an unnamed guard; a remedy outside the five classes | model; notation, the unnamed guard | the text checker; the schema | C2; C1 |
| 21 | a comparison with a bare `null`, an aggregate with no binder; a loop with no limit; `$` and `do … at` | model; notation, the last two | the text checker; the schema, the limit | C2; C1 |
| 22 | a version that did not advance; a required field added to an observation kind | notation; model, the field | the text checker; step 3 | retires; C1 |
| 23 | a removed state with no mapping | model | step 3 | C1 |
| 24 | an expression that does not type | model | neither, but for an evaluator's verdict, step 3 | C1 for that |
| 25 | an event input given anything but `this_event` | model | step 3 | C1 |
| 26 | a supersession's rules | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 27 | a `may admit` naming no reachable invariant | model | step 3 | C1 |
| 28 | `corrects` disagreeing with what is written | model | step 3 | C1 |
| 29 | an assertion or erasure with no reason, or a reason not an enumeration | model | step 3; the text checker | C1 |
| 30 | a cascade to an abstract type's transition some subtype lacks | model | neither | — |
| 31 | a quantity type with no counter, a counter elsewhere | model | the text checker | C2 |
| 32 | an invariant reading `now` | model | neither | — |
| 33 | duplicate names in one scope | model | step 3; the loader | C1 |
| 34 | a type with no creation | model | the text checker | C2 |
| 35 | a machine naming what it does not require | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 36 | a part and an owner of disagreeing types; a re-parent to an abstract owner | model | neither | — |
| 37 | a cascade trigger naming no transition; a final transition neither cascaded nor survived | model | step 3 | C1 |
| 38 | an assertion with `may admit` and no `admits` input | notation | the text checker, never from YAML, whose conversion writes the input | retires |
| 39 | an erasure not reaching a part's personal data | model | step 3 | C1 |
| 40 | an internal transition at a final state | model | the text checker | C2 |
| 41 | a relationship whose ends disagree or store the value twice or not at all | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 42 | a type with no tracking; a declaration with no version | model; notation, the version | step 3; the text checker | C1; retires |
| 43 | an `extends` to an undeclared, concrete or cyclic base | model | step 3; the text checker | C1 |
| 44 | an identifier's sequence, scope and format | model | step 3 | C1 |
| 45 | *withdrawn* (ADR-0133) | withdrawn | — | — |
| 46 | an indexed derivation reading what it may not | model | step 3 | C1 |
| 47 | an aggregate with no body; a cascade clause with no limit | model | neither, the body, which the expression parser accepts (probed 2026-10-05); the schema, the limit | —; C1 |
| 48 | an optional input inside a larger expression | model | step 3 | C1 |
| 49 | a supersede's operand | model | step 3 | C1 |
| 50 | an import of an undeclared name; a default reading what it may not | model | step 3 | C1 |
| 51 | a clause on a body's opening line; a continuation not indented | notation | the text checker | retires |
| 52 | a normative statement citing no check | document | the text checker, over the design | stays a design check |
| 53 | a mirror that can be written, or composed with what the store owns | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 54 | an observation kind's names, units and invariants | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 55 | a label read outside a metric's source | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 56 | a metric's sources, dimensions, windows and personal reads | model | step 3; the text checker | C1; C2 for what step 3 misses |
| 57 | `observe` or `flag` on an assertion's or an erasure's clause | model | the text checker | C2 |
| 58 | a backdating, a delay or a time read that is not a tracked member or a duration | model | the text checker; the schema, durations | C2 |
| 59 | an assignee that is not a single stored reference to one actor identity | model | the text checker | C2 |
| 60 | personal data with no erasure | model | step 3; the text checker | C1 |
| 61 | a call binding an argument from what its outcome writes, read by an evaluator or a metric guard | model | the text checker | C2 |
| 62 | an actor identity with no kind, or a type with two | model | the text checker; the schema, the kind | C2 |
| 63 | *withdrawn* (ADR-0114) | withdrawn | — | — |
| 64 | the actor read anywhere but an outcome value | model | the text checker | C2 |

## 3. How C2 is held

The design's modules and step 3's plants exercised four of the text checker's checks (7, 31, 53 and 64), since its own fixtures are in the internal form, which the core does not read. So `check-flows.py`'s self-test gained `syntax_plants`, 31 plants on six small modules each clean under every step, each changing one thing and caught at step 4 by the check it plants: 8, 11, 13, 15 (four ways), 17, 18, 20, 31 (two), 34, 40, 41, 54 (two), 56 (five), 57, 58 (two), 59 (three), 61, 62 and 64. The oracle holds the core to them by file, line and code. A check's line is the line of the YAML its finding stands for: the type, the state, the transition, the attribute or the clause the text checker reports at, through the conversion's map from each text line to the YAML it came from.

C2 is done (2026-10-05): the core runs every model check the text checker enforces, each read from the YAML's meaning, and all 36 step-4 findings over the 245 exported checks agree, with no extra one. Planting found three clauses YAML cannot bring to step 4, each caught earlier (2026-10-05): a call or create missing a required argument (check 2) is step 3's, a comparison with a bare `null` (check 21) is refused by the expression parser, and `time_in` of an undeclared state (check 58) is refused by step 3 as an unqualified name. The core's port of those clauses is held only by the absence of extra findings over every plant.
