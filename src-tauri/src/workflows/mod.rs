//! Moduł Workflows: komendy Tauri nad trwałymi definicjami (etap E1).
//!
//! Plan: docs/workflows/PLAN_WDROZENIA.md, decyzje E0: docs/workflows/E0_ANALIZA.md.
//! Każda komenda poza `workflow_access` i `workflow_unlock` sprawdza w Rust,
//! czy moduł odblokowano kluczem; ukrycie menu w interfejsie nie jest kontrolą.

pub mod access;
pub mod artifacts;
pub mod preflight;
pub mod run_commands;
pub mod runners;
pub mod runs;
pub mod runtime;
pub mod service;
pub mod store;
#[cfg(test)]
pub(crate) mod test_git;
pub mod workspace;

use rusqlite::Connection;
use serde::Serialize;
use tauri::Manager;
use workflow_core::template;
use workflow_core::{validate, Issue, WorkflowGraph};

use crate::Database;
use store::{Detail, StoreError, Summary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    pub unlocked: bool,
}

const LOCKED: &str = "Moduł Workflows nie jest odblokowany. Wpisz klucz dostępu w Ustawieniach.";

fn with_db<R>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&Connection) -> Result<R, StoreError>,
) -> Result<R, String> {
    let database = app.state::<Database>();
    let db = database.0.lock().map_err(|e| e.to_string())?;
    f(&db).map_err(|e| e.to_string())
}

/// Błąd `LOCKED`, gdy moduł nie jest odblokowany.
pub(crate) fn require_unlocked(db: &Connection) -> Result<(), StoreError> {
    if !store::is_unlocked(db)? {
        return Err(StoreError::BadInput(LOCKED.into()));
    }
    Ok(())
}

/// Jak `with_db`, ale tylko po odblokowaniu modułu.
fn with_unlocked<R>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&Connection) -> Result<R, StoreError>,
) -> Result<R, String> {
    with_db(app, |db| {
        require_unlocked(db)?;
        f(db)
    })
}

#[tauri::command]
pub fn workflow_access(app: tauri::AppHandle) -> Result<Access, String> {
    with_db(&app, |db| {
        Ok(Access {
            unlocked: store::is_unlocked(db)?,
        })
    })
}

/// Sprawdza klucz i trwale zapisuje odblokowanie. Klucz nie trafia do logów
/// ani do komunikatu błędu (4.1.1).
#[tauri::command]
pub fn workflow_unlock(app: tauri::AppHandle, key: String) -> Result<Access, String> {
    if !access::is_valid_workflows_key(&key) {
        return Err("Nieprawidłowy klucz dostępu.".into());
    }
    with_db(&app, |db| {
        store::set_unlocked(db)?;
        Ok(Access { unlocked: true })
    })
}

#[tauri::command]
pub fn workflow_list(
    app: tauri::AppHandle,
    include_archived: bool,
) -> Result<Vec<Summary>, String> {
    with_unlocked(&app, |db| Ok(store::list(db, include_archived)?))
}

#[tauri::command]
pub fn workflow_get(app: tauri::AppHandle, id: i64) -> Result<Detail, String> {
    with_unlocked(&app, |db| store::get(db, id))
}

/// Nowy proces: pusty (Start → Koniec) albo z szablonu „Analiza → implementacja → review”.
#[tauri::command]
pub fn workflow_create(
    app: tauri::AppHandle,
    name: String,
    description: String,
    from_template: bool,
) -> Result<Detail, String> {
    let graph = if from_template {
        template::analysis_implementation_review()
    } else {
        template::blank()
    };
    with_unlocked(&app, |db| {
        let id = store::create(db, &name, &description, &graph, crate::db::now())?;
        store::get(db, id)
    })
}

#[tauri::command]
pub fn workflow_save_draft(
    app: tauri::AppHandle,
    id: i64,
    expected_revision: i64,
    name: String,
    description: String,
    graph: WorkflowGraph,
) -> Result<Detail, String> {
    with_unlocked(&app, |db| {
        store::save_draft(
            db,
            id,
            expected_revision,
            &name,
            &description,
            &graph,
            crate::db::now(),
        )?;
        store::get(db, id)
    })
}

#[tauri::command]
pub fn workflow_validate(
    app: tauri::AppHandle,
    graph: WorkflowGraph,
) -> Result<Vec<Issue>, String> {
    with_unlocked(&app, |_| Ok(validate(&graph)))
}

#[tauri::command]
pub fn workflow_publish(
    app: tauri::AppHandle,
    id: i64,
    expected_revision: i64,
) -> Result<Detail, String> {
    with_unlocked(&app, |db| {
        store::publish(db, id, expected_revision, crate::db::now())?;
        store::get(db, id)
    })
}

#[tauri::command]
pub fn workflow_archive(app: tauri::AppHandle, id: i64, archived: bool) -> Result<Detail, String> {
    with_unlocked(&app, |db| {
        store::set_archived(db, id, archived, crate::db::now())?;
        store::get(db, id)
    })
}

#[tauri::command]
pub fn workflow_duplicate(app: tauri::AppHandle, id: i64) -> Result<Detail, String> {
    with_unlocked(&app, |db| {
        let copy = store::duplicate(db, id, crate::db::now())?;
        store::get(db, copy)
    })
}

#[tauri::command]
pub fn workflow_export(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    with_unlocked(&app, |db| store::export(db, id))
}

#[tauri::command]
pub fn workflow_import(app: tauri::AppHandle, text: String) -> Result<Detail, String> {
    with_unlocked(&app, |db| {
        let id = store::import(db, &text, crate::db::now())?;
        store::get(db, id)
    })
}

/// Gotowość agentów opublikowanej wersji procesu (E4). Definicję czyta krótka
/// transakcja; wykrywanie CLI i odczyt config.toml odbywają się poza blokadą bazy.
#[tauri::command]
pub async fn workflow_preflight(
    app: tauri::AppHandle,
    version_id: i64,
) -> Result<Vec<preflight::NodeReadiness>, String> {
    let graph = with_db(&app, |db| preflight_graph(db, version_id))?;
    tauri::async_runtime::spawn_blocking(move || {
        let detected = crate::ai::detected_cached(&app);
        preflight::preflight_reading_config(&graph, &detected, preflight::read_codex_config)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Graf opublikowanej wersji (formularz startu pokazuje kroki tego, co faktycznie
/// ruszy, a nie szkicu); tylko po odblokowaniu.
#[tauri::command]
pub fn workflow_version_graph(
    app: tauri::AppHandle,
    version_id: i64,
) -> Result<WorkflowGraph, String> {
    with_db(&app, |db| preflight_graph(db, version_id))
}

/// Definicja do preflightu: tylko po odblokowaniu modułu (ścieżka komendy).
pub(crate) fn preflight_graph(
    db: &Connection,
    version_id: i64,
) -> Result<WorkflowGraph, StoreError> {
    require_unlocked(db)?;
    store::version_graph(db, version_id)
}
