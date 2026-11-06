//! Retencja artefaktów przebiegów (PLAN_WDROZENIA.md, 7.2; E6).
//!
//! Diffy zakończonych przebiegów starsze niż `workflowRetentionDays` są usuwane z
//! dysku przy starcie aplikacji. Wiersz indeksu zostaje z `purged_at`, więc
//! historia i zgody nadal pokazują skrót, a odczyt zwraca komunikat o retencji
//! zamiast błędu. Przebiegi w toku i artefakty oczekujących zgód nie są ruszane.
//! Pliki są usuwane poza blokadą bazy.

use std::path::Path;

use rusqlite::params;

use super::artifacts::runs_dir;
use super::workspace::{canonical, is_under, SharedDb};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Wynik retencji.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeReport {
    pub purged: usize,
    /// Pliki, których nie udało się usunąć (zostają w indeksie do następnej próby).
    pub errors: Vec<String>,
}

fn lock_error() -> String {
    "Blokada bazy jest zatruta.".to_owned()
}

/// Usuwa diffy zakończonych przebiegów starsze niż `days` dni względem `now`.
pub fn purge_expired(
    db: &SharedDb,
    app_data: &Path,
    now: i64,
    days: u32,
) -> Result<PurgeReport, String> {
    let cutoff = now - i64::from(days) * DAY_MS;
    let candidates: Vec<(i64, String)> = {
        let conn = db.lock().map_err(|_| lock_error())?;
        let mut statement = conn
            .prepare(
                "SELECT f.id, f.rel_path FROM workflow_artifacts f
                 JOIN workflow_runs r ON r.id = f.run_id
                 WHERE f.purged_at IS NULL AND f.created_at < ?1
                   AND r.status IN ('succeeded', 'failed', 'rejected', 'cancelled', 'interrupted')
                   AND NOT EXISTS (SELECT 1 FROM workflow_approvals a
                                   WHERE a.run_id = f.run_id AND a.artifact_hash = f.hash
                                     AND a.status = 'pending')
                 ORDER BY f.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    let mut report = PurgeReport::default();
    let root = canonical(&runs_dir(app_data)).ok();
    for (id, rel) in candidates {
        match remove_artifact(root.as_deref(), &app_data.join(&rel)) {
            Ok(()) => {
                let conn = db.lock().map_err(|_| lock_error())?;
                conn.execute(
                    "UPDATE workflow_artifacts SET purged_at = ?2 WHERE id = ?1",
                    params![id, now],
                )
                .map_err(|e| e.to_string())?;
                report.purged += 1;
            }
            Err(e) => report.errors.push(format!("{rel}: {e}")),
        }
    }
    Ok(report)
}

/// Usuwa plik tylko pod katalogiem artefaktów; brak pliku to też koniec retencji.
fn remove_artifact(root: Option<&Path>, path: &Path) -> Result<(), String> {
    let Ok(resolved) = canonical(path) else {
        // Pliku już nie ma (albo ścieżki nie da się odczytać): nic do usunięcia.
        return if path.exists() {
            Err("nie można odczytać ścieżki".into())
        } else {
            Ok(())
        };
    };
    match root {
        Some(root) if is_under(&resolved, root) => {
            std::fs::remove_file(&resolved).map_err(|e| e.to_string())
        }
        _ => Err("ścieżka poza katalogiem artefaktów".into()),
    }
}

#[cfg(test)]
mod tests;
