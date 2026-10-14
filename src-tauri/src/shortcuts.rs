//! Globalne skróty: szybkie przełączanie i Start/Stop. Rejestracja z ustawień,
//! ponowna po `settings-changed`, błędy rejestracji osobno dla każdego skrótu.
use crate::{db, reminder, settings, Database};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use tauri::{Emitter, Listener, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

/// Błędy rejestracji: pusty tekst oznacza, że skrót działa.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Errors {
    pub quick: String,
    pub toggle: String,
}

/// Aktualnie zarejestrowane skróty (tekst z ustawień i sparsowana postać) oraz błędy.
#[derive(Default)]
struct Registered {
    quick: Option<(String, Shortcut)>,
    toggle: Option<(String, Shortcut)>,
    errors: Errors,
}

#[derive(Default)]
pub struct Shortcuts(Mutex<Registered>);

/// Wynik akcji Start/Stop.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Toggle {
    /// Zatrzymano trwający pomiar.
    #[serde(rename_all = "camelCase")]
    Stopped {
        task_id: i64,
        title: String,
        elapsed_ms: i64,
    },
    /// Wznowiono ostatnio mierzone zadanie.
    #[serde(rename_all = "camelCase")]
    Started { task_id: i64, title: String },
    /// Nie ma czego wznowić: trzeba wybrać zadanie w szybkim przełączaniu.
    Quick,
}

