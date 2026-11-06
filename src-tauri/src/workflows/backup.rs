//! Workflows w kopii zapasowej (PLAN_WDROZENIA.md, 7.2; E0_ANALIZA.md, A03; etap E6).
//!
//! - Przywrócenie jest odrzucane, gdy jakikolwiek przebieg jest nieskończony
//!   (`Queued`, `Running`, `Cancelling`, `WaitingForApproval`, `Paused`); sprawdzenie
//!   odbywa się pod tą samą blokadą bazy co zastąpienie danych. Przygotowanie
//!   aktualizacji (`active_work`) blokują tylko przebiegi z procesem w toku.
//! - Kopia z nowszym schematem modułu jest odrzucana przed migracją.
//! - Przywrócona kopia nie wznawia starych poleceń: każdy nieskończony przebieg
//!   przechodzi przez silnik (`Input::Abandon`) w `Interrupted`, a aktywne
//!   rezerwacje katalogów roboczych są zwalniane z powodem `restored`.
//! - Eksport osadza diffy (z weryfikacją skrótu i limitem rozmiaru) w tabeli
//!   `workflow_backup_artifacts` samej kopii; baza robocza nigdy jej nie ma.
//!   Przywrócenie przyjmuje tylko pliki, których ścieżka, przebieg, skrót i wiersz
//!   indeksu się zgadzają, usuwa tabelę przed skopiowaniem kopii i zapisuje
//!   pliki dopiero po udanym zastąpieniu bazy roboczej.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use workflow_core::engine::Input;

use super::artifacts::{hex_of, is_hash, rel_path, runs_dir, write_atomic, FileOps};
use super::recovery::apply_input;
use super::runs::SqliteRunRepository;
use super::store::{module_schema_of, MODULE_SCHEMA};
use super::workspace::{canonical, is_under};

/// Tabela z diffami w pliku kopii (tylko tam).
pub const BACKUP_ARTIFACTS_TABLE: &str = "workflow_backup_artifacts";
/// Najwięcej bajtów diffów osadzanych w jednej kopii (256 MiB).
pub const MAX_BACKUP_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
/// Komunikat przerwania przebiegu przez przywrócenie kopii.
pub const RESTORED_MESSAGE: &str =
    "Przebieg przerwany: przywrócono kopię zapasową; nie jest wznawiany.";

const ACTIVE_STATUSES: &str = "'queued', 'running', 'cancelling'";
/// Wstrzymany po restarcie (`paused`) też jest nieskończony: kopia odebrałaby mu
/// rezerwację katalogu roboczego (decyzja po E7).
const UNFINISHED_STATUSES: &str =
    "'queued', 'running', 'cancelling', 'waitingForApproval', 'paused'";

fn count_in(conn: &Connection, statuses: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM workflow_runs WHERE status IN ({statuses})"),
        [],
        |r| r.get(0),
    )
}

/// Opis przebiegów z procesami w toku albo `None` (blokada aktualizacji;
/// przebieg czekający na zgodę albo wstrzymany po restarcie nie ma procesu, więc
/// nie blokuje). Wołać pod
/// blokadą bazy, tą samą co przygotowanie aktualizacji.
pub fn active_work(conn: &Connection) -> rusqlite::Result<Option<String>> {
    let count = count_in(conn, ACTIVE_STATUSES)?;
    Ok((count > 0).then(|| {
        format!(
            "Przebiegi Workflows w toku: {count}. Poczekaj na ich koniec albo je zatrzymaj (przebieg czekający na zgodę nie blokuje)."
        )
    }))
}

/// Opis nieskończonych przebiegów (także czekających na zgodę) albo `None`.
/// Przywrócenie kopii zastąpiłoby taki przebieg i odebrało rezerwację jego
/// katalogu roboczego, więc każdy nieskończony przebieg je blokuje. Wołać pod
/// blokadą bazy, tą samą co zastąpienie danych.
pub fn restore_blocker(conn: &Connection) -> rusqlite::Result<Option<String>> {
    let count = count_in(conn, UNFINISHED_STATUSES)?;
    Ok((count > 0).then(|| {
        format!(
            "Nieskończone przebiegi Workflows: {count}. Poczekaj na ich koniec, zatwierdź albo odrzuć zgody, wznów wstrzymane przebiegi lub zatrzymaj przebiegi."
        )
    }))
}

