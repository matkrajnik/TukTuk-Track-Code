//! „Zakończ aplikację” z traya w trakcie kroku przebiegu Workflows (decyzja po E7,
//! docs/workflows/PLAN_WDROZENIA.md, rozdz. 15).
//!
//! Gdy krok agenta działa (`workflows::runtime::running_step_workflow`, niezależnie
//! od klucza modułu), tray nie kończy aplikacji od razu: zapamiętuje oczekujące
//! pytanie (nazwa procesu), pokazuje główne okno i wysyła `app-quit-request`.
//! Okno (`QuitDialog.vue`) odpytuje też stan komendą `app_quit_pending` po
//! rejestracji nasłuchu, więc zdarzenie wysłane przed nią nie ginie. „Zakończ” to
//! `app_quit_confirmed` (błąd zatrzymania pomiaru wraca do okna), „Anuluj”
//! czyści pytanie (`app_quit_cancel`). Bez kroku w toku, a także przy błędzie
//! odczytu przebiegów, wyjście jak dotąd: aplikacji da się zawsze zamknąć.

use std::sync::Mutex;

use tauri::{Emitter, Manager};

use crate::Database;

/// Zdarzenie do głównego okna; ładunek to nazwa procesu.
pub const QUIT_REQUEST: &str = "app-quit-request";

/// Co zrobić z „Zakończ aplikację”.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuitPlan {
    /// Krok przebiegu (nazwa procesu) działa: najpierw pytanie w oknie.
    Ask(String),
    /// Nic nie działa albo nie dało się tego sprawdzić: wyjście od razu.
    QuitNow,
}

/// Decyzja z wyniku `running_step_workflow`. Błąd odczytu nie blokuje wyjścia:
/// trafia do `log` i daje `QuitNow`.
pub fn plan(step: Result<Option<String>, String>, log: impl FnOnce(&str)) -> QuitPlan {
    match step {
        Ok(Some(workflow)) => QuitPlan::Ask(workflow),
        Ok(None) => QuitPlan::QuitNow,
        Err(e) => {
            log(&format!(
                "Nie sprawdzono przebiegów Workflows przed zakończeniem: {e}"
            ));
            QuitPlan::QuitNow
        }
    }
}

/// Oczekujące pytanie o zakończenie (nazwa procesu); odczyt go nie czyści.
pub struct PendingQuit(Mutex<Option<String>>);

impl PendingQuit {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn set(&self, workflow: String) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(workflow);
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

static PENDING: PendingQuit = PendingQuit::new();

/// „Zakończ aplikację” z traya.
pub fn request_quit(app: &tauri::AppHandle) {
    let step = app
        .state::<Database>()
        .0
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|db| crate::workflows::runtime::running_step_workflow(&db));
    match plan(step, |m| eprintln!("{m}")) {
        QuitPlan::Ask(workflow) => {
            PENDING.set(workflow.clone());
            crate::show(app);
            if let Err(e) = app.emit_to("main", QUIT_REQUEST, workflow) {
                eprintln!("Nie udało się zapytać o zakończenie aplikacji: {e}");
                PENDING.clear();
                crate::tray::stop_measurement(app, true);
            }
        }
        QuitPlan::QuitNow => crate::tray::stop_measurement(app, true),
    }
}

/// Oczekujące pytanie (okno odpytuje po rejestracji nasłuchu).
#[tauri::command]
pub fn app_quit_pending() -> Option<String> {
    PENDING.get()
}

/// „Anuluj” albo Esc w pytaniu: aplikacja działa dalej.
#[tauri::command]
pub fn app_quit_cancel() {
    PENDING.clear();
}

/// Potwierdzone „Zakończ”: to samo co wyjście bez kroku (zatrzymanie pomiaru, zapis
/// okna, wyjście; sterownik zatrzymuje `RunEvent::Exit`, a przerwany krok daje
/// przebieg `Interrupted`). Błąd zatrzymania pomiaru wraca do okna pytania.
#[tauri::command]
pub fn app_quit_confirmed(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::try_stop_measurement(&app, true)?;
    PENDING.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_step_asks_with_the_workflow_name() {
        let plan = plan(Ok(Some("Proces".into())), |_| panic!("bez logu"));
        assert_eq!(plan, QuitPlan::Ask("Proces".into()));
    }

    #[test]
    fn no_running_step_quits_now() {
        assert_eq!(plan(Ok(None), |_| panic!("bez logu")), QuitPlan::QuitNow);
    }

    #[test]
    fn read_error_quits_now_and_is_logged() {
        let mut logged = String::new();
        let result = plan(Err("baza zajęta".into()), |m| logged = m.to_owned());
        assert_eq!(result, QuitPlan::QuitNow);
        assert_eq!(
            logged,
            "Nie sprawdzono przebiegów Workflows przed zakończeniem: baza zajęta"
        );
    }

    #[test]
    fn pending_question_survives_reads_until_cleared() {
        let pending = PendingQuit::new();
        assert_eq!(pending.get(), None);
        pending.set("Proces".into());
        assert_eq!(pending.get().as_deref(), Some("Proces"));
        assert_eq!(
            pending.get().as_deref(),
            Some("Proces"),
            "odczyt nie czyści"
        );
        pending.set("Inny".into());
        assert_eq!(pending.get().as_deref(), Some("Inny"));
        pending.clear();
        assert_eq!(pending.get(), None);
    }
}
