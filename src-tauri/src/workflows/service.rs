//! Start przebiegu (E5): walidacja, idempotencja, worktree i preflight.
//!
//! Funkcja `start` nie zna Tauri, więc testuje się ją na prawdziwym gicie w
//! katalogu tymczasowym. Żadne wywołanie gita nie dzieje się pod blokadą bazy:
//! bazę blokują tylko krótkie transakcje między krokami.

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use workflow_core::engine::{decide, Input, RunState, SequencedEvent};
use workflow_core::ports::{NewRun, RepoError, RunRepository, StoredEvent, MAX_GOAL_CHARS};
use workflow_core::FailureCode;

use super::preflight::{preflight_reading_config, NodeReadiness};
use super::runs::{SqliteRunRepository, MAX_REQUEST_ID_CHARS};
use super::workspace::{check_source, prepare_workspace, SharedDb};
use super::{require_unlocked, store};
use crate::ai::Detected;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    pub version_id: i64,
    pub request_id: String,
    pub task_id: Option<i64>,
    pub repo: String,
    pub goal: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub run_id: i64,
    /// `false`, gdy ten `request_id` już istniał (podwójne kliknięcie).
    pub created: bool,
}

/// Zależności startu; testy podstawiają wykryte CLI i licznik kolejki.
pub struct StartDeps<'a> {
    pub db: &'a SharedDb,
    pub app_data: &'a Path,
    pub detected: &'a Detected,
    pub read_codex_config: fn() -> Result<Option<String>, String>,
    /// Wolne miejsce na dysku ścieżki (`agent_cli::available_bytes`; testy podstawiają).
    pub free_bytes: fn(&Path) -> Option<u64>,
    pub now: i64,
    /// Wstawia przebieg do kolejki sterownika.
    pub enqueue: &'a dyn Fn(i64),
    /// Dostaje zdarzenia zapisane przez sam serwis (np. błąd przygotowania).
    pub emit: &'a dyn Fn(&StoredEvent),
}

fn lock_error() -> String {
    "Blokada bazy jest zatruta.".to_owned()
}

fn validate_input(req: &StartRequest) -> Result<PathBuf, String> {
    let id_len = req.request_id.trim().chars().count();
    if id_len == 0 || id_len > MAX_REQUEST_ID_CHARS {
        return Err(format!(
            "Identyfikator żądania musi mieć od 1 do {MAX_REQUEST_ID_CHARS} znaków."
        ));
    }
    if req
        .goal
        .as_deref()
        .is_some_and(|g| g.chars().count() > MAX_GOAL_CHARS)
    {
        return Err(format!(
            "Opis celu jest za długi (limit {MAX_GOAL_CHARS} znaków)."
        ));
    }
    let repo = req.repo.trim();
    if repo.is_empty() {
        return Err("Wskaż repozytorium Git, w którym ma pracować przebieg.".into());
    }
    let path = PathBuf::from(repo);
    if !path.is_absolute() {
        return Err("Ścieżka repozytorium musi być bezwzględna.".into());
    }
    Ok(path)
}

/// Wynik pierwszej transakcji.
enum Prepared {
    Existing(i64),
    Fresh {
        graph: workflow_core::WorkflowGraph,
        task_title: Option<String>,
    },
}

