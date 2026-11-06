//! Odczyt historii i szczegółów przebiegów dla interfejsu (E5).
//!
//! Każda funkcja czyta projekcje jedną krótką transakcją, a JSON parsuje już po
//! zwolnieniu blokady bazy. Czasy agentów i oczekiwania na zgodę liczone są
//! osobno i nigdzie nie trafiają do raportu pracy.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params_from_iter, types::Value as Sql, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use workflow_core::ports::RunId;

use crate::workflows::workspace::canonical;

/// Największa strona listy przebiegów.
pub const MAX_RUNS_PAGE: usize = 100;

type Db = Arc<Mutex<Connection>>;

fn err(e: impl std::fmt::Display) -> String {
    format!("Błąd bazy przebiegów: {e}")
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunFilter {
    pub status: Vec<String>,
    pub workflow_id: Option<i64>,
    pub task_id: Option<i64>,
    pub repo: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub before_id: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: i64,
    pub workflow_id: i64,
    pub workflow_name: String,
    pub version: i64,
    pub status: String,
    pub task_id: Option<i64>,
    /// Snapshot z chwili startu: zostaje po usunięciu zadania.
    pub task_title: Option<String>,
    pub task_exists: bool,
    pub repo: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error_code: Option<String>,
    pub waiting_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionView {
    pub id: String,
    pub node_id: String,
    pub iteration: i64,
    pub attempt: i64,
    pub status: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i64>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub output: Option<Value>,
    pub provider_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalView {
    pub id: String,
    pub execution_id: String,
    pub status: String,
    pub artifact_hash: String,
    pub comment: Option<String>,
    pub decided_at: Option<i64>,
    pub artifact_available: bool,
    /// Plik diffu usunęła retencja (E6): interfejs pokazuje to zamiast błędu odczytu.
    pub artifact_purged: bool,
}

/// Stan odzyskania przerwanego przebiegu (E6, `workflow_recover`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryView {
    /// Katalog roboczy nadal zarezerwowany (blokada do potwierdzenia końca procesów).
    pub workspace_reserved: bool,
    /// Procesy z rejestru bez potwierdzonego końca.
    pub processes_unconfirmed: i64,
    /// PID tych procesów (do komunikatu: użytkownik może zakończyć je ręcznie).
    pub unconfirmed_pids: Vec<u32>,
    /// Liczba zapisanych prób odzyskania (zdarzenia `recovery`).
    pub attempts: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub summary: RunSummary,
    pub goal: Option<String>,
    /// Snapshot definicji z chwili startu.
    pub graph: Value,
    pub definition_hash: String,
    /// Ostatnia sekwencja zdarzeń: klient podaje ją jako `expectedRevision`.
    pub revision: u64,
    pub workspace: Option<String>,
    pub base_commit: Option<String>,
    pub error: Option<String>,
    pub executions: Vec<ExecutionView>,
    pub approvals: Vec<ApprovalView>,
    pub agent_ms: i64,
    pub waiting_ms: i64,
    /// Tokeny nie są zbierane: interfejs pokazuje „brak danych”.
    pub tokens: Option<i64>,
    /// Tylko dla przebiegu `interrupted`.
    pub recovery: Option<RecoveryView>,
}

const SUMMARY_SELECT: &str =
    "SELECT r.id, w.id, w.name, v.version, r.status, r.task_id, r.task_title,
        EXISTS(SELECT 1 FROM tasks t WHERE t.id = r.task_id), r.repo, r.created_at, r.started_at,
        r.finished_at, r.error_code,
        EXISTS(SELECT 1 FROM workflow_approvals a WHERE a.run_id = r.id AND a.status = 'pending')
     FROM workflow_runs r
     JOIN workflow_versions v ON v.id = r.version_id
     JOIN workflows w ON w.id = v.workflow_id";

fn summary_row(r: &rusqlite::Row) -> rusqlite::Result<RunSummary> {
    Ok(RunSummary {
        id: r.get(0)?,
        workflow_id: r.get(1)?,
        workflow_name: r.get(2)?,
        version: r.get(3)?,
        status: r.get(4)?,
        task_id: r.get(5)?,
        task_title: r.get(6)?,
        task_exists: r.get::<_, bool>(7)?,
        repo: r.get(8)?,
        created_at: r.get(9)?,
        started_at: r.get(10)?,
        finished_at: r.get(11)?,
        error_code: r.get(12)?,
        waiting_approval: r.get::<_, bool>(13)?,
    })
}

/// Klucz porównania repozytorium w SQL: `\` zamiast `/`, bez końcowego
/// separatora, małe litery ASCII (jak `lower()` w SQLite). Musi pasować do [`repo_key`].
const REPO_KEY_SQL: &str = r"lower(rtrim(replace(r.repo, '/', '\'), '\'))";

/// Klucz repozytorium z filtra: ścieżka kanoniczna, gdy istnieje (rzeczywista
/// wielkość liter jak przy starcie), inaczej przycięty tekst; dalej jak
/// [`REPO_KEY_SQL`]. Dzięki temu `c:/repo/`, `C:\Repo` i `C:\REPO\` dają to samo.
pub(crate) fn repo_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let text = canonical(Path::new(trimmed))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| trimmed.to_owned());
    text.replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// Lista przebiegów od najnowszego; kolejną stronę daje `before_id` = najmniejsze id strony.
pub fn list_runs(db: &Db, filter: &RunFilter) -> Result<Vec<RunSummary>, String> {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Sql> = Vec::new();
    if !filter.status.is_empty() {
        let marks = vec!["?"; filter.status.len()].join(",");
        clauses.push(format!("r.status IN ({marks})"));
        args.extend(filter.status.iter().cloned().map(Sql::Text));
    }
    let numeric = [
        ("w.id = ?", filter.workflow_id),
        ("r.task_id = ?", filter.task_id),
        ("r.created_at >= ?", filter.from),
        ("r.created_at <= ?", filter.to),
        ("r.id < ?", filter.before_id),
    ];
    for (clause, value) in numeric {
        if let Some(v) = value {
            clauses.push(clause.into());
            args.push(Sql::Integer(v));
        }
    }
    if let Some(repo) = filter.repo.as_deref().filter(|r| !r.trim().is_empty()) {
        // Porównanie ścieżek, nie wzorzec: bez LIKE, więc `%` i `_` są dosłowne.
        clauses.push(format!("{REPO_KEY_SQL} = ?"));
        args.push(Sql::Text(repo_key(repo)));
    }
    let limit = filter
        .limit
        .unwrap_or(MAX_RUNS_PAGE)
        .clamp(1, MAX_RUNS_PAGE);
    let filter_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let sql = format!("{SUMMARY_SELECT}{filter_sql} ORDER BY r.id DESC LIMIT {limit}");
    let conn = db
        .lock()
        .map_err(|_| "Blokada bazy jest zatruta.".to_owned())?;
    let mut statement = conn.prepare(&sql).map_err(err)?;
    let rows = statement
        .query_map(params_from_iter(args), summary_row)
        .map_err(err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(err)
}

struct RawExecution {
    id: String,
    node_id: String,
    iteration: i64,
    attempt: i64,
    status: String,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    exit_code: Option<i64>,
    error_code: Option<String>,
    error: Option<String>,
    output: Option<String>,
    session: Option<String>,
}

struct RawRun {
    summary: RunSummary,
    goal: Option<String>,
    definition: String,
    hash: String,
    workspace: Option<String>,
    base_commit: Option<String>,
    error: Option<String>,
    revision: i64,
    executions: Vec<RawExecution>,
    approvals: Vec<ApprovalView>,
    /// (approval_id, type, created_at) zdarzeń zgód, w kolejności sekwencji.
    approval_events: Vec<(String, String, i64)>,
    recovery: Option<RecoveryView>,
}

type Header = (
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn read_raw(conn: &Connection, run_id: RunId) -> Result<Option<RawRun>, String> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let sql = format!("{SUMMARY_SELECT} WHERE r.id = ?1");
    let summary = match tx.query_row(&sql, [run_id], summary_row) {
        Ok(s) => s,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let (goal, definition, hash, workspace, base_commit, error): Header = tx
        .query_row(
            "SELECT goal, definition_json, definition_hash, workspace, base_commit, error
             FROM workflow_runs WHERE id = ?1",
            [run_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .map_err(err)?;
    let revision: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM workflow_events WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let executions = {
        let mut st = tx
            .prepare(
                "SELECT id, node_id, iteration, attempt, status, started_at, finished_at, exit_code,
                    error_code, error, output_json, provider_session_id
                 FROM workflow_node_executions WHERE run_id = ?1 ORDER BY started_at, rowid",
            )
            .map_err(err)?;
        let rows = st
            .query_map([run_id], |r| {
                Ok(RawExecution {
                    id: r.get(0)?,
                    node_id: r.get(1)?,
                    iteration: r.get(2)?,
                    attempt: r.get(3)?,
                    status: r.get(4)?,
                    started_at: r.get(5)?,
                    finished_at: r.get(6)?,
                    exit_code: r.get(7)?,
                    error_code: r.get(8)?,
                    error: r.get(9)?,
                    output: r.get(10)?,
                    session: r.get(11)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)?
    };
    let approvals = {
        let mut st = tx
            .prepare(
                "SELECT a.id, a.execution_id, a.status, a.artifact_hash, a.comment, a.decided_at,
                    EXISTS(SELECT 1 FROM workflow_artifacts f WHERE f.run_id = a.run_id
                           AND f.kind = 'diff' AND f.hash = a.artifact_hash
                           AND f.purged_at IS NULL),
                    EXISTS(SELECT 1 FROM workflow_artifacts f WHERE f.run_id = a.run_id
                           AND f.kind = 'diff' AND f.hash = a.artifact_hash
                           AND f.purged_at IS NOT NULL)
                 FROM workflow_approvals a WHERE a.run_id = ?1 ORDER BY a.rowid",
            )
            .map_err(err)?;
        let rows = st
            .query_map([run_id], |r| {
                Ok(ApprovalView {
                    id: r.get(0)?,
                    execution_id: r.get(1)?,
                    status: r.get(2)?,
                    artifact_hash: r.get(3)?,
                    comment: r.get(4)?,
                    decided_at: r.get(5)?,
                    artifact_available: r.get::<_, bool>(6)?,
                    artifact_purged: r.get::<_, bool>(7)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)?
    };
    let approval_events = {
        let mut st = tx
            .prepare(
                "SELECT json_extract(payload_json, '$.approvalId'), type, created_at
                 FROM workflow_events
                 WHERE run_id = ?1 AND type IN ('approvalRequested', 'approvalResolved')
                 ORDER BY sequence",
            )
            .map_err(err)?;
        let rows = st
            .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)?
    };
    let recovery = if summary.status == "interrupted" {
        Some(
            tx.query_row(
                "SELECT
                   EXISTS(SELECT 1 FROM workflow_workspaces WHERE run_id = ?1 AND state = 'reserved'),
                   (SELECT COUNT(*) FROM workflow_processes
                    WHERE run_id = ?1 AND state IN ('running', 'unknown')),
                   (SELECT COUNT(*) FROM workflow_events WHERE run_id = ?1 AND type = 'recovery')",
                [run_id],
                |r| {
                    Ok(RecoveryView {
                        workspace_reserved: r.get(0)?,
                        processes_unconfirmed: r.get(1)?,
                        unconfirmed_pids: Vec::new(),
                        attempts: r.get(2)?,
                    })
                },
            )
            .map_err(err)
            .and_then(|view| {
                let mut st = tx
                    .prepare(
                        "SELECT pid FROM workflow_processes
                         WHERE run_id = ?1 AND state IN ('running', 'unknown')
                         ORDER BY recorded_at, execution_id",
                    )
                    .map_err(err)?;
                let pids = st
                    .query_map([run_id], |r| r.get(0))
                    .map_err(err)?
                    .collect::<Result<Vec<u32>, _>>()
                    .map_err(err)?;
                Ok(RecoveryView {
                    unconfirmed_pids: pids,
                    ..view
                })
            })?,
        )
    } else {
        None
    };
    Ok(Some(RawRun {
        recovery,
        summary,
        goal,
        definition,
        hash,
        workspace,
        base_commit,
        error,
        revision,
        executions,
        approvals,
        approval_events,
    }))
}

/// Czas oczekiwania na zgody: od `approvalRequested` do `approvalResolved`;
/// zgoda wciąż otwarta liczy się do `now`.
fn waiting_ms(events: &[(String, String, i64)], now: i64) -> i64 {
    let mut open: Vec<(&str, i64)> = Vec::new();
    let mut total = 0;
    for (id, kind, at) in events {
        if kind == "approvalRequested" {
            open.push((id, *at));
        } else if let Some(i) = open.iter().position(|(o, _)| o == id) {
            total += (at - open.remove(i).1).max(0);
        }
    }
    total + open.iter().map(|(_, at)| (now - at).max(0)).sum::<i64>()
}

/// Szczegóły przebiegu; `None`, gdy przebiegu nie ma.
pub fn run_detail(db: &Db, run_id: RunId, now: i64) -> Result<Option<RunDetail>, String> {
    let raw = {
        let conn = db
            .lock()
            .map_err(|_| "Blokada bazy jest zatruta.".to_owned())?;
        read_raw(&conn, run_id)?
    };
    let Some(raw) = raw else { return Ok(None) };
    let graph: Value = serde_json::from_str(&raw.definition).map_err(err)?;
    let agent_nodes: Vec<&str> = graph["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter(|n| n["kind"]["type"] == "agent")
                .filter_map(|n| n["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let agent_ms = raw
        .executions
        .iter()
        .filter(|e| agent_nodes.contains(&e.node_id.as_str()))
        .filter_map(|e| Some((e.finished_at? - e.started_at?).max(0)))
        .sum();
    let waiting_ms = waiting_ms(&raw.approval_events, now);
    let executions = raw
        .executions
        .into_iter()
        .map(|e| {
            Ok(ExecutionView {
                output: match e.output {
                    Some(text) => Some(serde_json::from_str(&text).map_err(err)?),
                    None => None,
                },
                id: e.id,
                node_id: e.node_id,
                iteration: e.iteration,
                attempt: e.attempt,
                status: e.status,
                started_at: e.started_at,
                finished_at: e.finished_at,
                exit_code: e.exit_code,
                error_code: e.error_code,
                error: e.error,
                provider_session_id: e.session,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Some(RunDetail {
        summary: raw.summary,
        goal: raw.goal,
        graph,
        definition_hash: raw.hash,
        revision: raw.revision as u64,
        workspace: raw.workspace,
        base_commit: raw.base_commit,
        error: raw.error,
        executions,
        approvals: raw.approvals,
        agent_ms,
        waiting_ms,
        tokens: None,
        recovery: raw.recovery,
    }))
}
