# ADR-0214: A release before 1.0.0, 0.3.0

- **Status:** **Decided** 2026-10-17 by the author ("instead of 1.0.0, I'd rather a pre-v1 release, we can keep 1.0.0 until we get a functional version of the operations system. before that we can keep iterating the objectflow"), decision 1. Decisions 2 to 5 are the recommendations given with it, taken under the author's direction in the same message ("go with your recommendations"), and stand for the author to change.
- **Date:** 2026-10-17
- **Supersedes in part:** ADR-0204 §7 (the release gate runs on the 1.0.0 candidate) and §8's last paragraph (the author then decides 1.0.0), which now gate 1.0.0 alone; ADR-0213 decision 1 (1.0.0 ships PostgreSQL), which 0.3.0 does
- **Relates to:** ADR-0180 (what a release promises, from 1.0.0); ADR-0177 (a store upgraded between releases); ADR-0182 decision 8 (fixes before 1.0.0 in the latest release only); ADR-0183 (the release workflow); `CHANGELOG.md`; `TODO.md`, the release gate and PostgreSQL in the release

## Context

The release being prepared was numbered 1.0.0, the release from which ADR-0180's promise binds: within `/v1` and flow format 1 a release only adds, a check that would refuse what an earlier release accepted arrives as a notice first, a major version is maintained for a year after the next. ADR-0204 set its gate: a 24-hour soak at the profile's load, a confirming review by cold reviewers, and the author's decision against a floor of open findings.

Its first deployment is the operations platform, which is being built on the engine now and will find what the engine lacks as it is built. Binding the promise before that would hold the engine to `/v1` and format 1 as they stand while its first real consumer is still shaping them.

## Decision

1. **The next release is 0.3.0**, a release before 1.0.0, and what the operations platform runs while it is built, on SQLite or on PostgreSQL. **1.0.0 follows a functional operations system.** Until then the engine keeps iterating in 0.x releases.
2. **0.3.0's gate** is what can be run without a soak or a review round: `scripts/check-all` on both backends (`OBJECTFLOW_CHECK_POSTGRES` set) and CI's `check.yml` green on the tagged commit; Gates P and I measured on both backends at the profile, each miss written in `TODO.md` and accepted by name; the PostgreSQL reviews' majors fixed, or stated as limits in `operate.md`; the release built by `release.yml` from the tag. **ADR-0204's soak, confirming review and stopping rule move to 1.0.0**, unchanged.
3. **Within 0.x, `/v1` and flow format 1 may change**, each change listed in `CHANGELOG.md` under its own heading where it may refuse what the release before accepted; ADR-0180's promise binds from 1.0.0 as before. **A store is never stranded**: every release after 0.3.0 carries an upgrade from the one before, on both backends, so the upgrade on PostgreSQL (`TODO.md`, PostgreSQL in the release, block 7g) gates 0.4.0.
4. **The release registry's entry for this release is 0.3.0**, its step from 0.2.0 the one the entry for 1.0.0 had, its fingerprint now also digesting what a store on PostgreSQL holds; no store was made at a released 1.0.0, so nothing is stranded. Stores made by development builds since 0.2.0, which named their release 1.0.0, are not opened, which `CHANGELOG.md` says. 1.0.0's entry is made when 1.0.0 is cut.
5. **Pushing and tagging stay the author's acts** (ADR-0204 §7).

## Alternatives rejected

- **1.0.0 now**, as planned: binds `/v1` and format 1 while the first consumer is still finding what the engine lacks, so each gap it finds would wait for 2.0 or arrive as an addition shaped around the promise.
- **0.2.1**: the change from 0.2.0 is a minor's, a second backend and the store's schema changed, not a patch's.
- **1.0.0-rc.1, or 0.9.0**: each says 1.0.0 is next, where 1.0.0 waits on the operations system and more 0.x releases may come first.
- **Keeping ADR-0204's soak and confirming review for 0.3.0**: they test the promise 1.0.0 makes, which 0.3.0 does not; the automated gate on both backends, the measured gates and the PostgreSQL reviews are what a release the first consumer builds on needs.

## Consequences

- `core/Cargo.toml` is at 0.3.0, the service says `objectflow-service 0.3.0, serving stores of objectflow-core 0.3.0`, and `release.rs` records 0.3.0 with its fingerprint.
- `CHANGELOG.md`'s unreleased section is 0.3.0's; `README.md`, `wire.md`, `operate.md` and `flow-format.md` say the promise binds from 1.0.0 and the release is 0.3.0; ADR-0204 and ADR-0213 point here.
- `TODO.md`'s release gate (section F) tracks 0.3.0 against decision 2, and 1.0.0 against ADR-0204 §§7 and 8.
