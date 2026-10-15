//! Ustawienia aplikacji: jedna struktura, zapisywana klucz po kluczu w tabeli `settings`.
use crate::Database;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{Emitter, Manager};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub idle_enabled: bool,
    /// Praca agentów AI (Claude Code, Codex) liczy się jako aktywność.
    pub idle_agents: bool,
    pub target_day_minutes: u32,
    pub work_days: Vec<u8>,
    pub day_start: String,
    pub widget_enabled: bool,
    pub widget_background_only: bool,
    pub autostart_hidden: bool,
    pub notify_due: bool,
    pub notify_long_minutes: u32,
    pub notify_evening: String,
    pub notify_evening_enabled: bool,
    pub notify_target: bool,
    pub shortcut_quick: String,
    pub shortcut_toggle: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_enabled: true,
            idle_agents: true,
            target_day_minutes: 480,
            work_days: vec![1, 2, 3, 4, 5],
            day_start: "08:00".into(),
            widget_enabled: false,
            widget_background_only: false,
            autostart_hidden: false,
            notify_due: true,
            notify_long_minutes: 240,
            notify_evening: "17:30".into(),
            notify_evening_enabled: true,
            notify_target: false,
            shortcut_quick: "Control+Shift+Space".into(),
            shortcut_toggle: "Control+Alt+Space".into(),
        }
    }
}

/// Nazwa pola w JSON (camelCase) -> klucz w tabeli `settings` (snake_case).
fn db_key(field: &str) -> String {
    let mut out = String::with_capacity(field.len() + 4);
    for c in field.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn fields() -> Map<String, Value> {
    match serde_json::to_value(AppSettings::default()) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

pub fn valid_time(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit())
        && (b[0] - b'0') * 10 + (b[1] - b'0') < 24
        && (b[3] - b'0') * 10 + (b[4] - b'0') < 60
}

fn parse_shortcut(value: &str) -> Option<tauri_plugin_global_shortcut::Shortcut> {
    use std::str::FromStr;
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    tauri_plugin_global_shortcut::Shortcut::from_str(value).ok()
}

/// Komunikat dla skrótu, który na polskim układzie klawiatury jest literą z AltGr.
pub const ALTGR_LETTER: &str = "Ta kombinacja to polska litera (AltGr) – wybierz inną.";

/// Ctrl+Alt (z Shift lub bez, bez Win) z A, C, E, L, N, O, S, X, Z to w Windows AltGr,
/// czyli ą ć ę ł ń ó ś ź ż. Globalny skrót zabrałby tę literę wszystkim programom.
pub fn altgr_letter(value: &str) -> bool {
    use tauri_plugin_global_shortcut::{Code, Modifiers};
    let Some(s) = parse_shortcut(value) else {
        return false;
    };
    s.mods & !Modifiers::SHIFT == Modifiers::CONTROL | Modifiers::ALT
        && matches!(
            s.key,
            Code::KeyA
                | Code::KeyC
                | Code::KeyE
                | Code::KeyL
                | Code::KeyN
                | Code::KeyO
                | Code::KeyS
                | Code::KeyX
                | Code::KeyZ
        )
}

/// Porównuje skróty po sparsowaniu, więc „Ctrl+Shift+Space” i „Shift+Control+Space” są tym samym.
fn same_shortcut(a: &str, b: &str) -> bool {
    match (parse_shortcut(a), parse_shortcut(b)) {
        (Some(a), Some(b)) => a.id() == b.id(),
        _ => a.eq_ignore_ascii_case(b),
    }
}

/// Pełna walidacja: pola osobno i reguły łączące pola.
pub fn validate(s: AppSettings) -> Result<AppSettings, String> {
    let s = validate_fields(s)?;
    if same_shortcut(&s.shortcut_quick, &s.shortcut_toggle) {
        return Err("Skróty szybkiego przełączania i Start/Stop muszą być różne.".into());
    }
    Ok(s)
}

/// Sprawdza zakresy każdego pola osobno i porządkuje dni robocze (rosnąco, bez powtórzeń).
fn validate_fields(mut s: AppSettings) -> Result<AppSettings, String> {
    if s.target_day_minutes > 1440 {
        return Err("Cel dzienny musi mieć od 0 do 1440 minut.".into());
    }
    if s.notify_long_minutes > 1440 {
        return Err("Próg długiego pomiaru musi mieć od 0 do 1440 minut.".into());
    }
    if s.work_days.iter().any(|d| !(1..=7).contains(d)) {
        return Err("Dni robocze muszą być liczbami od 1 do 7.".into());
    }
    s.work_days.sort_unstable();
    s.work_days.dedup();
    if !valid_time(&s.day_start) {
        return Err("Początek dnia musi mieć format GG:MM.".into());
    }
    if !valid_time(&s.notify_evening) {
        return Err("Godzina wieczornego przypomnienia musi mieć format GG:MM.".into());
    }
    if parse_shortcut(&s.shortcut_quick).is_none() || parse_shortcut(&s.shortcut_toggle).is_none() {
        return Err("Niepoprawny skrót klawiszowy.".into());
    }
    if altgr_letter(&s.shortcut_quick) || altgr_letter(&s.shortcut_toggle) {
        return Err(ALTGR_LETTER.into());
    }
    Ok(s)
}

/// Nakłada częściową zmianę (obiekt z kluczami camelCase) na ustawienia.
pub fn apply(current: &AppSettings, patch: &Value) -> Result<AppSettings, String> {
    let patch = patch
        .as_object()
        .ok_or("Zmiana ustawień musi być obiektem.")?;
    let Value::Object(mut merged) = serde_json::to_value(current).map_err(|e| e.to_string())?
    else {
        return Err("Niepoprawne ustawienia.".into());
    };
    for (key, value) in patch {
        if !merged.contains_key(key) {
            return Err(format!("Nieznane ustawienie: {key}."));
        }
        merged.insert(key.clone(), value.clone());
    }
    let next: AppSettings = serde_json::from_value(Value::Object(merged))
        .map_err(|_| "Niepoprawna wartość ustawienia.".to_string())?;
    validate(next)
}

/// Odczyt z bazy. Najpierw zbiera wszystkie zapisane wartości (brakujące lub uszkodzone
/// pole dostaje wartość domyślną), potem raz sprawdza reguły łączące pola. Gdy skróty
/// kolidują, oba wracają do domyślnych.
pub fn load(db: &Connection) -> AppSettings {
    let defaults = AppSettings::default();
    let mut merged = fields();
    for (field, default) in fields() {
        let Ok(raw) = db.query_row(
            "SELECT value FROM settings WHERE key=?1",
            [db_key(&field)],
            |r| r.get::<_, String>(0),
        ) else {
            continue;
        };
        let value = if default.is_string() {
            Value::String(raw)
        } else {
            match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            }
        };
        // Pole sprawdzane samo, na tle wartości domyślnych.
        let mut alone = fields();
        alone.insert(field.clone(), value.clone());
        let checked = serde_json::from_value::<AppSettings>(Value::Object(alone))
            .map_err(|e| e.to_string())
            .and_then(validate_fields);
        if checked.is_ok() {
            merged.insert(field, value);
        }
    }
    let Ok(Ok(mut settings)) =
        serde_json::from_value::<AppSettings>(Value::Object(merged)).map(validate_fields)
    else {
        return defaults;
    };
    if same_shortcut(&settings.shortcut_quick, &settings.shortcut_toggle) {
        settings.shortcut_quick = defaults.shortcut_quick;
        settings.shortcut_toggle = defaults.shortcut_toggle;
    }
    settings
}

