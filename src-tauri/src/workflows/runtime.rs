//! Runtime przebiegów w aplikacji (E5): sterownik na wątkach std, magistrala
//! `workflow-event` i powiadomienia o zgodzie oraz końcu przebiegu.
//!
//! Sink działa po zapisie zdarzenia, więc nie trzyma blokady repozytorium; zapis
//! klucza deduplikacji to osobna, krótka transakcja. Powiadomienia korzystają z
//! istniejących ustawień agentów (`agent_notify_waiting`, `agent_notify_done`).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use rusqlite::OptionalExtension;
use tauri::{Emitter, Manager};
use workflow_core::driver::{DriverConfig, Runtime, RuntimeHandle};
use workflow_core::ports::{EventSink, StoredEvent};

use super::processes::{SqliteLedger, SystemProcesses};
use super::runners::guard::GuardedRunner;
use super::runners::process::RealSpawner;
use super::runners::{CliPaths, CliRunner};
use super::runs::SqliteRunRepository;
use super::workspace::{GitWorkspace, SharedDb};
use crate::notify::{Message, View};

/// Nazwa zdarzenia Tauri ze strumieniem zdarzeń przebiegów (7.3).
pub const EVENT: &str = "workflow-event";
/// Zdarzenie o błędzie, którego nie dało się zapisać w historii przebiegu.
pub const FAULT_EVENT: &str = "workflow-fault";

/// Ile wyjście z aplikacji i aktualizacja czekają na koniec kroku w toku (8.3).
pub const EXIT_GRACE: std::time::Duration = std::time::Duration::from_secs(8);

/// Powód odmowy aktualizacji: przebieg z procesami w toku (`Queued`, `Running`,
/// `Cancelling`); `WaitingForApproval` i `Paused` nie blokują. Wołać pod blokadą bazy.
pub fn update_blocker(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    super::backup::active_work(conn)
        .map(|work| work.map(|m| format!("Aktualizacja wstrzymana. {m}")))
        .map_err(|e| e.to_string())
}

/// Kontrolowane zatrzymanie sterownika przy wyjściu z aplikacji i przed
/// aktualizacją: krok w toku jest anulowany, a koniec potwierdzony najwyżej w
/// [`EXIT_GRACE`]. Niepotwierdzony koniec trafia do logu; po restarcie
/// rekonsyliacja sprawdzi proces po PID i czasie utworzenia.
pub fn shutdown(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<RuntimeState>() else {
        return;
    };
    let Some(handle) = state.inner().0.get() else {
        return;
    };
    let report = handle.shutdown_graceful(EXIT_GRACE);
    if report.timed_out || report.unconfirmed.is_some() {
        eprintln!("Workflows: zamknięcie bez potwierdzenia końca kroku: {report:?}");
    }
}

/// Stan zarządzany przez Tauri: handle sterownika po jego uruchomieniu.
#[derive(Default)]
pub struct RuntimeState(OnceLock<RuntimeHandle>);

/// Komunikat, gdy sterownik został zatrzymany (wyjście z aplikacji).
pub const ERR_RUNTIME_STOPPED: &str =
    "Silnik przebiegów jest zatrzymany. Uruchom aplikację ponownie.";

/// Handle sterownika albo czytelny błąd, gdy jeszcze się uruchamia albo jest
/// już zatrzymany (wtedy start przebiegu zostałby bez wykonania).
pub fn runtime_handle(app: &tauri::AppHandle) -> Result<&RuntimeHandle, String> {
    let state = app.state::<RuntimeState>();
    live_handle(state.inner().0.get())
}

pub(crate) fn live_handle(handle: Option<&RuntimeHandle>) -> Result<&RuntimeHandle, String> {
    let handle = handle.ok_or_else(|| "Silnik przebiegów jeszcze się uruchamia.".to_owned())?;
    if handle.is_stopped() {
        return Err(ERR_RUNTIME_STOPPED.to_owned());
    }
    Ok(handle)
}

fn shared(app: &tauri::AppHandle) -> SharedDb {
    app.state::<crate::Database>().shared()
}

/// Ustawienia powiadomień przebiegów (te same co dla tablicy agentów).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeSettings {
    pub waiting: bool,
    pub done: bool,
}

/// Dane przebiegu potrzebne do treści powiadomienia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunInfo {
    pub workflow_name: String,
    pub task_title: Option<String>,
}

/// Jedno źródło stanów końcowych: `RunStatus::is_terminal` z core.
pub(crate) fn is_terminal_status(name: &str) -> bool {
    serde_json::from_value::<workflow_core::RunStatus>(serde_json::Value::String(name.to_owned()))
        .is_ok_and(|status| status.is_terminal())
}

