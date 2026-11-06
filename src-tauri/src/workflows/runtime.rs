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

/// Stan zarządzany przez Tauri: handle sterownika po jego uruchomieniu.
#[derive(Default)]
pub struct RuntimeState(OnceLock<RuntimeHandle>);

/// Handle sterownika albo czytelny błąd, gdy jeszcze się uruchamia.
pub fn runtime_handle(app: &tauri::AppHandle) -> Result<&RuntimeHandle, String> {
    let state = app.state::<RuntimeState>();
    state
        .inner()
        .0
        .get()
        .ok_or_else(|| "Silnik przebiegów jeszcze się uruchamia.".to_owned())
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

/// Powiadomienie dla zdarzenia: tylko prośba o zgodę i stan końcowy przebiegu.
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
        "runStateChanged" if settings.done => {
            let to = event.payload.get("to")?.as_str()?;
            if event.payload.get("type")?.as_str()? != "runStateChanged" || !is_terminal_status(to)
            {
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
        let payload = serde_json::json!({"runId": run_id, "message": message});
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

/// Uruchamia sterownik przebiegów w wątku w tle. Wołać po `manage(Database)`.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(RuntimeState::default());
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
            let git = Arc::new(GitWorkspace::new(db.clone()).with_app_data(app_data.clone()));
            let provider_app = app.clone();
            let provider_dir = app_data.clone();
            let cli = CliRunner::new(
                Arc::new(RealSpawner),
                cli_paths(&app, &app_data),
                git.clone(),
            )
            .with_paths_provider(Arc::new(move || cli_paths(&provider_app, &provider_dir)));
            let handle = Runtime::start(
                Arc::new(SqliteRunRepository::new(db)),
                Arc::new(GuardedRunner::new(Arc::new(cli), git.clone())),
                Arc::new(sink_for(&app)),
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
