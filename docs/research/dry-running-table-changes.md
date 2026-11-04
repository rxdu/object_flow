# Dry-running a version's table changes

**The question.** Issue #59 (filed 2026-11-03) found a version that the store's dry run accepted and its approval refused, `500 internal`. The approval's table changes sent a derived end, which has no column, to `ALTER TABLE … RENAME COLUMN`. The dry run is a simulation in memory: it applies the mappings to the objects and checks the result, and it runs none of the table changes (`publish.rs`, `dry_run`; `store.rs`, `install`). The issue asks that the dry run report what the approval would refuse. This document records how established migration tools relate a dry run to the DDL they apply, and what that suggests for ObjectFlow.

## 1. Most dry runs print the statements and run none

**Flyway** (S1, S2). A dry run sends the statements to a file instead of the database: "The database is no longer modified and all SQL statements that would have been applied are sent to the dry run output instead." It reads the database, never writes it: "When doing a migrate Dry Run, Flyway sets up a read-only connection to the database." The setting's default is the real run: "Omit to use the default mode of executing the SQL statements directly against the database."

**Liquibase, `update-sql`** (S3). "The update-sql command is a helper command that allows you to inspect the SQL Liquibase will run while using the update command." The page says what it cannot catch: "The command does not automatically check SQL for correctness and does not anticipate database deployment errors resulting from malformed SQL."

**Atlas, `--dry-run`** (S5). "By using the dry-run option, Atlas prints the migration files and their SQL statements that are pending to be applied without executing them." One part does run: "However, if the migration plan contains pre-migration checks, Atlas executes them on the database and report the results."

**Alembic, offline mode** (S9). "A major capability of Alembic is to generate migrations as SQL scripts, instead of running them against the database - this is also referred to as offline mode." No database is reached, so a script that reads rows fails there: "This means a migration script that pulls some rows into memory via a SELECT statement will not work in --sql mode."

## 2. The tools that catch a refused statement run it on an empty copy

**Atlas, the dev database** (S4). A dev database is "a temporary, isolated database instance that Atlas uses as a sandbox to simulate the real environment". The page says why parsing is not enough: "Parsing schemas and migrations is not enough: every expression, constraint, default, function call, and DDL/DML statement must be semantically valid and accepted by a real database of the same type and version." Its example is a constraint naming a column that does not exist, which passes a plan without it and fails mid-apply: "With --dev-url, Atlas runs it on the dev-database first, so the error shows up at plan time instead of mid-apply".

**Percona's `pt-online-schema-change`** (S7). The tool works on a copy: "pt-online-schema-change works by creating an empty copy of the table to alter, modifying it as desired, and then copying rows from the original table into the new table." Its dry run stops after the change to the empty copy: "Create and alter the new table, but do not create triggers, copy data, or replace the original table."

**GitHub's `gh-ost`** (S8). Without `--execute` a migration is "a noop: testing table creation and validity of migration, but not touching data."

**Prisma Migrate, the shadow database** (S6). The shadow database is "a second, temporary database that is created and deleted automatically" whenever `prisma migrate dev` runs, and it is "primarily used to detect problems such as schema drift or potential data loss of the generated migration". It is a development tool: "The shadow database is not required in production, and is not used by production-focused commands such as prisma migrate resolve and prisma migrate deploy."

**Skeema, workspaces** (S12). "Skeema actually runs these CREATE statements in a temporary location, called a workspace." The page names its cost on a shared server: "On busy production database servers with high-volume OLTP workloads, Skeema’s workspace setup and workspace cleanup can negatively impact overall database server performance."

## 3. A real apply rolled back is possible, and no tool read uses it as a dry run

**PostgreSQL** (S10, S11). Its DDL is transactional: "If you put all such changes into a transaction block, you can make sure they all apply atomically or not at all." A change to a table locks it: "An ACCESS EXCLUSIVE lock is acquired unless explicitly noted."

**Liquibase, `update-testing-rollback`** (S13). It applies and then undoes: "update-testing-rollback tests rollback support by deploying all pending changesets to the database, executes a rollback sequentially for the equal number of changesets that were deployed, and then runs the update again deploying all changesets to the database." A failure keeps what came before it: "Any changesets that come before the failed changesets are deployed to the database." *Inferred:* it tests a deployment on a target database, and none of the pages read offers an apply rolled back as a dry run.

## 4. ObjectFlow beside them

