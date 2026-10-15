//! Powiadomienia systemowe Windows wysyłane z Rust (tauri-plugin-notification).
//!
//! Reguły (każda z osobnym wyłącznikiem w ustawieniach i kluczem w `notifications_sent`):
//! - `due:<dzień>`: przy starcie i o 09:00 są nieukończone zadania z terminem dziś lub wcześniej;
//! - `long:<id sesji>`: aktywny pomiar trwa co najmniej `notify_long_minutes` (0 wyłącza);
//! - `evening:<dzień>`: o `notify_evening` pomiar nadal trwa;
//! - `target:<dzień>`: suma dnia osiągnęła cel (domyślnie wyłączone).
//!
//! Logika siedzi w `NotifyClock` z czasem i strefą podawanymi z zewnątrz, więc testy
//! sterują zegarem. Klucz trafia do bazy przed wysłaniem (`INSERT OR IGNORE`), dlatego
//! ponowne uruchomienie aplikacji nie powtarza wiadomości.
use crate::{settings::AppSettings, stats, tray, Database};
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

/// Co ile wątek sprawdza reguły.
const CHECK: Duration = Duration::from_secs(30);
/// Wpisy dedupe starsze niż 7 dni są usuwane przy starcie.
pub const KEEP_MS: i64 = 7 * 24 * 3_600_000;
/// Liczba tytułów wymienionych w powiadomieniu o terminach.
const DUE_TITLES: usize = 3;

/// Gotowe powiadomienie: klucz dedupe, tytuł i treść.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub key: String,
    pub title: String,
    pub body: String,
}

fn due_hour() -> NaiveTime {
    NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN)
}

/// Stan w pamięci: czy był już pierwszy przebieg po starcie i w które dni minęły
/// godziny 09:00 (terminy) i `notify_evening` (wieczór). Reguły „o godzinie” działają
/// przy pierwszym sprawdzeniu po tej godzinie, także po wybudzeniu komputera.
#[derive(Debug, Default)]
pub struct NotifyClock {
    started: bool,
    due_checked: Option<NaiveDate>,
    evening_checked: Option<NaiveDate>,
}

/// „1 zadanie”, „3 zadania”, „5 zadań”.
pub fn tasks_word(n: usize) -> String {
    let word = if n == 1 {
        "zadanie"
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "zadania"
    } else {
        "zadań"
    };
    format!("{n} {word}")
}