/// Zadanie do wznowienia: ostatnio mierzone, nieukończone, niezarchiwizowane i nie w koszu.
/// „Ostatnio” według końca przedziału (aktywny liczy się od startu).
pub fn resume_candidate(db: &Connection) -> rusqlite::Result<Option<(i64, String)>> {
    db.query_row(
        "SELECT t.id, t.title FROM sessions s
         JOIN tasks t ON t.id = s.task_id
         LEFT JOIN task_lifecycle l ON l.task_id = t.id
         WHERE t.done = 0 AND COALESCE(l.archived, 0) = 0 AND l.deleted_at IS NULL
         ORDER BY COALESCE(s.ended_at, s.started_at) DESC, s.started_at DESC, s.id DESC
         LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// Start/Stop: trwający pomiar → Stop; brak pomiaru → Start ostatniego zadania;
/// brak zadania do wznowienia → szybkie przełączanie (bez zmian w bazie).
pub fn toggle(db: &mut Connection, now: i64) -> Result<Toggle, String> {
    let active = db
        .query_row(
            "SELECT t.id, t.title, s.started_at FROM sessions s JOIN tasks t ON t.id = s.task_id
             WHERE s.ended_at IS NULL LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((task_id, title, started)) = active {
        db::stop(db, now).map_err(|e| e.to_string())?;
        return Ok(Toggle::Stopped {
            task_id,
            title,
            elapsed_ms: (now - started).max(0),
        });
    }
    match resume_candidate(db).map_err(|e| e.to_string())? {
        Some((task_id, title)) => {
            db::change(db, "start", Some(task_id), None, now)?;
            Ok(Toggle::Started { task_id, title })
        }
        None => Ok(Toggle::Quick),
    }
}

/// Treść potwierdzenia: „Stop: X · 01:23:45” albo „Start: X”.
pub fn feedback(result: &Toggle) -> Option<String> {
    match result {
        Toggle::Stopped {
            title, elapsed_ms, ..
        } => Some(format!("Stop: {title} · {}", crate::tray::hms(*elapsed_ms))),
        Toggle::Started { title, .. } => Some(format!("Start: {title}")),
        Toggle::Quick => None,
    }
}

/// Komunikat po polsku dla błędu rejestracji z pluginu.
pub fn registration_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("already registered") || lower.contains("unable to register") {
        "Skrót jest zajęty przez inny program.".into()
    } else {
        format!("Nie udało się zarejestrować skrótu: {message}")
    }
}

fn parse(value: &str) -> Result<Shortcut, String> {
    use std::str::FromStr;
    Shortcut::from_str(value).map_err(|e| e.to_string())
}

/// Wykonuje Start/Stop i powiadamia resztę aplikacji: `data-changed`, potwierdzenie
/// w dymku traya i zdarzenie `toggle-feedback` (tekst „Stop: X · 01:23:45” / „Start: X”)
/// dla widgetu (B) i powiadomień (J). Przy `Toggle::Quick` niczego nie otwiera:
/// szybkie przełączanie otwiera wywołujący (skrót w `handle`, frontend po komendzie).
pub fn run_toggle(app: &tauri::AppHandle) -> Result<Toggle, String> {
    let result = {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        toggle(&mut db, db::now())?
    };
    if let Some(text) = feedback(&result) {
        crate::tray::show_feedback(app, &text);
        reminder::reset(app);
        let _ = app.emit("data-changed", ());
        let _ = app.emit("toggle-feedback", text);
    }
    Ok(result)
}

/// Obsługa wciśnięcia dowolnego zarejestrowanego skrótu.
pub fn handle(app: &tauri::AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let (is_quick, is_toggle) = {
        let state = app.state::<Shortcuts>();
        // Handler działa na wątku głównym, a `apply` trzyma blokadę, czekając na ten
        // wątek przy rejestracji. Naciśnięcie w trakcie zmiany skrótów jest pomijane.
        let Ok(r) = state.0.try_lock() else {
            return;
        };
        let same = |s: &Option<(String, Shortcut)>| {
            s.as_ref().is_some_and(|(_, s)| s.id() == shortcut.id())
        };
        (same(&r.quick), same(&r.toggle))
    };
    let result = if is_quick {
        crate::quick_open(app.clone())
    } else if is_toggle {
        match run_toggle(app) {
            Ok(Toggle::Quick) => crate::quick_open(app.clone()),
            other => other.map(|_| ()),
        }
    } else {
        Ok(())
    };
    if let Err(e) = result {
        crate::show(app);
        let _ = app.emit("storage-error", e);
    }
}

/// Rejestruje skróty z ustawień. Zmienia tylko te, które się różnią od zarejestrowanych.
pub fn apply(app: &tauri::AppHandle, quick: &str, toggle: &str) -> Errors {
    let state = app.state::<Shortcuts>();
    let Ok(mut r) = state.0.lock() else {
        return Errors::default();
    };
    let gs = app.global_shortcut();
    let unchanged = |slot: &Option<(String, Shortcut)>, value: &str| {
        slot.as_ref().is_some_and(|(text, _)| text == value)
    };
    let quick_same = unchanged(&r.quick, quick) && r.errors.quick.is_empty();
    let toggle_same = unchanged(&r.toggle, toggle) && r.errors.toggle.is_empty();
    // Najpierw zwolnij stare skróty, żeby zamiana miejscami nie kolidowała sama ze sobą.
    if !quick_same {
        if let Some((_, s)) = r.quick.take() {
            let _ = gs.unregister(s);
        }
    }
    if !toggle_same {
        if let Some((_, s)) = r.toggle.take() {
            let _ = gs.unregister(s);
        }
    }
    let register = |value: &str| -> (Option<(String, Shortcut)>, String) {
        match parse(value) {
            Err(e) => (None, format!("Niepoprawny skrót: {e}")),
            Ok(s) => match gs.register(s) {
                Ok(()) => (Some((value.to_string(), s)), String::new()),
                Err(e) => (None, registration_error(&e.to_string())),
            },
        }
    };
    if !quick_same {
        let (slot, error) = register(quick);
        r.quick = slot;
        r.errors.quick = error;
    }
    if !toggle_same {
        let (slot, error) = register(toggle);
        r.toggle = slot;
        r.errors.toggle = error;
    }
    r.errors.clone()
}

/// Bieżące błędy. Bez czekania na blokadę (patrz `handle`); w trakcie ponownej
/// rejestracji zwraca puste, a wynik przychodzi zdarzeniem `shortcuts-changed`.
pub fn errors(app: &tauri::AppHandle) -> Errors {
    app.state::<Shortcuts>()
        .0
        .try_lock()
        .map(|r| r.errors.clone())
        .unwrap_or_default()
}

/// Pierwsza rejestracja i nasłuch zmian ustawień.
pub fn setup(app: &tauri::App) {
    app.manage(Shortcuts::default());
    let s = settings::current(app.handle());
    apply(app.handle(), &s.shortcut_quick, &s.shortcut_toggle);
    // Rejestracja blokuje do wykonania na wątku głównym, a zdarzenie bywa emitowane
    // właśnie z niego, więc listener tylko budzi osobny wątek.
    let (wake, signal) = mpsc::channel::<settings::AppSettings>();
    app.listen_any("settings-changed", move |event| {
        if let Ok(s) = serde_json::from_str::<settings::AppSettings>(event.payload()) {
            let _ = wake.send(s);
        }
    });
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        while let Ok(mut s) = signal.recv() {
            // Kilka zmian pod rząd: liczy się ostatnia.
            while let Ok(next) = signal.try_recv() {
                s = next;
            }
            let errors = apply(&handle, &s.shortcut_quick, &s.shortcut_toggle);
            let _ = handle.emit("shortcuts-changed", errors);
        }
    });
}

