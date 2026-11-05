//! Repozytorium przebiegów w SQLite (PLAN_WDROZENIA.md, 7.1 i 5.4; etap E3).
//!
//! Implementuje `workflow_core::ports::RunRepository`. Każda metoda blokuje
//! połączenie tylko na czas jednej krótkiej transakcji i zwalnia je przed
//! wyjściem, więc praca runnera nigdy nie trzyma blokady bazy. Zdarzenia są
//! źródłem prawdy; tabele `workflow_runs`, `workflow_node_executions` i
//! `workflow_approvals` to projekcje liczone w tej samej transakcji.
//!
//! Moduł nie ma jeszcze komend IPC ani podpięcia do startu aplikacji: to E5
//! (komendy, magistrala `workflow-event`) i E6 (recovery przy starcie).
#![cfg_attr(not(test), allow(dead_code))]

use std::fmt::Display;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use workflow_core::engine::RunEvent;
use workflow_core::ports::{CreatedRun, NewRun, RepoError, RunId, RunRepository, StoredEvent};
use workflow_core::{validate, RunStatus, WorkflowGraph};

/// Najdłuższy `request_id` w znakach.
pub const MAX_REQUEST_ID_CHARS: usize = 128;
/// Największa strona zdarzeń zwracana przez `events_since`.
pub const MAX_EVENTS_PAGE: usize = 500;

/// Repozytorium nad współdzielonym połączeniem: aplikacja przekazuje tu
/// `Database::shared()`, czyli ten sam `Arc<Mutex<Connection>>`, którego używają
/// komendy Tauri (jedno połączenie i jedna blokada; podpięcie w E5).
#[derive(Clone)]
pub struct SqliteRunRepository {
    db: Arc<Mutex<Connection>>,
}

fn storage(error: impl Display) -> RepoError {
    RepoError::Storage(format!("Błąd bazy przebiegów: {error}"))
}

fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn insert_event(
    tx: &Transaction,
    run_id: RunId,
    sequence: u64,
    now: i64,
    event: &RunEvent,
) -> Result<StoredEvent, RepoError> {
    let stored = StoredEvent::new(run_id, sequence, now, event);
    tx.execute(
        "INSERT INTO workflow_events(run_id, sequence, execution_id, type, payload_json, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run_id,
            sequence as i64,
            stored.execution_id,
            stored.kind,
            stored.payload.to_string(),
            now
        ],
    )
    .map_err(storage)?;
    Ok(stored)
}

