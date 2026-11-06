//! Ustawienia aplikacji: jedna struktura, zapisywana klucz po kluczu w tabeli `settings`.
use crate::Database;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Mutex;
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
    /// Hooki agentów (B8): wpisy w `~/.claude/settings.json` i `~/.codex/hooks.json`,
    /// z których aplikacja wie, że agent pracuje albo czeka na zgodę.
    pub agent_hooks: bool,
    /// Zewnętrzne sterowanie pomiarem (`api.rs`): potok dla innych programów
    /// i `tuktuk-track-ctl.exe`; domyślnie wyłączone.
    pub external_api: bool,
    /// Ścieżka do `orca.cmd` (B9); pusta = wykrywana (katalog Programs\orca w LOCALAPPDATA).
    pub orca_path: String,
    /// Uruchamiaj Orkę, gdy nie działa (B9); domyślnie wyłączone.
    pub orca_autostart: bool,
    pub target_day_minutes: u32,
    pub work_days: Vec<u8>,
    /// Polskie święta ustawowe nie są dniami roboczymi w celu dnia, tygodnia i miesiąca.
    pub holidays_in_target: bool,
    pub day_start: String,
    pub widget_enabled: bool,
    pub widget_background_only: bool,
    /// Malutki widget (C2): tylko czas, pełny widok po najechaniu myszą.
    pub widget_compact: bool,
    pub autostart_hidden: bool,
    pub notify_due: bool,
    pub notify_long_minutes: u32,
    pub notify_evening: String,
    pub notify_evening_enabled: bool,
    pub notify_target: bool,
    /// Powiadomienie, gdy sesja agenta czeka na użytkownika (zgoda, pytanie).
    pub agent_notify_waiting: bool,
    /// Powiadomienie, gdy sesja agenta skończy turę (domyślnie wyłączone).
    pub agent_notify_done: bool,
    /// Przypomnienie przed każdym spotkaniem (C3); nadpisywane per spotkanie.
    pub meeting_reminder_enabled: bool,
    /// Ile minut przed spotkaniem (1, 5, 10, 15 albo 30).
    pub meeting_reminder_minutes: u32,
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
    /// Chwila ostatniego „Rozpocznij dzień” (Unix ms, 0 = nigdy): od niej liczą się
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
    /// Sprawdzaj aktualizacje przy starcie i raz dziennie (`src/updater.ts`);
    /// wyłączone zostawia tylko „Sprawdź teraz” w Ustawieniach.
    pub auto_update_check: bool,
    /// Przy pierwszym uruchomieniu albo pokazaniu okna w dniu roboczym bez
    /// zapisanego przeglądu dnia główne okno otwiera się na Plan i historia →
    /// Rano (B11). Stan „pokazano dziś” trzyma okno (`localStorage`), nie baza.
    pub open_morning_on_start: bool,
    /// Główny przełącznik wysyłki Raportu TS do Ideo Timesheet (PLAN_1.8.md):
    /// wyłączony chowa przyciski i blokuje `timesheet::ts_send` w backendzie.
    pub ts_send: bool,
    /// Statusy Redmine oznaczające czekanie (PLAN_1.9.md, R2; pole w
    /// Ustawieniach → Redmine od R5). Osobna lista od `waiting_jira_statuses`.
    pub waiting_redmine_statuses: Vec<String>,
    /// Co 5 min dodaje nowe zgłoszenia przypisane w Redmine do „Do zaplanowania”
    /// (PLAN_1.9.md, R3; przełącznik w Ustawieniach → Redmine od R5). Niezależny
    /// od `jira_auto_import`.
    pub redmine_auto_import: bool,
    /// Główny wyłącznik zapisów do Redmine (PLAN_1.9.md, R4; przełącznik w
    /// Ustawieniach → Redmine od R5): wyłączony blokuje każdą zmianę zgłoszenia,
    /// także w backendzie (`redmine::ensure_write`). Niezależny od `jira_write`.
    pub redmine_write: bool,
    /// Pytaj o przesunięcie zgłoszenia Redmine w statusie początkowym przy
    /// starcie pomiaru (R4).
    pub redmine_ask_move: bool,
    /// Nazwa statusu Redmine „w toku” — cel pytania przy starcie pracy.
    pub redmine_move_status: String,
    /// Statusy Redmine oznaczające „Do zrobienia” (Redmine nie ma kategorii
    /// statusów): tylko z nich aplikacja pyta o przesunięcie.
    pub redmine_new_statuses: Vec<String>,
    /// Prefiks nazwy gałęzi z „Kopiuj nazwę gałęzi” dla zgłoszeń Redmine.
    pub redmine_branch_prefix: String,
    /// Nazwy (jedna osoba albo słowo na pozycję), które filtr danych osobowych
    /// (`ai_privacy`, A0) zawsze ukrywa przed modelem; pole w Ustawieniach →
    /// Asystent AI dochodzi w A1.
    pub ai_hidden_names: Vec<String>,
    /// Wybrany silnik asystenta AI (A1): `""` = jeszcze nie pytano, `claude`,
    /// `codex` albo `off`.
    pub ai_engine: String,
    /// Ręczna ścieżka do `claude` (ostatnie miejsce wykrywania); pusta = brak.
    pub ai_claude_path: String,
    /// Ręczna ścieżka do `codex` (ostatnie miejsce wykrywania); pusta = brak.
    pub ai_codex_path: String,
    /// Model „szybki” (dopasowania, pytania o dane); pusty = domyślny model CLI.
    pub ai_model_fast: String,
    /// Model „główny” (Zamknięcie dnia, dłuższe rozmowy); pusty = domyślny model CLI.
    pub ai_model_main: String,
    /// Tytuły spotkań mogą wychodzić do modelu (po filtrze); domyślnie nie.
    pub ai_send_meeting_titles: bool,
    /// Tytuły okien mogą wychodzić do modelu (po filtrze); domyślnie nie.
    pub ai_send_window_titles: bool,
    /// Serwer MCP dla agentów (A2); działa tylko przy wybranym silniku.
    pub ai_mcp: bool,
    /// Czat w aplikacji (A3); działa tylko przy wybranym silniku.
    pub ai_chat: bool,
    /// Podpowiedzi przypisań (A4); działa tylko przy wybranym silniku.
    pub ai_assign_suggest: bool,
    /// Asystent w Zamknięciu dnia (A5); działa tylko przy wybranym silniku.
    pub ai_day_close: bool,
    /// Asystent w Rano i przeglądzie tygodnia (A6); działa tylko przy wybranym silniku.
    pub ai_plan_assist: bool,
    /// Ile dni trzymać diffy zakończonych przebiegów Workflows (E6, 1–365,
    /// domyślnie 30); starsze usuwa retencja przy starcie, historia zostaje.
    pub workflow_retention_days: u32,
}
/// Zakres `workflow_retention_days` (dni). Co najmniej tydzień: krótsza retencja
/// mogłaby usunąć diff świeżo zakończonego przebiegu w trakcie jego przeglądu.
pub const WORKFLOW_RETENTION_DAYS: std::ops::RangeInclusive<u32> = 7..=365;

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
/// Domyślne statusy Redmine oznaczające czekanie.
pub fn default_waiting_redmine_statuses() -> Vec<String> {
    vec!["Feedback".into()]
}
/// Domyślne statusy początkowe Redmine (pytanie o przesunięcie przy starcie pracy).
pub fn default_redmine_new_statuses() -> Vec<String> {
    vec!["New".into(), "Nowy".into()]
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
            agent_hooks: true,
            external_api: false,
            orca_path: String::new(),
            orca_autostart: false,
            target_day_minutes: 480,
            work_days: vec![1, 2, 3, 4, 5],
            holidays_in_target: true,
            day_start: "08:00".into(),
            widget_enabled: false,
            widget_background_only: false,
            widget_compact: false,
            autostart_hidden: false,
            notify_due: true,
            notify_long_minutes: 240,
            notify_evening: "17:30".into(),
            notify_evening_enabled: true,
            notify_target: false,
            agent_notify_waiting: true,
            agent_notify_done: false,
            meeting_reminder_enabled: true,
            meeting_reminder_minutes: 5,
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
            auto_update_check: true,
            open_morning_on_start: true,
            ts_send: true,
            waiting_redmine_statuses: default_waiting_redmine_statuses(),
            redmine_auto_import: true,
            redmine_write: true,
            redmine_ask_move: true,
            redmine_move_status: "In Progress".into(),
            redmine_new_statuses: default_redmine_new_statuses(),
            redmine_branch_prefix: "feature/".into(),
            ai_hidden_names: Vec::new(),
            ai_engine: String::new(),
            ai_claude_path: String::new(),
            ai_codex_path: String::new(),
            ai_model_fast: String::new(),
            ai_model_main: String::new(),
            ai_send_meeting_titles: false,
            ai_send_window_titles: false,
            ai_mcp: true,
            ai_chat: true,
            ai_assign_suggest: true,
            ai_day_close: true,
            ai_plan_assist: true,
            workflow_retention_days: 30,
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

/// Lista statusów oznaczających czekanie (Jira albo Redmine): bez pustych
/// i bez powtórek (wielkość liter bez znaczenia), najwyżej 50 nazw po 80 znaków.
/// `system` to dopełniacz w komunikacie („Jiry”, „Redmine”).
fn clean_statuses(list: &[String], system: &str) -> Result<Vec<String>, String> {
    clean_list(list, system, "oznaczających czekanie")
}
/// Lista nazw statusów jak [`clean_statuses`]; `purpose` kończy komunikat o
/// zbyt długiej liście.
fn clean_list(list: &[String], system: &str, purpose: &str) -> Result<Vec<String>, String> {
    let mut statuses: Vec<String> = Vec::new();
    for status in list.iter().map(|v| v.trim()) {
        if status.chars().count() > 80 {
            return Err(format!("Status {system} może mieć najwyżej 80 znaków."));
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
        return Err(format!("Za dużo statusów {system} {purpose}."));
    }
    Ok(statuses)
}

/// Komunikat dla niepoprawnej listy ukrywanych nazw.
pub const HIDDEN_NAMES_ERROR: &str =
    "Ukrywane nazwy: najwyżej 200 pozycji po 80 znaków, bez znaków sterujących.";
/// Lista „Ukrywaj te nazwy” (A0): przycięte pozycje, bez pustych i bez powtórek
/// (wielkość liter bez znaczenia), w kolejności dodania.
fn clean_hidden_names(list: &[String]) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = Vec::new();
    for name in list.iter().map(|v| v.trim()) {
        if name.chars().count() > 80 || name.chars().any(|c| c.is_control()) {
            return Err(HIDDEN_NAMES_ERROR.into());
        }
        if !name.is_empty()
            && !names
                .iter()
                .any(|v| v.to_lowercase() == name.to_lowercase())
        {
            names.push(name.to_string());
        }
    }
    if names.len() > 200 {
        return Err(HIDDEN_NAMES_ERROR.into());
    }
    Ok(names)
}

/// Komunikat dla niepoprawnego silnika AI.
pub const AI_ENGINE_ERROR: &str = "Silnik asystenta AI: claude, codex albo off.";
/// Komunikat dla niepoprawnej ścieżki do claude albo codex.
pub const AI_PATH_ERROR: &str = "Ścieżka do claude i codex musi wskazywać plik .exe, .cmd albo .bat (do 1024 znaków, bez cudzysłowów).";
/// Komunikat dla niepoprawnej nazwy modelu AI.
pub const AI_MODEL_ERROR: &str =
    "Nazwa modelu AI: do 80 znaków (litery łacińskie, cyfry oraz . _ : / [ ] -).";
/// Pusta ścieżka (brak) albo plik `.exe` / `.cmd` / `.bat` bez cudzysłowów, `%` i znaków sterujących.
fn valid_ai_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    let lower = path.to_lowercase();
    path.chars().count() <= 1024
        && (lower.ends_with(".exe") || lower.ends_with(".cmd") || lower.ends_with(".bat"))
        && !path.chars().any(|c| c.is_control() || c == '"' || c == '%')
}
/// Pusta nazwa (domyślny model CLI) albo krótka nazwa bez spacji i znaków powłoki.
fn valid_ai_model(model: &str) -> bool {
    model.chars().count() <= 80
        && model.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '[' | ']' | '-')
        })
}

