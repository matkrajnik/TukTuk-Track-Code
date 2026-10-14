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
use std::collections::HashMap;
use tauri::{AppHandle, Manager, PhysicalSize, Runtime};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, DEFAULT_FILENAME};

/// Okna pomijane przez plugin.
pub const DENYLIST: [&str; 2] = ["reminder", "quick"];

/// Okna, których stan zapisuje plugin.
const TRACKED: [&str; 2] = ["main", "widget"];

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
pub struct SavedSize {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

/// Rozmiary okien śledzonych z pliku pluginu, czytane przed utworzeniem okien.
pub fn saved_sizes<R: Runtime>(app: &AppHandle<R>) -> HashMap<String, SavedSize> {
    let saved: HashMap<String, SavedSize> = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(DEFAULT_FILENAME)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    tracked_sizes(saved)
}

/// Tylko okna śledzone i z niezerowym rozmiarem. Dla zmaksymalizowanego okna
/// plugin trzyma rozmiar sprzed maksymalizacji, więc też go poprawiamy.
pub fn tracked_sizes(saved: HashMap<String, SavedSize>) -> HashMap<String, SavedSize> {
    saved
        .into_iter()
        .filter(|(label, s)| TRACKED.contains(&label.as_str()) && s.width > 0 && s.height > 0)
        .collect()
}

/// Zaraz po utworzeniu okien ponownie ustawia zapisane rozmiary.
///
/// Okna `main` i `widget` powstają schowane, a plugin odtwarza rozmiar
/// zaraz po ich utworzeniu. Na Windows pierwszy `set_size` w świeżym oknie bez
/// ramki (`decorations: false`, z cieniem) liczy przesunięcia z systemowym
/// paskiem tytułu, więc okno wychodzi o jego wysokość wyższe (37 px przy 125%)
/// i rośnie tak przy każdym restarcie (walidacja natywna v6). Drugi
/// `set_size` liczy już z przeliczoną ramką i daje zapisany rozmiar, także
/// gdy okno zostaje schowane (autostart z `--hidden`). Zmaksymalizowane okno
/// na chwilę przywracamy, żeby poprawić rozmiar, do którego wróci.
pub fn reapply_saved_sizes<R: Runtime>(app: &AppHandle<R>, saved: &HashMap<String, SavedSize>) {
    for (label, size) in saved {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        let maximized = window.is_maximized().unwrap_or(false);
        if maximized {
            let _ = window.unmaximize();
        }
        let _ = window.set_size(PhysicalSize {
            width: size.width,
            height: size.height,
        });
        if maximized {
            let _ = window.maximize();
        }
    }
}

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
    fn tracked_sizes_skip_untracked_and_empty() {
        let size = |width, height, maximized| SavedSize {
            width,
            height,
            maximized,
        };
        let saved = HashMap::from([
            ("main".to_string(), size(1475, 1000, false)),
            ("widget".to_string(), size(400, 80, true)),
            ("quick".to_string(), size(700, 550, false)),
            ("other".to_string(), size(0, 0, false)),
        ]);
        let tracked = tracked_sizes(saved);
        assert_eq!(tracked.len(), 2);
        assert_eq!(tracked["main"], size(1475, 1000, false));
        assert_eq!(tracked["widget"], size(400, 80, true));
    }

    #[test]
    fn saved_sizes_read_plugin_file_format() {
        let text = r#"{"main":{"width":1475,"height":1000,"x":186,"y":166,"prev_x":0,"prev_y":0,"maximized":false,"visible":true,"decorated":false,"fullscreen":false},"widget":{"width":400,"height":80,"x":400,"y":300,"prev_x":0,"prev_y":0,"maximized":true,"visible":true,"decorated":false,"fullscreen":false}}"#;
        let parsed: HashMap<String, SavedSize> = serde_json::from_str(text).unwrap();
        assert_eq!(
            parsed["main"],
            SavedSize {
                width: 1475,
                height: 1000,
                maximized: false
            }
        );
        assert!(parsed["widget"].maximized);
    }

    #[test]
    fn only_main_and_widget_are_tracked() {
        for label in TRACKED {
            assert!(tracked(label));
        }
        assert!(tracked("main"));
        assert!(tracked("widget"));
        assert!(!tracked("reminder"));
        assert!(!tracked("quick"));
    }
}
