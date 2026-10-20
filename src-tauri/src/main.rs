#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agent_time;
mod agents;
mod autostart;
mod backup;
mod browser;
mod calendar;
mod capacity;
mod db;
mod export;
mod focus;
mod foreground;
mod holidays;
mod idle;
mod jira;
mod jira_board;
mod jira_fields;
mod jira_move;
mod maintenance;
mod native_msg;
mod notify;
mod planner;
mod quickadd;
mod recurrence;
mod redmine;
mod reminder;
mod settings;
mod shortcuts;
mod stats;
mod suggest;
mod templates;
mod timesheet;
mod tray;
mod week_notes;
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
/// Okna z `tauri.conf.json` mają `"create": false` i powstają dopiero tutaj, po
/// zarejestrowaniu stanu (`manage`). Tauri tworzy okna z konfiguracji przed hookiem `setup`,
/// a WebView2 pompuje komunikaty podczas tworzenia kolejnego okna; wcześniejsze
/// okno zdążało wtedy wywołać np. `get_settings` i proces kończył się paniką
/// „state() called before manage()”.
fn create_windows(app: &tauri::App) -> tauri::Result<()> {
    for config in app.config().app.windows.clone() {
        if app.get_webview_window(&config.label).is_none() {
            tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
        }
    }
    Ok(())
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
/// Przed instalacją aktualizacji (`src/updater.ts`, między pobraniem a `install()`):
/// koniec bieżącego pomiaru i zapis stanu okien jak przy `--prepare-update`, bo
/// w Windows plugin updater po uruchomieniu instalatora kończy proces od razu,
/// bez zdarzeń zamknięcia okien.
#[tauri::command]
fn prepare_update(app: tauri::AppHandle) -> Result<(), String> {
    {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        db::stop(&db, db::now()).map_err(|e| e.to_string())?;
    }
    window_state::save(&app);
    let _ = app.emit("data-changed", ());
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
fn reorder_tasks(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    entries: Vec<planner::ReorderEntry>,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = planner::reorder(&mut connection, &entries)?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn plan_task(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    day: String,
    order: i64,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = planner::plan_task(&mut connection, id, &day, order)?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[tauri::command]
fn plan_tasks(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    entries: Vec<planner::PlanEntry>,
) -> Result<db::Snapshot, String> {
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = planner::plan_tasks(&mut connection, &entries)?;
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
/// Blokada „Czekam na” z kafelka: zablokuj, odblokuj, odrzuć propozycję z Jiry.
#[tauri::command]
fn set_waiting(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    change: planner::WaitingChange,
) -> Result<db::Snapshot, String> {
    let result = {
        let mut connection = state.0.lock().map_err(|e| e.to_string())?;
        planner::set_waiting(&mut connection, id, &change, db::now())?
    };
    reminder::reset(&app);
    let _ = app.emit("data-changed", ());
    // Blokada mogła zatrzymać pomiar w czasie bloku skupienia.
    focus::sync(&app);
    Ok(result)
}
/// Przypięcie zadania do widgetu („w toku”) i odpięcie.
#[tauri::command]
fn set_pinned(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
    pinned: bool,
) -> Result<db::Snapshot, String> {
    let result = {
        let mut connection = state.0.lock().map_err(|e| e.to_string())?;
        planner::set_pinned(&mut connection, id, pinned, db::now())?
    };
    let _ = app.emit("data-changed", ());
    Ok(result)
}
/// Szybkie dodawanie ze składnią (pole „Nowe zadanie” i szybkie przełączanie).
#[tauri::command]
fn quick_add(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    task: quickadd::NewTask,
) -> Result<db::Snapshot, String> {
    let config = jira::config(&app)?;
    // Klucz Jiry wiążemy tylko przy połączonym koncie (zapisany e-mail);
    // numer Redmine tylko przy połączonym Redmine (zapisany login).
    let site = (!config.email.is_empty()).then_some(config.site);
    let redmine = redmine::config(&app)?;
    let redmine_site = (!redmine.login.is_empty()).then_some(redmine.site);
    let start = task.start;
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = quickadd::create_linked(
        &mut connection,
        site.as_deref(),
        |key| jira::cached_summary(&app, key),
        redmine_site.as_deref(),
        |id| redmine::cached_subject(&app, id),
        task,
        db::now(),
    )?;
    if start {
        reminder::reset(&app);
    }
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
        // Aktualizacje z publicznego repozytorium wydań (RELEASE.md): sprawdzanie
        // i pobieranie wywołuje frontend (`src/updater.ts`), instalator NSIS startuje
        // w trybie pasywnym i uruchamia aplikację ponownie.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            // Czas agentów starszy niż 400 dni (PLAN_1.7.md, W1).
            agent_time::prune(&connection, db::now())?;
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
            // Redmine obok Jiry (PLAN_1.9.md): osobny stan, niezależny od Jiry.
            redmine::setup(app.handle());
            timesheet::setup(app.handle());
            jira_move::setup(app.handle());
            calendar::setup(app.handle());
            let tray_refresher = tray::setup(app)?;
            let widget_waker = widget::setup(app);
            reminder::setup(app.handle());
            idle::setup(app.handle());
            agents::setup(app.handle());
            suggest::setup(app.handle());
            foreground::setup(app.handle());
            browser::setup(app.handle());
            notify::setup(app.handle());
            focus::setup(app.handle());
            // Okna dopiero po zarejestrowaniu całego stanu (manage), z którego
            // korzystają komendy. Wątki tray, widget, reminder, idle, agents, notify i jira
            // kończą się, gdy nie ma okna main (zamykanie aplikacji), więc
            // startują dopiero po create_windows; inaczej przy wolnym starcie
            // (np. autostart zaraz po zalogowaniu) kończyłyby się od razu.
            let saved_sizes = window_state::saved_sizes(app.handle());
            create_windows(app)?;
            window_state::reapply_saved_sizes(app.handle(), &saved_sizes);
            tray::start(app.handle(), tray_refresher);
            widget::start(app.handle(), widget_waker);
            reminder::start(app.handle());
            idle::start(app.handle());
            agents::start(app.handle());
            notify::start(app.handle());
            focus::start(app.handle());
            // Rozszerzenie przeglądarki (W4): potok dla hosta i wpisy w rejestrze
            // zgodne z ustawieniem (ścieżka exe po aktualizacji).
            browser::start(app.handle());
            browser::sync(app.handle(), &settings::current(app.handle()));
            jira::start(app.handle());
            // Obserwacja przypisań w Redmine (R3): osobny wątek, niezależny od Jiry.
            redmine::start(app.handle());
            // Pytanie o przesunięcie zgłoszenia przy starcie pracy (J1).
            jira_move::start(app.handle());
            calendar::start(app.handle());
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
            set_pinned,
            set_waiting,
            add_session,
            quick_add,
            quick_open,
            shortcuts::toggle_measurement,
            app_info,
            prepare_update,
            settings::get_settings,
            settings::set_settings,
            autostart::autostart_get,
            autostart::autostart_set,
            close_day,
            set_priority,
            reorder_tasks,
            plan_task,
            plan_tasks,
            idle::idle_notice,
            idle::idle_resolve,
            agents::agents_status,
            browser::browser_status,
            browser::browser_open_folder,
            agents::agent_bindings,
            agents::agent_bind,
            agents::agent_unbind,
            agent_time::agent_time,
            suggest::agent_suggestions,
            suggest::agent_switch,
            suggest::agent_dismiss,
            backup::export_backup,
            backup::inspect_backup,
            backup::restore_backup,
            export::export_report,
            focus::focus_status,
            focus::focus_action,
            focus::focus_blocks,
            week_notes::week_note,
            week_notes::week_note_save,
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
            jira::jira_open,
            jira_move::jira_move_prompts,
            jira_move::jira_move_dismiss,
            jira_move::jira_no_ask,
            jira_move::jira_transitions,
            jira_move::jira_transition,
            jira_board::jira_board,
            jira_board::jira_board_move,
            jira_board::jira_assignable,
            jira_board::jira_assign,
            jira_board::jira_comment,
            jira_board::jira_branch_name,
            jira_board::jira_projects,
            jira_board::jira_issue_types,
            jira_board::jira_issue_type_fields,
            jira_board::jira_create,
            jira_fields::jira_fields,
            jira_fields::jira_refresh_fields,
            redmine::redmine_settings,
            redmine::redmine_connect,
            redmine::redmine_test,
            redmine::redmine_disconnect,
            redmine::redmine_issues,
            redmine::redmine_projects,
            redmine::redmine_select_projects,
            redmine::redmine_statuses,
            redmine::redmine_start,
            redmine::redmine_open,
            calendar::calendar_status,
            calendar::calendar_connect,
            calendar::calendar_disconnect,
            calendar::calendar_refresh,
            calendar::calendar_events,
            templates::templates_list,
            templates::template_save,
            templates::template_delete,
            timesheet::ts_settings,
            timesheet::ts_connect,
            timesheet::ts_disconnect,
            timesheet::ts_tasks,
            timesheet::ts_filled,
            timesheet::ts_send,
            timesheet::ts_sent,
            timesheet::ts_project_map,
            timesheet::ts_set_project_map
        ])
        .run(tauri::generate_context!())
        .expect("Nie udało się uruchomić Todo Track");
}

#[cfg(test)]
mod tests {
    /// Okna z konfiguracji mają powstawać w `create_windows`, po `manage(Database)`.
    /// Okno utworzone przez Tauri przed `setup` może wywołać komendę czytającą
    /// bazę, zanim stan istnieje, a to kończy proces paniką.
    #[test]
    fn config_windows_are_created_after_database() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows = config["app"]["windows"].as_array().unwrap();
        assert!(!windows.is_empty());
        for window in windows {
            assert_eq!(
                window["create"],
                serde_json::Value::Bool(false),
                "okno {} musi mieć \"create\": false",
                window["label"]
            );
        }
        let labels: Vec<&str> = windows
            .iter()
            .map(|w| w["label"].as_str().unwrap())
            .collect();
        for label in ["main", "quick", "reminder", "widget"] {
            assert!(labels.contains(&label), "brak okna {label}");
        }
    }
    /// Wątki tray, widget, reminder, idle, agents, notify i jira kończą się, gdy nie ma okna
    /// main, więc muszą startować po `create_windows` w `setup`; inaczej tray
    /// nie odświeżałby tooltipu i ikony przez całą sesję.
    #[test]
    fn background_threads_start_after_windows() {
        let source = include_str!("main.rs");
        let setup = &source[source.find(".setup(|app| {").unwrap()..];
        let windows = setup.find("create_windows(app)?;").unwrap();
        for call in [
            "tray::start(",
            "widget::start(",
            "reminder::start(",
            "idle::start(",
            "agents::start(",
            "notify::start(",
            "jira::start(",
        ] {
            let at = setup.find(call).unwrap_or_else(|| panic!("brak {call}"));
            assert!(at > windows, "{call} musi być po create_windows");
        }
        for call in [
            "tray::setup(",
            "widget::setup(",
            "reminder::setup(",
            "idle::setup(",
            "agents::setup(",
            "notify::setup(",
        ] {
            let at = setup.find(call).unwrap_or_else(|| panic!("brak {call}"));
            assert!(
                at < windows,
                "{call} (manage) musi być przed create_windows"
            );
        }
    }
}
