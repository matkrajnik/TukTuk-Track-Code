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
    pub target_day_minutes: u32,
    pub work_days: Vec<u8>,
    pub day_start: String,
    pub widget_enabled: bool,
    pub widget_background_only: bool,
    pub autostart_hidden: bool,
    pub notify_due: bool,
    pub notify_long_minutes: u32,
    pub notify_evening: String,
    pub shortcut_quick: String,
    pub shortcut_toggle: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_enabled: true,
            target_day_minutes: 480,
            work_days: vec![1, 2, 3, 4, 5],
            day_start: "08:00".into(),
            widget_enabled: false,
            widget_background_only: false,
            autostart_hidden: false,
            notify_due: true,
            notify_long_minutes: 240,
            notify_evening: "17:30".into(),
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

fn valid_time(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit())
        && (b[0] - b'0') * 10 + (b[1] - b'0') < 24
        && (b[3] - b'0') * 10 + (b[4] - b'0') < 60
}

fn valid_shortcut(value: &str) -> bool {
    use std::str::FromStr;
    !value.is_empty()
        && value.len() <= 64
        && tauri_plugin_global_shortcut::Shortcut::from_str(value).is_ok()
}

/// Sprawdza zakresy i porządkuje dni robocze (rosnąco, bez powtórzeń).
pub fn validate(mut s: AppSettings) -> Result<AppSettings, String> {
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
    if !valid_shortcut(&s.shortcut_quick) || !valid_shortcut(&s.shortcut_toggle) {
        return Err("Niepoprawny skrót klawiszowy.".into());
    }
    if s.shortcut_quick.eq_ignore_ascii_case(&s.shortcut_toggle) {
        return Err("Skróty szybkiego przełączania i Start/Stop muszą być różne.".into());
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

/// Odczyt z bazy. Brakujące lub uszkodzone wartości dostają wartość domyślną.
pub fn load(db: &Connection) -> AppSettings {
    let mut settings = AppSettings::default();
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
        let mut patch = Map::new();
        patch.insert(field, value);
        if let Ok(next) = apply(&settings, &Value::Object(patch)) {
            settings = next;
        }
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
}
