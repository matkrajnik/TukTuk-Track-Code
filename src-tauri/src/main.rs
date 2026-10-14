#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod autostart;
mod backup;
mod db;
mod idle;
mod jira;
mod maintenance;
mod notify;
mod planner;
mod recurrence;
mod reminder;
mod settings;
mod shortcuts;
mod stats;
mod tray;
mod widget;
mod window_state;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
struct Database(Mutex<rusqlite::Connection>);
#[tauri::command]
fn get_snapshot(state: tauri::State<Database>) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    recurrence::generate(&mut connection, chrono::Local::now().date_naive())?;
    db::snapshot(&connection).map_err(|e| e.to_string())
}
#[tauri::command]
fn mutate(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    action: String,
    id: Option<i64>,
    title: Option<String>,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = db::change(&mut connection, &action, id, title, db::now())?;
    if matches!(
        action.as_str(),
        "start" | "stop" | "toggle" | "delete" | "archive"
    ) {
        reminder::reset(&app);
    }
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn edit_session(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    started_at: i64,
    ended_at: i64,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = db::edit_session(&mut connection, id, started_at, ended_at, db::now())?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn split_session(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    at: i64,
    target_task: Option<i64>,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = db::split_session(&mut connection, id, at, target_task, db::now())?;
    reminder::reset(&app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
#[tauri::command]
fn quick_open(app: tauri::AppHandle) -> Result<(), String> {
    let w = app
        .get_webview_window("quick")
        .ok_or("Brak okna szybkiego przełączania.")?;
    w.center().map_err(|e| e.to_string())?;
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    let _ = app.emit_to("quick", "quick-open", ());
    Ok(())
}
/// Informacje spoza ustawień: wersja i błędy rejestracji skrótów globalnych.
#[tauri::command]
fn app_info(app: tauri::AppHandle) -> serde_json::Value {
    serde_json::json!({"version":app.config().version,"shortcutErrors":shortcuts::errors(&app)})
}
#[tauri::command]
fn close_day(app: tauri::AppHandle, day: String, closed: bool) -> Result<db::Snapshot, String> {
    if !planner::date_valid(&day) {
        return Err("Niepoprawny dzień.".into());
    }
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    if closed {
        db.execute("INSERT OR IGNORE INTO closed_days(day) VALUES(?1)", [day])
            .map_err(|e| e.to_string())?;
    } else {
        db.execute("DELETE FROM closed_days WHERE day=?1", [day])
            .map_err(|e| e.to_string())?;
    }
    let snapshot = db::snapshot(&db).map_err(|e| e.to_string())?;
    let _ = app.emit("data-changed", ());
    Ok(snapshot)
}
#[tauri::command]
fn set_priority(app: tauri::AppHandle, id: i64, priority: i32) -> Result<db::Snapshot, String> {
    if !(-1..=1).contains(&priority) {
        return Err("Niepoprawny priorytet.".into());
    }
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.execute("INSERT INTO task_details(task_id,json) VALUES(?1,json_object('priority',?2)) ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.priority',?2)",rusqlite::params![id,priority]).map_err(|e|e.to_string())?;
    let result = db::snapshot(&db).map_err(|e| e.to_string())?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn save_details(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    details: planner::Details,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = planner::save(&mut connection, id, details)?;
    reminder::reset(&app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn add_session(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    task_id: i64,
    started_at: i64,
    ended_at: i64,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = planner::add_session(&mut connection, task_id, started_at, ended_at, db::now())?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shortcuts::handle)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(window_state::plugin())
        .plugin(autostart::plugin())
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|arg| arg == "--prepare-update") {
                let state = app.state::<Database>();
                let result = state
                    .0
                    .lock()
                    .map_err(|e| e.to_string())
                    .and_then(|db| db::stop(&db, db::now()).map_err(|e| e.to_string()));
                match result {
                    Ok(()) => {
                        window_state::save(app);
                        app.exit(0);
                    }
                    Err(e) => {
                        show(app);
                        let _ = app.emit("storage-error", e);
                    }
                }
            } else if !autostart::started_hidden(&args) {
                // Wpis autostartu odpalony przy już działającej aplikacji nie
                // wyciąga okna na wierzch.
                show(app);
            }
        }))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("todo-track.sqlite");
            let existing = path.exists();
            let connection = rusqlite::Connection::open(path)?;
            maintenance::backup_before_upgrade(
                &connection,
                &dir,
                existing,
                app.config()
                    .version
                    .as_deref()
                    .unwrap_or(env!("CARGO_PKG_VERSION")),
            )?;
            db::init(&connection)?;
            maintenance::record_version(
                &connection,
                app.config()
                    .version
                    .as_deref()
                    .unwrap_or(env!("CARGO_PKG_VERSION")),
            )?;
            if std::env::args().any(|arg| arg == "--prepare-update") {
                db::stop(&connection, db::now())?;
                app.handle().exit(0);
                return Ok(());
            }
            app.manage(Database(Mutex::new(connection)));
            app.manage(backup::Pending::default());
            shortcuts::setup(app);
            jira::setup(app.handle())?;
            tray::setup(app)?;
            widget::setup(app);
            reminder::start(app.handle());
            idle::start(app.handle());
            notify::start(app.handle());
            // Główne okno startuje schowane (visible: false w konfiguracji); bez --hidden
            // pokazujemy je od razu, z --hidden (autostart) aplikacja zostaje w trayu.
            if !autostart::started_hidden(std::env::args()) {
                show(app.handle());
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                if handle.get_webview_window("main").is_none() {
                    break;
                }
                if let Ok(mut db) = handle.state::<Database>().0.lock() {
                    match recurrence::generate(&mut db, chrono::Local::now().date_naive()) {
                        Ok(()) => {
                            let _ = handle.emit("data-changed", ());
                        }
                        Err(e) => {
                            let _ = handle.emit("storage-error", e);
                        }
                    }
                };
            });
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if window_state::tracked(window.label()) {
                    window_state::save(window.app_handle());
                }
                let _ = window.hide();
                if window.label() == "reminder" {
                    let _ = reminder::reminder_action(window.app_handle().clone(), "snooze".into());
                }
                if window.label() == widget::LABEL {
                    widget::widget_dismiss(window.app_handle().clone(), true);
                }
                if window.label() == "main" {
                    widget::wake(window.app_handle());
                }
            }
            // Tryb widgetu „tylko w tle” reaguje od razu, nie dopiero w pętli co 2 s.
            tauri::WindowEvent::Focused(_) if window.label() == "main" => {
                widget::wake(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            mutate,
            edit_session,
            split_session,
            save_details,
            add_session,
            quick_open,
            shortcuts::toggle_measurement,
            app_info,
            settings::get_settings,
            settings::set_settings,
            autostart::autostart_get,
            autostart::autostart_set,
            close_day,
            set_priority,
            idle::idle_notice,
            idle::idle_resolve,
            backup::export_backup,
            backup::inspect_backup,
            backup::restore_backup,
            reminder::reminder_action,
            widget::widget_status,
            widget::widget_dismiss,
            widget::widget_open_main,
            jira::jira_settings,
            jira::jira_connect,
            jira::jira_disconnect,
            jira::jira_boards,
            jira::jira_select_boards,
            jira::jira_issues,
            jira::jira_start,
            jira::jira_open
        ])
        .run(tauri::generate_context!())
        .expect("Nie udało się uruchomić Todo Track");
}
