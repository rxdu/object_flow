# ADR-0143: The core reads YAML with saphyr and a strict loader of its own, and computes decimals in 128-bit fixed point, a declared decimal holding at most 18 digits

- **Status:** Proposed, 2026-09-30, for the author's decision. The questions it leaves are listed at its end.
- **Date:** 2026-09-30
- **Refines:** ADR-0132 decision 3 (two checks before the first line of engine code)
- **Relates to:** `declaration-syntax.md` §3.1, §8.3; `storage-schema.md` §3.1; `scripts/check-flows.py`'s strict loader; D484

## Context

ADR-0132 decision 3 asked for two checks before the first line of the Rust core. A YAML parser must be maintained and strict, "the long-standard `serde_yaml` is believed unmaintained since 2024". A decimal type must be wide enough for `int × decimal(p,s)`, a `decimal(p+19,s)` "up to 38 digits", and "`rust_decimal` is believed to stop near 28". Both were beliefs. This records what was checked on 2026-09-30, from crates.io's API, the RustSec advisory database, each project's repository, and SQLite's documentation.

**What the parser must do** is what the flow checker's strict loader does (`scripts/check-flows.py`): only `true` and `false` are booleans, so an enumeration member named `NO` stays a name; a key written twice in one mapping is an error, not silently the last; and every key and list item has a line, by which a finding is reported where it is.

**What was found about parsers:**
- `serde_yaml`'s last release is `0.9.34+deprecated`, of 2024-03-25.
- `serde_yml` and the `libyml` it rests on are "unsound and unmaintained", their repositories archived (RUSTSEC-2025-0068, RUSTSEC-2025-0067, 2025-09-11).
- `yaml-rust` is unmaintained (RUSTSEC-2024-0320).
- `serde_yaml_ng` and `serde_norway`, forks of `serde_yaml` over ports of C libyaml, last released on 2024-05-26 and 2024-12-21.
- `saphyr` and `saphyr-parser` (0.1.0, 2026-09-19) are pure Rust, and the parser "tests against (and passes) the YAML test suite". Every node of `saphyr`'s `MarkedYaml` carries a `Span`. Its loader inserts a repeated key over the first, with no error (`saphyr/src/loader.rs`), and it resolves `true`, `True` and `TRUE` as booleans (`saphyr/src/scalar.rs`), which is YAML 1.2's core schema, wider than the strict loader's `true` alone.
- `serde-saphyr` (1.3.0, 2026-09-16) deserializes into Rust types over `saphyr-parser`. It refuses a repeated key by default, has a `strict_booleans` option, and gives locations through `Spanned`.
- `yaml-rust2` (0.13.0, 2026-09-11) is pure Rust and states full YAML 1.2 compliance. Whether its nodes carry positions was not checked.

**What was found about decimals.** `rust_decimal` (1.43.0) is "a 96 bit integer number" and a scale, about 28 digits. `bigdecimal` (0.4.11, arbitrary precision), `fastnum` (0.7.5), `decimal-rs` (0.2.0) and `dashu-float` (0.6.1) are all released in 2026. Whether each holds 38 digits exactly was not checked. The model bounds no decimal's precision: `declaration-syntax.md` §3.1 and §8.3 name `decimal(p,s)` with no largest `p`. Yet `storage-schema.md` §3.1 stores one on SQLite as an `INTEGER` scaled by 10^s, and SQLite's integer is "stored in 0, 1, 2, 3, 4, 6, or 8 bytes", at most 64 bits, so at most 18 digits in full. ADR-0132's "up to 38 digits" assumes a `p` of at most 19 that nothing states (D484).

## Decision

1. **A declared decimal holds at most 18 digits.** `decimal(p,s)` requires `1 ≤ p ≤ 18` and `0 ≤ s ≤ p`, and step 3 and the model's check 17 refuse a larger one. This is what `storage-schema.md`'s SQLite mapping already needs, and a store's decimals, like its money and its integers, are then 64-bit on both backends.
2. **The core computes decimals as 128-bit fixed point, a type of its own**, with no dependency. A value is an `i128` and a scale, and every operation follows `declaration-syntax.md` §8.3's rules for the result's scale, rounding half to even where §8.3 rounds. With `p ≤ 18`, the widest intermediate, `int × decimal(18,s)`, is 37 digits, and an `i128` holds every 38-digit number. Arithmetic is checked, and a value that overflows its target's precision refuses the request, as §8.3 already says.
3. **The core reads YAML with `saphyr-parser`, under a strict loader of its own.** The loader builds the node tree with a span on every node, from which the line of every key and list item follows. It refuses a repeated key. It resolves a plain scalar as the flow checker's strict loader does, so `TRUE`, like `NO`, stays a name. `serde-saphyr` may deserialize the checked tree into the core's types, with repeated keys refused and `strict_booleans` set. The differential tests of ADR-0132 decision 2 hold the two loaders to the same reading of every module in the design.

## Alternatives rejected

- **`serde_yaml`, `serde_yml`, `yaml-rust`**: deprecated, or unsound and archived, or unmaintained, as above.
- **`serde_yaml_ng` or `serde_norway`**: forks resting on ports of C libyaml, neither released since 2024. They would bring C-derived unsafe code into the core to parse the only input an author writes.
- **`saphyr`'s own loader, taken as is**: it keeps the last of two equal keys and reads `TRUE` as a boolean, so a module could mean one thing to the core and another to the flow checker.
- **`rust_decimal`**: 28 digits cannot hold `int × decimal(18,s)`.
- **An arbitrary-precision crate** (`bigdecimal`, `dashu-float`): correct at any width, but each value allocates, and width the model never needs is paid for on every guard. A fixed width also makes overflow a check the core can state.
- **Allowing decimals of up to 38 digits**, stored on SQLite as text: every comparison and sum on SQLite would then need a decimal the backend does not have, and the backends would be made to agree by code rather than by type.

## Consequences

- `declaration-syntax.md` §3.1 and check 17, `flow-format.md` §4.3 and the schema state decision 1, and D484 is resolved by it.
- ADR-0132 decision 3's two checks are made; TODO's two items close when this is decided.
- The core's decimal type is written with the first slice of the core, proven against `scripts/flowrun.py`'s exact fractions by the differential tests.

## Open questions

1. **The bound on a declared decimal**: 18 digits, as decision 1 has it, or 38, with SQLite storing decimals as text?
2. **The parser**: `saphyr-parser` under a strict loader of the core's own, as decision 3 has it?
3. **The decimal type**: a 128-bit fixed-point type of the core's own, as decision 2 has it, or a crate?
