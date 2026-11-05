//! Trwałe definicje procesów (PLAN_WDROZENIA.md, 7.1; etap E1).
//!
//! Tabele modułu powstają w `init_tables`, wołanym z `db::init`, więc są też
//! w przywracanych kopiach. Szkic ma rewizję: zapis z nieaktualną rewizją
//! zwraca konflikt zamiast nadpisać zmiany z innego okna. Opublikowana wersja
//! jest niezmienna (wyzwalacze blokują UPDATE i DELETE).

use std::fmt;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workflow_core::{validate, Issue, WorkflowGraph, SCHEMA_VERSION};

/// Klucz w `settings` ze znacznikiem odblokowania modułu.
pub const UNLOCKED_KEY: &str = "workflows_unlocked";
/// Wersja schematu tabel modułu (niezależna od formatu grafu). 2 dodaje
/// tabele przebiegów (E3), 3 rezerwacje katalogów roboczych (E4); `init_tables` zapisuje wszystkie wersje do tej włącznie.
const MODULE_SCHEMA: i64 = 3;
/// Najdłuższa nazwa i opis procesu w znakach.
const MAX_NAME_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 2_000;
/// Największy plik importu: 1 MB tekstu.
const MAX_IMPORT_BYTES: usize = 1_000_000;
/// Największy zapisany szkic: 1 MB JSON (także przez bezpośrednie IPC).
const MAX_DRAFT_BYTES: usize = 1_000_000;
/// Dopisek nazwy kopii procesu.
const COPY_SUFFIX: &str = " (kopia)";
/// Identyfikator formatu pliku eksportu.
const EXPORT_FORMAT: &str = "tuktuk-track-workflow";
const EXPORT_FORMAT_VERSION: u32 = 1;

pub fn init_tables(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS workflows (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', draft_json TEXT NOT NULL, draft_hash TEXT NOT NULL, draft_revision INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, archived_at INTEGER);
        CREATE INDEX IF NOT EXISTS workflows_updated ON workflows(updated_at);
        CREATE TABLE IF NOT EXISTS workflow_versions (id INTEGER PRIMARY KEY AUTOINCREMENT, workflow_id INTEGER NOT NULL REFERENCES workflows(id) ON DELETE RESTRICT, version INTEGER NOT NULL, schema_version INTEGER NOT NULL, definition_json TEXT NOT NULL, hash TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE(workflow_id, version));
        CREATE TRIGGER IF NOT EXISTS workflow_versions_no_update BEFORE UPDATE ON workflow_versions BEGIN SELECT RAISE(ABORT, 'opublikowana wersja procesu jest niezmienna'); END;
        CREATE TRIGGER IF NOT EXISTS workflow_versions_no_delete BEFORE DELETE ON workflow_versions BEGIN SELECT RAISE(ABORT, 'opublikowana wersja procesu jest niezmienna'); END;",
    )?;
    init_run_tables(db)?;
    init_workspace_tables(db)?;
    for version in 1..=MODULE_SCHEMA {
        db.execute(
            "INSERT OR IGNORE INTO workflow_schema_migrations(version, applied_at) VALUES(?1, ?2)",
            params![version, crate::db::now()],
        )?;
    }
    Ok(())
}

