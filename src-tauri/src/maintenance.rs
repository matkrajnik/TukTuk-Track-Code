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

/// Czy plik to program pomocniczy odsunięty przez instalator
/// (`TukTukTrackMoveAside`: `tuktuk-track-<…>.exe.<n>.old`).
pub fn is_moved_aside(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("tuktuk-track-") && lower.contains(".exe.") && lower.ends_with(".old")
}

/// Usuwa z katalogu programu pliki odsunięte przy aktualizacji. Plik, który
/// nadal działa (np. serwer MCP w otwartej sesji agenta), zostaje do następnego
/// startu. Zwraca liczbę usuniętych.
pub fn remove_moved_aside(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| is_moved_aside(&e.file_name().to_string_lossy()))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_helpers_moved_aside_by_the_installer_are_removed() {
        assert!(is_moved_aside("tuktuk-track-mcp.exe.1.old"));
        assert!(is_moved_aside("TukTuk-Track-Bridge.EXE.12.OLD"));
        for keep in [
            "tuktuk-track-mcp.exe",
            "tuktuk-track.exe",
            "notatki.old",
            "tuktuk-track-mcp.old",
            "inny-program.exe.1.old",
        ] {
            assert!(!is_moved_aside(keep), "{keep}");
        }
        let dir = std::env::temp_dir().join(format!(
            "tuktuk-track-aside-{}-{}",
            std::process::id(),
            crate::db::now()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "tuktuk-track-mcp.exe.1.old",
            "tuktuk-track-hook.exe.2.old",
            "tuktuk-track-mcp.exe",
            "dane.old",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        assert_eq!(remove_moved_aside(&dir), 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["dane.old", "tuktuk-track-mcp.exe"]);
        assert_eq!(remove_moved_aside(&dir.join("nie-ma")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn backup_contains_wal_and_runs_once_per_version() {
        let dir = std::env::temp_dir().join(format!(
            "tuktuk-track-backup-{}-{}",
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
