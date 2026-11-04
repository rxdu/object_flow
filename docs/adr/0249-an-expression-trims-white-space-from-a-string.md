# ADR-0249: An expression trims the white space around a string

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #63. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0103 decision 3 (text bounded by `length`); `declaration-syntax.md` §9, which left trimming to the caller's edge or an evaluator
- **Relates to:** issue #63; `storage-schema.md` §3.4 (an indexed derivation's generated column)

## Context

Issue #63, filed 2026-11-03 at 0.12.0, found that a guard cannot refuse a text that is only white space. Reproduced 2026-11-04 on the published 0.14.0 (`TODO.md`, #63): guards `inputs.reason != ""` and `length(inputs.reason) > 0` each refused `""` and took `"   "`. A consumer that wants a reason said trims it at its own edge, and every other writer can still record a blank one.

`length` was the one string operation beside equality and membership (ADR-0103 decision 3). The declaration syntax left *"trimming blank input to absence"* to the caller's edge or to an evaluator, *"since a pattern's meaning differs between Python and each backend's SQL"* (`declaration-syntax.md` §9). Trimming has the same risk in a smaller form: the core is Rust, whose `str::trim` removes Unicode's `White_Space` characters; the reference runner is Python, whose `str.strip()` also removes U+001C to U+001F; and SQL's `TRIM` removes spaces alone. An indexed derivation is to be a generated column the database computes (`storage-schema.md` §3.4).

## Decision

1. **`trim(<string>)` yields the string without the white space around it**, the characters of Unicode's `White_Space` property and no other, and `unknown` for an absent value, as `length` does. A guard `trim(inputs.reason) != ""` then refuses a reason of white space alone from every writer.
2. **It changes no value written.** A request records its input as sent.
3. **Both runners trim the same 25 characters.** The core uses Rust's `str::trim`. The reference runner strips its `WHITE_SPACE`, the same set listed, since Python's `str.strip()` would also strip U+001C to U+001F.
4. **An indexed derivation may not call it** (check 46), since its generated column would compute it in SQL, whose `TRIM` removes spaces alone.
5. **It takes a string alone.** A value of another type does not type (check 24).

## Alternatives rejected

- **A pattern match**, such as a regular expression. It is larger than the need, harder to read in a condition, and its meaning differs between Python and each backend's SQL.
- **`blank(<string>)`, a test alone.** It serves one test, where `trim` serves it and a comparison of trimmed text.
- **The engine trimming every string input before recording it.** It changes what a writer wrote, which a record does not do silently.
- **Trimming left to the caller's edge**, as before. It holds for that caller's writes alone.

## Consequences

- Built in the core's `eval.rs`, `check/names.rs` (its type and check 46), `check/data.rs` and `request.rs` (what a call reads), and in `scripts/flowrun.py` and `scripts/check-flows.py`; `declaration-syntax.md` §9 and §9.5 say so.
- **Tested:** a guard refusing `"   "`, a tab, a newline and U+3000 together, and `""`, taking `" a "` and U+001C, and writing each as sent, on both backends (`a_guard_refuses_a_text_of_white_space_alone_by_trim`); the core's white space checked against the reference runner's list over every code point (`trim_takes_unicodes_white_space_alone`); both checkers agreeing on three plants, its use, a number given it and an indexed derivation calling it; and the Flow Studio's test through both runners alike, which fails with Python's own `strip`.
