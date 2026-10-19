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
    /// Proponuj przełączenie timera na zadanie, nad którym pracuje agent AI.
    pub agent_suggest: bool,
    /// Foldery (z podfolderami), dla których nie proponujemy przełączenia.
    pub agent_muted_folders: Vec<String>,
    /// Podpowiadaj zadanie na podstawie aktywnego okna (W3; tytuły tylko w pamięci).
    pub window_suggest: bool,
    /// Rozszerzenie przeglądarki Chrome i Edge (W4): rejestracja hosta native
    /// messaging w HKCU i propozycje ze źródłem „Przeglądarka”.
    pub browser_extension: bool,
    pub target_day_minutes: u32,
    pub work_days: Vec<u8>,
    /// Polskie święta ustawowe nie są dniami roboczymi w celu dnia, tygodnia i miesiąca.
    pub holidays_in_target: bool,
    pub day_start: String,
    pub widget_enabled: bool,
    pub widget_background_only: bool,
    pub autostart_hidden: bool,
    pub notify_due: bool,
    pub notify_long_minutes: u32,
    pub notify_evening: String,
    pub notify_evening_enabled: bool,
    pub notify_target: bool,
    /// Co 5 min dodaje nowe zgłoszenia przypisane w Jirze do „Do zaplanowania”.
    pub jira_auto_import: bool,
    /// Główny wyłącznik zapisów do Jiry (J1, J2): wyłączony blokuje każdą zmianę
    /// zgłoszenia, także w backendzie (`jira::ensure_write`).
    pub jira_write: bool,
    /// Pytaj o przesunięcie zgłoszenia „Do zrobienia” przy starcie pomiaru (J1).
    pub jira_ask_move: bool,
    /// Nazwa preferowanego statusu docelowego przy kilku przejściach „w toku”.
    pub jira_move_status: String,
    /// Prefiks nazwy gałęzi z „Kopiuj nazwę gałęzi” (J2), np. `feature/`.
    pub jira_branch_prefix: String,
    /// Pola Jiry pokazywane na kafelku zadania, karcie tygodnia i kartach widoku
    /// Jira (identyfikatory, np. `labels`, `assignee`, `customfield_10020`),
    /// w kolejności wyświetlania; najwyżej [`MAX_CARD_FIELDS`].
    pub jira_card_fields: Vec<String>,
    pub shortcut_quick: String,
    pub shortcut_toggle: String,
    /// Długość bloku skupienia w minutach (1–240, domyślnie 25).
    pub focus_block_minutes: u32,
    /// Długość przerwy po bloku w minutach (1–60, domyślnie 5).
    pub focus_break_minutes: u32,
    /// Przerwa zatrzymuje pomiar (domyślnie tak); inaczej pomiar trwa przez przerwę.
    pub focus_break_stops: bool,
    /// Tytuły spotkań z kalendarza w interfejsie; wyłączone = „Spotkanie”.
    pub calendar_show_titles: bool,
    /// Szacunki z poprawką z historii (P1) w pojemności, osi planu i „Rozłóż”.
    pub plan_with_correction: bool,
    /// Powiadomienie „Przegląd dnia” w dzień roboczy po początku dnia (P2).
    pub notify_morning_review: bool,
    /// Dzień ostatniego porannego przeglądu (`RRRR-MM-DD`, pusty = nigdy).
    pub morning_review_day: String,
    /// Chwila ostatniego „Zaczynam dzień” (Unix ms, 0 = nigdy): od niej liczą się
    /// „Nowe z Jiry” w następnym przeglądzie.
    pub morning_review_at: i64,
    /// Powiadomienie „Czekasz na…” po tylu dniach roboczych blokady (0 wyłącza, P3).
    pub waiting_notify_days: u32,
    /// Statusy Jiry oznaczające czekanie (propozycje „Czekam na” na kafelku).
    pub waiting_jira_statuses: Vec<String>,
    /// Powiadomienie „Przegląd tygodnia” w ostatni dzień roboczy tygodnia (W2).
    pub notify_week_review: bool,
    /// Godzina powiadomienia „Przegląd tygodnia” (`GG:MM`, domyślnie 15:00).
    pub week_review_time: String,
    /// Główny przełącznik wysyłki Raportu TS do Ideo Timesheet (PLAN_1.8.md):
    /// wyłączony chowa przyciski i blokuje `timesheet::ts_send` w backendzie.
    pub ts_send: bool,
}