/// Waliduje i zapisuje w jednej transakcji tylko klucze obecne w `patch`.
pub fn save(db: &mut Connection, patch: &Value) -> Result<AppSettings, String> {
    let next = apply(&load(db), patch)?;
    let Value::Object(values) = serde_json::to_value(&next).map_err(|e| e.to_string())? else {
        return Err("Niepoprawne ustawienia.".into());
    };
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for key in patch.as_object().into_iter().flat_map(|p| p.keys()) {
        let raw = match &values[key] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        tx.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![db_key(key), raw],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(next)
}

pub fn current(app: &tauri::AppHandle) -> AppSettings {
    app.state::<Database>()
        .0
        .lock()
        .map(|db| load(&db))
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    Ok(load(&db))
}

#[tauri::command]
pub fn set_settings(app: tauri::AppHandle, patch: Value) -> Result<AppSettings, String> {
    let result = {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        save(&mut db, &patch)?
    };
    if !result.idle_enabled {
        crate::idle::clear(&app);
    }
    let _ = app.emit("settings-changed", result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        db
    }

    #[test]
    fn defaults_and_legacy_idle_value_are_read() {
        let db = db();
        assert_eq!(load(&db), AppSettings::default());
        db.execute(
            "INSERT INTO settings(key,value) VALUES('idle_enabled','false')",
            [],
        )
        .unwrap();
        let s = load(&db);
        assert!(!s.idle_enabled);
        assert_eq!(s.target_day_minutes, 480);
    }

    #[test]
    fn validates_ranges_and_saves_only_patched_keys() {
        let mut db = db();
        let base = AppSettings::default();
        for bad in [
            json!({"targetDayMinutes": 1441}),
            json!({"targetDayMinutes": -1}),
            json!({"notifyLongMinutes": 5000}),
            json!({"workDays": [0, 1]}),
            json!({"workDays": [8]}),
            json!({"dayStart": "24:00"}),
            json!({"dayStart": "8:00"}),
            json!({"notifyEvening": "17:60"}),
            json!({"shortcutQuick": ""}),
            json!({"shortcutToggle": "Control+Shift+Space"}),
            json!({"shortcutToggle": "Shift+Ctrl+Space"}),
            json!({"shortcutToggle": "CmdOrCtrl+Shift+Space"}),
            json!({"shortcutQuick": "Control+Shift+Spac"}),
            json!({"shortcutQuick": "Foo+Space"}),
            json!({"idleEnabled": "tak"}),
            json!({"unknown": true}),
            json!([1, 2]),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let ok = apply(
            &base,
            &json!({"targetDayMinutes": 1440, "workDays": [5, 1, 1], "dayStart": "23:59"}),
        )
        .unwrap();
        assert_eq!(ok.work_days, vec![1, 5]);
        assert_eq!(ok.target_day_minutes, 1440);

        assert!(save(&mut db, &json!({"targetDayMinutes": 2000})).is_err());
        let saved = save(
            &mut db,
            &json!({"idleEnabled": false, "dayStart": "07:30", "workDays": [1, 2, 3]}),
        )
        .unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='day_start'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "07:30");
        assert_eq!(load(&db), saved);
        assert!(!saved.idle_enabled);
        assert_eq!(saved.work_days, vec![1, 2, 3]);
    }

    #[test]
    fn corrupt_stored_values_fall_back_to_defaults() {
        let db = db();
        db.execute_batch(
            "INSERT INTO settings(key,value) VALUES('target_day_minutes','abc'),('work_days','[9]'),('notify_evening','x'),('notify_long_minutes','60')",
        )
        .unwrap();
        let s = load(&db);
        assert_eq!(s.target_day_minutes, 480);
        assert_eq!(s.work_days, vec![1, 2, 3, 4, 5]);
        assert_eq!(s.notify_evening, "17:30");
        assert_eq!(s.notify_long_minutes, 60);
    }

    #[test]
    fn shortcuts_saved_together_survive_reload() {
        let mut db = db();
        // quick = domyślny toggle: poprawne tylko razem z nowym toggle.
        let saved = save(
            &mut db,
            &json!({"shortcutQuick": "Control+Alt+Space", "shortcutToggle": "Control+Alt+K"}),
        )
        .unwrap();
        assert_eq!(load(&db), saved);
        let swapped = save(
            &mut db,
            &json!({"shortcutQuick": "Control+Alt+K", "shortcutToggle": "Control+Alt+Space"}),
        )
        .unwrap();
        assert_eq!(load(&db), swapped);
        // Zamiana domyślnych skrótów miejscami.
        let swapped = save(
            &mut db,
            &json!({"shortcutQuick": "Control+Alt+Space", "shortcutToggle": "Control+Shift+Space"}),
        )
        .unwrap();
        assert_eq!(swapped.shortcut_quick, "Control+Alt+Space");
        assert_eq!(load(&db), swapped);
    }

    #[test]
    fn colliding_stored_shortcuts_fall_back_to_defaults() {
        let db = db();
        db.execute_batch(
            "INSERT INTO settings(key,value) VALUES('shortcut_quick','Ctrl+Alt+J'),('shortcut_toggle','Alt+Control+J'),('day_start','09:00')",
        )
        .unwrap();
        let s = load(&db);
        let d = AppSettings::default();
        assert_eq!(s.shortcut_quick, d.shortcut_quick);
        assert_eq!(s.shortcut_toggle, d.shortcut_toggle);
        assert_eq!(s.day_start, "09:00");
    }

    #[test]
    fn altgr_letters_are_rejected() {
        let base = AppSettings::default();
        for letter in ["A", "C", "E", "L", "N", "O", "S", "X", "Z"] {
            for value in [
                format!("Control+Alt+{letter}"),
                format!("Alt+Ctrl+Shift+{letter}"),
            ] {
                assert!(altgr_letter(&value), "{value}");
                assert_eq!(
                    apply(&base, &json!({"shortcutToggle": value})).unwrap_err(),
                    ALTGR_LETTER
                );
            }
        }
        for value in [
            "Control+Alt+Space",
            "Control+Alt+K",
            "Control+Shift+S",
            "Control+Alt+Super+S",
            "Alt+S",
        ] {
            assert!(!altgr_letter(value), "{value}");
        }
        // Zapisany wcześniej skrót-litera wraca do domyślnego przy odczycie.
        let db = db();
        db.execute(
            "INSERT INTO settings(key,value) VALUES('shortcut_toggle','Control+Alt+S')",
            [],
        )
        .unwrap();
        assert_eq!(load(&db).shortcut_toggle, base.shortcut_toggle);
    }
}