fn end_label(status: &str) -> &'static str {
    match status {
        "succeeded" => "zakończony powodzeniem",
        "rejected" => "odrzucony",
        "failed" => "zakończony błędem",
        "cancelled" => "zatrzymany",
        _ => "przerwany",
    }
}

fn run_text(info: &RunInfo) -> String {
    match &info.task_title {
        Some(task) => format!("{}: {task}", info.workflow_name),
        None => info.workflow_name.clone(),
    }
}

/// Tytuł powiadomienia o przebiegu wstrzymanym po restarcie (decyzja po E7).
pub const PAUSED_TITLE: &str = "Przebieg wstrzymany po restarcie — wznów go w monitorze.";

/// Powiadomienie dla zdarzenia: prośba o zgodę, wstrzymanie po restarcie (raz na
/// przebieg, przy ustawieniu „czeka na Ciebie”) i stan końcowy przebiegu.
pub fn notice_for(
    event: &StoredEvent,
    info: &RunInfo,
    settings: NoticeSettings,
) -> Option<Message> {
    let open = View {
        view: Some("workflow-runs"),
        run: Some(event.run_id),
        ..View::default()
    };
    match event.kind.as_str() {
        "approvalRequested" if settings.waiting => {
            let id = event.payload.get("approvalId")?.as_str()?;
            Some(Message {
                key: format!("wf-approval:{id}"),
                title: "Przebieg czeka na Twoją zgodę".into(),
                body: run_text(info),
                actions: Vec::new(),
                open,
            })
        }
        "runStateChanged" => {
            if event.payload.get("type")?.as_str()? != "runStateChanged" {
                return None;
            }
            let to = event.payload.get("to")?.as_str()?;
            if to == "paused" {
                return settings.waiting.then(|| Message {
                    key: format!("wf-paused:{}", event.run_id),
                    title: PAUSED_TITLE.into(),
                    body: run_text(info),
                    actions: Vec::new(),
                    open,
                });
            }
            if !settings.done || !is_terminal_status(to) {
                return None;
            }
            Some(Message {
                key: format!("wf-end:{}", event.run_id),
                title: format!("Przebieg {}", end_label(to)),
                body: run_text(info),
                actions: Vec::new(),
                open,
            })
        }
        _ => None,
    }
}

/// Czy „Zakończ aplikację” ma zapytać: przebieg ma działający krok agenta
/// (`Running` albo `Cancelling` z krokiem w toku). Wstrzymany, czekający na zgodę
/// i przebieg na granicy kroków nie mają procesu, więc wyjście ich nie przerywa.
pub fn quit_needs_confirmation(status: workflow_core::RunStatus, step_running: bool) -> bool {
    use workflow_core::RunStatus::*;
    let active = match status {
        Running | Cancelling => true,
        Queued | WaitingForApproval | Paused | Cancelled | Succeeded | Rejected | Failed
        | Interrupted => false,
    };
    active && step_running
}

