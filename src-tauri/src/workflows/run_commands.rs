//! Komendy IPC przebiegów (E5). Logika siedzi w `service`, `runs::query` i
//! `artifacts`; tu tylko dostęp (klucz), mapowanie błędów na polskie teksty i
//! cienkie komendy Tauri. Funkcje rdzenia biorą `&SharedDb` i `&RuntimeHandle`,
//! więc testuje się je bez `AppHandle`.
//!
//! Dostęp: start, decyzja i odczyt artefaktu wymagają odblokowania modułu.
//! Anulowanie oraz historia (lista, szczegóły, zdarzenia) działają także bez
//! klucza, żeby przy odzyskiwaniu dostępu dało się przejrzeć i zatrzymać przebieg.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use workflow_core::driver::{DecideError, DecideRequest, RuntimeHandle};
use workflow_core::ports::StoredEvent;
use workflow_core::{ApprovalStatus, Decision, RunStatus};

use super::artifacts::{read_diff, DiffView, MAX_DIFF_VIEW_BYTES};
use super::processes::{ProcessControl, SystemProcesses};
use super::recovery::RecoverOutcome;
use super::require_unlocked;
use super::runs::{
    list_runs, run_detail, RunDetail, RunFilter, RunSummary, SqliteRunRepository, MAX_EVENTS_PAGE,
};
use super::runtime::runtime_handle;
use super::service::{start, StartDeps, StartRequest, StartResult};
use super::workspace::SharedDb;

/// Stały prefiks błędu unieważnionej zgody: interfejs go rozpoznaje i odświeża przebieg.
pub const INVALIDATED_PREFIX: &str = "Zgoda unieważniona: ";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideCommand {
    pub run_id: i64,
    pub approval_id: String,
    pub decision: Decision,
    pub request_id: String,
    pub expected_revision: u64,
    pub artifact_hash: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideOutcome {
    pub approval_status: ApprovalStatus,
    pub run_status: RunStatus,
}

fn db_error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn unlocked(db: &SharedDb) -> Result<(), String> {
    let conn = db
        .lock()
        .map_err(|_| "Blokada bazy jest zatruta.".to_owned())?;
    require_unlocked(&conn).map_err(db_error)
}

fn decide_message(error: DecideError) -> String {
    match error {
        // Nieaktualna rewizja też unieważnia decyzję: interfejs odświeża przebieg.
        DecideError::Invalidated | DecideError::Stale => format!("{INVALIDATED_PREFIX}{error}"),
        other => other.to_string(),
    }
}

/// Decyzja o zgodzie. Odrzucenie wymaga komentarza; akceptacja przyjmuje go opcjonalnie.
pub fn decide(
    db: &SharedDb,
    handle: &RuntimeHandle,
    command: DecideCommand,
) -> Result<DecideOutcome, String> {
    unlocked(db)?;
    let comment = command
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned);
    if command.decision == Decision::Reject && comment.is_none() {
        return Err("Odrzucenie wymaga komentarza.".into());
    }
    let result = handle
        .decide(DecideRequest {
            run_id: command.run_id,
            approval_id: command.approval_id,
            decision: command.decision,
            request_id: command.request_id,
            expected_revision: command.expected_revision,
            artifact_hash: command.artifact_hash,
            comment,
        })
        .map_err(decide_message)?;
    Ok(DecideOutcome {
        approval_status: result.approval_status,
        run_status: result.run_status,
    })
}

/// Start przebiegu: tylko po odblokowaniu (sprawdzane tu i jeszcze raz w transakcji `service`).
pub fn start_run(deps: &StartDeps, request: StartRequest) -> Result<StartResult, String> {
    unlocked(deps.db)?;
    start(deps, request)
}

/// Historia przebiegów; bez klucza.
pub fn history_list(db: &SharedDb, filter: &RunFilter) -> Result<Vec<RunSummary>, String> {
    list_runs(db, filter)
}

/// Szczegóły przebiegu; bez klucza.
pub fn history_get(db: &SharedDb, run_id: i64, now: i64) -> Result<RunDetail, String> {
    run_detail(db, run_id, now)?.ok_or_else(|| "Nie znaleziono przebiegu.".into())
}

/// Strona zdarzeń przebiegu; bez klucza.
pub fn history_events(
    db: &SharedDb,
    run_id: i64,
    after: u64,
    limit: usize,
) -> Result<Vec<StoredEvent>, String> {
    SqliteRunRepository::new(db.clone())
        .events_since(run_id, after, limit.min(MAX_EVENTS_PAGE))
        .map_err(db_error)
}

/// Zatrzymanie przebiegu w stanie Queued, Running albo WaitingForApproval; bez klucza.
pub fn cancel(db: &SharedDb, handle: &RuntimeHandle, run_id: i64) -> Result<(), String> {
    let status: String = {
        let conn = db
            .lock()
            .map_err(|_| "Blokada bazy jest zatruta.".to_owned())?;
        conn.query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .map_err(|_| "Nie znaleziono przebiegu.".to_owned())?
    };
    if super::runtime::is_terminal_status(&status) {
        return Err("Przebieg już się zakończył.".into());
    }
    if status == "cancelling" {
        return Ok(());
    }
    handle.cancel(run_id);
    Ok(())
}

