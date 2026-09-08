# ADR-0026: Declarations compose by `extends` and bind a named state machine; states carry a category

- **Status:** Accepted — taken in autonomous design iteration 2 (2026-09-07); pending author review
- **Date:** 2026-09-07
- **Amended by:** ADR-0056 §5 — a machine declares what it requires of its binders. ADR-0064 makes a binder's own creations replace the machine's rather than add to them, and ADR-0065 keeps the machine's creation guards binding on the replacement.

## Context

A ticket system gives the same issue type different workflows in different projects, and a project different field requirements from the next. ADR-0003 gives every object type exactly one state machine. The choice was a union machine with per-project guards, or one type per (project, issue type) with shared declarations. Case study: `docs/design/case-study-tickets.md` §3.

## Decision

1. **`extends`.** A type declaration may extend a base declaration, inheriting its attributes, relationships, invariants and derived attributes, and adding its own. A base may be abstract (no objects of it exist) or concrete.
2. **State machines are named declarations.** A type binds exactly one, whole. Machines are not inherited or partially overridden; two types that need the same lifecycle bind the same named machine. ADR-0003 stands.
3. **Type family.** The read surface accepts a base declaration as a query target and returns objects of every type extending it, each carrying its concrete type.
4. **State category.** Every state in a machine declares a category from a small set the consumer defines (for example `open`, `in_progress`, `done`), in addition to being terminal or not. Guards, invariants and queries that span a family use categories, so they need not know each machine's state names: `none(b in blocked_by where b.state.category != done)`.

## Alternatives rejected

### One union machine with per-scope guards

Every project's transitions in one diagram, each guarded on the project. Rejected: the printed lifecycle is one nobody's project uses, which defeats legibility (DESIGN.md, Known limits), and every guard must repeat the scope test.

### Per-instance workflows

Bind the machine per object. Rejected by ADR-0010: what is allowed must be a property of the type, inspectable without an instance.

### Inheriting and overriding machines

Rejected: a guard or outcome in a base machine references states; an override that removes or renames one silently breaks it, which is the drift this project exists to remove.

## Consequences

- "Bug in project A" and "Bug in project B" are two types extending `Bug`; a board over all bugs queries the family by category.
- Declarations become a small module system; the printable rule set for a type resolves `extends` and shows the whole.
- A category set is part of the consumer's declaration, not a fixed vocabulary.
