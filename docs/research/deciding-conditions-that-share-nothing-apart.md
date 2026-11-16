# Deciding conditions that share nothing apart

How a solver keeps a set of conditions that share nothing from multiplying into each other's cases: KLEE's constraint independence decides each independent subset alone, Z3's `elim-uncnstr` replaces a term over variables that occur once with a fresh one, and the Tseitin transformation avoids the expansion altogether. Asked by issue #98, filed 2026-11-15 at 0.27.0 and reviewed 2026-11-16 at the author's word ("review open issues and address properly").

## The question

Step 5 (`flow-format.md` §7) proves that a state's invariants can hold together, and that a transition's guards and what it writes can, by expanding each condition into cases, each a conjunction of atoms, and taking every combination of one case from each condition (`flowlogic.py`'s `product`, `check/logic.rs`'s port). Past 4,096 combinations it reports the set `undecided` rather than deciding it (ADR-0137 decision 3). A part it cannot read, such as a call or a path to another object, is an opaque atom, which may be true, false or unknown.

#98 reports six invariants of the form `is_whole(q1) or item.f1`, each reading parts no other reads, reported `undecided` in every state, while the same six written as one call each are decided. It asks that a combination of opaque parts, none appearing elsewhere in the set, be read as one opaque part.

## Findings

### KLEE: constraint independence

KLEE, a symbolic execution engine, sends sets of constraints to a solver. Its paper (Cadar, Dunbar and Engler, OSDI 2008, §3.3 "Query Optimization", p. 5) describes one optimisation as follows: "Constraint independence (taken from EXE) divides constraint sets into disjoint independent subsets based on the symbolic variables they reference." Its example: "given the constraint set {i < j, j < 20, k > 0}, a query of whether i = 20 just requires the first two constraints." On a sample of the coreutils, "the independence optimization by itself does not eliminate any queries, but the simplifications it performs reduce the overall running time by almost half (45%)."

### Z3: eliminating unconstrained variables

Z3's tactic `elim-uncnstr` is registered as "eliminate application containing unconstrained variables." (`src/tactic/core/elim_uncnstr_tactic.h`). Its helper `collect_occs` marks each variable it meets a second time (`m_more_than_once`) and keeps those never marked (`collect_occs.cpp`). The tactic then replaces an application over such a variable with a fresh constant: "Create a fresh variable for abstracting (f args[0] ... args[num-1])" (`elim_uncnstr_tactic.cpp`, `mk_fresh_uncnstr_var_for`). Its comments warn that one occurrence is not always enough: in their example two constants "have only one occurrence in the formula above, but they are not really unconstrained", since a quantifier elsewhere constrains their sort, and so the tactic applies only to interpreted sorts with more than one element.

### The Tseitin transformation

Converting a formula to conjunctive normal form by De Morgan's laws and distribution "can result in an exponential increase in equation size. The Tseytin transformation outputs a formula whose size grows linearly relative to the input circuit's." It does so by naming each subformula: "For each gate, a new variable representing its output is introduced." The result is equisatisfiable, and a SAT solver then searches it, rather than any procedure listing its cases.

## ObjectFlow beside them

Step 5 lists cases, as the conversion the Tseitin transformation avoids does, and bounds the list at 4,096 rather than growing it exponentially. Each `or` an invariant must not make false takes its true cases and three products of its parts' cases (`flowlogic.py`, `cases`), so `is_whole(q1) or item.f1`, both parts opaque, takes five, and six such invariants take 5⁶ = 15,625 combinations (derived here from the code). Measured 2026-11-16 at `e9b83ba`: the issue's flow is reported `undecided` in both states and for its creation, by both checkers; with five of its six invariants, decided.

An opaque atom is keyed by its expression's text, and a combination holding one key with two values is refused (`flowlogic.py`, `satisfiable`), so a condition and its negation are found to contradict. Requirements that share no key, no variable and no opaque part cannot constrain each other: the set can hold exactly when each group can, the difference bounds included, since a cycle of bounds through the constant zero lies within one group (derived here). Where a group's bounds are all on whole numbers, deciding it alone tightens its strict bounds as whole numbers, which a decimal bound elsewhere in the set had prevented (`consistent` decides one test of whole numbers over the whole combination), so it finds more contradictions, each one among the bounds. The same bounds in one condition are still decided together, so deciding each connected part of the bounds with its own test makes the answer the same however the rules are split (derived here).

A prototype of independence in the reference checker, kept out of the repository, was run 2026-11-16 at `e9b83ba`. It decided the issue's flow, and still found a contradiction between two guards reading one opaque part. Over the design's modules, the checker's self-tests, its 948 planted checks and the metric scenarios, it changed one answer: a plant built to pass the bound with four pairs of invariants, each pair reading one text, which it decided. It decided #87's type, eight optional texts each never blank, with ADR-0274's exclusion of their generated invariants switched off. Given guards `n > 3`, `n < 4` and `amount > 0`, `n` an `int` and `amount` a decimal, it found the first two contradict, which 0.27.0's checker does not.

## What it suggests

These are recommendations for the author, not decisions.

- **Decide each group of requirements that share no atom apart, as KLEE does.** It is exact for step 5's atoms, it decides the issue's flow, and it covers what the issue's merge leaves out: rules whose parts step 5 reads, such as one optional value each, which ADR-0176 and ADR-0274 met and answered by leaving one generated invariant out of step 5 each.
- **The issue's merge follows Z3's `elim-uncnstr`.** Under three-valued logic, a combination of opaque parts joined by `and`, `or`, `not` and `implies`, each part read once in it and nowhere else in the set, can be true, false or unknown, so a fresh opaque part stands for it without changing what is decided (derived here). A part read twice breaks it: `is_whole(q) or not is_whole(q)` can never be false, where a fresh part can. It covers one condition whose own expansion passes the bound, which #98 does not meet, and needs every opaque part of a set counted before any is expanded.
- **A Tseitin encoding searched by a solver** removes the expansion and its bound, at the cost of rewriting step 5 in both checkers, or the dependency ADR-0137 rejected.

## Sources

| Source | URL | Retrieved | Version |
|---|---|---|---|
| Cadar, Dunbar, Engler, "KLEE: Unassisted and Automatic Generation of High-Coverage Tests for Complex Systems Programs", OSDI 2008 | https://llvm.org/pubs/2008-12-OSDI-KLEE.pdf | 2026-11-16 | the paper as published |
| Z3, `elim_uncnstr_tactic.h` | https://raw.githubusercontent.com/Z3Prover/z3/master/src/tactic/core/elim_uncnstr_tactic.h | 2026-11-16 | commit `f46b3c5` |
| Z3, `elim_uncnstr_tactic.cpp` | https://raw.githubusercontent.com/Z3Prover/z3/master/src/tactic/core/elim_uncnstr_tactic.cpp | 2026-11-16 | commit `f46b3c5` |
| Z3, `collect_occs.cpp` | https://raw.githubusercontent.com/Z3Prover/z3/master/src/tactic/core/collect_occs.cpp | 2026-11-16 | commit `f46b3c5` |
| Wikipedia, "Tseytin transformation" | https://en.wikipedia.org/w/index.php?title=Tseytin_transformation&action=raw | 2026-11-16 | revision 1385463288 |
