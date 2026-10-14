//! Pamięć pozycji i rozmiaru okien (`tauri-plugin-window-state`).
//!
//! Okna `main` i `widget` są zapamiętywane. `reminder` i `quick` mają stałe
//! położenie ustawiane przez aplikację, więc plugin ich nie dotyka.
//!
//! Nasze okna są chowane zamiast zamykane, a proces żyje w trayu, więc plugin
//! sam zapisałby stan na dysk dopiero przy wyjściu z aplikacji. Dlatego stan
//! zapisujemy jawnie przy `CloseRequested` (X na pasku tytułu i „Schowaj do
//! traya” wołają `close()`, nie `hide()`), przy „Zakończ aplikację” z traya
//! i przy `--prepare-update`.
use tauri::{AppHandle, Runtime};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// Okna pomijane przez plugin.
pub const DENYLIST: [&str; 2] = ["reminder", "quick"];

/// Wszystko poza widocznością. O tym, czy okno jest widoczne przy starcie,
/// decyduje aplikacja (główne okno, autostart z `--hidden`, ustawienie
/// widgetu), a nie stan z chwili schowania okna do traya.
pub fn flags() -> StateFlags {
    StateFlags::all() - StateFlags::VISIBLE
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_window_state::Builder::new()
        .with_state_flags(flags())
        .with_denylist(&DENYLIST)
        .build()
}

/// Czy stan okna o danej etykiecie jest zapamiętywany.
pub fn tracked(label: &str) -> bool {
    !DENYLIST.contains(&label)
}

/// Zapisuje stan wszystkich śledzonych okien na dysk. Błąd zapisu nie może
/// blokować chowania okna ani wyjścia z aplikacji, więc jest tylko logowany.
pub fn save<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.save_window_state(flags()) {
        eprintln!("Nie udało się zapisać położenia okien: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_keep_geometry_but_not_visibility() {
        let flags = flags();
        assert!(flags.contains(StateFlags::POSITION));
        assert!(flags.contains(StateFlags::SIZE));
        assert!(flags.contains(StateFlags::MAXIMIZED));
        assert!(!flags.contains(StateFlags::VISIBLE));
    }

    #[test]
    fn only_main_and_widget_are_tracked() {
        assert!(tracked("main"));
        assert!(tracked("widget"));
        assert!(!tracked("reminder"));
        assert!(!tracked("quick"));
    }
}
