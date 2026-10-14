//! Mini-widget: małe okno zawsze na wierzchu z bieżącym pomiarem.
//!
//! O widoczności decyduje `apply`: ustawienia `widget_enabled` i
//! `widget_background_only`, schowanie przyciskiem X (do końca sesji aplikacji,
//! bez zmiany ustawienia) oraz stan głównego okna (tryb „tylko w tle”).
//! `apply` jest wołane w `setup`, po `settings-changed`, po zmianie fokusu
//! głównego okna i co 2 s z pętli w `reminder.rs`.
use crate::settings::{self, AppSettings};
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use tauri::{Emitter, Listener, Manager};

pub const LABEL: &str = "widget";

/// Stan widgetu widziany z frontendu (przełącznik w nagłówku głównego okna).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Ustawienie `widget_enabled`.
    pub enabled: bool,
    /// Ustawienie `widget_background_only`.
    pub background_only: bool,
    /// Schowany przyciskiem X do końca sesji aplikacji.
    pub dismissed: bool,
}

impl Status {
    fn from_settings(s: &AppSettings, dismissed: bool) -> Self {
        Self {
            enabled: s.widget_enabled,
            background_only: s.widget_background_only,
            dismissed,
        }
    }
}

/// Czy okno widgetu ma być widoczne. `main_background`: główne okno jest
/// schowane, zminimalizowane albo bez fokusu.
pub fn should_show(status: Status, main_background: bool) -> bool {
    status.enabled && !status.dismissed && (!status.background_only || main_background)
}

pub struct WidgetState {
    /// Ustawienia trzymane w pamięci, żeby `apply` nie brało blokady bazy.
    status: Mutex<Status>,
    wake: Mutex<mpsc::Sender<()>>,
}

fn status(app: &tauri::AppHandle) -> Status {
    app.try_state::<WidgetState>()
        .and_then(|s| s.status.lock().ok().map(|s| *s))
        .unwrap_or_default()
}

/// Czy główne okno jest w tle: schowane, zminimalizowane albo bez fokusu.
pub fn main_background(app: &tauri::AppHandle) -> bool {
    match app.get_webview_window("main") {
        Some(main) => {
            !main.is_visible().unwrap_or(true)
                || main.is_minimized().unwrap_or(false)
                || !main.is_focused().unwrap_or(true)
        }
        None => true,
    }
}

/// Pokazuje lub chowa okno widgetu. `main_background` liczy wywołujący (pętla
/// w `reminder.rs` i tak czyta stan głównego okna). Okno nie przejmuje fokusu
/// (`focusable: false`), więc `show` nie zabiera go aplikacji, w której pracujesz.
pub fn apply(app: &tauri::AppHandle, main_background: bool) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let show = should_show(status(app), main_background);
    if window.is_visible().unwrap_or(!show) == show {
        return;
    }
    if show {
        let _ = window.show();
        let _ = app.emit_to(LABEL, "widget-shown", ());
    } else {
        let _ = window.hide();
    }
}

/// Budzi wątek widgetu, który wywoła `apply`. Dla miejsc, które nie mogą czekać
/// na odczyt stanu okien (zdarzenia z wątku głównego).
pub fn wake(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<WidgetState>() {
        if let Ok(tx) = state.wake.lock() {
            let _ = tx.send(());
        }
    }
}

fn update(app: &tauri::AppHandle, change: impl FnOnce(&mut Status)) {
    let next = {
        let Some(state) = app.try_state::<WidgetState>() else {
            return;
        };
        let Ok(mut s) = state.status.lock() else {
            return;
        };
        change(&mut s);
        *s
    };
    let _ = app.emit("widget-state", next);
    wake(app);
}

/// Stan początkowy z ustawień, nasłuch `settings-changed` i wątek odświeżający.
pub fn setup(app: &tauri::App) {
    let (wake, signal) = mpsc::channel::<()>();
    app.manage(WidgetState {
        status: Mutex::new(Status::from_settings(
            &settings::current(app.handle()),
            false,
        )),
        wake: Mutex::new(wake),
    });
    let handle = app.handle().clone();
    app.listen_any("settings-changed", move |event| {
        if let Ok(s) = serde_json::from_str::<AppSettings>(event.payload()) {
            update(&handle, |status| {
                *status = Status::from_settings(&s, status.dismissed);
            });
        }
    });
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        apply(&handle, main_background(&handle));
        while signal.recv().is_ok() {
            // Kilka sygnałów pod rząd: wystarczy jedno sprawdzenie.
            while signal.try_recv().is_ok() {}
            if handle.get_webview_window("main").is_none() {
                break;
            }
            apply(&handle, main_background(&handle));
        }
    });
}

/// Stan dla przełącznika w nagłówku głównego okna.
#[tauri::command]
pub fn widget_status(app: tauri::AppHandle) -> Status {
    status(&app)
}

/// Chowa widget do końca sesji aplikacji (`true`, przycisk X) albo go przywraca
/// (`false`, przełącznik w nagłówku). Ustawienie `widget_enabled` się nie zmienia.
#[tauri::command]
pub fn widget_dismiss(app: tauri::AppHandle, dismissed: bool) -> Status {
    if dismissed {
        crate::window_state::save(&app);
    }
    update(&app, |s| s.dismissed = dismissed);
    status(&app)
}

/// Klik w nazwę zadania na widgecie: pokazuje główne okno.
#[tauri::command]
pub fn widget_open_main(app: tauri::AppHandle) {
    crate::show(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(enabled: bool, background_only: bool, dismissed: bool) -> Status {
        Status {
            enabled,
            background_only,
            dismissed,
        }
    }

    #[test]
    fn hidden_when_disabled_or_dismissed() {
        for background in [false, true] {
            assert!(!should_show(status(false, false, false), background));
            assert!(!should_show(status(false, true, false), background));
            assert!(!should_show(status(true, false, true), background));
            assert!(!should_show(status(true, true, true), background));
        }
    }

    #[test]
    fn background_only_follows_main_window() {
        assert!(should_show(status(true, false, false), false));
        assert!(should_show(status(true, false, false), true));
        assert!(!should_show(status(true, true, false), false));
        assert!(should_show(status(true, true, false), true));
    }

    #[test]
    fn status_reads_widget_settings_and_keeps_dismissal() {
        let s = AppSettings {
            widget_enabled: true,
            widget_background_only: true,
            ..AppSettings::default()
        };
        assert_eq!(Status::from_settings(&s, true), status(true, true, true));
        assert_eq!(
            Status::from_settings(&AppSettings::default(), false),
            status(false, false, false)
        );
    }

    #[test]
    fn status_serializes_in_camel_case() {
        let json = serde_json::to_value(status(true, false, true)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"enabled": true, "backgroundOnly": false, "dismissed": true})
        );
    }
}