fn read_phase(deps: &StartDeps, req: &StartRequest) -> Result<Prepared, String> {
    let db = deps.db.lock().map_err(|_| lock_error())?;
    require_unlocked(&db).map_err(|e| e.to_string())?;
    let archived: Option<Option<i64>> = db
        .query_row(
            "SELECT w.archived_at FROM workflow_versions v
             JOIN workflows w ON w.id = v.workflow_id WHERE v.id = ?1",
            [req.version_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match archived {
        None => return Err("Nie znaleziono opublikowanej wersji procesu.".into()),
        Some(Some(_)) => return Err("Proces jest zarchiwizowany; nie można go uruchomić.".into()),
        Some(None) => {}
    }
    let existing: Option<(i64, i64)> = db
        .query_row(
            "SELECT id, version_id FROM workflow_runs WHERE request_id = ?1",
            [req.request_id.trim()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((run_id, version_id)) = existing {
        return if version_id == req.version_id {
            Ok(Prepared::Existing(run_id))
        } else {
            Err("Ten identyfikator żądania został użyty przy innym procesie.".into())
        };
    }
    let graph = store::version_graph(&db, req.version_id).map_err(|e| e.to_string())?;
    // Tytuł zadania czyta host: klient nie podaje go sam.
    let task_title = match req.task_id {
        Some(id) => Some(
            db.query_row("SELECT title FROM tasks WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("Nie znaleziono zadania.")?,
        ),
        None => None,
    };
    Ok(Prepared::Fresh { graph, task_title })
}

fn blocked_message(readiness: &[NodeReadiness]) -> Option<String> {
    readiness.iter().find_map(|n| {
        n.blocked
            .as_ref()
            .map(|b| format!("Krok „{}” jest zablokowany: {}", n.node_id, b.message))
    })
}

/// Kończy przebieg Queued jako Failed (`WorkspaceUnavailable`) przez silnik i repozytorium.
fn fail_queued_run(
    repo: &SqliteRunRepository,
    deps: &StartDeps,
    run_id: i64,
    message: &str,
) -> Result<(), String> {
    let graph = repo.graph(run_id).map_err(|e| e.to_string())?;
    let stored = repo.load_events(run_id, 0).map_err(|e| e.to_string())?;
    let sequenced = stored
        .iter()
        .map(|s| {
            Ok(SequencedEvent {
                sequence: s.sequence,
                event: s.to_run_event().map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let state = RunState::replay(&graph, &sequenced).map_err(|e| e.to_string())?;
    let transition = decide(
        &graph,
        &state,
        Input::Fault {
            code: FailureCode::WorkspaceUnavailable,
            message: message.to_owned(),
        },
    )
    .map_err(|e| e.to_string())?;
    let events = repo
        .append(run_id, state.event_count + 1, &transition.events, deps.now)
        .map_err(|e: RepoError| e.to_string())?;
    events.iter().for_each(|e| (deps.emit)(e));
    Ok(())
}

/// Oznacza przebieg jako nieudany i zwraca pierwotną przyczynę; gdy samo
/// oznaczenie zawiedzie, komunikat mówi też o tym (przebieg może zostać w kolejce).
fn fail_and_report(
    repo: &SqliteRunRepository,
    deps: &StartDeps,
    run_id: i64,
    message: String,
) -> String {
    match fail_queued_run(repo, deps, run_id, &message) {
        Ok(()) => message,
        Err(second) => format!(
            "{message} Dodatkowo nie udało się oznaczyć przebiegu jako nieudany (może zostać w kolejce): {second}"
        ),
    }
}

pub fn start(deps: &StartDeps, req: StartRequest) -> Result<StartResult, String> {
    let source = validate_input(&req)?;
    let (graph, task_title) = match read_phase(deps, &req)? {
        Prepared::Existing(run_id) => {
            return Ok(StartResult {
                run_id,
                created: false,
            })
        }
        Prepared::Fresh { graph, task_title } => (graph, task_title),
    };
    // Preflight i sprawdzenie źródła: poza blokadą bazy, przed utworzeniem przebiegu.
    let readiness = preflight_reading_config(&graph, deps.detected, deps.read_codex_config);
    if let Some(message) = blocked_message(&readiness) {
        return Err(message);
    }
    check_source(&source, deps.app_data).map_err(|e| e.to_string())?;
    // Brak miejsca blokuje start przed utworzeniem przebiegu i worktree (7.2).
    super::disk::ensure_free_space((deps.free_bytes)(deps.app_data))?;

    let repo = SqliteRunRepository::new(deps.db.clone());
    let created = repo
        .create_run(&NewRun {
            version_id: req.version_id,
            request_id: req.request_id.trim().to_owned(),
            task_id: req.task_id,
            task_title,
            repo: Some(source.to_string_lossy().into_owned()),
            goal: req.goal.clone().filter(|g| !g.trim().is_empty()),
            now: deps.now,
            ..NewRun::default()
        })
        .map_err(|e| e.to_string())?;
    if !created.created {
        return Ok(StartResult {
            run_id: created.run_id,
            created: false,
        });
    }
    let run_id = created.run_id;
    match prepare_workspace(deps.db, &source, deps.app_data, run_id, deps.now) {
        Ok(workspace) => {
            let update = || -> Result<(), String> {
                let db = deps.db.lock().map_err(|_| lock_error())?;
                db.execute(
                    "UPDATE workflow_runs SET repo = ?1, workspace = ?2, base_commit = ?3 WHERE id = ?4",
                    params![
                        workspace.source_repo.to_string_lossy(),
                        workspace.worktree_path.to_string_lossy(),
                        workspace.base_commit,
                        run_id
                    ],
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
            };
            if let Err(message) = update() {
                return Err(fail_and_report(&repo, deps, run_id, message));
            }
        }
        Err(e) => return Err(fail_and_report(&repo, deps, run_id, e.to_string())),
    }
    (deps.enqueue)(run_id);
    Ok(StartResult {
        run_id,
        created: true,
    })
}

#[cfg(test)]
mod tests;