/// Migracja 2 (E3): przebiegi, kroki, zdarzenia i zgody. `task_id` bez klucza
/// obcego, żeby usunięcie zadania nie kasowało historii (zostaje `task_title`).
/// Zdarzenia są dopisywane tylko na końcu: wyzwalacze blokują UPDATE i DELETE.
fn init_run_tables(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, version_id INTEGER NOT NULL REFERENCES workflow_versions(id) ON DELETE RESTRICT, request_id TEXT NOT NULL UNIQUE, definition_json TEXT NOT NULL, definition_hash TEXT NOT NULL, task_id INTEGER, task_title TEXT, repo TEXT, workspace TEXT, base_commit TEXT, status TEXT NOT NULL, error_code TEXT, error TEXT, created_at INTEGER NOT NULL, started_at INTEGER, finished_at INTEGER, updated_at INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS workflow_runs_status ON workflow_runs(status, updated_at);
        CREATE INDEX IF NOT EXISTS workflow_runs_task ON workflow_runs(task_id);
        CREATE TABLE IF NOT EXISTS workflow_node_executions (id TEXT PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES workflow_runs(id) ON DELETE RESTRICT, node_id TEXT NOT NULL, iteration INTEGER NOT NULL, attempt INTEGER NOT NULL, status TEXT NOT NULL, input_json TEXT, output_json TEXT, provider_session_id TEXT, exit_code INTEGER, error_code TEXT, error TEXT, started_at INTEGER, finished_at INTEGER, UNIQUE(run_id, node_id, iteration, attempt));
        CREATE TABLE IF NOT EXISTS workflow_events (run_id INTEGER NOT NULL REFERENCES workflow_runs(id) ON DELETE RESTRICT, sequence INTEGER NOT NULL, execution_id TEXT, type TEXT NOT NULL, payload_json TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(run_id, sequence));
        CREATE TRIGGER IF NOT EXISTS workflow_events_no_update BEFORE UPDATE ON workflow_events BEGIN SELECT RAISE(ABORT, 'historia przebiegu jest tylko do dopisywania'); END;
        CREATE TRIGGER IF NOT EXISTS workflow_events_no_delete BEFORE DELETE ON workflow_events BEGIN SELECT RAISE(ABORT, 'historia przebiegu jest tylko do dopisywania'); END;
        CREATE TABLE IF NOT EXISTS workflow_approvals (id TEXT PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES workflow_runs(id) ON DELETE RESTRICT, execution_id TEXT NOT NULL, artifact_hash TEXT NOT NULL, status TEXT NOT NULL, comment TEXT, decision_request_id TEXT UNIQUE, decided_at INTEGER);
        CREATE INDEX IF NOT EXISTS workflow_approvals_execution ON workflow_approvals(execution_id);",
    )
}

/// Migracja 3 (E4): rezerwacje worktree. Jedna aktywna rezerwacja na ścieżkę
/// (częściowy indeks unikalny); wiersze zostają po zwolnieniu jako historia.
fn init_workspace_tables(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_workspaces (id INTEGER PRIMARY KEY AUTOINCREMENT, run_id INTEGER NOT NULL REFERENCES workflow_runs(id) ON DELETE RESTRICT, source_repo TEXT NOT NULL, worktree_path TEXT NOT NULL, base_commit TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('reserved','released')), created_at INTEGER NOT NULL, released_at INTEGER);
        CREATE UNIQUE INDEX IF NOT EXISTS workflow_workspaces_reserved_path ON workflow_workspaces(worktree_path) WHERE state = 'reserved';
        CREATE INDEX IF NOT EXISTS workflow_workspaces_run ON workflow_workspaces(run_id);",
    )
}

#[derive(Debug)]
pub enum StoreError {
    NotFound,
    Conflict,
    Invalid(Vec<Issue>),
    BadInput(String),
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e)
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound => f.write_str("Nie znaleziono procesu."),
            StoreError::Conflict => f.write_str(
                "Proces zmienił się w innym oknie. Odśwież go przed zapisem, żeby nie nadpisać zmian.",
            ),
            StoreError::Invalid(issues) => write!(
                f,
                "Proces ma błędy ({}); popraw je przed publikacją.",
                issues.len()
            ),
            StoreError::BadInput(message) => f.write_str(message),
            StoreError::Db(e) => write!(f, "Błąd bazy procesów: {e}"),
        }
    }
}

/// Graf opublikowanej wersji procesu (niezmienny zapis z `workflow_versions`).
pub fn version_graph(db: &Connection, version_id: i64) -> Result<WorkflowGraph, StoreError> {
    let text: String = db
        .query_row(
            "SELECT definition_json FROM workflow_versions WHERE id = ?1",
            [version_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::NotFound)?;
    serde_json::from_str(&text)
        .map_err(|e| StoreError::BadInput(format!("Uszkodzona definicja wersji procesu: {e}")))
}

pub fn is_unlocked(db: &Connection) -> rusqlite::Result<bool> {
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [UNLOCKED_KEY],
            |r| r.get(0),
        )
        .optional()?;
    Ok(value.as_deref() == Some("true"))
}

