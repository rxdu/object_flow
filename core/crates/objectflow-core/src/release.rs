//! An engine release, and a store's upgrade from the release before it
//! (`storage-schema.md` §12, ADR-0177): the releases this core knows, each
//! with the releases it upgrades from and its migration, and the step that
//! carries a store through them, one transaction a release.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::model::OPERATOR;
use crate::store::{BUILTINS, StoreError, next_counter, release_fingerprint};
use crate::value::timestamp_text;

/// This core's release, which every store it creates or upgrades records.
pub const RELEASE: &str = BUILTINS;

/// What a release changes in a store, and how a store comes to it.
#[derive(Debug)]
pub struct Release {
    pub name: &'static str,
    /// The releases a store may be at for this one's migration to run.
    pub from: &'static [&'static str],
    /// The store's own schema and built-ins as this release creates them
    /// (`release_fingerprint`), which a change to either without a new
    /// release fails (ADR-0177 decision 1); empty for a release before the
    /// registry, which no store is upgraded to.
    pub fingerprint: &'static str,
    /// What it changes, in a sentence for the upgrade's report.
    pub changes: &'static str,
    /// Whether its built-in types or standard metrics differ from the
    /// release before, which the installed flows are re-recorded under
    /// (§12 step 3, not built: ADR-0177 decision 6).
    pub builtins_changed: bool,
    /// Its migration of the store's own tables and the tables of its types,
    /// at the time given.
    pub migrate: fn(&Transaction<'_>, i64) -> Result<(), StoreError>,
}

/// The releases, oldest first, the last this core's.
pub const RELEASES: &[Release] = &[
    Release {
        name: "objectflow-core 0.1.0",
        from: &[],
        fingerprint: "",
        changes: "the core's first release, which recorded its release only with version 0",
        builtins_changed: false,
        migrate: |_, _| Ok(()),
    },
    Release {
        name: "objectflow-core 0.2.0",
        from: &["objectflow-core 0.1.0"],
        fingerprint: "sha256:838cef222b85c1034a8f86e146c9c4286cd94138d4ff1b769189ed44c9bc4f2c",
        changes: "the store records its engine release in of_engine_release, and every operation reads it, so no copy of an earlier release serves it once upgraded",
        builtins_changed: false,
        migrate: release_table,
    },
];

/// 0.1.0 to 0.2.0: the release table, recording the release that created
/// the store, as version 0 names it, when its operator was installed.
fn release_table(tx: &Transaction<'_>, _now: i64) -> Result<(), StoreError> {
    tx.execute_batch(RELEASE_TABLE)?;
    tx.execute(
        "INSERT INTO of_engine_release (seq, release, installed_at, installed_by) \
         SELECT 1, d.builtins, COALESCE((SELECT recorded_at FROM of_event WHERE transition = 'install' \
         ORDER BY position LIMIT 1), d.published_at), ?1 FROM of_declaration d WHERE d.version = 0",
        params![OPERATOR],
    )?;
    Ok(())
}

/// The release table (`storage-schema.md` §6).
pub const RELEASE_TABLE: &str = "
CREATE TABLE of_engine_release (
  seq           INTEGER PRIMARY KEY,
  release       TEXT    NOT NULL,
  installed_at  TEXT    NOT NULL,
  installed_by  TEXT    NOT NULL
);
";

/// Whether a file that holds something holds a store: a SQLite database
/// with the store's declarations' table. A file that is no database, or is
/// another application's, does not, and is never made one.
pub fn holds_store(path: &Path) -> Result<bool, StoreError> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    match conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'of_declaration'",
        [],
        |r| r.get::<_, i64>(0),
    ) {
        Ok(n) => Ok(n > 0),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::NotADatabase =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// The release a store's file is at, or none where it holds no store: the
/// last row of its release table, or, for a store of 0.1.0, which has none,
/// the release version 0 names.
pub fn store_release_at(path: &Path) -> Result<Option<String>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    store_release(&crate::store::looking(path)?)
}

