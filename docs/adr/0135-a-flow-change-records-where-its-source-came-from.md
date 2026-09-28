# ADR-0135: A flow change records where its source came from, and a digest that lets the repository check it

- **Status:** Accepted by the author in advance, 2026-09-28, under the author's standing instruction "for all similar items that you have confidence in an recommendation, just accept", as the design work ADR-0131 decision 1 left, which the author started the same day ("let's start the design work").
- **Date:** 2026-09-28
- **Refines:** ADR-0131 (how flows are authored and checked), ADR-0085 and ADR-0097 (a flow change's lifecycle)
- **Relates to:** PRD F1, F7, F10; `DESIGN.md` §5.9, §9, §10; `storage-schema.md` §6; `publish-and-import.md` §1, §2; `library-api.md` §6; `flow-format.md` §7

## Context

ADR-0131 decision 1 put flows in a version-controlled repository, an upper layer, and left the engine to keep governance: "a `DeclarationChange`, and the version it installs, record the source reference, such as the commit, the change was drafted from". It did not say what the reference is, who supplies it, what the engine checks of it, or how anyone could tell that a published version really came from the commit it names. The engine holds the text it installed (`of_declaration.source`) and never reads the repository, so a reference on its own is an assertion by whoever drafted the change.

A change's source is a module and the closure of its imports (`publish-and-import.md` §1): several files, one module each.

## Decision

1. **`draft` and `revise` take a required input `source_ref`**, a string naming where the source came from, such as `git:ops-flows@3f2a1c9`. The engine does not interpret it. It refuses, as `invalid input`, one that is empty, longer than 512 characters, or holds a control character. A `revise` replaces the source and its reference together, since a revised source comes from another commit.
2. **The engine records a digest of the source beside the reference**, on the change when it is drafted or revised and on the version a publish installs. The digest is SHA-256 over the source's modules in ascending order of their names' UTF-8 bytes, each written as its name, a line feed, its length in bytes in decimal, a line feed, and its bytes as submitted, and is written `sha256:<hex>`. Whoever holds the repository computes the same over the files at the reference and compares, so a published version is traceable to its commit and checkable against it, without the engine reading the repository. `scripts/check-flows.py --digest <files>` computes it, as the reference the repository's own checks use.
3. **Version 0 has neither.** Its source is the engine release's built-in module, which `builtins` names (ADR-0105).
4. **Reads return both.** `declaration(type, version)` returns the version's reference and digest, a change's `get` returns its own, and the publish report names both, so a reviewer reads which commit is being approved.

## Alternatives rejected

- **The engine fetches the repository to verify the reference.** It would need the repository's credentials and a network path, and would fail when the repository is down; ADR-0131 places the repository in an upper layer, as ADR-0110 places every caller.
- **The reference alone, with no digest.** Nothing could then show that the installed text is the text at the commit; a drafter's mistake, or a rewritten history, would be invisible.
- **A structured reference: repository, commit and path as fields.** It binds the engine to one version-control system, and the engine reads none of the parts.
- **An optional reference.** ADR-0131 puts every flow in a repository, and an optional field is the one left empty; the engine cannot make a reference true, but it can make its absence a refusal.
- **A digest over the text normalised, such as line endings or YAML re-serialised.** Normalisation is a second definition both sides must implement alike; hashing the bytes as submitted leaves nothing to agree on but the order and the framing, which this record fixes.
- **Recording only the commit and not the text.** The store is the record of what applied; a repository can be rewritten, and every event names a version whose rules must stay readable (`storage-schema.md` §6).

## Consequences

- `DESIGN.md` §9 and §10, `storage-schema.md` §6 (`source_ref` and `source_digest` on `t_declaration_change` and `of_declaration`), `publish-and-import.md` §1 and §2 and `library-api.md` §6 say so.
- `scripts/check-flows.py` computes the digest, and its self-test holds it independent of file order and sensitive to a single byte and to the boundary between two modules.
- PRD F10 states the requirement in the author's words of 2026-09-25, and `traceability.md` traces it.