/// Nazwa procesu pierwszego przebiegu z krokiem w toku (pytanie przy „Zakończ
/// aplikację”) albo `None`. Nie zależy od klucza modułu: zatrzymanie i
/// bezpieczeństwo danych dotyczą też zablokowanego modułu. Wołać pod blokadą bazy.
pub fn running_step_workflow(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT r.status, w.name, EXISTS(SELECT 1 FROM workflow_node_executions e
                    WHERE e.run_id = r.id AND e.status = 'running')
             FROM workflow_runs r
             JOIN workflow_versions v ON v.id = r.version_id
             JOIN workflows w ON w.id = v.workflow_id
             WHERE r.finished_at IS NULL ORDER BY r.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (status, name, step_running) = row.map_err(|e| e.to_string())?;
        let status = serde_json::from_value::<workflow_core::RunStatus>(serde_json::Value::String(
            status.clone(),
        ))
        .map_err(|_| format!("Nieznany stan przebiegu: {status}"))?;
        if quit_needs_confirmation(status, step_running) {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// Zapisuje klucz powiadomienia: `Ok(true)` tylko za pierwszym razem (także po restarcie).
pub fn try_claim_key(db: &SharedDb, key: &str, now: i64) -> Result<bool, String> {
    let conn = db
        .lock()
        .map_err(|_| "blokada bazy jest zatruta".to_owned())?;
    conn.execute(
        "INSERT OR IGNORE INTO notifications_sent(key, at) VALUES(?1, ?2)",
        rusqlite::params![key, now],
    )
    .map(|changed| changed == 1)
    .map_err(|e| e.to_string())
}

/// Jak [`try_claim_key`], ale błąd bazy jest logowany i nie wysyła toastu:
/// bez zapisanego klucza nie da się zagwarantować, że nie będzie powtórki.
pub fn claim_key(db: &SharedDb, key: &str, now: i64) -> bool {
    match try_claim_key(db, key, now) {
        Ok(claimed) => claimed,
        Err(error) => {
            eprintln!("Workflows: nie zapisano klucza powiadomienia {key}: {error}");
            false
        }
    }
}

/// Czy pokazać toast. Klucz zapisuje się zawsze przed sprawdzeniem okna, więc
/// zdarzenie obejrzane w aktywnym oknie nie wróci jako toast po restarcie.
pub fn should_show_toast(claim: impl FnOnce() -> bool, focused: impl FnOnce() -> bool) -> bool {
    claim() && !focused()
}

/// Czy zdarzenie może dać powiadomienie. Sprawdzane przed odczytem ustawień i
/// bazy: komunikaty agentów (tysiące na krok) nie blokują bazy po nic.
pub fn may_notify(event: &StoredEvent) -> bool {
    matches!(event.kind.as_str(), "approvalRequested" | "runStateChanged")
}

/// Dane przebiegu do powiadomienia; błąd bazy jest zwracany, nie połykany.
pub fn try_run_info(db: &SharedDb, run_id: i64) -> Result<Option<RunInfo>, String> {
    let conn = db
        .lock()
        .map_err(|_| "blokada bazy jest zatruta".to_owned())?;
    conn.query_row(
        "SELECT w.name, r.task_title FROM workflow_runs r
         JOIN workflow_versions v ON v.id = r.version_id
         JOIN workflows w ON w.id = v.workflow_id WHERE r.id = ?1",
        [run_id],
        |r| {
            Ok(RunInfo {
                workflow_name: r.get(0)?,
                task_title: r.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Jak [`try_run_info`], ale błąd bazy jest logowany (jak w [`claim_key`]) i
/// powiadomienie jest pomijane.
pub fn run_info(db: &SharedDb, run_id: i64) -> Option<RunInfo> {
    match try_run_info(db, run_id) {
        Ok(info) => info,
        Err(error) => {
            eprintln!("Workflows: brak danych przebiegu {run_id} do powiadomienia: {error}");
            None
        }
    }
}

/// Ile ostatnich problemów pamięta `FaultLog`.
pub const MAX_FAULTS: usize = 50;

/// Problem zgłoszony przez silnik (np. po rekonsyliacji przy starcie).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fault {
    pub run_id: i64,
    pub message: String,
    pub at: i64,
}

/// Ograniczony dziennik problemów w pamięci: okno otwarte po starcie nie straci
/// zgłoszeń wysłanych, zanim zaczęło nasłuchiwać. Nie jest blokadą bazy.
#[derive(Default)]
pub struct FaultLog(std::sync::Mutex<std::collections::VecDeque<Fault>>);

impl FaultLog {
    /// Dopisuje problem; ten sam (przebieg, komunikat) nie powtarza się, a po
    /// przekroczeniu limitu odpada najstarszy.
    pub fn record(&self, run_id: i64, message: &str, at: i64) {
        let mut log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if log
            .iter()
            .any(|f| f.run_id == run_id && f.message == message)
        {
            return;
        }
        if log.len() >= MAX_FAULTS {
            log.pop_front();
        }
        log.push_back(Fault {
            run_id,
            message: message.to_owned(),
            at,
        });
    }

    pub fn snapshot(&self) -> Vec<Fault> {
        let log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        log.iter().cloned().collect()
    }
}

#[derive(Clone)]
pub struct TauriSink {
    app: tauri::AppHandle,
}

pub fn sink_for(app: &tauri::AppHandle) -> TauriSink {
    TauriSink { app: app.clone() }
}

fn main_focused(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

impl EventSink for TauriSink {
    fn emit(&self, event: &StoredEvent) {
        if let Err(e) = self.app.emit(EVENT, event) {
            eprintln!("Workflows: nie udało się wysłać zdarzenia: {e}");
        }
        if !may_notify(event) {
            return;
        }
        let current = crate::settings::current(&self.app);
        let settings = NoticeSettings {
            waiting: current.agent_notify_waiting,
            done: current.agent_notify_done,
        };
        let db = shared(&self.app);
        let Some(info) = run_info(&db, event.run_id) else {
            return;
        };
        let Some(message) = notice_for(event, &info, settings) else {
            return;
        };
        let claim = || claim_key(&db, &message.key, crate::db::now());
        if !should_show_toast(claim, || main_focused(&self.app)) {
            return;
        }
        crate::notify::show(&self.app, &message);
    }

    fn fault(&self, run_id: i64, message: &str) {
        let at = crate::db::now();
        if let Some(log) = self.app.try_state::<FaultLog>() {
            log.record(run_id, message, at);
        }
        let payload = serde_json::json!({"runId": run_id, "message": message, "at": at});
        if let Err(e) = self.app.emit(FAULT_EVENT, payload) {
            eprintln!("Workflows: nie udało się zgłosić błędu przebiegu {run_id}: {e}");
        }
    }
}

fn cli_paths(app: &tauri::AppHandle, app_data: &PathBuf) -> CliPaths {
    let detected = crate::ai::detected_cached(app);
    CliPaths {
        claude: detected.claude.map(|c| PathBuf::from(c.path)),
        codex: detected.codex.map(|c| PathBuf::from(c.path)),
        app_data: app_data.clone(),
    }
}

/// Rekonsyliacja procesów i przebiegów oraz retencja artefaktów przy starcie.
/// Błędy trafiają do logu; nie zatrzymują startu sterownika.
fn startup_maintenance(
    db: &SharedDb,
    app_data: &std::path::Path,
    sink: &TauriSink,
    retention_days: u32,
) {
    let emit = |event: &StoredEvent| sink.emit(event);
    let tmp = super::recovery::remove_orphan_tmp(app_data);
    if tmp.removed > 0 {
        eprintln!(
            "Workflows: usunięto pliki tymczasowe artefaktów: {}",
            tmp.removed
        );
    }
    let report =
        super::recovery::reconcile_at_startup(db, &SystemProcesses, &emit, crate::db::now())
            .unwrap_or_else(|e| super::recovery::StartupReport {
                errors: vec![format!("rekonsyliacja nie powiodła się: {e}")],
                ..Default::default()
            });
    if report.killed + report.unknown + report.failed_runs.len() + report.paused_runs.len() > 0 {
        eprintln!("Workflows: rekonsyliacja po starcie: {report:?}");
    }
    // Problemy trafiają do interfejsu przez FAULT_EVENT (i do logu).
    for (run_id, message) in super::recovery::startup_faults(&report, &tmp) {
        eprintln!("Workflows: po starcie: {message}");
        sink.fault(run_id, &message);
    }
    match super::retention::purge_expired(db, app_data, crate::db::now(), retention_days) {
        Ok(report) => {
            for error in &report.errors {
                eprintln!("Workflows: retencja artefaktów: {error}");
            }
        }
        Err(e) => eprintln!("Workflows: retencja artefaktów nie powiodła się: {e}"),
    }
}

/// Uruchamia sterownik przebiegów w wątku w tle. Wołać po `manage(Database)`.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(RuntimeState::default());
    app.manage(FaultLog::default());
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("workflow-runtime-setup".into())
        .spawn(move || {
            let Ok(app_data) = app.path().app_data_dir() else {
                eprintln!(
                    "Workflows: brak katalogu danych aplikacji; silnik przebiegów wyłączony."
                );
                return;
            };
            let db = shared(&app);
            let sink = sink_for(&app);
            // Rekonsyliacja przed startem sterownika (E6): żaden przerwany krok nie
            // rusza ponownie, a przebieg bez pewnego katalogu roboczego nie startuje.
            let retention_days = crate::settings::current(&app).workflow_retention_days;
            startup_maintenance(&db, &app_data, &sink, retention_days);
            let git = Arc::new(GitWorkspace::new(db.clone()).with_app_data(app_data.clone()));
            let provider_app = app.clone();
            let provider_dir = app_data.clone();
            let cli = CliRunner::new(
                Arc::new(RealSpawner),
                cli_paths(&app, &app_data),
                git.clone(),
            )
            .with_paths_provider(Arc::new(move || cli_paths(&provider_app, &provider_dir)))
            .with_ledger(Arc::new(SqliteLedger::new(db.clone(), crate::db::now)));
            let handle = Runtime::start(
                Arc::new(SqliteRunRepository::new(db)),
                Arc::new(GuardedRunner::new(Arc::new(cli), git.clone())),
                Arc::new(sink),
                git,
                DriverConfig::default(),
            );
            if app.state::<RuntimeState>().inner().0.set(handle).is_err() {
                eprintln!("Workflows: silnik przebiegów był już uruchomiony.");
            }
        });
    if let Err(e) = spawned {
        eprintln!("Workflows: nie udało się uruchomić silnika przebiegów: {e}");
    }
}

#[cfg(test)]
mod tests;
