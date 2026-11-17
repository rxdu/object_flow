# ADR-0284: A condition is typed with each transition guarding with it

- **Status:** **Decided** by the author 2026-11-17 ("go with your recommendations"), on the point put 2026-11-16 for the finding that check 24 does not type an input a condition reads, with a recommendation. Built the same day.
- **Date:** 2026-11-17
- **Refines:** `declaration-syntax.md` §10's check 24 (where a type can be told)
- **Relates to:** `docs/research/typing-a-shared-condition-at-each-use.md`; the author's direction of 2026-11-12 to reuse a construct rather than declare the same data twice

## Context

A condition is declared once on a type and guards any number of its transitions. What it reads of `inputs` has a type only where a transition is known. Each checker reads a condition with each transition guarding with it for the input's name, refusing `inputs.nothing`, "deliver takes no input 'nothing'" (`check-flows.py`, `check/names.rs`). Its typing alone is done once, with no transition, where an effect is typed with its transition. So an input's type is unknown there, and check 24 refuses nothing whose type it cannot tell.

Found 2026-11-15 reproducing #97, and reproduced 2026-11-16 at `87a6ae8` on both checkers, the core's through `/sandbox/load`. A condition `inputs.count == "three"`, `count` an `int` input of the transition guarding with it, loads clean, and the core refuses every request through it. `inputs.count <= this.quantity`, a `decimal(9,2)`, loads clean and is compared by value, where `this.delivered <= this.quantity`, an `int` attribute, is refused, "it compares an int with a decimal of scale 2". `inputs.snap.hash`, `snap` a `file` input, loads clean and the core answers `501 not_run`, "a member `hash` of a file", where `photo.hash`, a `file` attribute, is refused.

The research note finds C++'s two-phase lookup checking a template's names that do not depend on its use once, where it is written, and those that do at each use, against what that use supplies.

## Decision

1. **A condition that reads `inputs` is typed with each transition guarding with it**, its inputs the transition's, as an effect is typed with its transition. One that reads none is typed once, as before.
2. **What breaks with no transition is reported as before**, once and naming no transition, since it breaks at every use. Otherwise each guarding transition whose inputs break the condition is reported by name: "condition word, where deliver guards on it, does not type: it compares an int with a string".
3. **A condition that reads `inputs` and guards no transition is typed once**, with no transition, as before.

## Alternatives rejected

- **Declaring a condition's inputs on the condition.** The transition already declares them, so each input would be declared twice, and the two could disagree. The author directed on 2026-11-12 that a construct be reused before a parallel one is added.
- **Leaving it.** A module the checks pass refuses every request its guard reaches, or stops it as not run, and compares an `int` input with a decimal where two attributes are refused.

## Consequences

- **What a flow meets.** A condition comparing an input across types, or reading a file input's member other than `media_type` and `size`, is refused at step 3. So is an `int` input compared with a decimal, which the core compares by value: a module 0.28.0 accepted and runs may be refused, and is mended by declaring the input a `decimal`, since two decimals compare whatever their scales.
- **The code:** `check-flows.py`'s check 24 typing of conditions and `check/names.rs`'s alike.
- **The docs:** `declaration-syntax.md` §10's check 24 says a condition reading `inputs` is typed with each transition guarding with it.
- **Tested** by cases added to the reference's planted checks, which the core's oracle replays: an input compared with a string, a file input's unknown member and an `int` input against a decimal, each refused naming the transition; a condition two transitions guard with, one breaking it, refused naming that one; a condition whose inputs fit every transition guarding with it, and a file input's size, clean; and a break that needs no transition, said once, naming none.