/// Przenosi zdarzenie do tabel projekcji.
fn project(tx: &Transaction, run_id: RunId, now: i64, event: &RunEvent) -> Result<(), RepoError> {
    match event {
        RunEvent::RunStateChanged { to, code, .. } => {
            tx.execute(
                "UPDATE workflow_runs SET status = ?1, updated_at = ?2,
                    error_code = COALESCE(?3, error_code),
                    error = CASE WHEN ?3 IS NOT NULL THEN ?4 ELSE error END,
                    started_at = CASE WHEN ?1 = 'running' AND started_at IS NULL THEN ?2 ELSE started_at END,
                    finished_at = CASE WHEN ?5 THEN ?2 ELSE finished_at END
                 WHERE id = ?6",
                params![
                    enum_text(to),
                    now,
                    code.map(|c| enum_text(&c)),
                    code.map(|c| c.message()),
                    to.is_terminal(),
                    run_id
                ],
            )
            .map_err(storage)?;
        }
        RunEvent::ExecutionPlanned {
            execution_id,
            node_id,
            iteration,
            attempt,
            inputs,
        } => {
            tx.execute(
                "INSERT INTO workflow_node_executions(id, run_id, node_id, iteration, attempt, status, input_json, started_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 'running', ?6, ?7)",
                params![
                    execution_id,
                    run_id,
                    node_id,
                    iteration,
                    attempt,
                    serde_json::to_string(inputs).map_err(storage)?,
                    now
                ],
            )
            .map_err(storage)?;
        }
        RunEvent::ExecutionFinished {
            execution_id,
            status,
            output,
            code,
            message,
            exit_code,
            session_id,
        } => {
            tx.execute(
                "UPDATE workflow_node_executions SET status = ?1, output_json = ?2,
                    provider_session_id = ?3, exit_code = ?4, error_code = ?5, error = ?6,
                    finished_at = ?7
                 WHERE id = ?8 AND run_id = ?9",
                params![
                    enum_text(status),
                    output.as_ref().map(|o| o.to_string()),
                    session_id,
                    exit_code,
                    code.map(|c| enum_text(&c)),
                    message,
                    now,
                    execution_id,
                    run_id
                ],
            )
            .map_err(storage)?;
        }
        RunEvent::ApprovalRequested {
            approval_id,
            execution_id,
            artifact_hash,
        } => {
            tx.execute(
                "INSERT INTO workflow_approvals(id, run_id, execution_id, artifact_hash, status)
                 VALUES(?1, ?2, ?3, ?4, 'pending')",
                params![approval_id, run_id, execution_id, artifact_hash],
            )
            .map_err(storage)?;
            tx.execute(
                "UPDATE workflow_node_executions SET status = 'waiting' WHERE id = ?1 AND status = 'running'",
                [execution_id],
            )
            .map_err(storage)?;
        }
        RunEvent::ApprovalResolved {
            approval_id,
            status,
            request_id,
            ..
        } => {
            tx.execute(
                "UPDATE workflow_approvals SET status = ?1, decision_request_id = ?2, decided_at = ?3
                 WHERE id = ?4",
                params![enum_text(status), request_id, now, approval_id],
            )
            .map_err(storage)?;
        }
        RunEvent::RunCreated { .. }
        | RunEvent::ConditionEvaluated { .. }
        | RunEvent::RepairRoundCounted { .. }
        | RunEvent::NodesSkipped { .. }
        | RunEvent::CancelRequested
        | RunEvent::Message { .. }
        | RunEvent::Tool { .. } => {}
    }
    Ok(())
}

impl SqliteRunRepository {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>, RepoError> {
        self.db
            .lock()
            .map_err(|_| RepoError::Storage("Blokada bazy jest zatruta.".into()))
    }