/// Świadome odzyskanie przerwanego przebiegu (5.2, 7.3); bez klucza, jak
/// anulowanie: przy odzyskiwaniu dostępu da się zwolnić katalog roboczy.
pub fn recover_run(
    db: &SharedDb,
    control: &dyn ProcessControl,
    emit: &dyn Fn(&StoredEvent),
    run_id: i64,
    now: i64,
) -> Result<RecoverOutcome, String> {
    super::recovery::recover(db, control, emit, run_id, now)
}

/// Diff do zgody: tylko po odblokowaniu, wyłącznie przez (przebieg, skrót).
pub fn artifact_read(
    db: &SharedDb,
    app_data: &Path,
    run_id: i64,
    artifact_hash: &str,
) -> Result<DiffView, String> {
    unlocked(db)?;
    read_diff(db, app_data, run_id, artifact_hash, MAX_DIFF_VIEW_BYTES).map_err(db_error)
}

fn app_data(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn shared(app: &tauri::AppHandle) -> SharedDb {
    app.state::<crate::Database>().shared()
}

#[tauri::command]
pub async fn workflow_start(
    app: tauri::AppHandle,
    request: StartRequest,
) -> Result<StartResult, String> {
    let db = shared(&app);
    let data = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let handle = runtime_handle(&app)?;
        let detected = crate::ai::detected_cached(&app);
        let sink = super::runtime::sink_for(&app);
        let enqueue = |id: i64| handle.start_run(id);
        let emit = |event: &StoredEvent| workflow_core::ports::EventSink::emit(&sink, event);
        start_run(
            &StartDeps {
                db: &db,
                app_data: &data,
                detected: &detected,
                read_codex_config: super::preflight::read_codex_config,
                free_bytes: agent_cli::available_bytes,
                now: crate::db::now(),
                enqueue: &enqueue,
                emit: &emit,
            },
            request,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn workflow_runs(
    app: tauri::AppHandle,
    filter: Option<RunFilter>,
) -> Result<Vec<RunSummary>, String> {
    let db = shared(&app);
    tauri::async_runtime::spawn_blocking(move || history_list(&db, &filter.unwrap_or_default()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn workflow_run_get(app: tauri::AppHandle, run_id: i64) -> Result<RunDetail, String> {
    let db = shared(&app);
    let data = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Plik diffu sprawdzany po odczycie z bazy, bez jej blokady (E6).
        history_get(&db, run_id, crate::db::now())
            .map(|detail| super::artifacts::with_file_availability(detail, &data))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn workflow_run_events(
    app: tauri::AppHandle,
    run_id: i64,
    after: u64,
    limit: Option<usize>,
) -> Result<Vec<StoredEvent>, String> {
    let db = shared(&app);
    tauri::async_runtime::spawn_blocking(move || {
        history_events(&db, run_id, after, limit.unwrap_or(MAX_EVENTS_PAGE))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn workflow_cancel(app: tauri::AppHandle, run_id: i64) -> Result<(), String> {
    let db = shared(&app);
    cancel(&db, runtime_handle(&app)?, run_id)
}

#[tauri::command]
pub async fn workflow_decide(
    app: tauri::AppHandle,
    command: DecideCommand,
) -> Result<DecideOutcome, String> {
    let db = shared(&app);
    tauri::async_runtime::spawn_blocking(move || decide(&db, runtime_handle(&app)?, command))
        .await
        .map_err(|e| e.to_string())?
}

/// Sprawdzanie i kończenie procesów oraz Git poza wątkiem interfejsu.
#[tauri::command]
pub async fn workflow_recover(
    app: tauri::AppHandle,
    run_id: i64,
) -> Result<RecoverOutcome, String> {
    let db = shared(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let sink = super::runtime::sink_for(&app);
        let emit = |event: &StoredEvent| workflow_core::ports::EventSink::emit(&sink, event);
        recover_run(&db, &SystemProcesses, &emit, run_id, crate::db::now())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Problemy zgłoszone przez silnik od startu aplikacji; bez klucza, bo dotyczą
/// odzyskiwania i muszą być widoczne także po wyłączeniu modułu.
#[tauri::command]
pub fn workflow_faults(app: tauri::AppHandle) -> Vec<super::runtime::Fault> {
    app.try_state::<super::runtime::FaultLog>()
        .map(|log| log.snapshot())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn workflow_artifact_read(
    app: tauri::AppHandle,
    run_id: i64,
    artifact_hash: String,
) -> Result<DiffView, String> {
    let db = shared(&app);
    let data = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || artifact_read(&db, &data, run_id, &artifact_hash))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