pub fn store_release(conn: &Connection) -> Result<Option<String>, StoreError> {
    let has = |table: &str| -> Result<bool, StoreError> {
        Ok(conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |r| r.get::<_, i64>(0),
        )? > 0)
    };
    if !has("of_declaration")? {
        return Ok(None);
    }
    if has("of_engine_release")? {
        return Ok(conn
            .query_row(
                "SELECT release FROM of_engine_release ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?);
    }
    Ok(conn
        .query_row(
            "SELECT builtins FROM of_declaration WHERE version = 0",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

/// The steps from a release to the last of `releases`, each a release whose
/// migration runs from the one before it; none where no chain reaches it,
/// a later release's store among them.
pub fn steps_from<'r>(from: &str, releases: &'r [Release]) -> Option<Vec<&'r Release>> {
    let target = releases.last()?.name;
    let mut at = from.to_string();
    let mut steps = Vec::new();
    while at != target {
        let next = releases.iter().find(|r| r.from.contains(&at.as_str()))?;
        if steps.len() > releases.len() {
            return None;
        }
        at = next.name.to_string();
        steps.push(next);
    }
    Some(steps)
}

/// What an upgrade did, or a dry run would (§12).
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeReport {
    pub from: String,
    pub to: String,
    /// Each release passed through, and what it changes.
    pub steps: Vec<(String, String)>,
    pub dry_run: bool,
    /// Whether the store was changed: never on a dry run, nor where it was
    /// at this release already.
    pub upgraded: bool,
}

/// `upgrade` (§12, ADR-0177): the store carried from its release to this
/// core's, through each release between, one transaction a release, each
/// running that release's migration, appending its row to the release
/// table and recording the operator's act as an `upgrade` event on the
/// operator; a dry run runs every step in one transaction and rolls it
/// back. Refused, as a value, where the file holds no store, the store is at
/// a release no chain leads from, the actor is no operator of the store, or
/// a step changes the built-ins (decision 6).
pub fn upgrade(
    path: &Path,
    now: i64,
    actor: &str,
    dry_run: bool,
) -> Result<Result<UpgradeReport, String>, StoreError> {
    upgrade_through(path, now, actor, dry_run, RELEASES)
}

pub fn upgrade_through(
    path: &Path,
    now: i64,
    actor: &str,
    dry_run: bool,
    releases: &[Release],
) -> Result<Result<UpgradeReport, String>, StoreError> {
    let Some(to) = releases.last().map(|r| r.name.to_string()) else {
        return Ok(Err("no release is known".into()));
    };
    let Some(from) = store_release_at(path)? else {
        return Ok(Err(format!("{} holds no store", path.display())));
    };
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
    )?;
    conn.busy_timeout(crate::store::BUSY_TIMEOUT)?;
    let operator: Option<String> = conn
        .query_row(
            "SELECT id FROM t_operator WHERE id = ?1 OR identity = ?1",
            params![actor],
            |r| r.get(0),
        )
        .optional()?;
    let Some(operator) = operator else {
        return Ok(Err(format!(
            "an upgrade is an operator's act, and {actor} is no operator of the store"
        )));
    };
    let Some(steps) = steps_from(&from, releases) else {
        return Ok(Err(format!(
            "the store is at {from}, which {to} does not upgrade from"
        )));
    };
    if let Some(r) = steps.iter().find(|r| r.builtins_changed) {
        return Ok(Err(format!(
            "{} changes the built-ins, and re-recording the installed flows under them is not built (ADR-0177 decision 6)",
            r.name
        )));
    }
    let report = UpgradeReport {
        from: from.clone(),
        to,
        steps: steps
            .iter()
            .map(|r| (r.name.to_string(), r.changes.to_string()))
            .collect(),
        dry_run,
        upgraded: !dry_run && !steps.is_empty(),
    };
    if dry_run {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for r in &steps {
            step(&tx, r, now, &operator)?;
        }
        tx.rollback()?;
    } else {
        for r in &steps {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            step(&tx, r, now, &operator)?;
            tx.commit()?;
        }
    }
    Ok(Ok(report))
}

/// One release's step: its migration, its row, and the operator's act.
fn step(tx: &Transaction<'_>, r: &Release, now: i64, operator: &str) -> Result<(), StoreError> {
    (r.migrate)(tx, now)?;
    let at = timestamp_text(now);
    tx.execute(
        "INSERT INTO of_engine_release (seq, release, installed_at, installed_by) \
         VALUES ((SELECT COALESCE(max(seq), 0) + 1 FROM of_engine_release), ?1, ?2, ?3)",
        params![r.name, at, operator],
    )?;
    // the operator's act, naming the release, as creating the store is
    // (ADR-0126): an event on the operator that changes nothing of it
    let position = next_counter(tx, "position")?;
    let txn = next_counter(tx, "txn")?;
    let (version, state): (i64, String) = tx.query_row(
        "SELECT version, state FROM t_operator WHERE id = ?1",
        params![operator],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let declared: i64 = tx.query_row("SELECT max(version) FROM of_declaration", [], |row| {
        row.get(0)
    })?;
    tx.execute(
        "INSERT INTO of_event (position, txn, object_id, object_seq, transition, from_state, to_state, changes_state, \
         source, actor_id, actor_kind, declaration_version, taint_version, recorded_at, occurred_at, payload, reads, \
         context) \
         VALUES (?1, ?2, ?3, ?4, 'upgrade', ?5, ?5, 0, 'observed', ?3, 'human', ?6, 0, ?7, ?7, '{}', '{}', ?8)",
        params![
            position,
            txn,
            operator,
            version + 1,
            state,
            declared,
            at,
            format!("upgrade:{}", r.name)
        ],
    )?;
    tx.execute(
        "UPDATE t_operator SET version = version + 1, last_event = ?2 WHERE id = ?1",
        params![operator, position],
    )?;
    Ok(())
}

/// The fingerprint this core's store has, which its release records.
pub fn fingerprint() -> String {
    release_fingerprint()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_last_release_is_this_cores_and_holds_its_store() {
        let last = RELEASES.last().unwrap();
        assert_eq!(last.name, RELEASE);
        assert_eq!(
            last.fingerprint,
            fingerprint(),
            "the store's own tables or the built-ins changed: that is a new release, with a migration from \
             this one, in RELEASES, recording this fingerprint (ADR-0177 decision 1)"
        );
    }

    #[test]
    fn a_store_further_behind_passes_through_each_release_between() {
        let releases = [
            Release {
                name: "a",
                from: &[],
                fingerprint: "",
                changes: "",
                builtins_changed: false,
                migrate: |_, _| Ok(()),
            },
            Release {
                name: "b",
                from: &["a"],
                fingerprint: "",
                changes: "",
                builtins_changed: false,
                migrate: |_, _| Ok(()),
            },
            Release {
                name: "c",
                from: &["b"],
                fingerprint: "",
                changes: "",
                builtins_changed: false,
                migrate: |_, _| Ok(()),
            },
        ];
        let names =
            |s: Option<Vec<&Release>>| s.map(|v| v.iter().map(|r| r.name).collect::<Vec<_>>());
        assert_eq!(names(steps_from("a", &releases)), Some(vec!["b", "c"]));
        assert_eq!(names(steps_from("c", &releases)), Some(vec![]));
        // a release none upgrades from, a later one's among them
        assert_eq!(names(steps_from("d", &releases)), None);
    }
}
