# ADR-0211: Seven questions decided as recommended

- **Status:** **Decided** 2026-10-17 by the author ("go with your recommendations."), given after the seven questions were listed on 2026-10-16, each with the recommendation `TODO.md` records; each decision is that recommendation. The release gate the author keeps (ADR-0204 §7, §8) is not decided by it.
- **Date:** 2026-10-17
- **Refined by:** ADR-0212 — decision 4's remainder: a dry run's class says what excluding it uncovers.
- **Refines:** ADR-0147 (a dry run's dispositions); ADR-0191 decision 7 (the repository's settings)
- **Relates to:** `DESIGN.md` §5 and §9; ADR-0017, ADR-0041, ADR-0087 (a file's bytes); `design/hiring-third-review.md` E16, E24, E32; ADR-0208 (the writer at the peak); ADR-0210 decision 2

## Context

Seven questions stood open on 2026-10-16, each in `TODO.md` with its options:

- **Who deletes an erased file's bytes.** `DESIGN.md` makes deleting a file's bytes, once no unerased reference holds it, the engine's one byte-level operation; `integrate.md` gives the bytes to the application; the core holds a file as its hash, size and type, with no file table and no access to the blob store, so after an erasure nothing tells the application which bytes became unreferenced, and identical files shared across objects hide it from the application's own records.
- **The repository's settings.** The fifth review found `main` taking a push whose `check-all` failed, `v*` tags unprotected and Dependabot off.
- **E16.** A request whose effect creates answers the parent's id, and a screen opening the new object re-reads and guesses.
- **E24.** A disposition decides a whole class, and excluding objects uncovers new classes, references to what was excluded, one dry-run round at a time.
- **E32.** Nothing stops many pending proposals of one transition on one object from one proposer.
- **An examples step expecting an import's refusal.**
- **A window over transitions**, which needs an index on the log's time that every write would keep (ADR-0210 decision 2's remainder).

## Decision

1. **A read answers which of the file hashes given are still referenced by an unerased object** (option (b)). The application asks it of its blob store's inventory, after an erasure or on a schedule, and deletes the bytes nothing holds; no erased hash is kept anywhere, sharing is seen, and the engine needs no access to the blob store.
2. **The repository's settings: a ruleset letting only the repository's administrators create, move or delete a `v*` tag, and Dependabot for cargo, pip, npm and the workflows' actions** (options (b) and (c)); requiring `check-all` on `main`, option (a), waits until pushes go through pull requests. Each is applied by the author: Dependabot's configuration is a file the repository holds, effective once pushed, and the ruleset a call `scripts/setup/github-settings.sh` makes.

   *Refined 2026-10-18, as recommended under the author's direction ("handle all the PRs", "go with your recommendations"):* its first run opened twenty pull requests, each Python one twice, since the studio server's requirements include the scripts' and both directories were watched; the server's entry now watches its own pin, pytest, alone. jsonschema's major versions are ignored: 4 brings three packages the tree does not hold, one compiled, and words of its own in the checks' findings, so it is a change of its own (`scripts/requirements-dev.txt`). Each pull request is still one dependency.
3. **A satisfied verdict lists `created`**, each object the request created in order, its own first where it is a creation (E16).
4. **A dry run applies its exclusions to a fixed point**, reporting in one round every class they uncover; a per-object decision stays the mapping's, as ADR-0147 designed (E24). *Corrected 2026-10-17: the dry run already did this, and the recommendation was written without reading `import.rs`, whose loop applies the file's exclusions "until nothing more is". A probe the same day excluded the class of an employee in an undeclared state, and that dry run reported `Team.lead: names an excluded object` beside it. Nothing is built for decision 4. What E24 lacked is one round earlier: an undecided class does not say what excluding it would uncover, which is brought to the author as its own question (`TODO.md`, under the hiring review).*
5. **A second pending proposal from the same proposer, of the same transition on the same object, is refused**, naming the pending one; another proposer's is taken, each proposal being its proposer's record (E32). `Proposal.inputs` stays the JSON string `wire.md` documents.
6. **An examples step expecting an import's refusal is deferred past 1.0.0**: it adds to the format, which adding later does not break, and an import's refusals are tested by the batch's dry run.
7. **A window over transitions waits on a measurement**: the cost to a write of an index on the log's time, measured on the two-year store, before an index is chosen; transitions are read whole meanwhile, under the stated limit.

## Alternatives rejected

- **The erasure's answer and event naming the hashes it left unreferenced** (option (a)): each such hash kept in the log for ever. **As now** (option (c)): the application keeping its own references, blind to sharing.
- **Requiring `check-all` on `main` now** (option (a) of the settings): it would hold each of the author's direct pushes for the run, about 17 minutes.
- **A per-object exclusion list in the disposition file** (E24's other ask): a second place to decide per object beside the mapping.
- **Refusing a duplicate from any proposer** (E32): another proposer's is their own record of what they asked.
- **Building the examples step before 1.0.0.** Nothing needs it before then.
- **Choosing the log's time index without measuring it.**

## Consequences

- `wire.md`, `wire-v1.json`, `library-api.md`, `integrate.md`, `DESIGN.md` §5 and §10 and `storage-schema.md` §9 step 4 change with decision 1. *Corrected 2026-10-17: this said the reference runner answers the read alike; the Studio's sandbox answers the part of the wire examples use, and the read, an operation's as `query`, `export`, `diagnostics` and `maintain` are, is the core's alone.* Built the same day as a scan of every table's text for the hashes asked, which keeps no index the writer maintains; its cost measured 2026-10-17 on stores the release service made with `--create` and filled by imports of synthetic mirror objects, each a row and an import event holding a photo's hash and a 600-byte note, on a 12-core machine at a load of about 1, the file in the page cache: a 184 MiB store (52,000 objects, the two-year store's file being 182 MB) answered in 0.30 s at the median of three for a hash it holds nowhere, which reads it whole, and 0.31 s for 1,000 such hashes; a 550 MiB store (155,000 objects, the six-year store's 548 MB) in 0.89 s and 0.92 s.
- `.github/dependabot.yml` and `scripts/setup/github-settings.sh` with decision 2.
- The satisfied verdict's shape in both backends, the Studio's verdict type, `errors.md` and `wire.md` with decision 3.
- ~~`publish-and-import.md` §4's dry run with decision 4.~~ *None: decision 4 was already the dry run's behaviour.*
- `errors.md` gains the duplicate proposal's refusal with decision 5.