/// „4:05” — godziny i minuty.
fn hm(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// Tytuł zadania w jednej linii, najwyżej 80 znaków.
pub fn short(title: &str) -> String {
    let line = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 80 {
        line
    } else {
        format!("{}…", line.chars().take(79).collect::<String>())
    }
}

/// Zapisuje klucz; `true`, gdy wcześniej go nie było (powiadomienie do wysłania).
fn claim(db: &Connection, key: &str, now: i64) -> rusqlite::Result<bool> {
    Ok(db.execute(
        "INSERT OR IGNORE INTO notifications_sent(key,at) VALUES(?1,?2)",
        params![key, now],
    )? == 1)
}

/// Aktywny pomiar: id sesji, tytuł zadania, początek.
fn active(db: &Connection) -> rusqlite::Result<Option<(i64, String, i64)>> {
    db.query_row(
        "SELECT s.id, t.title, s.started_at FROM sessions s JOIN tasks t ON t.id=s.task_id
         WHERE s.ended_at IS NULL ORDER BY s.started_at DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

/// Tytuły nieukończonych zadań (bez archiwum i kosza) z terminem `<= day`.
fn due_titles(db: &Connection, day: NaiveDate) -> rusqlite::Result<Vec<String>> {
    let mut stmt = db.prepare(
        "SELECT t.title FROM tasks t
         JOIN task_details d ON d.task_id = t.id
         LEFT JOIN task_lifecycle l ON l.task_id = t.id
         WHERE t.done = 0 AND COALESCE(l.archived, 0) = 0 AND l.deleted_at IS NULL
           AND COALESCE(json_extract(d.json, '$.dueDay'), '') <> ''
           AND json_extract(d.json, '$.dueDay') <= ?1
         ORDER BY json_extract(d.json, '$.dueDay'), t.id",
    )?;
    let rows = stmt.query_map([day.format("%Y-%m-%d").to_string()], |r| r.get(0))?;
    rows.collect()
}

/// Treść powiadomienia o terminach: liczba i pierwsze trzy tytuły.
pub fn due_message(day: NaiveDate, titles: &[String]) -> Message {
    let mut lines: Vec<String> = titles
        .iter()
        .take(DUE_TITLES)
        .map(|t| format!("• {}", short(t)))
        .collect();
    if titles.len() > DUE_TITLES {
        lines.push(format!("i jeszcze {}", titles.len() - DUE_TITLES));
    }
    Message {
        key: format!("due:{day}"),
        title: format!("Termin dziś lub po terminie: {}", tasks_word(titles.len())),
        body: lines.join("\n"),
    }
}

impl NotifyClock {
    /// Jedno sprawdzenie reguł w chwili `now` (Unix ms) w strefie `tz`. Zwraca
    /// powiadomienia do wysłania; ich klucze są już zapisane w `notifications_sent`.
    pub fn tick<Tz: TimeZone>(
        &mut self,
        db: &Connection,
        s: &AppSettings,
        tz: &Tz,
        now: i64,
    ) -> rusqlite::Result<Vec<Message>> {
        let Some(local) = tz.timestamp_millis_opt(now).earliest() else {
            return Ok(Vec::new());
        };
        let today = local.date_naive();
        let time = local.time();
        let startup = !std::mem::replace(&mut self.started, true);
        let mut out = Vec::new();
        let running = active(db)?;

        // Termin: przy starcie i przy pierwszym sprawdzeniu po 09:00.
        let nine = time >= due_hour() && self.due_checked != Some(today);
        if time >= due_hour() {
            self.due_checked = Some(today);
        }
        if (startup || nine) && s.notify_due {
            let titles = due_titles(db, today)?;
            if !titles.is_empty() {
                let message = due_message(today, &titles);
                if claim(db, &message.key, now)? {
                    out.push(message);
                }
            }
        }

        // Długi przedział: jedna wiadomość na sesję.
        if let Some((id, title, started)) = &running {
            let limit = i64::from(s.notify_long_minutes) * 60_000;
            let elapsed = now - started;
            if limit > 0 && elapsed >= limit && claim(db, &format!("long:{id}"), now)? {
                out.push(Message {
                    key: format!("long:{id}"),
                    title: "Długi pomiar bez Stop".into(),
                    body: format!("{} · trwa już {}", short(title), hm(elapsed)),
                });
            }
        }

        // Wieczór: przy pierwszym sprawdzeniu po `notify_evening` pomiar nadal trwa.
        if let Ok(evening) = NaiveTime::parse_from_str(&s.notify_evening, "%H:%M") {
            if time >= evening && self.evening_checked != Some(today) {
                self.evening_checked = Some(today);
                if let (true, Some((_, title, started))) = (s.notify_evening_enabled, &running) {
                    let key = format!("evening:{today}");
                    if claim(db, &key, now)? {
                        out.push(Message {
                            key,
                            title: "Pomiar nadal trwa".into(),
                            body: format!("{} · {}", short(title), tray::hms(now - started)),
                        });
                    }
                }
            }
        }

        // Cel dnia osiągnięty (tylko dni robocze z niezerowym celem).
        let weekday = today.weekday().number_from_monday() as u8;
        if s.notify_target && s.target_day_minutes > 0 && s.work_days.contains(&weekday) {
            let target = i64::from(s.target_day_minutes) * 60_000;
            let total = stats::day_total_ms_in(db, tz, today, now)?;
            let key = format!("target:{today}");
            if total >= target && claim(db, &key, now)? {
                out.push(Message {
                    key,
                    title: "Cel dnia osiągnięty".into(),
                    body: format!("Dzisiaj {} / {}", hm(total), hm(target)),
                });
            }
        }
        Ok(out)
    }
}

/// Usuwa wpisy dedupe starsze niż 7 dni. Klucz trwającej sesji zostaje, żeby
/// wielodniowy pomiar nie dostał drugiego powiadomienia o długim przedziale.
pub fn cleanup(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    let keep = active(db)?
        .map(|(id, _, _)| format!("long:{id}"))
        .unwrap_or_default();
    db.execute(
        "DELETE FROM notifications_sent WHERE COALESCE(at, 0) < ?1 AND key <> ?2",
        params![now - KEEP_MS, keep],
    )
}

/// Wysyła powiadomienie Windows. Błąd (np. brak AUMID w `tauri dev`) jest pomijany.
pub fn show(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Potwierdzenie Start/Stop ze skrótu globalnego, gdy nie widać go w widgecie
/// ani w głównym oknie na pierwszym planie.
pub fn toggle_feedback(app: &tauri::AppHandle, text: &str) {
    let visible = |label: &str| {
        app.get_webview_window(label)
            .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
            .unwrap_or(false)
    };
    let main_front = visible("main")
        && app
            .get_webview_window("main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
    if !visible(crate::widget::LABEL) && !main_front {
        show(app, "Todo Track", text);
    }
}

/// Czyści stare wpisy i uruchamia wątek sprawdzający reguły co 30 s.
pub fn start(app: &tauri::AppHandle) {
    if let Ok(db) = app.state::<Database>().0.lock() {
        let _ = cleanup(&db, crate::db::now());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut clock = NotifyClock::default();
        loop {
            if app.get_webview_window("main").is_none() {
                break;
            }
            let messages = match app.state::<Database>().0.lock() {
                Ok(db) => {
                    let s = crate::settings::load(&db);
                    clock
                        .tick(&db, &s, &chrono::Local, crate::db::now())
                        .unwrap_or_default()
                }
                Err(_) => Vec::new(),
            };
            for m in messages {
                show(&app, &m.title, &m.body);
            }
            std::thread::sleep(CHECK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono_tz::Europe::Warsaw;
    use serde_json::json;

    const MIN: i64 = 60_000;

    fn at(s: &str) -> i64 {
        let local = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        Warsaw
            .from_local_datetime(&local)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    fn setup() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["A", "B", "C", "D", "E"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        db
    }
    fn details(db: &Connection, task: i64, value: serde_json::Value) {
        db.execute(
            "INSERT INTO task_details(task_id,json) VALUES(?1,?2)",
            params![task, value.to_string()],
        )
        .unwrap();
    }
    fn session(db: &Connection, task: i64, start: i64, end: Option<i64>) -> i64 {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            params![task, start, end],
        )
        .unwrap();
        db.last_insert_rowid()
    }
    /// Tylko wskazana reguła włączona.
    fn only(rule: &str) -> AppSettings {
        AppSettings {
            notify_due: rule == "due",
            notify_long_minutes: if rule == "long" { 240 } else { 0 },
            notify_evening_enabled: rule == "evening",
            notify_target: rule == "target",
            ..AppSettings::default()
        }
    }
    fn keys(m: &[Message]) -> Vec<&str> {
        m.iter().map(|m| m.key.as_str()).collect()
    }
    fn sent(db: &Connection) -> Vec<String> {
        db.prepare("SELECT key FROM notifications_sent ORDER BY key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn due_fires_at_startup_and_at_nine_once_per_day() {
        let db = setup();
        let s = only("due");
        // Środa 14.10.2026: termin wczoraj, dziś, jutro; ukończone i zarchiwizowane pomijane.
        details(&db, 1, json!({"dueDay": "2026-10-13"}));
        details(&db, 2, json!({"dueDay": "2026-10-14"}));
        details(&db, 3, json!({"dueDay": "2026-10-15"}));
        details(&db, 4, json!({"dueDay": "2026-10-01"}));
        details(&db, 5, json!({"dueDay": "2026-10-14"}));
        db.execute("UPDATE tasks SET done=1 WHERE id=4", [])
            .unwrap();
        db.execute(
            "INSERT INTO task_lifecycle(task_id,archived) VALUES(5,1)",
            [],
        )
        .unwrap();
        let mut clock = NotifyClock::default();
        // Start o 07:00: sprawdzenie od razu.
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 07:00"))
            .unwrap();
        assert_eq!(keys(&m), ["due:2026-10-14"]);
        assert_eq!(m[0].title, "Termin dziś lub po terminie: 2 zadania");
        assert_eq!(m[0].body, "• A\n• B");
        // O 09:00 ten sam dzień: już wysłane.
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 09:00"))
            .unwrap()
            .is_empty());
        // Nowy dzień przed 09:00: nic, o 09:00 (po wybudzeniu o 09:40) — jest.
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 08:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 09:40"))
            .unwrap();
        assert_eq!(keys(&m), ["due:2026-10-15"]);
        assert_eq!(m[0].title, "Termin dziś lub po terminie: 3 zadania");
        // Po 09:00 nowy termin nie wywołuje drugiej wiadomości tego dnia.
        db.execute("UPDATE tasks SET done=0 WHERE id=4", [])
            .unwrap();
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 10:00"))
            .unwrap()
            .is_empty());
        // Wyłączona reguła nie zapisuje klucza.
        let mut clock = NotifyClock::default();
        let off = AppSettings {
            notify_due: false,
            ..s.clone()
        };
        assert!(clock
            .tick(&db, &off, &Warsaw, at("2026-10-16 09:00"))
            .unwrap()
            .is_empty());
        assert!(!sent(&db).contains(&"due:2026-10-16".to_owned()));
    }

    #[test]
    fn due_without_morning_check_waits_for_nine() {
        let db = setup();
        let s = only("due");
        let mut clock = NotifyClock::default();
        // Start bez terminów; termin dopisany o 08:00 wychodzi o 09:00, nie wcześniej.
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 07:00"))
            .unwrap()
            .is_empty());
        details(&db, 1, json!({"dueDay": "2026-10-14"}));
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 08:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 09:00"))
            .unwrap();
        assert_eq!(m[0].title, "Termin dziś lub po terminie: 1 zadanie");
    }

    #[test]
    fn due_message_lists_first_three_titles() {
        let titles: Vec<String> = ["A", "B", "C", "D", "E"].map(String::from).into();
        let m = due_message(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(), &titles);
        assert_eq!(m.title, "Termin dziś lub po terminie: 5 zadań");
        assert_eq!(m.body, "• A\n• B\n• C\ni jeszcze 2");
        assert_eq!(tasks_word(22), "22 zadania");
        assert_eq!(tasks_word(12), "12 zadań");
    }

    #[test]
    fn long_session_fires_once_per_session() {
        let db = setup();
        let s = only("long");
        let mut clock = NotifyClock::default();
        let first = session(&db, 1, at("2026-10-14 08:00"), None);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 11:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 12:00"))
            .unwrap();
        assert_eq!(keys(&m), [format!("long:{first}").as_str()]);
        assert_eq!(m[0].body, "A · trwa już 4:00");
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 13:00"))
            .unwrap()
            .is_empty());
        // Nowa sesja liczy się od nowa.
        db.execute("UPDATE sessions SET ended_at=?1", [at("2026-10-14 13:00")])
            .unwrap();
        let second = session(&db, 2, at("2026-10-14 13:00"), None);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 16:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 17:00"))
            .unwrap();
        assert_eq!(keys(&m), [format!("long:{second}").as_str()]);
        // 0 minut wyłącza regułę: kolejna długa sesja bez wiadomości i bez klucza.
        db.execute(
            "UPDATE sessions SET ended_at=?1 WHERE id=?2",
            params![at("2026-10-14 17:30"), second],
        )
        .unwrap();
        let third = session(&db, 3, at("2026-10-14 17:30"), None);
        assert!(clock
            .tick(&db, &only("none"), &Warsaw, at("2026-10-14 23:00"))
            .unwrap()
            .is_empty());
        assert!(!sent(&db).contains(&format!("long:{third}")));
    }

    #[test]
    fn long_session_split_does_not_repeat_notification() {
        let mut db = setup();
        let s = only("long");
        let mut clock = NotifyClock::default();
        let first = session(&db, 1, at("2026-10-14 08:00"), None);
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 13:00"))
            .unwrap();
        assert_eq!(keys(&m), [format!("long:{first}").as_str()]);
        // Podział trwającego pomiaru w 30. minucie: nowy wiersz, ten sam pomiar.
        db::split_session(
            &mut db,
            first,
            at("2026-10-14 08:30"),
            None,
            at("2026-10-14 13:00"),
        )
        .unwrap();
        let second: i64 = db
            .query_row("SELECT id FROM sessions WHERE ended_at IS NULL", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_ne!(second, first);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 13:00"))
            .unwrap()
            .is_empty());
        // Kolejny podział także nie powtarza wiadomości.
        db::split_session(
            &mut db,
            second,
            at("2026-10-14 09:00"),
            Some(2),
            at("2026-10-14 13:30"),
        )
        .unwrap();
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 13:30"))
            .unwrap()
            .is_empty());
        // Podział zanim przyszło powiadomienie: nowa część liczy się od momentu podziału.
        db.execute(
            "UPDATE sessions SET ended_at=?1 WHERE ended_at IS NULL",
            [at("2026-10-14 14:00")],
        )
        .unwrap();
        let third = session(&db, 3, at("2026-10-14 14:00"), None);
        db::split_session(
            &mut db,
            third,
            at("2026-10-14 15:00"),
            None,
            at("2026-10-14 15:30"),
        )
        .unwrap();
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 18:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 19:00"))
            .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].title, "Długi pomiar bez Stop");
    }

    #[test]
    fn evening_fires_at_configured_time_only_when_measuring() {
        let db = setup();
        let s = only("evening");
        let mut clock = NotifyClock::default();
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 17:00"))
            .unwrap()
            .is_empty());
        // 17:30 bez pomiaru: dzień sprawdzony, późniejszy Start nie wywołuje wiadomości.
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 17:30"))
            .unwrap()
            .is_empty());
        session(&db, 1, at("2026-10-14 18:00"), None);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 18:30"))
            .unwrap()
            .is_empty());
        // Następny dzień: pomiar trwa przez 17:30.
        db.execute("UPDATE sessions SET ended_at=?1", [at("2026-10-14 19:00")])
            .unwrap();
        session(&db, 2, at("2026-10-15 16:00"), None);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 17:29"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 17:30"))
            .unwrap();
        assert_eq!(keys(&m), ["evening:2026-10-15"]);
        assert_eq!(m[0].body, "B · 01:30:00");
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-15 18:00"))
            .unwrap()
            .is_empty());
        // Własna godzina z ustawień.
        let late = AppSettings {
            notify_evening: "20:15".into(),
            ..s.clone()
        };
        assert!(clock
            .tick(&db, &late, &Warsaw, at("2026-10-16 20:14"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &late, &Warsaw, at("2026-10-16 20:15"))
            .unwrap();
        assert_eq!(keys(&m), ["evening:2026-10-16"]);
    }

    #[test]
    fn target_fires_when_day_total_reaches_goal_on_work_days() {
        let db = setup();
        let s = AppSettings {
            target_day_minutes: 120,
            ..only("target")
        };
        let mut clock = NotifyClock::default();
        session(&db, 1, at("2026-10-14 08:00"), Some(at("2026-10-14 09:00")));
        session(&db, 2, at("2026-10-14 10:00"), None);
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 10:59"))
            .unwrap()
            .is_empty());
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 11:00"))
            .unwrap();
        assert_eq!(keys(&m), ["target:2026-10-14"]);
        assert_eq!(m[0].body, "Dzisiaj 2:00 / 2:00");
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 12:00"))
            .unwrap()
            .is_empty());
        // Sobota nie jest dniem roboczym; domyślnie reguła wyłączona.
        db.execute(
            "UPDATE sessions SET ended_at=?1 WHERE ended_at IS NULL",
            [at("2026-10-14 12:00")],
        )
        .unwrap();
        session(&db, 1, at("2026-10-17 08:00"), Some(at("2026-10-17 12:00")));
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-17 13:00"))
            .unwrap()
            .is_empty());
        assert!(!AppSettings::default().notify_target);
    }

    #[test]
    fn dedupe_survives_restart_and_old_keys_are_cleaned() {
        let db = setup();
        let s = AppSettings {
            notify_target: true,
            target_day_minutes: 60,
            ..AppSettings::default()
        };
        details(&db, 1, json!({"dueDay": "2026-10-14"}));
        let id = session(&db, 2, at("2026-10-14 08:00"), None);
        let now = at("2026-10-14 18:00");
        let m = NotifyClock::default().tick(&db, &s, &Warsaw, now).unwrap();
        assert_eq!(
            keys(&m),
            [
                "due:2026-10-14".to_owned(),
                format!("long:{id}"),
                "evening:2026-10-14".to_owned(),
                "target:2026-10-14".to_owned()
            ]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
        );
        // Restart: nowy zegar, ta sama baza — nic się nie powtarza.
        assert!(NotifyClock::default()
            .tick(&db, &s, &Warsaw, now + 5 * MIN)
            .unwrap()
            .is_empty());
        // Czyszczenie po 7 dniach zostawia klucz trwającej sesji.
        db.execute(
            "INSERT INTO notifications_sent(key,at) VALUES('due:2026-10-01',?1),('long:999',?1)",
            [at("2026-10-01 09:00")],
        )
        .unwrap();
        let removed = cleanup(&db, now + KEEP_MS + MIN).unwrap();
        assert_eq!(removed, 5);
        assert_eq!(sent(&db), [format!("long:{id}")]);
        // Świeże wpisy zostają.
        db.execute(
            "INSERT INTO notifications_sent(key,at) VALUES('due:2026-10-20',?1)",
            [at("2026-10-20 09:00")],
        )
        .unwrap();
        assert_eq!(cleanup(&db, at("2026-10-21 09:00")).unwrap(), 0);
    }
}