- **The dry run is neither of §1's or §2's kinds.** It prints no statement and runs none. It applies the mappings to every object in memory and checks the invariants, the references and what each attribute can hold. The table changes are the approval's alone (`store.rs`, `install`).
- **A refused statement changes nothing.** A publish's table changes, mappings and `of_declaration` row are one transaction (`storage-schema.md` §10). Probed 2026-11-03 with the published 0.12.0: the approval #59 reports was refused on SQLite and on PostgreSQL, and every table of the store, its rows and its schema read alike before and after it (*Issues filed on GitHub*, `TODO.md`).
- **The table changes depend on the two versions.** *Inferred from reading `install` at `af72aaa`, and probed 2026-11-04 with a prototype running `install`'s own statements on an empty copy on both backends (`TODO.md`, #59):* each statement before the mappings is made from the installed version's tables, the new version's, and the migration's renames. The rows it then moves, intervals and write-index rows, are selected by name. An empty copy of the installed version's tables would therefore receive every statement the approval sends before the mappings, as `pt-online-schema-change`'s empty copy receives its `ALTER`.
- **What an empty copy cannot show.** A statement refused only because of the rows it meets, such as a constraint added over a violating row, passes on an empty copy. The simulation is what judges the rows today.
- **A dry run holds no writer.** A change's checks run with no read of the store open (ADR-0195 decision 1), and the dry run reads the store as a snapshot. An apply rolled back would hold the writer, take PostgreSQL's `ACCESS EXCLUSIVE` lock on each table it changes, and copy each changed table's rows, since a table brought to a version's shape is rebuilt (`store.rs`, `rebuild`). Its cost would grow with the store, where a dry run's checks do not.

## 5. What it suggests

For the author, not decided here:

1. **Run the approval's table changes in the dry run on an empty copy of the installed version's tables**, on the store's own backend, and roll the copy back. A statement the copy refuses is a fatal finding naming the statement, so the submit is refused `dry_run_clean`. This is Atlas's dev database and `pt-online-schema-change`'s dry run, which are the sources' tools that catch a refused statement before the apply.
2. **Keep the simulation as the judge of the rows**, since an empty copy holds none.
3. **Do not apply on the store and roll back.** No source read uses it as a dry run, and it would hold the writer and lock the tables it changes for as long as their rows take to copy.
4. **Do not stop at printing the statements.** Liquibase says outright that such output does not check the SQL.

## Sources

Each retrieved 2026-11-03 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text. A GitHub-hosted page's version is the latest commit to its file.

| | Source | Version |
|---|---|---|
| S1 | Redgate, *Migration command dry runs*, Flyway: https://documentation.red-gate.com/flyway/flyway-concepts/migrations/migration-command-dry-runs | Teams edition; page last updated 15 January 2025 |
| S2 | Redgate, *Flyway dry run output setting*: https://documentation.red-gate.com/flyway/reference/configuration/flyway-namespace/flyway-dry-run-output-setting | page last updated 30 October 2026 |
| S3 | Liquibase, *update-sql*: https://docs.liquibase.com/secure/reference-guide-5-1/init-update-and-rollback-commands/update-sql | Liquibase Secure 5.1, last updated October 7, 2026 |
| S4 | Atlas, *Dev Database*: https://atlasgo.io/concepts/dev-database | the page gives none |
| S5 | Atlas, *Applying migrations*: https://atlasgo.io/versioned/apply | the page gives none |
| S6 | Prisma, *Shadow database*: https://www.prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/shadow-database, served from `/docs/orm/v7/prisma-migrate/understanding-prisma-migrate/shadow-database` | Prisma ORM v7; `prisma/docs` at `61e16da`, 2026-10-13 |
| S7 | Percona, *pt-online-schema-change*: https://docs.percona.com/percona-toolkit/pt-online-schema-change.html | pt-online-schema-change 3.7.2 |
| S8 | GitHub, gh-ost, *Command line flags*: https://raw.githubusercontent.com/github/gh-ost/master/doc/command-line-flags.md | `github/gh-ost` at `2425722`, 2026-08-05 |
| S9 | Alembic, *Generating SQL Scripts (a.k.a. “Offline Mode”)*: https://alembic.sqlalchemy.org/en/latest/offline.html | Alembic 1.20.1 |
| S10 | PostgreSQL wiki, *Transactional DDL in PostgreSQL: A Competitive Analysis*: https://wiki.postgresql.org/wiki/Transactional_DDL_in_PostgreSQL:_A_Competitive_Analysis | last edited 11 June 2012 |
| S11 | PostgreSQL, *ALTER TABLE*: https://www.postgresql.org/docs/current/sql-altertable.html | PostgreSQL 18 |
| S12 | Skeema, *Workspaces*: https://www.skeema.io/docs/features/workspaces/ | the page gives none; it describes Skeema v1.13 on |
| S13 | Liquibase, *update-testing-rollback*: https://docs.liquibase.com/secure/reference-guide-5-1/init-update-and-rollback-commands/update-testing-rollback | Liquibase Secure 5.1, last updated August 21, 2026 |
