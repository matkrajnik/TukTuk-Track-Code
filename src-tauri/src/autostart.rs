//! Uruchamianie z Windows (`tauri-plugin-autostart`).
//!
//! Stan „włączone” czytamy z rejestru przez plugin (wpis w `Run` i ewentualne
//! wyłączenie w Menedżerze zadań), nie z bazy, żeby nie rozjechał się po
//! ręcznej zmianie. Plugin ma argumenty ustalane raz przy rejestracji, a my
//! potrzebujemy `--hidden` tylko przy wybranej opcji, dlatego wpis zapisujemy
//! przez `auto-launch` (tę samą bibliotekę i tę samą nazwę wpisu co plugin)
//! z argumentami dobranymi przy każdym włączeniu. Wybór „schowane do traya”
//! jest w ustawieniu `autostart_hidden`, bo Windows nie pozwala go zmienić
//! bez aplikacji.
use crate::settings;
use serde::Serialize;
use tauri::{Emitter, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// Argument wpisu `Run`: aplikacja startuje do traya, główne okno zostaje schowane.
pub const HIDDEN_ARG: &str = "--hidden";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Autostart {
    pub enabled: bool,
    pub hidden: bool,
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_autostart::Builder::new().build()
}

/// Czy proces uruchomiono z `--hidden` (autostart „schowane do traya”).
pub fn started_hidden<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|a| a.as_ref() == HIDDEN_ARG)
}

/// Argumenty zapisywane we wpisie `Run`.
pub fn launch_args(hidden: bool) -> Vec<String> {
    if hidden {
        vec![HIDDEN_ARG.to_string()]
    } else {
        Vec::new()
    }
}

/// Ścieżka exe w cudzysłowie: katalog instalacji ma spację („TukTuk Track”),
/// a `auto-launch` skleja ścieżkę i argumenty bez cudzysłowów.
pub fn quoted_path(path: &str) -> String {
    format!("\"{}\"", path.trim_matches('"'))
}

fn enable<R: Runtime>(app: &tauri::AppHandle<R>, hidden: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder
        .set_app_name(&app.package_info().name)
        .set_app_path(&quoted_path(&exe.display().to_string()))
        .set_args(&launch_args(hidden));
    builder
        .build()
        .and_then(|launcher| launcher.enable())
        .map_err(|e| format!("Nie udało się włączyć uruchamiania z Windows: {e}"))
}

fn is_enabled<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Nie udało się odczytać uruchamiania z Windows: {e}"))
}

/// Włącza wpis `Run` z argumentami dla `hidden` albo go usuwa.
fn apply_run_entry<R: Runtime>(
    app: &tauri::AppHandle<R>,
    enabled: bool,
    hidden: bool,
) -> Result<(), String> {
    if enabled {
        return enable(app, hidden);
    }
    if let Err(e) = app.autolaunch().disable() {
        // Brak wpisu to też „wyłączone”; błąd tylko, gdy wpis nadal działa.
        if is_enabled(app)? {
            return Err(format!(
                "Nie udało się wyłączyć uruchamiania z Windows: {e}"
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn autostart_get(app: tauri::AppHandle) -> Result<Autostart, String> {
    Ok(Autostart {
        enabled: is_enabled(&app)?,
        hidden: settings::current(&app).autostart_hidden,
    })
}

#[tauri::command]
pub fn autostart_set(
    app: tauri::AppHandle,
    enabled: bool,
    hidden: bool,
) -> Result<Autostart, String> {
    // Najpierw ustawienie, potem rejestr: gdy zapis do bazy się nie uda, wpis
    // `Run` zostaje bez zmian. Gdy nie uda się zmiana w rejestrze, ustawienie
    // wraca do poprzedniej wartości, żeby UI widział stan zgodny z wpisem.
    // Blokada bazy trwa do końca, więc nikt nie zmieni ustawienia w trakcie.
    let saved = {
        let state = tauri::Manager::state::<crate::Database>(&app);
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let previous = settings::cached(&app, &db).autostart_hidden;
        let saved = settings::save_cached(
            &app,
            &mut db,
            &serde_json::json!({ "autostartHidden": hidden }),
        )?;
        if let Err(e) = apply_run_entry(&app, enabled, hidden) {
            if let Err(rollback) = settings::save_cached(
                &app,
                &mut db,
                &serde_json::json!({ "autostartHidden": previous }),
            ) {
                return Err(format!(
                    "{e} (nie udało się też przywrócić ustawienia: {rollback})"
                ));
            }
            return Err(e);
        }
        saved
    };
    let _ = app.emit("settings-changed", saved.clone());
    Ok(Autostart {
        enabled: is_enabled(&app)?,
        hidden: saved.autostart_hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_flag_is_detected_only_as_whole_argument() {
        assert!(started_hidden(["tuktuk-track.exe", "--hidden"]));
        assert!(!started_hidden(["tuktuk-track.exe"]));
        assert!(!started_hidden(["tuktuk-track.exe", "--hidden-not"]));
        assert!(!started_hidden(["tuktuk-track.exe", "--prepare-update"]));
    }

    #[test]
    fn run_entry_has_quoted_path_and_hidden_only_when_chosen() {
        assert_eq!(launch_args(true), vec!["--hidden".to_string()]);
        assert!(launch_args(false).is_empty());
        assert_eq!(
            quoted_path(r"C:\Programy\TukTuk Track\tuktuk-track.exe"),
            r#""C:\Programy\TukTuk Track\tuktuk-track.exe""#
        );
        assert_eq!(quoted_path(r#""C:\a b\x.exe""#), r#""C:\a b\x.exe""#);
    }
}
