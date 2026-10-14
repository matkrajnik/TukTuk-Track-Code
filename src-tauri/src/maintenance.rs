use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

// VACUUM INTO produces a consistent backup including WAL data.
pub fn backup_before_upgrade(
    db: &Connection,
    dir: &Path,
    existing: bool,
    version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !existing {
        return Ok(());
    }
    let old: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='app_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    if old.as_deref() == Some(version) {
        return Ok(());
    }
    let folder = dir.join("backups");
    std::fs::create_dir_all(&folder)?;
    let target = folder.join(format!("before-{}-{}.sqlite", version, crate::db::now()));
    db.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
    Ok(())
}
pub fn record_version(db: &Connection, version: &str) -> rusqlite::Result<()> {
    db.execute("INSERT INTO settings(key,value) VALUES('app_version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![version])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_contains_wal_and_runs_once_per_version() {
        let dir = std::env::temp_dir().join(format!(
            "todo-track-backup-{}-{}",
            std::process::id(),
            crate::db::now()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("data.sqlite")).unwrap();
        crate::db::init(&db).unwrap();
        db.execute(
            "INSERT INTO tasks(title,created_at) VALUES('Keep me',1)",
            [],
        )
        .unwrap();
        backup_before_upgrade(&db, &dir, true, "0.2.0").unwrap();
        let path = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let backup = Connection::open(path).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT title FROM tasks", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "Keep me"
        );
        drop(backup);
        record_version(&db, "0.2.0").unwrap();
        backup_before_upgrade(&db, &dir, true, "0.2.0").unwrap();
        assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 1);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