pub fn set_unlocked(db: &Connection) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO settings(key, value) VALUES(?1, 'true')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [UNLOCKED_KEY],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub draft_revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub published_version: Option<i64>,
    pub published_at: Option<i64>,
    /// Szkic różni się od ostatniej opublikowanej wersji (albo jej brak).
    pub unpublished_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    /// Identyfikator wiersza wersji (argument `workflow_preflight`).
    pub id: i64,
    pub version: i64,
    pub hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    #[serde(flatten)]
    pub summary: Summary,
    pub draft: WorkflowGraph,
    pub issues: Vec<Issue>,
    pub versions: Vec<VersionInfo>,
}

/// Skrót SHA-256 zapisanego tekstu JSON. Szkic i wersja liczą go z tego samego
/// tekstu, który trafia do bazy, więc zmiana typów Rust nie zmienia skrótów
/// już zapisanych definicji.
fn text_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Skrót kanonicznego JSON grafu (kolejność pól z typów Rust).
pub fn graph_hash(graph: &WorkflowGraph) -> Result<String, StoreError> {
    Ok(text_hash(&to_json(graph)?))
}

fn clean_name(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::BadInput("Proces musi mieć nazwę.".into()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(StoreError::BadInput(format!(
            "Nazwa procesu może mieć najwyżej {MAX_NAME_CHARS} znaków."
        )));
    }
    Ok(name.to_owned())
}

fn clean_description(description: &str) -> Result<String, StoreError> {
    let description = description.trim();
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(StoreError::BadInput(format!(
            "Opis procesu może mieć najwyżej {MAX_DESCRIPTION_CHARS} znaków."
        )));
    }
    Ok(description.to_owned())
}

fn check_schema(graph: &WorkflowGraph) -> Result<(), StoreError> {
    if graph.schema_version != SCHEMA_VERSION {
        return Err(StoreError::BadInput(format!(
            "Nieobsługiwana wersja formatu procesu: {}.",
            graph.schema_version
        )));
    }
    Ok(())
}

fn to_json(graph: &WorkflowGraph) -> Result<String, StoreError> {
    let text = serde_json::to_string(graph)
        .map_err(|e| StoreError::BadInput(format!("Nie udało się zapisać grafu: {e}")))?;
    if text.len() > MAX_DRAFT_BYTES {
        return Err(StoreError::BadInput(
            "Proces jest za duży (limit 1 MB).".into(),
        ));
    }
    Ok(text)
}

fn parse_graph(text: &str) -> Result<WorkflowGraph, StoreError> {
    serde_json::from_str(text)
        .map_err(|e| StoreError::BadInput(format!("Zapisany graf jest uszkodzony: {e}")))
}

const SUMMARY_SQL: &str = "SELECT w.id, w.name, w.description, w.draft_revision, w.created_at,
        w.updated_at, w.archived_at, w.draft_hash, v.version, v.created_at, v.hash
     FROM workflows w
     LEFT JOIN workflow_versions v ON v.workflow_id = w.id
        AND v.version = (SELECT MAX(version) FROM workflow_versions WHERE workflow_id = w.id)";

fn summary_row(row: &rusqlite::Row) -> rusqlite::Result<Summary> {
    let draft_hash: String = row.get(7)?;
    let published_hash: Option<String> = row.get(10)?;
    Ok(Summary {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        draft_revision: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        archived_at: row.get(6)?,
        published_version: row.get(8)?,
        published_at: row.get(9)?,
        unpublished_changes: published_hash.as_deref() != Some(draft_hash.as_str()),
    })
}