/// Pełna walidacja: pola osobno i reguły łączące pola.
pub fn validate(s: AppSettings) -> Result<AppSettings, String> {
    let s = validate_fields(s)?;
    if same_shortcut(&s.shortcut_quick, &s.shortcut_toggle) {
        return Err("Skróty szybkiego przełączania i Start/Stop muszą być różne.".into());
    }
    Ok(s)
}

/// Komunikat dla niepoprawnej ścieżki do orca.cmd.
pub const ORCA_PATH_ERROR: &str =
    "Ścieżka do orca.cmd musi wskazywać plik .cmd albo .exe (do 1024 znaków, bez cudzysłowów).";
/// Pusta ścieżka (wykrywanie) albo plik `.cmd` / `.exe` bez cudzysłowów i znaków sterujących.
fn valid_orca_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    let lower = path.to_lowercase();
    path.chars().count() <= 1024
        && (lower.ends_with(".cmd") || lower.ends_with(".exe"))
        && !path.chars().any(|c| c.is_control() || c == '"' || c == '%')
}

/// Sprawdza zakresy każdego pola osobno i porządkuje dni robocze (rosnąco, bez powtórzeń).
fn validate_fields(mut s: AppSettings) -> Result<AppSettings, String> {
    if s.target_day_minutes > 1440 {
        return Err("Cel dzienny musi mieć od 0 do 1440 minut.".into());
    }
    if s.notify_long_minutes > 1440 {
        return Err("Próg długiego pomiaru musi mieć od 0 do 1440 minut.".into());
    }
    if !crate::reminders::MEETING_MINUTES.contains(&s.meeting_reminder_minutes) {
        return Err("Przypomnienie przed spotkaniem: 1, 5, 10, 15 albo 30 minut.".into());
    }
    if !(1..=240).contains(&s.focus_block_minutes) {
        return Err("Blok skupienia musi mieć od 1 do 240 minut.".into());
    }
    if !(1..=60).contains(&s.focus_break_minutes) {
        return Err("Przerwa musi mieć od 1 do 60 minut.".into());
    }
    if !WORKFLOW_RETENTION_DAYS.contains(&s.workflow_retention_days) {
        return Err("Przechowywanie diffów Workflows: od 7 do 365 dni.".into());
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
    s.orca_path = s.orca_path.trim().to_string();
    if !valid_orca_path(&s.orca_path) {
        return Err(ORCA_PATH_ERROR.into());
    }
    if s.waiting_notify_days > 30 {
        return Err("Przypomnienie o blokadzie musi mieć od 0 do 30 dni roboczych.".into());
    }
    // Statusy czekania: bez pustych i bez powtórek (wielkość liter bez znaczenia).
    s.waiting_jira_statuses = clean_statuses(&s.waiting_jira_statuses, "Jiry")?;
    s.waiting_redmine_statuses = clean_statuses(&s.waiting_redmine_statuses, "Redmine")?;
    s.jira_move_status = s.jira_move_status.trim().to_string();
    if s.jira_move_status.is_empty() || s.jira_move_status.chars().count() > 80 {
        return Err("Nazwa statusu w Jirze musi mieć od 1 do 80 znaków.".into());
    }
    s.jira_branch_prefix = s.jira_branch_prefix.trim().to_string();
    if !valid_branch_prefix(&s.jira_branch_prefix) {
        return Err(BRANCH_PREFIX_ERROR.into());
    }
    s.redmine_new_statuses = clean_list(&s.redmine_new_statuses, "Redmine", "początkowych")?;
    s.redmine_move_status = s.redmine_move_status.trim().to_string();
    if s.redmine_move_status.is_empty() || s.redmine_move_status.chars().count() > 80 {
        return Err("Nazwa statusu w Redmine musi mieć od 1 do 80 znaków.".into());
    }
    s.redmine_branch_prefix = s.redmine_branch_prefix.trim().to_string();
    if !valid_branch_prefix(&s.redmine_branch_prefix) {
        return Err(BRANCH_PREFIX_ERROR.into());
    }
    s.ai_hidden_names = clean_hidden_names(&s.ai_hidden_names)?;
    if !matches!(s.ai_engine.as_str(), "" | "claude" | "codex" | "off") {
        return Err(AI_ENGINE_ERROR.into());
    }
    s.ai_claude_path = s.ai_claude_path.trim().to_string();
    s.ai_codex_path = s.ai_codex_path.trim().to_string();
    if !valid_ai_path(&s.ai_claude_path) || !valid_ai_path(&s.ai_codex_path) {
        return Err(AI_PATH_ERROR.into());
    }
    s.ai_model_fast = s.ai_model_fast.trim().to_string();
    s.ai_model_main = s.ai_model_main.trim().to_string();
    if !valid_ai_model(&s.ai_model_fast) || !valid_ai_model(&s.ai_model_main) {
        return Err(AI_MODEL_ERROR.into());
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

/// Wszystkie wiersze tabeli `settings` jednym zapytaniem (B6); błąd odczytu
/// daje pustą mapę, czyli wartości domyślne.
fn stored(db: &Connection) -> HashMap<String, String> {
    db.prepare("SELECT key, value FROM settings")
        .and_then(|mut statement| {
            statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect()
        })
        .unwrap_or_default()
}

/// Odczyt z bazy. Najpierw zbiera wszystkie zapisane wartości (brakujące lub uszkodzone
/// pole dostaje wartość domyślną), potem raz sprawdza reguły łączące pola. Gdy skróty
/// kolidują, oba wracają do domyślnych. Jeden `SELECT` na całą tabelę (B6);
/// w aplikacji odczyt idzie przez cache (`current`, `cached`).
pub fn load(db: &Connection) -> AppSettings {
    let defaults = AppSettings::default();
    let mut merged = fields();
    let mut stored = stored(db);
    for (field, default) in fields() {
        let Some(raw) = stored.remove(&db_key(&field)) else {
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

/// Cache ustawień w `AppHandle` (B6): `load` waliduje każde pole osobno, a
/// ustawienia czytają wątki co 2 s (bezczynność), co 5 s (tray), co 30 s
/// (powiadomienia) i komendy. Cache unieważniają zapis ustawień (`set_settings`,
/// `autostart_set`, przez `save_cached`) i przywrócenie kopii (`backup::replace`).
///
/// Kolejność blokad: najpierw baza (`Database`), potem cache. Wypełnienie i
/// zapis cache odbywają się przy zablokowanej bazie, więc nieaktualna wartość
/// odczytana przed zapisem nie może nadpisać nowszej.
#[derive(Default)]
pub struct Cache(Mutex<Option<AppSettings>>);

impl Cache {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<AppSettings>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Ustawienia z cache bez dotykania bazy (`None`, gdy cache jest pusty).
    pub fn peek(&self) -> Option<AppSettings> {
        self.slot().clone()
    }
    /// Ustawienia z cache albo z `db` (wtedy zapamiętane). Wołać przy
    /// zablokowanej bazie.
    pub fn get(&self, db: &Connection) -> AppSettings {
        self.slot().get_or_insert_with(|| load(db)).clone()
    }
    /// Zapis jak `save`; po sukcesie cache ma nowe ustawienia, po błędzie jest pusty.
    pub fn save(&self, db: &mut Connection, patch: &Value) -> Result<AppSettings, String> {
        let result = save(db, patch);
        *self.slot() = result.as_ref().ok().cloned();
        result
    }
    /// Następny odczyt pójdzie do bazy (np. po przywróceniu kopii).
    pub fn invalidate(&self) {
        *self.slot() = None;
    }
}

/// Ustawienia przy już zablokowanej bazie: z cache aplikacji albo z `db`.
pub fn cached(app: &tauri::AppHandle, db: &Connection) -> AppSettings {
    match app.try_state::<Cache>() {
        Some(cache) => cache.get(db),
        None => load(db),
    }
}

/// Bieżące ustawienia: z cache bez blokady bazy, a przy pustym cache z bazy.
/// Nie wołać przy zablokowanej bazie (`cached`).
pub fn current(app: &tauri::AppHandle) -> AppSettings {
    if let Some(settings) = app.try_state::<Cache>().and_then(|c| c.peek()) {
        return settings;
    }
    app.state::<Database>()
        .0
        .lock()
        .map(|db| cached(app, &db))
        .unwrap_or_default()
}

/// `save` z aktualizacją cache; wołać przy zablokowanej bazie.
pub fn save_cached(
    app: &tauri::AppHandle,
    db: &mut Connection,
    patch: &Value,
) -> Result<AppSettings, String> {
    match app.try_state::<Cache>() {
        Some(cache) => cache.save(db, patch),
        None => save(db, patch),
    }
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    Ok(current(&app))
}

#[tauri::command]
pub fn set_settings(app: tauri::AppHandle, patch: Value) -> Result<AppSettings, String> {
    let before = current(&app);
    let hooks_before = before.agent_hooks;
    let result = {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        save_cached(&app, &mut db, &patch)?
    };
    if !result.idle_enabled {
        crate::idle::clear(&app);
    }
    let _ = app.emit("settings-changed", result.clone());
    // Wyłącznik propozycji agentów i wyciszone foldery działają od razu.
    crate::suggest::refresh(&app, crate::db::now());
    // Rozszerzenie przeglądarki: wpisy hosta w rejestrze przy zmianie wyłącznika.
    crate::browser::sync(&app, &result);
    // Hooki agentów: wpisy w plikach Claude Code i Codexa zgodne z wyłącznikiem,
    // tylko przy jego zmianie (pozostałe przypadki ponawia pętla agentów co minutę).
    if result.agent_hooks != hooks_before {
        crate::hooks::sync(&app);
    }
    // Serwer MCP (A2): rejestracja w Claude Code i Codexie zgodna z `ai_mcp` i silnikiem.
    if result.ai_mcp != before.ai_mcp || result.ai_engine != before.ai_engine {
        crate::mcp::sync(&app);
    }
    // Asystent przypisań (A4): pierwszy prompt sesji zapamiętywany tylko przy
    // włączonym przełączniku i wybranym silniku; wyłączenie czyści pamięć.
    if result.ai_assign_suggest != before.ai_assign_suggest || result.ai_engine != before.ai_engine
    {
        if let Some(agents) = app.try_state::<crate::agents::State>() {
            agents.set_keep_prompts(crate::assign::keep_prompts(&result));
        }
        crate::agents::rematch(&app);
    }
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
    fn cache_reads_once_and_follows_saves_until_invalidated() {
        let mut db = db();
        let cache = Cache::default();
        assert!(cache.peek().is_none());
        assert_eq!(cache.get(&db), AppSettings::default());
        // Zapis poza cache (np. kopia albo ręczna zmiana bazy) nie jest widoczny,
        // dopóki cache nie zostanie unieważniony: odczyt nie dotyka bazy.
        save(&mut db, &json!({"targetDayMinutes": 360})).unwrap();
        assert_eq!(cache.get(&db).target_day_minutes, 480);
        cache.invalidate();
        assert_eq!(cache.get(&db).target_day_minutes, 360);
        // `set_settings` zapisuje przez cache: nowa wartość od razu, bez odczytu.
        let saved = cache
            .save(&mut db, &json!({"targetDayMinutes": 300}))
            .unwrap();
        assert_eq!(cache.peek(), Some(saved));
        assert_eq!(cache.get(&db).target_day_minutes, 300);
        assert_eq!(load(&db).target_day_minutes, 300);
        // Odrzucona zmiana nie zostawia w cache niczego niezapisanego.
        assert!(cache
            .save(&mut db, &json!({"targetDayMinutes": -5}))
            .is_err());
        assert_eq!(cache.get(&db).target_day_minutes, 300);
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
    fn waiting_redmine_statuses_default_validate_and_save_separately_from_jira() {
        let mut db = db();
        assert_eq!(load(&db).waiting_redmine_statuses, vec!["Feedback"]);
        // Czyszczenie jak dla Jiry: przycięcie, bez pustych i bez powtórek.
        let saved = save(
            &mut db,
            &json!({"waitingRedmineStatuses": [" Feedback ", "", "feedback", "Oczekuje"]}),
        )
        .unwrap();
        assert_eq!(saved.waiting_redmine_statuses, vec!["Feedback", "Oczekuje"]);
        assert_eq!(
            load(&db).waiting_redmine_statuses,
            vec!["Feedback", "Oczekuje"]
        );
        // Lista Jiry zostaje nietknięta.
        assert_eq!(saved.waiting_jira_statuses, default_waiting_statuses());
        let err = save(
            &mut db,
            &json!({"waitingRedmineStatuses": ["x".repeat(81)]}),
        )
        .err()
        .unwrap();
        assert_eq!(err, "Status Redmine może mieć najwyżej 80 znaków.");
        let many: Vec<String> = (0..51).map(|i| format!("S{i}")).collect();
        let err = save(&mut db, &json!({"waitingRedmineStatuses": many}))
            .err()
            .unwrap();
        assert_eq!(err, "Za dużo statusów Redmine oznaczających czekanie.");
        // Komunikaty Jiry bez zmian.
        let err = save(&mut db, &json!({"waitingJiraStatuses": ["x".repeat(81)]}))
            .err()
            .unwrap();
        assert_eq!(err, "Status Jiry może mieć najwyżej 80 znaków.");
        assert!(save(&mut db, &json!({"waitingRedmineStatuses": "Feedback"})).is_err());
        assert_eq!(
            save(&mut db, &json!({"waitingRedmineStatuses": []}))
                .unwrap()
                .waiting_redmine_statuses,
            Vec::<String>::new()
        );
    }

    #[test]
    fn redmine_auto_import_defaults_on_and_is_separate_from_jira() {
        let mut db = db();
        assert!(load(&db).redmine_auto_import);
        let saved = save(&mut db, &json!({"redmineAutoImport": false})).unwrap();
        assert!(!saved.redmine_auto_import);
        assert!(saved.jira_auto_import, "Jira bez zmian");
        assert!(!load(&db).redmine_auto_import);
        assert!(save(&mut db, &json!({"redmineAutoImport": "nie"})).is_err());
    }

    #[test]
    fn redmine_write_settings_default_validate_and_save_separately_from_jira() {
        let mut db = db();
        let s = load(&db);
        assert!(s.redmine_write && s.redmine_ask_move);
        assert_eq!(s.redmine_move_status, "In Progress");
        assert_eq!(s.redmine_new_statuses, vec!["New", "Nowy"]);
        assert_eq!(s.redmine_branch_prefix, "feature/");
        let saved = save(
            &mut db,
            &json!({
                "redmineWrite": false,
                "redmineAskMove": false,
                "redmineMoveStatus": "  W toku ",
                "redmineNewStatuses": [" Nowy ", "", "nowy", "Otwarte"],
                "redmineBranchPrefix": " bugfix/ "
            }),
        )
        .unwrap();
        assert!(!saved.redmine_write && !saved.redmine_ask_move);
        assert_eq!(saved.redmine_move_status, "W toku");
        assert_eq!(saved.redmine_new_statuses, vec!["Nowy", "Otwarte"]);
        assert_eq!(saved.redmine_branch_prefix, "bugfix/");
        let loaded = load(&db);
        assert_eq!(loaded, saved);
        // Jira bez zmian.
        assert!(loaded.jira_write && loaded.jira_ask_move);
        assert_eq!(loaded.jira_move_status, "In Progress");
        assert_eq!(loaded.jira_branch_prefix, "feature/");
        for bad in [
            json!({"redmineMoveStatus": " "}),
            json!({"redmineMoveStatus": "x".repeat(81)}),
            json!({"redmineBranchPrefix": "../x"}),
            json!({"redmineBranchPrefix": "feature /"}),
            json!({"redmineNewStatuses": ["x".repeat(81)]}),
            json!({"redmineNewStatuses": "New"}),
            json!({"redmineWrite": "tak"}),
        ] {
            assert!(save(&mut db, &bad).is_err(), "{bad}");
        }
        let many: Vec<String> = (0..51).map(|i| format!("S{i}")).collect();
        assert_eq!(
            save(&mut db, &json!({"redmineNewStatuses": many}))
                .err()
                .unwrap(),
            "Za dużo statusów Redmine początkowych."
        );
        // Pusty prefiks i pusta lista są dozwolone.
        let saved = save(
            &mut db,
            &json!({"redmineBranchPrefix": "", "redmineNewStatuses": []}),
        )
        .unwrap();
        assert_eq!(saved.redmine_branch_prefix, "");
        assert!(saved.redmine_new_statuses.is_empty());
    }

    /// Ustawienia → Redmine (PLAN_1.9.md, R5): każde pole sekcji zapisuje się
    /// osobno (pozostałe pola Redmine i Jiry bez zmian) i wraca po odczycie.
    #[test]
    fn redmine_section_fields_save_one_by_one_and_reload() {
        let mut db = db();
        let defaults = load(&db);
        assert!(
            defaults.redmine_write && defaults.redmine_ask_move && defaults.redmine_auto_import
        );
        assert_eq!(defaults.redmine_move_status, "In Progress");
        assert_eq!(defaults.redmine_new_statuses, vec!["New", "Nowy"]);
        assert_eq!(defaults.waiting_redmine_statuses, vec!["Feedback"]);
        assert_eq!(defaults.redmine_branch_prefix, "feature/");
        let cases: Vec<(Value, Box<dyn Fn(&AppSettings) -> bool>)> = vec![
            (
                json!({"redmineWrite": false}),
                Box::new(|s| !s.redmine_write),
            ),
            (
                json!({"redmineAskMove": false}),
                Box::new(|s| !s.redmine_ask_move),
            ),
            (
                json!({"redmineAutoImport": false}),
                Box::new(|s| !s.redmine_auto_import),
            ),
            (
                json!({"redmineMoveStatus": "W toku"}),
                Box::new(|s| s.redmine_move_status == "W toku"),
            ),
            (
                json!({"redmineNewStatuses": ["Nowy", "Otwarte"]}),
                Box::new(|s| s.redmine_new_statuses == ["Nowy", "Otwarte"]),
            ),
            (
                json!({"waitingRedmineStatuses": ["Feedback", "Oczekuje"]}),
                Box::new(|s| s.waiting_redmine_statuses == ["Feedback", "Oczekuje"]),
            ),
            (
                json!({"redmineBranchPrefix": "bugfix/"}),
                Box::new(|s| s.redmine_branch_prefix == "bugfix/"),
            ),
        ];
        let mut expected = defaults.clone();
        for (patch, check) in &cases {
            let saved = save(&mut db, patch).unwrap();
            assert!(check(&saved), "{patch}");
            let loaded = load(&db);
            assert!(check(&loaded), "{patch}");
            // Jedno pole naraz: reszta jak po poprzednich zapisach.
            let key = patch.as_object().unwrap().keys().next().unwrap().clone();
            expected = merged(&expected, &key, &loaded);
            assert_eq!(loaded, expected, "{patch}");
        }
        // Granica 80 znaków statusu docelowego.
        let status80 = "s".repeat(80);
        assert_eq!(
            save(&mut db, &json!({"redmineMoveStatus": status80}))
                .unwrap()
                .redmine_move_status,
            status80
        );
        // Jira bez zmian po wszystkich zapisach.
        let loaded = load(&db);
        assert_eq!(loaded.jira_write, defaults.jira_write);
        assert_eq!(loaded.jira_ask_move, defaults.jira_ask_move);
        assert_eq!(loaded.jira_auto_import, defaults.jira_auto_import);
        assert_eq!(loaded.jira_move_status, defaults.jira_move_status);
        assert_eq!(loaded.waiting_jira_statuses, defaults.waiting_jira_statuses);
        assert_eq!(loaded.jira_branch_prefix, defaults.jira_branch_prefix);
    }
    /// `expected` z jednym polem Redmine przepisanym z `loaded`.
    fn merged(expected: &AppSettings, key: &str, loaded: &AppSettings) -> AppSettings {
        let mut next = expected.clone();
        match key {
            "redmineWrite" => next.redmine_write = loaded.redmine_write,
            "redmineAskMove" => next.redmine_ask_move = loaded.redmine_ask_move,
            "redmineAutoImport" => next.redmine_auto_import = loaded.redmine_auto_import,
            "redmineMoveStatus" => next.redmine_move_status = loaded.redmine_move_status.clone(),
            "redmineNewStatuses" => next.redmine_new_statuses = loaded.redmine_new_statuses.clone(),
            "waitingRedmineStatuses" => {
                next.waiting_redmine_statuses = loaded.waiting_redmine_statuses.clone()
            }
            "redmineBranchPrefix" => {
                next.redmine_branch_prefix = loaded.redmine_branch_prefix.clone()
            }
            other => panic!("nieznane pole {other}"),
        }
        next
    }

    /// `ai_hidden_names` (A0): domyślnie pusta, czyszczona jak lista statusów,
    /// zapisywana i odczytywana z bazy.
    #[test]
    fn ai_hidden_names_default_clean_and_save() {
        let mut db = db();
        assert!(load(&db).ai_hidden_names.is_empty());
        let saved = save(
            &mut db,
            &json!({"aiHiddenNames": [" Jan Kowalski ", "", "jan kowalski", "Marzec"]}),
        )
        .unwrap();
        assert_eq!(saved.ai_hidden_names, vec!["Jan Kowalski", "Marzec"]);
        assert_eq!(load(&db), saved);
        for bad in [
            json!({"aiHiddenNames": ["x".repeat(81)]}),
            json!({"aiHiddenNames": ["a\nb"]}),
            json!({"aiHiddenNames": "Jan"}),
        ] {
            assert!(save(&mut db, &bad).is_err(), "{bad}");
        }
        let many: Vec<String> = (0..201).map(|i| format!("Nazwa{i}")).collect();
        assert_eq!(
            save(&mut db, &json!({"aiHiddenNames": many}))
                .err()
                .unwrap(),
            HIDDEN_NAMES_ERROR
        );
        assert!(save(&mut db, &json!({"aiHiddenNames": []}))
            .unwrap()
            .ai_hidden_names
            .is_empty());
    }

    /// Ustawienia silnika AI (A1): domyślnie `ai_engine` pusty, walidacja
    /// silnika, ścieżek i modeli, zapis i odczyt z bazy.
    #[test]
    fn ai_engine_settings_default_validate_and_save() {
        let mut db = db();
        let base = AppSettings::default();
        assert_eq!(base.ai_engine, "");
        assert!(!base.ai_send_meeting_titles && !base.ai_send_window_titles);
        assert!(base.ai_mcp && base.ai_chat && base.ai_assign_suggest && base.ai_day_close);
        for bad in [
            json!({"aiEngine": "gpt"}),
            json!({"aiEngine": "Claude"}),
            json!({"aiClaudePath": "claude"}),
            json!({"aiCodexPath": "C:\\\"x\".exe"}),
            json!({"aiClaudePath": "C:\\%X%\\claude.exe"}),
            json!({"aiModelFast": "a b"}),
            json!({"aiModelMain": "x".repeat(81)}),
            json!({"aiModelMain": "m;rm"}),
            json!({"aiEngine": 5}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad}");
        }
        let saved = save(
            &mut db,
            &json!({
                "aiEngine": "codex",
                "aiClaudePath": "  D:/bin/claude.exe ",
                "aiCodexPath": "D:/bin/codex.cmd",
                "aiModelFast": " haiku ",
                "aiModelMain": "claude-opus-4-5",
                "aiSendMeetingTitles": true,
                "aiMcp": false,
            }),
        )
        .unwrap();
        assert_eq!(saved.ai_engine, "codex");
        assert_eq!(saved.ai_claude_path, "D:/bin/claude.exe");
        assert_eq!(saved.ai_model_fast, "haiku");
        assert_eq!(load(&db), saved);
        assert!(load(&db).ai_send_meeting_titles && !load(&db).ai_mcp);
        for engine in ["claude", "off", ""] {
            assert_eq!(
                save(&mut db, &json!({ "aiEngine": engine }))
                    .unwrap()
                    .ai_engine,
                engine
            );
        }
    }

    /// Przypomnienia przed spotkaniem (C3): domyślnie włączone, 5 minut; tylko
    /// 1/5/10/15/30 i zapis pod kluczami snake_case.
    #[test]
    fn meeting_reminder_settings_default_validate_and_save() {
        let mut db = db();
        let s = load(&db);
        assert!(s.meeting_reminder_enabled);
        assert_eq!(s.meeting_reminder_minutes, 5);
        for minutes in [1, 5, 10, 15, 30] {
            let saved = save(&mut db, &json!({"meetingReminderMinutes": minutes})).unwrap();
            assert_eq!(saved.meeting_reminder_minutes, minutes);
            assert_eq!(load(&db), saved);
        }
        for bad in [0, 2, 20, 60] {
            assert!(
                save(&mut db, &json!({"meetingReminderMinutes": bad})).is_err(),
                "{bad}"
            );
        }
        assert_eq!(load(&db).meeting_reminder_minutes, 30);
        let saved = save(&mut db, &json!({"meetingReminderEnabled": false})).unwrap();
        assert!(!saved.meeting_reminder_enabled);
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='meeting_reminder_enabled'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "false");
        assert!(save(&mut db, &json!({"meetingReminderEnabled": "tak"})).is_err());
    }

    /// `widget_compact` (C2): domyślnie wyłączony, zapis pod kluczem snake_case.
    #[test]
    fn widget_compact_defaults_off_and_saves() {
        let mut db = db();
        assert!(!load(&db).widget_compact);
        assert!(!AppSettings::default().widget_compact);
        let saved = save(&mut db, &json!({"widgetCompact": true})).unwrap();
        assert!(saved.widget_compact);
        assert_eq!(load(&db), saved);
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='widget_compact'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "true");
        assert!(
            !save(&mut db, &json!({"widgetCompact": false}))
                .unwrap()
                .widget_compact
        );
        assert!(save(&mut db, &json!({"widgetCompact": "tak"})).is_err());
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
        // Automatyczne sprawdzanie aktualizacji jest domyślnie włączone i zapisuje się
        // pod kluczem snake_case jak pozostałe pola.
        assert!(s.auto_update_check);
        let mut db = db;
        let saved = save(&mut db, &json!({ "autoUpdateCheck": false })).unwrap();
        assert!(!saved.auto_update_check);
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='auto_update_check'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "false");
        assert!(!load(&db).auto_update_check);
    }

    /// Plan dnia przy pierwszym uruchomieniu (B11): domyślnie włączony,
    /// zapisuje się pod kluczem snake_case, zła wartość jest odrzucana.
    #[test]
    fn open_morning_on_start_defaults_on_and_saves() {
        let mut db = db();
        assert!(load(&db).open_morning_on_start);
        let saved = save(&mut db, &json!({ "openMorningOnStart": false })).unwrap();
        assert!(!saved.open_morning_on_start);
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='open_morning_on_start'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "false");
        assert!(!load(&db).open_morning_on_start);
        assert!(save(&mut db, &json!({ "openMorningOnStart": "tak" })).is_err());
        assert!(!load(&db).open_morning_on_start);
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
    fn orca_settings_default_validate_and_save() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        let base = AppSettings::default();
        assert_eq!(base.orca_path, "");
        assert!(!base.orca_autostart);
        for bad in [
            r"C:\orca\orca.txt",
            r#"C:\"orca".cmd"#,
            r"C:\%X%\orca.cmd",
            "orca",
        ] {
            assert!(apply(&base, &json!({"orcaPath": bad})).is_err(), "{bad}");
        }
        assert!(apply(&base, &json!({"orcaPath": 5})).is_err());
        let saved = save(
            &mut db,
            &json!({"orcaPath": "  D:/Orca/bin/orca.cmd ", "orcaAutostart": true}),
        )
        .unwrap();
        assert_eq!(saved.orca_path, "D:/Orca/bin/orca.cmd");
        let loaded = load(&db);
        assert_eq!(loaded.orca_path, "D:/Orca/bin/orca.cmd");
        assert!(loaded.orca_autostart);
        // Pusta ścieżka wraca do wykrywania.
        assert_eq!(
            save(&mut db, &json!({"orcaPath": ""})).unwrap().orca_path,
            ""
        );
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
    fn workflow_retention_days_are_validated_and_saved() {
        let mut db = db();
        let base = AppSettings::default();
        assert_eq!(base.workflow_retention_days, 30);
        for bad in [
            json!({"workflowRetentionDays": 0}),
            json!({"workflowRetentionDays": 1}),
            json!({"workflowRetentionDays": 6}),
            json!({"workflowRetentionDays": 366}),
        ] {
            assert!(apply(&base, &bad).is_err(), "{bad} powinno być odrzucone");
        }
        let saved = save(&mut db, &json!({"workflowRetentionDays": 7})).unwrap();
        assert_eq!(load(&db).workflow_retention_days, 7);
        assert_eq!(saved.workflow_retention_days, 7);
        assert!(apply(&base, &json!({"workflowRetentionDays": 365})).is_ok());
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
