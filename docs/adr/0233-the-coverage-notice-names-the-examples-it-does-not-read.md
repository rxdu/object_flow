# ADR-0233: The coverage notice names the examples it does not read

- **Status:** **Decided** 2026-10-26 under the author's direction ("go with your recommendation"), a follow-up of issue #42.
- **Date:** 2026-10-26
- **Refines:** ADR-0173 decision 6 (the coverage notice, a cascade counted where an example expects it); ADR-0232 decision 4 (an example starting at the version before is checked when it runs)
- **Relates to:** ADR-0131 decision 5 (coverage reported, not required)

## Context

**The notice.** Step 3 reports `coverage` in two parts: the examined module's transitions no example takes, and those no example requests as the request under test (`flow-format.md` §11.5). It counts what the examples name and does not run them, which is why a cascade counts only where an example expects it in `cascaded` (ADR-0173 decision 6, amended).

**What it leaves out.** An example starting at the version before runs under that version until it publishes the change, so step 3 reads none of it, and it is checked when it runs (ADR-0232 decision 4). Its request under test names an object made under the version before, whose type after the migration only the run knows. So the notice counts nothing of it. Over issue #42's grants case it said no example takes `AccessGrant.revoke`, which the migration example's request under test takes. Over `service-v2.examples.yaml` it lists `ServiceJob.start` among the transitions no example requests, with nothing to say the file's migration example was not read.

## Decision

1. **Coverage stays step 3's.** An example starting at the version before counts nothing, as `flow-format.md` §11.5 says.
2. **Each part of the notice names those examples**, where the file has any, by name in order: `; <example> starts at the version before and counts nothing, since this check does not run it`, or, for several, `; <a>, <b> start at the version before and count nothing, since this check does not run them`. A file with none is noticed as before.

## Alternatives rejected

- **The notice unchanged.** It reads as a claim that no example takes a transition a migration example takes, and the author has nothing to tell the two apart.
- **Counting what the run takes, for these examples alone.** Exact for them, but coverage would count by what step 3 reads for one kind of example and by what the run did for another, where ADR-0173's amendment chose what step 3 reads for cascades. It would also hold the notice until the run, in both checkers and the core's publish, and an example not run would still count nothing.
- **Counting what the run takes, for every example and cascade.** The consistent form of the last, and closest to how a test-coverage tool measures what runs. It reverses ADR-0173's amendment and changes every notice, for a gap only migration examples show.

## Consequences

- `flow-format.md` §11.5 says the notice names them; `CHANGELOG.md` lists the change under the next release.
- The core's checker and the reference checker word it alike: the reference checker checks one example, two and none over `service-v2.yaml` and `service.yaml`, and the core's checker oracle compares the core's notices with those checks' word for word.