/// Odrzuca kopię z nowszym schematem modułu Workflows (przed `db::init`).
pub fn check_schema(source: &Connection) -> Result<(), String> {
    let version = module_schema_of(source);
    if version > MODULE_SCHEMA {
        return Err(format!(
            "Kopia pochodzi z nowszej wersji aplikacji (schemat Workflows {version}, obsługiwany {MODULE_SCHEMA}). Zaktualizuj aplikację przed przywróceniem."
        ));
    }
    Ok(())
}

/// Neutralizuje przebiegi z kopii: każdy nieskończony kończy się `Interrupted`
/// przez silnik (bez wznawiania), a aktywne rezerwacje są zwalniane jako
/// `restored`. Działa na przygotowanej kopii, zanim trafi do bazy roboczej.
pub fn neutralize_restored(staged: Connection, now: i64) -> Result<Connection, String> {
    let shared = Arc::new(Mutex::new(staged));
    let result = neutralize_shared(&shared, now);
    let staged = Arc::try_unwrap(shared)
        .map_err(|_| "Kopia jest nadal używana.".to_owned())?
        .into_inner()
        .map_err(|_| "Blokada kopii jest zatruta.".to_owned())?;
    result.map(|()| staged)
}

fn neutralize_shared(shared: &Arc<Mutex<Connection>>, now: i64) -> Result<(), String> {
    let unfinished: Vec<i64> = {
        let conn = shared.lock().map_err(|_| "Blokada kopii jest zatruta.")?;
        let mut statement = conn
            .prepare(&format!(
                "SELECT id FROM workflow_runs WHERE status IN ({UNFINISHED_STATUSES}) ORDER BY id"
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    let repo = SqliteRunRepository::new(shared.clone());
    for run_id in unfinished {
        apply_input(
            &repo,
            run_id,
            Input::Abandon {
                message: RESTORED_MESSAGE.to_owned(),
            },
            now,
        )
        .map_err(|e| format!("Przebieg {run_id} z kopii: {e}"))?;
    }
    drop(repo);
    let conn = shared.lock().map_err(|_| "Blokada kopii jest zatruta.")?;
    conn.execute(
        "UPDATE workflow_workspaces
         SET state = 'released', released_at = ?1, release_reason = 'restored'
         WHERE state = 'reserved'",
        [now],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Wynik osadzania diffów w kopii.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbedReport {
    pub embedded: usize,
    /// Pliki brakujące albo ze złym skrótem (kopia powstaje bez nich).
    pub missing: usize,
    /// Pominięte przez limit rozmiaru kopii.
    pub over_limit: usize,
}

/// Osadza diffy (bez usuniętych przez retencję) w tabeli kopii. `snapshot` to
/// osobne połączenie z kopią, nie baza robocza; pliki czytane są bez jej blokady.
pub fn embed_artifacts(
    snapshot: &Connection,
    app_data: &Path,
    max_bytes: u64,
) -> Result<EmbedReport, String> {
    snapshot
        .execute_batch(&format!(
            "DROP TABLE IF EXISTS {BACKUP_ARTIFACTS_TABLE};
             CREATE TABLE {BACKUP_ARTIFACTS_TABLE} (run_id INTEGER NOT NULL, hash TEXT NOT NULL, rel_path TEXT NOT NULL, content BLOB NOT NULL, PRIMARY KEY(run_id, hash));"
        ))
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String, String)> = {
        let mut statement = snapshot
            .prepare(
                "SELECT run_id, hash, rel_path FROM workflow_artifacts
                 WHERE kind = 'diff' AND purged_at IS NULL ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    let root = canonical(&runs_dir(app_data)).ok();
    let mut report = EmbedReport::default();
    let mut total = 0_u64;
    for (run_id, hash, rel) in rows {
        let content = root
            .as_deref()
            .and_then(|root| read_verified(root, &app_data.join(&rel), &hash));
        let Some(content) = content else {
            report.missing += 1;
            continue;
        };
        if total + content.len() as u64 > max_bytes {
            report.over_limit += 1;
            continue;
        }
        total += content.len() as u64;
        snapshot
            .execute(
                &format!(
                    "INSERT INTO {BACKUP_ARTIFACTS_TABLE}(run_id, hash, rel_path, content) VALUES(?1, ?2, ?3, ?4)"
                ),
                params![run_id, hash, rel, content],
            )
            .map_err(|e| e.to_string())?;
        report.embedded += 1;
    }
    Ok(report)
}

/// Bajty pliku pod katalogiem artefaktów, tylko gdy ich SHA-256 to `hash`.
fn read_verified(root: &Path, path: &Path, hash: &str) -> Option<Vec<u8>> {
    let resolved = canonical(path).ok()?;
    if !is_under(&resolved, root) {
        return None;
    }
    let bytes = std::fs::read(&resolved).ok()?;
    (hex_of(&bytes) == hash).then_some(bytes)
}

/// Wynik odtwarzania diffów z kopii.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractReport {
    pub written: usize,
    /// Wpisy odrzucone: zła ścieżka, cudzy przebieg, zły skrót albo brak wiersza indeksu.
    pub rejected: usize,
}

/// Zweryfikowany diff z kopii, jeszcze niezapisany na dysk.
#[derive(Debug, Clone)]
struct StagedArtifact {
    rel: String,
    hash: String,
    content: Vec<u8>,
}

/// Diffy wyjęte z kopii i zweryfikowane, czekające na zapis. Zapis
/// (`write`) następuje dopiero po udanym zastąpieniu bazy roboczej, więc
/// odrzucone albo nieudane przywrócenie nie zostawia plików z kopii.
#[derive(Debug, Clone, Default)]
pub struct StagedArtifacts {
    entries: Vec<StagedArtifact>,
    rejected: usize,
}

impl StagedArtifacts {
    /// Zapisuje przyjęte diffy do katalogu danych (istniejące pliki zostają).
    /// Wołać bez blokady bazy, po udanym `restore_locked`.
    pub fn write(&self, app_data: &Path) -> Result<ExtractReport, String> {
        for entry in &self.entries {
            let target = app_data.join(&entry.rel);
            if !target.is_file() {
                write_atomic(&target, &entry.hash, &entry.content, &FileOps::real())
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(ExtractReport {
            written: self.entries.len(),
            rejected: self.rejected,
        })
    }
}

/// Wyjmuje zweryfikowane diffy z tabeli kopii i usuwa tabelę z kopii (baza
/// robocza jej nie dostaje). Niczego nie zapisuje na dysk. Wpis jest przyjmowany
/// tylko, gdy ścieżka to dokładnie `workflows/runs/<run_id>/diff-<hash>.patch`,
/// skrót treści się zgadza, a kopia ma wiersz indeksu z tą samą ścieżką.
pub fn take_artifacts(staged: &Connection) -> Result<StagedArtifacts, String> {
    let mut result = StagedArtifacts::default();
    let present: bool = staged
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [BACKUP_ARTIFACTS_TABLE],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !present {
        return Ok(result);
    }
    let rows: Vec<(i64, String, String, Vec<u8>)> = {
        let mut statement = staged
            .prepare(&format!(
                "SELECT run_id, hash, rel_path, content FROM {BACKUP_ARTIFACTS_TABLE}"
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    for (run_id, hash, rel, content) in rows {
        if accepted(staged, run_id, &hash, &rel, &content)? {
            result.entries.push(StagedArtifact { rel, hash, content });
        } else {
            result.rejected += 1;
        }
    }
    staged
        .execute_batch(&format!("DROP TABLE {BACKUP_ARTIFACTS_TABLE};"))
        .map_err(|e| e.to_string())?;
    Ok(result)
}

fn accepted(
    staged: &Connection,
    run_id: i64,
    hash: &str,
    rel: &str,
    content: &[u8],
) -> Result<bool, String> {
    if !is_hash(hash) || rel != rel_path(run_id, hash) {
        return Ok(false);
    }
    if hex_of(content) != hash {
        return Ok(false);
    }
    let indexed = staged
        .query_row(
            "SELECT 1 FROM workflow_artifacts
             WHERE run_id = ?1 AND kind = 'diff' AND hash = ?2 AND rel_path = ?3",
            params![run_id, hash, rel],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(indexed.is_some())
}

#[cfg(test)]
mod tests;