    fn query_events(
        &self,
        run_id: RunId,
        after: u64,
        limit: i64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let db = self.conn()?;
        let mut statement = db
            .prepare(
                "SELECT sequence, execution_id, type, payload_json, created_at FROM workflow_events
                 WHERE run_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![run_id, after as i64, limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(storage)?;
        // Pod blokadą tylko surowe wiersze; parsowanie JSON już po jej zwolnieniu.
        let raw = rows.collect::<Result<Vec<_>, _>>().map_err(storage)?;
        drop(statement);
        drop(db);
        raw.into_iter()
            .map(|(sequence, execution_id, kind, payload, timestamp)| {
                Ok(StoredEvent {
                    run_id,
                    sequence: sequence as u64,
                    execution_id,
                    kind,
                    timestamp,
                    payload: serde_json::from_str(&payload).map_err(storage)?,
                })
            })
            .collect()
    }

    /// Strona zdarzeń o sekwencji większej niż `from_sequence` (czytanie nadrabiające).
    pub fn events_since(
        &self,
        run_id: RunId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let limit = limit.clamp(1, MAX_EVENTS_PAGE) as i64;
        self.query_events(run_id, from_sequence, limit)
    }
}

impl RunRepository for SqliteRunRepository {
    fn create_run(&self, new: &NewRun) -> Result<CreatedRun, RepoError> {
        let request_id = new.request_id.trim();
        if request_id.is_empty() || request_id.chars().count() > MAX_REQUEST_ID_CHARS {
            return Err(RepoError::BadInput(format!(
                "Identyfikator żądania musi mieć od 1 do {MAX_REQUEST_ID_CHARS} znaków."
            )));
        }
        let db = self.conn()?;
        let tx = db.unchecked_transaction().map_err(storage)?;
        let existing: Option<(i64, i64)> = tx
            .query_row(
                "SELECT id, version_id FROM workflow_runs WHERE request_id = ?1",
                [request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        if let Some((run_id, version_id)) = existing {
            return if version_id == new.version_id {
                Ok(CreatedRun {
                    run_id,
                    created: false,
                })
            } else {
                Err(RepoError::Conflict)
            };
        }
        let (definition_json, hash): (String, String) = tx
            .query_row(
                "SELECT definition_json, hash FROM workflow_versions WHERE id = ?1",
                [new.version_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or(RepoError::NotFound)?;
        let graph: WorkflowGraph = serde_json::from_str(&definition_json)
            .map_err(|e| RepoError::InvalidDefinition(format!("Uszkodzona definicja: {e}")))?;
        let issues = validate(&graph);
        if !issues.is_empty() {
            return Err(RepoError::InvalidDefinition(format!(
                "Definicja procesu ma błędy ({}); nie można jej uruchomić.",
                issues.len()
            )));
        }
        tx.execute(
            "INSERT INTO workflow_runs(version_id, request_id, definition_json, definition_hash, task_id,
                task_title, repo, workspace, base_commit, status, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'queued', ?10, ?10)",
            params![
                new.version_id,
                request_id,
                definition_json,
                hash,
                new.task_id,
                new.task_title,
                new.repo,
                new.workspace,
                new.base_commit,
                new.now
            ],
        )
        .map_err(storage)?;
        let run_id = tx.last_insert_rowid();
        let created = RunEvent::RunCreated {
            run_id,
            graph_hash: hash,
        };
        insert_event(&tx, run_id, 1, new.now, &created)?;
        tx.commit().map_err(storage)?;
        Ok(CreatedRun {
            run_id,
            created: true,
        })
    }

    fn graph(&self, run_id: RunId) -> Result<WorkflowGraph, RepoError> {
        let db = self.conn()?;
        let json: String = db
            .query_row(
                "SELECT definition_json FROM workflow_runs WHERE id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .ok_or(RepoError::NotFound)?;
        serde_json::from_str(&json)
            .map_err(|e| RepoError::InvalidDefinition(format!("Uszkodzona definicja: {e}")))
    }

    fn append(
        &self,
        run_id: RunId,
        expected_next_sequence: u64,
        events: &[RunEvent],
        now: i64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let db = self.conn()?;
        // Bez `commit` transakcja wycofuje się przy każdym wczesnym wyjściu.
        let tx = db.unchecked_transaction().map_err(storage)?;
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM workflow_runs WHERE id = ?1",
                [run_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage)?
            .is_some();
        if !exists {
            return Err(RepoError::NotFound);
        }
        let last: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM workflow_events WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if expected_next_sequence != last as u64 + 1 {
            return Err(RepoError::Conflict);
        }
        let mut stored = Vec::with_capacity(events.len());
        for (offset, event) in events.iter().enumerate() {
            let sequence = expected_next_sequence + offset as u64;
            stored.push(insert_event(&tx, run_id, sequence, now, event)?);
            project(&tx, run_id, now, event)?;
        }
        tx.commit().map_err(storage)?;
        Ok(stored)
    }

    fn load_events(&self, run_id: RunId, after: u64) -> Result<Vec<StoredEvent>, RepoError> {
        self.query_events(run_id, after, i64::MAX)
    }

    fn last_sequence(&self, run_id: RunId) -> Result<u64, RepoError> {
        let db = self.conn()?;
        let last: i64 = db
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM workflow_events WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        Ok(last as u64)
    }

    fn active_runs(&self) -> Result<Vec<RunId>, RepoError> {
        let active: Vec<String> = RunStatus::ALL
            .iter()
            .filter(|s| !s.is_terminal())
            .map(enum_text)
            .collect();
        let db = self.conn()?;
        let mut statement = db
            .prepare("SELECT id, status FROM workflow_runs ORDER BY id")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(storage)?;
        let mut ids = Vec::new();
        for row in rows {
            let (id, status) = row.map_err(storage)?;
            if active.contains(&status) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests;