/// Start/Stop dla przycisków (widget). Przy `{action: "quick"}` wywołujący sam otwiera
/// szybkie przełączanie (`quick_open`); komenda go nie otwiera.
#[tauri::command]
pub fn toggle_measurement(app: tauri::AppHandle) -> Result<Toggle, String> {
    run_toggle(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["A", "B", "C"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        db
    }
    fn session(db: &Connection, task: i64, start: i64, end: Option<i64>) {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            rusqlite::params![task, start, end],
        )
        .unwrap();
    }

    #[test]
    fn resume_picks_last_measured_open_task() {
        let mut db = db();
        assert_eq!(resume_candidate(&db).unwrap(), None);
        // B zaczęte później, ale A skończone później: liczy się koniec przedziału.
        session(&db, 1, 100, Some(900));
        session(&db, 2, 200, Some(300));
        assert_eq!(resume_candidate(&db).unwrap(), Some((1, "A".into())));
        // Ukończone zadanie jest pomijane.
        db::change(&mut db, "toggle", Some(1), None, 1000).unwrap();
        assert_eq!(resume_candidate(&db).unwrap(), Some((2, "B".into())));
        // Zarchiwizowane i w koszu też.
        session(&db, 3, 1000, Some(2000));
        assert_eq!(resume_candidate(&db).unwrap(), Some((3, "C".into())));
        db::change(&mut db, "archive", Some(3), None, 2100).unwrap();
        assert_eq!(resume_candidate(&db).unwrap(), Some((2, "B".into())));
        db::change(&mut db, "delete", Some(2), None, 2200).unwrap();
        assert_eq!(resume_candidate(&db).unwrap(), None);
        db::change(&mut db, "restore", Some(2), None, 2300).unwrap();
        assert_eq!(resume_candidate(&db).unwrap(), Some((2, "B".into())));
    }

    #[test]
    fn toggle_stops_then_resumes_then_falls_back_to_quick() {
        let mut db = db();
        // Bez historii: szybkie przełączanie, baza bez zmian.
        assert_eq!(toggle(&mut db, 1000).unwrap(), Toggle::Quick);
        let count = |db: &Connection| -> i64 {
            db.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count(&db), 0);
        db::change(&mut db, "start", Some(2), None, 1000).unwrap();
        let stopped = toggle(&mut db, 1000 + 5_025_000).unwrap();
        assert_eq!(
            stopped,
            Toggle::Stopped {
                task_id: 2,
                title: "B".into(),
                elapsed_ms: 5_025_000
            }
        );
        assert_eq!(feedback(&stopped).unwrap(), "Stop: B · 01:23:45");
        let active: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active, 0);
        let started = toggle(&mut db, 9_000_000).unwrap();
        assert_eq!(
            started,
            Toggle::Started {
                task_id: 2,
                title: "B".into()
            }
        );
        assert_eq!(feedback(&started).unwrap(), "Start: B");
        let (task, start): (i64, i64) = db
            .query_row(
                "SELECT task_id, started_at FROM sessions WHERE ended_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((task, start), (2, 9_000_000));
        // Ukończenie jedynego mierzonego zadania: znów szybkie przełączanie.
        db::change(&mut db, "toggle", Some(2), None, 9_500_000).unwrap();
        assert_eq!(toggle(&mut db, 9_600_000).unwrap(), Toggle::Quick);
        assert_eq!(count(&db), 2);
    }

    #[test]
    fn toggle_serializes_for_the_frontend() {
        let json = serde_json::to_value(Toggle::Stopped {
            task_id: 1,
            title: "A".into(),
            elapsed_ms: 5,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"action":"stopped","taskId":1,"title":"A","elapsedMs":5})
        );
        assert_eq!(
            serde_json::to_value(Toggle::Quick).unwrap(),
            serde_json::json!({"action":"quick"})
        );
    }

    #[test]
    fn registration_errors_are_polish() {
        assert_eq!(
            registration_error("Unable to register hotkey: Control+Alt+Space"),
            "Skrót jest zajęty przez inny program."
        );
        assert_eq!(
            registration_error("HotKey already registered: HotKey { .. }"),
            "Skrót jest zajęty przez inny program."
        );
        assert!(registration_error("inne").starts_with("Nie udało się"));
    }
}