pub fn list(db: &Connection, include_archived: bool) -> rusqlite::Result<Vec<Summary>> {
    let sql = format!(
        "{SUMMARY_SQL} WHERE (?1 OR w.archived_at IS NULL) ORDER BY w.updated_at DESC, w.id DESC"
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement.query_map([include_archived], summary_row)?;
    rows.collect()
}

fn summary(db: &Connection, id: i64) -> Result<Summary, StoreError> {
    let sql = format!("{SUMMARY_SQL} WHERE w.id = ?1");
    db.query_row(&sql, [id], summary_row)
        .optional()?
        .ok_or(StoreError::NotFound)
}

pub fn get(db: &Connection, id: i64) -> Result<Detail, StoreError> {
    let summary = summary(db, id)?;
    let draft_json: String = db.query_row(
        "SELECT draft_json FROM workflows WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    let draft = parse_graph(&draft_json)?;
    let mut statement = db.prepare(
        "SELECT version, hash, created_at, id FROM workflow_versions
         WHERE workflow_id = ?1 ORDER BY version DESC",
    )?;
    let versions = statement
        .query_map([id], |r| {
            Ok(VersionInfo {
                version: r.get(0)?,
                hash: r.get(1)?,
                created_at: r.get(2)?,
                id: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Detail {
        summary,
        issues: validate(&draft),
        draft,
        versions,
    })
}

pub fn create(
    db: &Connection,
    name: &str,
    description: &str,
    graph: &WorkflowGraph,
    now: i64,
) -> Result<i64, StoreError> {
    let name = clean_name(name)?;
    let description = clean_description(description)?;
    check_schema(graph)?;
    let json = to_json(graph)?;
    let hash = text_hash(&json);
    db.execute(
        "INSERT INTO workflows(name, description, draft_json, draft_hash, draft_revision, created_at, updated_at)
         VALUES(?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![name, description, json, hash, now],
    )?;
    Ok(db.last_insert_rowid())
}

/// Zapis szkicu. Szkic może mieć błędy walidacji; nie może mieć złej wersji formatu.
pub fn save_draft(
    db: &Connection,
    id: i64,
    expected_revision: i64,
    name: &str,
    description: &str,
    graph: &WorkflowGraph,
    now: i64,
) -> Result<i64, StoreError> {
    let name = clean_name(name)?;
    let description = clean_description(description)?;
    check_schema(graph)?;
    let json = to_json(graph)?;
    let hash = text_hash(&json);
    let changed = db.execute(
        "UPDATE workflows SET name = ?1, description = ?2, draft_json = ?3, draft_hash = ?4,
            draft_revision = draft_revision + 1, updated_at = ?5
         WHERE id = ?6 AND draft_revision = ?7 AND archived_at IS NULL",
        params![name, description, json, hash, now, id, expected_revision],
    )?;
    if changed == 0 {
        return Err(missing_or_conflict(db, id)?);
    }
    Ok(expected_revision + 1)
}

/// Rozróżnia brak procesu, archiwum i nieaktualną rewizję po nieudanym UPDATE.
fn missing_or_conflict(db: &Connection, id: i64) -> Result<StoreError, StoreError> {
    let archived: Option<Option<i64>> = db
        .query_row(
            "SELECT archived_at FROM workflows WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match archived {
        None => StoreError::NotFound,
        Some(Some(_)) => {
            StoreError::BadInput("Proces jest w archiwum; przywróć go, żeby edytować.".into())
        }
        Some(None) => StoreError::Conflict,
    })
}

/// Publikuje szkic jako nową, niezmienną wersję. Błędny graf nie przechodzi.
pub fn publish(
    db: &Connection,
    id: i64,
    expected_revision: i64,
    now: i64,
) -> Result<VersionInfo, StoreError> {
    let tx = db.unchecked_transaction()?;
    let row: Option<(String, i64, Option<i64>)> = tx
        .query_row(
            "SELECT draft_json, draft_revision, archived_at FROM workflows WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((draft_json, revision, archived_at)) = row else {
        return Err(StoreError::NotFound);
    };
    if archived_at.is_some() {
        return Err(StoreError::BadInput(
            "Proces jest w archiwum; przywróć go przed publikacją.".into(),
        ));
    }
    if revision != expected_revision {
        return Err(StoreError::Conflict);
    }
    let graph = parse_graph(&draft_json)?;
    let issues = validate(&graph);
    if !issues.is_empty() {
        return Err(StoreError::Invalid(issues));
    }
    // Wersja dostaje dokładnie tekst szkicu; skrót liczony z tego tekstu.
    let hash = text_hash(&draft_json);
    let latest: Option<VersionInfo> = tx
        .query_row(
            "SELECT version, hash, created_at, id FROM workflow_versions
             WHERE workflow_id = ?1 ORDER BY version DESC LIMIT 1",
            [id],
            |r| {
                Ok(VersionInfo {
                    version: r.get(0)?,
                    hash: r.get(1)?,
                    created_at: r.get(2)?,
                    id: r.get(3)?,
                })
            },
        )
        .optional()?;
    // Ponowna publikacja bez zmian zwraca istniejącą wersję (podwójne kliknięcie).
    if let Some(existing) = latest.as_ref().filter(|v| v.hash == hash) {
        return Ok(existing.clone());
    }
    let version = latest.map_or(1, |v| v.version + 1);
    tx.execute(
        "INSERT INTO workflow_versions(workflow_id, version, schema_version, definition_json, hash, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, version, SCHEMA_VERSION, draft_json, hash, now],
    )?;
    let version_id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE workflows SET updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    tx.commit()?;
    Ok(VersionInfo {
        id: version_id,
        version,
        hash,
        created_at: now,
    })
}

/// Archiwizacja zachowuje szkic i wersje; `archived = false` przywraca proces.
pub fn set_archived(db: &Connection, id: i64, archived: bool, now: i64) -> Result<(), StoreError> {
    let changed = db.execute(
        "UPDATE workflows SET archived_at = CASE WHEN ?1 THEN ?2 ELSE NULL END, updated_at = ?2
         WHERE id = ?3",
        params![archived, now, id],
    )?;
    if changed == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// Kopia szkicu jako nowy, nieopublikowany proces.
pub fn duplicate(db: &Connection, id: i64, now: i64) -> Result<i64, StoreError> {
    let detail = get(db, id)?;
    // Skraca nazwę źródła, żeby dopisek „(kopia)” zawsze się zmieścił.
    let base: String = detail
        .summary
        .name
        .chars()
        .take(MAX_NAME_CHARS - COPY_SUFFIX.chars().count())
        .collect();
    let name = format!("{base}{COPY_SUFFIX}");
    create(db, &name, &detail.summary.description, &detail.draft, now)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    format: String,
    format_version: u32,
    name: String,
    #[serde(default)]
    description: String,
    graph: WorkflowGraph,
}

/// Eksport szkicu do JSON. Bez historii, ścieżek repozytoriów i uprawnień maszyny.
pub fn export(db: &Connection, id: i64) -> Result<String, StoreError> {
    let detail = get(db, id)?;
    let file = ExportFile {
        format: EXPORT_FORMAT.into(),
        format_version: EXPORT_FORMAT_VERSION,
        name: detail.summary.name,
        description: detail.summary.description,
        graph: detail.draft,
    };
    serde_json::to_string_pretty(&file)
        .map_err(|e| StoreError::BadInput(format!("Nie udało się przygotować eksportu: {e}")))
}

/// Import tworzy nowy, nieopublikowany szkic (4.2): nie uruchamia agentów
/// i niczego nie publikuje. Graf z błędami walidacji jest dozwolony w szkicu.
pub fn import(db: &Connection, text: &str, now: i64) -> Result<i64, StoreError> {
    if text.len() > MAX_IMPORT_BYTES {
        return Err(StoreError::BadInput(
            "Plik procesu jest za duży (limit 1 MB).".into(),
        ));
    }
    let file: ExportFile = serde_json::from_str(text).map_err(|e| {
        StoreError::BadInput(format!(
            "To nie jest poprawny plik procesu TukTuk Track: {e}"
        ))
    })?;
    if file.format != EXPORT_FORMAT || file.format_version != EXPORT_FORMAT_VERSION {
        return Err(StoreError::BadInput(
            "Nieobsługiwany format pliku procesu.".into(),
        ));
    }
    create(db, &file.name, &file.description, &file.graph, now)
}

#[cfg(test)]
mod tests;