/// Domyślne statusy Jiry oznaczające czekanie.
pub fn default_waiting_statuses() -> Vec<String> {
    [
        "Code review",
        "Review",
        "Blocked",
        "Waiting for customer",
        "Oczekuje",
    ]
    .map(String::from)
    .to_vec()
}

/// Domyślne pola Jiry na kafelku: etykiety i przypisany.
pub fn default_card_fields() -> Vec<String> {
    vec!["labels".into(), "assignee".into()]
}
/// Najwięcej pól Jiry na kafelku.
pub const MAX_CARD_FIELDS: usize = 6;
/// Komunikat dla niepoprawnej listy pól Jiry.
pub const CARD_FIELDS_ERROR: &str =
    "Pola Jiry na kafelku: najwyżej 6 identyfikatorów pól (np. labels, customfield_10020), bez summary i status.";

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_enabled: true,
            idle_agents: true,
            agent_suggest: true,
            agent_muted_folders: Vec::new(),
            window_suggest: false,
            browser_extension: false,
            target_day_minutes: 480,
            work_days: vec![1, 2, 3, 4, 5],
            holidays_in_target: true,
            day_start: "08:00".into(),
            widget_enabled: false,
            widget_background_only: false,
            autostart_hidden: false,
            notify_due: true,
            notify_long_minutes: 240,
            notify_evening: "17:30".into(),
            notify_evening_enabled: true,
            notify_target: false,
            jira_auto_import: true,
            jira_write: true,
            jira_ask_move: true,
            jira_move_status: "In Progress".into(),
            jira_branch_prefix: "feature/".into(),
            jira_card_fields: default_card_fields(),
            shortcut_quick: "Control+Shift+Space".into(),
            shortcut_toggle: "Control+Alt+Space".into(),
            focus_block_minutes: 25,
            focus_break_minutes: 5,
            focus_break_stops: true,
            calendar_show_titles: true,
            plan_with_correction: false,
            notify_morning_review: true,
            morning_review_day: String::new(),
            morning_review_at: 0,
            waiting_notify_days: 3,
            waiting_jira_statuses: default_waiting_statuses(),
            notify_week_review: true,
            week_review_time: "15:00".into(),
            ts_send: true,
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

/// Dzień `RRRR-MM-DD` istniejący w kalendarzu.
pub fn valid_day(value: &str) -> bool {
    value.len() == 10 && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
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

/// Komunikat dla niepoprawnego prefiksu gałęzi.
pub const BRANCH_PREFIX_ERROR: &str =
    "Prefiks gałęzi może mieć najwyżej 40 znaków: litery łacińskie, cyfry, „/”, „-”, „_” i „.”.";

/// Prefiks gałęzi: pusty albo do 40 znaków ASCII bezpiecznych w nazwie gałęzi git.
pub fn valid_branch_prefix(value: &str) -> bool {
    value.len() <= 40
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
        && !value.contains("..")
        && !value.starts_with(['/', '.', '-'])
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
    if !(1..=240).contains(&s.focus_block_minutes) {
        return Err("Blok skupienia musi mieć od 1 do 240 minut.".into());
    }
    if !(1..=60).contains(&s.focus_break_minutes) {
        return Err("Przerwa musi mieć od 1 do 60 minut.".into());
    }
    if s.work_days.iter().any(|d| !(1..=7).contains(d)) {
        return Err("Dni robocze muszą być liczbami od 1 do 7.".into());
    }
    s.work_days.sort_unstable();
    s.work_days.dedup();
    // Wyciszone foldery: bez pustych, bez powtórek (ten sam folder innymi
    // separatorami albo wielkością liter), w kolejności dodania.
    let mut folders: Vec<String> = Vec::new();
    for folder in s.agent_muted_folders.iter().map(|f| f.trim()) {
        if folder.is_empty() || folder.chars().count() > 1024 {
            return Err("Niepoprawny folder agenta.".into());
        }
        let norm = crate::agents::norm_folder(folder);
        if !folders
            .iter()
            .any(|f| crate::agents::norm_folder(f) == norm)
        {
            folders.push(folder.to_string());
        }
    }
    if folders.len() > 200 {
        return Err("Za dużo wyciszonych folderów agentów.".into());
    }
    s.agent_muted_folders = folders;
    if s.waiting_notify_days > 30 {
        return Err("Przypomnienie o blokadzie musi mieć od 0 do 30 dni roboczych.".into());
    }
    // Statusy czekania: bez pustych i bez powtórek (wielkość liter bez znaczenia).
    let mut statuses: Vec<String> = Vec::new();
    for status in s.waiting_jira_statuses.iter().map(|v| v.trim()) {
        if status.chars().count() > 80 {
            return Err("Status Jiry może mieć najwyżej 80 znaków.".into());
        }
        if !status.is_empty()
            && !statuses
                .iter()
                .any(|v| v.to_lowercase() == status.to_lowercase())
        {
            statuses.push(status.to_string());
        }
    }
    if statuses.len() > 50 {
        return Err("Za dużo statusów Jiry oznaczających czekanie.".into());
    }
    s.waiting_jira_statuses = statuses;
    s.jira_move_status = s.jira_move_status.trim().to_string();
    if s.jira_move_status.is_empty() || s.jira_move_status.chars().count() > 80 {
        return Err("Nazwa statusu w Jirze musi mieć od 1 do 80 znaków.".into());
    }
    s.jira_branch_prefix = s.jira_branch_prefix.trim().to_string();
    if !valid_branch_prefix(&s.jira_branch_prefix) {
        return Err(BRANCH_PREFIX_ERROR.into());
    }
    // Pola Jiry na kafelku: poprawne identyfikatory, bez powtórek, bez pól
    // pokazywanych zawsze (tytuł i status).
    let mut fields: Vec<String> = Vec::new();
    for id in s.jira_card_fields.iter().map(|v| v.trim()) {
        if !crate::jira_fields::valid_field_id(id) || matches!(id, "summary" | "status") {
            return Err(CARD_FIELDS_ERROR.into());
        }
        if !fields.iter().any(|f| f == id) {
            fields.push(id.to_string());
        }
    }
    if fields.len() > MAX_CARD_FIELDS {
        return Err(CARD_FIELDS_ERROR.into());
    }
    s.jira_card_fields = fields;
    if !valid_time(&s.day_start) {
        return Err("Początek dnia musi mieć format GG:MM.".into());
    }
    if !(s.morning_review_day.is_empty() || valid_day(&s.morning_review_day)) {
        return Err("Dzień przeglądu musi mieć format RRRR-MM-DD.".into());
    }
    if s.morning_review_at < 0 {
        return Err("Niepoprawna chwila przeglądu dnia.".into());
    }
    if !valid_time(&s.week_review_time) {
        return Err("Godzina przeglądu tygodnia musi mieć format GG:MM.".into());
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
    // Wyłącznik propozycji agentów i wyciszone foldery działają od razu.
    crate::suggest::refresh(&app, crate::db::now());
    // Rozszerzenie przeglądarki: wpisy hosta w rejestrze przy zmianie wyłącznika.
    crate::browser::sync(&app, &result);
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
    fn ts_send_defaults_to_on_and_is_saved() {
        let mut db = db();
        assert!(load(&db).ts_send);
        let saved = save(&mut db, &json!({"tsSend": false})).unwrap();
        assert!(!saved.ts_send);
        let raw: String = db
            .query_row("SELECT value FROM settings WHERE key='ts_send'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(raw, "false");
        assert!(!load(&db).ts_send);
        assert!(save(&mut db, &json!({"tsSend": "tak"})).is_err());
        assert!(save(&mut db, &json!({"tsSend": true})).unwrap().ts_send);
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
    fn agent_muted_folders_are_cleaned_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        assert!(base.agent_suggest);
        assert!(base.agent_muted_folders.is_empty());
        assert!(apply(&base, &json!({"agentMutedFolders": ["  "]})).is_err());
        assert!(apply(&base, &json!({"agentMutedFolders": "C:\\x"})).is_err());
        let saved = save(
            &mut db,
            &json!({"agentMutedFolders": [" C:\\Projekty\\wms ", "c:/projekty/WMS/", "D:\\api"], "agentSuggest": false}),
        )
        .unwrap();
        assert_eq!(
            saved.agent_muted_folders,
            vec!["C:\\Projekty\\wms".to_string(), "D:\\api".to_string()]
        );
        assert_eq!(load(&db), saved);
        assert!(!load(&db).agent_suggest);
    }

    #[test]
    fn focus_lengths_are_validated_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        for bad in [
            json!({"focusBlockMinutes": 0}),
            json!({"focusBlockMinutes": 241}),
            json!({"focusBreakMinutes": 0}),
            json!({"focusBreakMinutes": 61}),
            json!({"focusBreakStops": "nie"}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let saved = save(
            &mut db,
            &json!({"focusBlockMinutes": 50, "focusBreakMinutes": 10, "focusBreakStops": false}),
        )
        .unwrap();
        assert_eq!(load(&db), saved);
        assert_eq!(
            (
                saved.focus_block_minutes,
                saved.focus_break_minutes,
                saved.focus_break_stops
            ),
            (50, 10, false)
        );
        // Uszkodzona zapisana wartość wraca do domyślnej.
        db.execute(
            "UPDATE settings SET value='0' WHERE key='focus_block_minutes'",
            [],
        )
        .unwrap();
        assert_eq!(load(&db).focus_block_minutes, 25);
    }

    #[test]
    fn morning_review_fields_are_validated_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        assert!(base.notify_morning_review);
        assert_eq!(base.morning_review_day, "");
        assert_eq!(base.morning_review_at, 0);
        for bad in [
            json!({"morningReviewDay": "2026-02-30"}),
            json!({"morningReviewDay": "15.10.2026"}),
            json!({"morningReviewDay": "2026-10-15x"}),
            json!({"morningReviewAt": -1}),
            json!({"notifyMorningReview": "tak"}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let saved = save(
            &mut db,
            &json!({"morningReviewDay": "2026-10-15", "morningReviewAt": 1_792_058_400_000_i64, "notifyMorningReview": false}),
        )
        .unwrap();
        assert_eq!(load(&db), saved);
        assert_eq!(saved.morning_review_day, "2026-10-15");
        assert_eq!(saved.morning_review_at, 1_792_058_400_000);
        assert!(!saved.notify_morning_review);
        // Uszkodzony zapisany dzień wraca do pustego.
        db.execute(
            "UPDATE settings SET value='wczoraj' WHERE key='morning_review_day'",
            [],
        )
        .unwrap();
        assert_eq!(load(&db).morning_review_day, "");
    }

    #[test]
    fn week_review_fields_are_validated_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        assert!(base.notify_week_review);
        assert_eq!(base.week_review_time, "15:00");
        for bad in [
            json!({"weekReviewTime": "24:00"}),
            json!({"weekReviewTime": "9:00"}),
            json!({"weekReviewTime": ""}),
            json!({"notifyWeekReview": "tak"}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let saved = save(
            &mut db,
            &json!({"weekReviewTime": "16:30", "notifyWeekReview": false}),
        )
        .unwrap();
        assert_eq!(load(&db), saved);
        assert_eq!(saved.week_review_time, "16:30");
        assert!(!saved.notify_week_review);
        // Uszkodzona zapisana godzina wraca do domyślnej.
        db.execute(
            "UPDATE settings SET value='po obiedzie' WHERE key='week_review_time'",
            [],
        )
        .unwrap();
        assert_eq!(load(&db).week_review_time, "15:00");
    }

    #[test]
    fn jira_card_fields_are_validated_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        assert_eq!(base.jira_card_fields, vec!["labels", "assignee"]);
        for bad in [
            json!({"jiraCardFields": ["summary"]}),
            json!({"jiraCardFields": ["status"]}),
            json!({"jiraCardFields": [""]}),
            json!({"jiraCardFields": ["custom field"]}),
            json!({"jiraCardFields": ["1abc"]}),
            json!({"jiraCardFields": ["a", "b", "c", "d", "e", "f", "g"]}),
            json!({"jiraCardFields": "labels"}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let saved = save(
            &mut db,
            &json!({"jiraCardFields": [" customfield_10020 ", "labels", "customfield_10020", "assignee"]}),
        )
        .unwrap();
        assert_eq!(
            saved.jira_card_fields,
            vec!["customfield_10020", "labels", "assignee"]
        );
        assert_eq!(load(&db), saved);
        let none = save(&mut db, &json!({"jiraCardFields": []})).unwrap();
        assert!(none.jira_card_fields.is_empty());
        assert!(load(&db).jira_card_fields.is_empty());
        // Uszkodzona zapisana lista wraca do domyślnej.
        db.execute(
            "UPDATE settings SET value='[\"summary\"]' WHERE key='jira_card_fields'",
            [],
        )
        .unwrap();
        assert_eq!(load(&db).jira_card_fields, vec!["labels", "assignee"]);
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
