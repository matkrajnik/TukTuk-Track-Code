//! Powiadomienia systemowe Windows wysyłane z Rust.
//!
//! Na Windows toast idzie bezpośrednio przez `tauri-winrt-notification`, bo
//! `tauri-plugin-notification` 2.5.0 na desktopie ignoruje akcje i nie przekazuje
//! kliknięć (uchwyt z `notify_rust::Notification::show` jest porzucany). Kliknięcie
//! treści lub przycisku trafia kanałem do wątku z `setup`, który wykonuje akcję na
//! bazie (`perform`) i emituje `navigate` do głównego okna. Plugin zostaje dla innych
//! systemów. Akcje działają, gdy aplikacja jest uruchomiona; kliknięcie starego toastu
//! po restarcie tylko uruchamia aplikację (główne okno).
//!
//! Reguły (każda z osobnym wyłącznikiem w ustawieniach i kluczem w `notifications_sent`):
//! - `due:<dzień>`: przy starcie i o 09:00 są nieukończone zadania z terminem dziś lub wcześniej;
//! - `long:<id sesji>`: aktywny pomiar trwa co najmniej `notify_long_minutes` (0 wyłącza);
//! - `evening:<dzień>`: o `notify_evening` pomiar nadal trwa;
//! - `target:<dzień>`: suma dnia osiągnęła cel (domyślnie wyłączone);
//! - `morning:<dzień>`: dzień roboczy po godzinie początku dnia, a poranny przegląd
//!   jeszcze nie zrobiony (`morning_review`, osobno od `tick`);
//! - `waiting:<zadanie>:<od kiedy>[:<dzień>]`: blokada „Czekam na” trwa
//!   `waiting_notify_days` dni roboczych albo nadszedł dzień „Przypomnij jutro”
//!   (`waiting_reminders`, osobno od `tick`);
//! - `week:<poniedziałek>`: ostatni dzień roboczy tygodnia od `week_review_time`
//!   (`week_review`, osobno od `tick`).
//!
//! Logika siedzi w `NotifyClock` z czasem i strefą podawanymi z zewnątrz, więc testy
//! sterują zegarem. Klucz trafia do bazy przed wysłaniem (`INSERT OR IGNORE`), dlatego
//! ponowne uruchomienie aplikacji nie powtarza wiadomości.
use crate::{settings::AppSettings, stats, tray, Database};
use chrono::{NaiveDate, NaiveTime, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Co ile wątek sprawdza reguły.
const CHECK: Duration = Duration::from_secs(30);
/// Wpisy dedupe starsze niż 7 dni są usuwane przy starcie.
pub const KEEP_MS: i64 = 7 * 24 * 3_600_000;
/// Liczba tytułów wymienionych w powiadomieniu o terminach.
const DUE_TITLES: usize = 3;

/// Dokąd prowadzi kliknięcie: ładunek zdarzenia `navigate` dla głównego okna.
/// `view: None` tylko pokazuje główne okno.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub view: Option<&'static str>,
    /// Filtr listy zadań: `today` = „Dzisiaj: plan i praca”, `due` = ten sam filtr
    /// rozszerzony o terminy dziś i zaległe (tylko z powiadomienia o terminach).
    pub filter: Option<&'static str>,
    /// Zadanie do rozwinięcia na liście zadań.
    pub task: Option<i64>,
    /// Przedział do otwarcia w edytorze na zakładce Podziel.
    pub split: Option<i64>,
    /// Grupa listy zadań (np. „Nierozpoczęte”), do której przewinąć, przy
    /// `view: "settings"` sekcja Ustawień (np. `notifications`), a przy
    /// `view: "journal"` zakładka `morning` (Plan i historia → Rano) albo
    /// `week-review` (Plan i historia → Tydzień → Przegląd).
    pub section: Option<&'static str>,
    /// Plan i historia → Zamknięcie dnia (dzisiaj).
    pub day_close: bool,
}

impl View {
    pub fn tasks() -> Self {
        View {
            view: Some("tasks"),
            ..View::default()
        }
    }
    pub fn day_close() -> Self {
        View {
            view: Some("journal"),
            day_close: true,
            ..View::default()
        }
    }
    /// Plan i historia → Rano („Przegląd dnia”).
    pub fn morning_review() -> Self {
        View {
            view: Some("journal"),
            section: Some("morning"),
            ..View::default()
        }
    }
    /// Plan i historia → Tydzień → Przegląd („Przegląd tygodnia”).
    pub fn week_review() -> Self {
        View {
            view: Some("journal"),
            section: Some("week-review"),
            ..View::default()
        }
    }
}

/// Identyfikatory przycisków. Argument toastu to `id` albo `id:arg`.
pub const STOP: &str = "stop";
pub const SPLIT: &str = "split";
pub const JIRA_START: &str = "jira-start";
pub const PLAN_TODAY: &str = "plan-today";
/// „Przełącz” z propozycji agenta AI (`suggest`): start zadania o podanym id.
pub const AGENT_SWITCH: &str = "agent-switch";
/// Tryb skupienia (`focus`): „Przerwa” po bloku o podanym id.
pub const FOCUS_BREAK: &str = "focus-break";
/// „Kolejny blok” na zadaniu o podanym id.
pub const FOCUS_NEXT: &str = "focus-next";
/// „Rozpocznij” po przerwie: start pomiaru zadania o podanym id.
pub const FOCUS_RESUME: &str = "focus-resume";
/// „Odblokuj” z przypomnienia o blokadzie „Czekam na” (id zadania).
pub const WAITING_UNBLOCK: &str = "waiting-unblock";
/// „Przypomnij jutro” z przypomnienia o blokadzie (id zadania).
pub const WAITING_REMIND: &str = "waiting-remind";
/// „Przesuń” z pytania o przesunięcie zgłoszenia w Jirze (id zadania, J1).
pub const JIRA_MOVE: &str = "jira-move";

/// Przycisk w powiadomieniu; `arg` (np. id przedziału) jest doklejany do `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub id: &'static str,
    pub label: String,
    pub arg: String,
}

impl Action {
    pub fn new(id: &'static str, label: &str, arg: impl ToString) -> Self {
        Action {
            id,
            label: label.into(),
            arg: arg.to_string(),
        }
    }
    /// Tekst przekazywany przez Windows do `on_activated`.
    pub fn argument(&self) -> String {
        if self.arg.is_empty() {
            self.id.to_owned()
        } else {
            format!("{}:{}", self.id, self.arg)
        }
    }
}

/// Gotowe powiadomienie: klucz dedupe, tytuł, treść, przyciski i cel kliknięcia treści.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub key: String,
    pub title: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub open: View,
}

/// Co zrobić po aktywacji powiadomienia.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Zatrzymaj pomiar, jeśli wskazany przedział nadal trwa.
    Stop(i64),
    /// Edytor bieżącego przedziału na zakładce Podziel.
    Split(i64),
    /// Start zadania powiązanego ze zgłoszeniem Jiry (jak „Rozpocznij” w widoku Jira).
    JiraStart(i64),
    /// Zaplanuj zadania na dziś.
    PlanToday(Vec<i64>),
    /// Start zadania z propozycji agenta (poprzedni pomiar staje).
    AgentSwitch(i64),
    /// „Przerwa” po bloku skupienia (id bloku); wykonuje `focus::Focus::perform`.
    FocusBreak(i64),
    /// „Kolejny blok” na zadaniu (id zadania); wykonuje `focus::Focus::perform`.
    FocusNext(i64),
    /// „Rozpocznij” po przerwie: start pomiaru zadania (poprzedni staje).
    FocusResume(i64),
    /// Zdejmij blokadę „Czekam na” z zadania (timer bez zmian).
    WaitingUnblock(i64),
    /// Przypomnij o blokadzie zadania jutro.
    WaitingRemind(i64),
    /// „Przesuń” zgłoszenie zadania w Jirze; wykonuje `jira_move::activate` (sieć).
    JiraMove(i64),
    /// Pokaż główne okno z widokiem.
    Open(View),
}

/// Argument z Windows na polecenie. Kliknięcie treści (bez argumentu) otwiera `open`;
/// nieznany lub uszkodzony argument (np. stary toast po aktualizacji) tylko pokazuje
/// główne okno, bez zmian w danych.
pub fn command(argument: Option<&str>, open: &View) -> Command {
    let Some(argument) = argument.filter(|a| !a.is_empty()) else {
        return Command::Open(open.clone());
    };
    let (id, arg) = argument.split_once(':').unwrap_or((argument, ""));
    let number = arg.parse::<i64>().ok().filter(|n| *n > 0);
    let parsed = match id {
        STOP => number.map(Command::Stop),
        SPLIT => number.map(Command::Split),
        JIRA_START => number.map(Command::JiraStart),
        AGENT_SWITCH => number.map(Command::AgentSwitch),
        FOCUS_BREAK => number.map(Command::FocusBreak),
        FOCUS_NEXT => number.map(Command::FocusNext),
        FOCUS_RESUME => number.map(Command::FocusResume),
        WAITING_UNBLOCK => number.map(Command::WaitingUnblock),
        WAITING_REMIND => number.map(Command::WaitingRemind),
        JIRA_MOVE => number.map(Command::JiraMove),
        PLAN_TODAY => arg
            .split(',')
            .map(|n| n.parse::<i64>().ok().filter(|n| *n > 0))
            .collect::<Option<Vec<i64>>>()
            .filter(|ids| !ids.is_empty())
            .map(Command::PlanToday),
        _ => None,
    };
    parsed.unwrap_or_else(|| Command::Open(View::default()))
}

/// Skutek polecenia: czy dane się zmieniły i dokąd nawigować (wtedy pokazujemy okno).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub changed: bool,
    pub navigate: Option<View>,
}

/// Wykonuje polecenie na bazie. `today` to lokalny dzień `YYYY-MM-DD`. Timer zmienia
/// się tylko po jawnym kliknięciu „Zatrzymaj” albo „Rozpocznij”.
pub fn perform(
    db: &mut Connection,
    command: Command,
    today: &str,
    now: i64,
) -> Result<Outcome, String> {
    let changed = |changed| Outcome {
        changed,
        navigate: None,
    };
    match command {
        Command::Stop(session) => {
            // Zatrzymujemy tylko pomiar z toastu (także jego dalszy ciąg po podziale);
            // stary toast nie zatrzymuje pomiaru, który zaczął się później. Gdy nie ma
            // czego zatrzymać, okno pokazuje, co faktycznie trwa.
            if continuation(db, session)
                .map_err(|e| e.to_string())?
                .is_some()
            {
                crate::db::stop(db, now).map_err(|e| e.to_string())?;
                return Ok(changed(true));
            }
            Ok(Outcome {
                changed: false,
                navigate: Some(View::tasks()),
            })
        }
        Command::Split(session) => {
            // Po wcześniejszym podziale trwa już nowy wiersz tego samego pomiaru;
            // dzielimy go. Pomiar innego zadania nie jest celem starego toastu: wtedy
            // edytor przedziału z toastu, a gdy go już nie ma, sama lista zadań.
            let target: Option<(i64, i64)> =
                match continuation(db, session).map_err(|e| e.to_string())? {
                    Some(found) => Some(found),
                    None => db
                        .query_row(
                            "SELECT id, task_id FROM sessions WHERE id=?1",
                            [session],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?,
                };
            Ok(Outcome {
                changed: false,
                navigate: Some(View {
                    task: target.map(|(_, task)| task),
                    split: target.map(|(id, _)| id),
                    ..View::tasks()
                }),
            })
        }
        Command::JiraStart(task) => {
            crate::jira::start_task(db, task, now)?;
            Ok(changed(true))
        }
        Command::PlanToday(ids) => {
            let mut any = false;
            for id in ids {
                let order: i64 = db
                    .query_row(
                        "SELECT COALESCE(MAX(json_extract(json,'$.order')),-1)+1 FROM task_details
                         WHERE json_extract(json,'$.plannedDay')=?1",
                        [today],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                // Zadanie usunięte w międzyczasie pomijamy.
                if crate::planner::plan_task(db, id, today, order.clamp(0, 1_000_000)).is_ok() {
                    any = true;
                }
            }
            Ok(changed(any))
        }
        Command::AgentSwitch(task) | Command::FocusResume(task) => {
            crate::db::change(db, "start", Some(task), None, now)?;
            Ok(changed(true))
        }
        Command::WaitingUnblock(task) | Command::WaitingRemind(task) => {
            let change = match command {
                Command::WaitingUnblock(_) => crate::planner::WaitingChange {
                    action: "unblock".into(),
                    text: String::new(),
                },
                _ => {
                    let tomorrow = NaiveDate::parse_from_str(today, "%Y-%m-%d")
                        .ok()
                        .and_then(|d| d.succ_opt())
                        .ok_or("Niepoprawny dzień.")?;
                    crate::planner::WaitingChange {
                        action: "remind".into(),
                        text: tomorrow.format("%Y-%m-%d").to_string(),
                    }
                }
            };
            // Zadanie usunięte w międzyczasie: tylko lista zadań.
            match crate::planner::set_waiting(db, task, &change, now) {
                Ok(_) => Ok(changed(true)),
                Err(_) => Ok(Outcome {
                    changed: false,
                    navigate: Some(View::tasks()),
                }),
            }
        }
        // Wymagają stanu trybu skupienia: `focus::Focus::perform` (z `activate`).
        // Bez niego tylko pokazujemy listę zadań.
        // Przejście w Jirze wymaga sieci: `jira_move::activate` (z `activate`).
        Command::FocusBreak(_) | Command::FocusNext(_) | Command::JiraMove(_) => Ok(Outcome {
            changed: false,
            navigate: Some(View::tasks()),
        }),
        Command::Open(view) => Ok(Outcome {
            changed: false,
            navigate: Some(view),
        }),
    }
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

/// Trwający dalszy ciąg pomiaru `session` (id przedziału, id zadania): sam przedział,
/// gdy jeszcze trwa, albo wiersz powstały z jego podziału (kolejne podziały też) na
/// tym samym zadaniu. Podział zamyka przedział w chwili `at` i wstawia nowy od `at`,
/// więc idziemy po łańcuchu „koniec = początek następnego”. Ponowny start tego samego
/// zadania później nie jest dalszym ciągiem.
fn continuation(db: &Connection, session: i64) -> rusqlite::Result<Option<(i64, i64)>> {
    let mut current: Option<(i64, i64, Option<i64>)> = db
        .query_row(
            "SELECT id, task_id, ended_at FROM sessions WHERE id=?1",
            [session],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    // Ograniczenie na wypadek zapętlonych danych (przedziały zerowej długości).
    for _ in 0..1_000 {
        let Some((id, task, ended)) = current else {
            return Ok(None);
        };
        let Some(end) = ended else {
            return Ok(Some((id, task)));
        };
        current = db
            .query_row(
                "SELECT id, task_id, ended_at FROM sessions
                 WHERE task_id=?1 AND started_at=?2 AND id<>?3
                 ORDER BY id DESC LIMIT 1",
                params![task, end, id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
    }
    Ok(None)
}

/// Aktywny pomiar: id sesji, id zadania, tytuł zadania, początek.
fn active(db: &Connection) -> rusqlite::Result<Option<(i64, i64, String, i64)>> {
    db.query_row(
        "SELECT s.id, t.id, t.title, s.started_at FROM sessions s JOIN tasks t ON t.id=s.task_id
         WHERE s.ended_at IS NULL ORDER BY s.started_at DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
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
        actions: Vec::new(),
        open: View {
            filter: Some("due"),
            ..View::tasks()
        },
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
        if let Some((id, task, title, started)) = &running {
            let limit = i64::from(s.notify_long_minutes) * 60_000;
            let elapsed = now - started;
            if limit > 0 && elapsed >= limit && claim(db, &format!("long:{id}"), now)? {
                out.push(Message {
                    key: format!("long:{id}"),
                    title: "Długi pomiar bez Stop".into(),
                    body: format!("{} · trwa już {}", short(title), hm(elapsed)),
                    actions: vec![
                        Action::new(STOP, "Zatrzymaj", id),
                        Action::new(SPLIT, "Podziel…", id),
                    ],
                    open: View {
                        task: Some(*task),
                        ..View::tasks()
                    },
                });
            }
        }

        // Wieczór: przy pierwszym sprawdzeniu po `notify_evening` pomiar nadal trwa.
        if let Ok(evening) = NaiveTime::parse_from_str(&s.notify_evening, "%H:%M") {
            if time >= evening && self.evening_checked != Some(today) {
                self.evening_checked = Some(today);
                if let (true, Some((id, _, title, started))) = (s.notify_evening_enabled, &running)
                {
                    let key = format!("evening:{today}");
                    if claim(db, &key, now)? {
                        out.push(Message {
                            key,
                            title: "Pomiar nadal trwa".into(),
                            body: format!("{} · {}", short(title), tray::hms(now - started)),
                            actions: vec![Action::new(STOP, "Zatrzymaj teraz", id)],
                            open: View::day_close(),
                        });
                    }
                }
            }
        }

        // Cel dnia osiągnięty (tylko dni robocze z niezerowym celem, bez świąt).
        // Cel pomniejszają spotkania niepokryte pomiarem; urlop całodniowy = brak celu.
        let target = crate::capacity::goal_ms_in(db, tz, &s, today, now)?;
        if s.notify_target && target > 0 {
            let total = stats::day_total_ms_in(db, tz, today, now)?;
            let key = format!("target:{today}");
            if total >= target && claim(db, &key, now)? {
                out.push(Message {
                    key,
                    title: "Cel dnia osiągnięty".into(),
                    body: format!("Dzisiaj {} / {}", hm(total), hm(target)),
                    actions: Vec::new(),
                    open: View::day_close(),
                });
            }
        }
        Ok(out)
    }
}

/// Liczba niedokończonych zadań zaplanowanych przed `day` (bez archiwum, kosza
/// i wzorców cykli, których `plannedDay` to tylko początek cyklu).
fn unfinished_before(db: &Connection, day: NaiveDate) -> rusqlite::Result<usize> {
    db.query_row(
        "SELECT COUNT(*) FROM tasks t
         JOIN task_details d ON d.task_id = t.id
         LEFT JOIN task_lifecycle l ON l.task_id = t.id
         WHERE t.done = 0 AND COALESCE(l.archived, 0) = 0 AND l.deleted_at IS NULL
           AND COALESCE(json_extract(d.json, '$.plannedDay'), '') <> ''
           AND json_extract(d.json, '$.plannedDay') < ?1
           AND COALESCE(json_array_length(json_extract(d.json, '$.repeatDays')), 0) = 0
           AND COALESCE(json_extract(d.json, '$.repeatRule.kind'), '')
               NOT IN ('monthDay', 'lastWorkday', 'nthWeekday')",
        [day.format("%Y-%m-%d").to_string()],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as usize)
}

/// Liczba nieukończonych zadań z terminem `<= day` bez tych, które liczy już
/// `unfinished_before` — jak krok „Terminy” w Porannym przeglądzie (`dueTasks`
/// z pominięciem zadań z kroku 1), żeby jedno zadanie nie liczyło się dwa razy.
fn due_after_unfinished(db: &Connection, day: NaiveDate) -> rusqlite::Result<usize> {
    db.query_row(
        "SELECT COUNT(*) FROM tasks t
         JOIN task_details d ON d.task_id = t.id
         LEFT JOIN task_lifecycle l ON l.task_id = t.id
         WHERE t.done = 0 AND COALESCE(l.archived, 0) = 0 AND l.deleted_at IS NULL
           AND COALESCE(json_extract(d.json, '$.dueDay'), '') <> ''
           AND json_extract(d.json, '$.dueDay') <= ?1
           AND NOT (COALESCE(json_extract(d.json, '$.plannedDay'), '') <> ''
                AND json_extract(d.json, '$.plannedDay') < ?1
                AND COALESCE(json_array_length(json_extract(d.json, '$.repeatDays')), 0) = 0
           AND COALESCE(json_extract(d.json, '$.repeatRule.kind'), '')
               NOT IN ('monthDay', 'lastWorkday', 'nthWeekday'))",
        [day.format("%Y-%m-%d").to_string()],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as usize)
}

/// Czy przypomnieć o porannym przeglądzie: wyłącznik włączony, dzień roboczy
/// (dni tygodnia i święta z ustawień), nie urlop całodniowy z kalendarza (`off`),
/// po godzinie początku dnia i bez przeglądu zapisanego na ten dzień.
pub fn morning_review_due(s: &AppSettings, today: NaiveDate, time: NaiveTime, off: bool) -> bool {
    let Ok(start) = NaiveTime::parse_from_str(&s.day_start, "%H:%M") else {
        return false;
    };
    s.notify_morning_review
        && crate::holidays::is_work_day(s, today)
        && !off
        && time >= start
        && s.morning_review_day != today.format("%Y-%m-%d").to_string()
}

/// Treść powiadomienia „Przegląd dnia”: niedokończone z wcześniejszych dni i terminy.
pub fn morning_message(day: NaiveDate, unfinished: usize, due: usize) -> Message {
    let body = match (unfinished, due) {
        (0, 0) => "Zaplanuj dzień: spotkania, plan i pojemność.".to_owned(),
        (u, 0) => format!("Niedokończone: {}", tasks_word(u)),
        (0, d) => format!("Terminy: {}", tasks_word(d)),
        (u, d) => format!(
            "Niedokończone: {} · terminy: {}",
            tasks_word(u),
            tasks_word(d)
        ),
    };
    Message {
        key: format!("morning:{day}"),
        title: "Przegląd dnia".into(),
        body,
        actions: Vec::new(),
        open: View::morning_review(),
    }
}

/// Reguła „Przegląd dnia” w chwili `now`: najwyżej jedno powiadomienie dziennie
/// (klucz `morning:<dzień>` w `notifications_sent`, także po restarcie). Nie
/// zmienia planu ani timera; kliknięcie otwiera Plan i historia → Rano.
pub fn morning_review<Tz: TimeZone>(
    db: &Connection,
    s: &AppSettings,
    tz: &Tz,
    now: i64,
) -> rusqlite::Result<Option<Message>> {
    let Some(local) = tz.timestamp_millis_opt(now).earliest() else {
        return Ok(None);
    };
    let today = local.date_naive();
    if !morning_review_due(s, today, local.time(), false)
        || crate::capacity::off_day_in(db, tz, today)
    {
        return Ok(None);
    }
    let message = morning_message(
        today,
        unfinished_before(db, today)?,
        due_after_unfinished(db, today)?,
    );
    Ok(claim(db, &message.key, now)?.then_some(message))
}

/// Czy `today` to ostatni dzień roboczy swojego tygodnia (Pon–Nd): dzień roboczy
/// z ustawień (dni tygodnia i święta), a do niedzieli żaden kolejny dzień nie jest
/// roboczy. `off` mówi, czy dzień jest wolny z kalendarza (całodniowe „poza
/// biurem”); taki dzień nie jest roboczy.
pub fn last_work_day_of_week(
    s: &AppSettings,
    today: NaiveDate,
    off: impl Fn(NaiveDate) -> bool,
) -> bool {
    use chrono::Datelike;
    let work = |d: NaiveDate| crate::holidays::is_work_day(s, d) && !off(d);
    if !work(today) {
        return false;
    }
    let left = 6 - i64::from(today.weekday().num_days_from_monday());
    (1..=left).all(|i| !work(today + chrono::Duration::days(i)))
}

/// Czy przypomnieć o przeglądzie tygodnia: wyłącznik włączony, ostatni dzień
/// roboczy tygodnia (`last_work_day_of_week`) i nie wcześniej niż
/// `week_review_time`.
pub fn week_review_due(
    s: &AppSettings,
    today: NaiveDate,
    time: NaiveTime,
    off: impl Fn(NaiveDate) -> bool,
) -> bool {
    let Ok(at) = NaiveTime::parse_from_str(&s.week_review_time, "%H:%M") else {
        return false;
    };
    s.notify_week_review && time >= at && last_work_day_of_week(s, today, off)
}

/// Powiadomienie „Przegląd tygodnia” dla tygodnia zaczynającego się w `monday`.
pub fn week_message(monday: NaiveDate) -> Message {
    Message {
        key: format!("week:{monday}"),
        title: "Przegląd tygodnia".into(),
        body: "Podsumuj tydzień: czas, plan, rozproszenie i wnioski.".into(),
        actions: Vec::new(),
        open: View::week_review(),
    }
}

/// Reguła „Przegląd tygodnia” w chwili `now`: najwyżej jedno powiadomienie na
/// tydzień (klucz `week:<poniedziałek>` w `notifications_sent`, także po
/// restarcie). Nie zmienia planu ani timera; kliknięcie otwiera Plan i historia
/// → Tydzień → Przegląd.
pub fn week_review<Tz: TimeZone>(
    db: &Connection,
    s: &AppSettings,
    tz: &Tz,
    now: i64,
) -> rusqlite::Result<Option<Message>> {
    use chrono::Datelike;
    let Some(local) = tz.timestamp_millis_opt(now).earliest() else {
        return Ok(None);
    };
    let today = local.date_naive();
    if !week_review_due(s, today, local.time(), |d| {
        crate::capacity::off_day_in(db, tz, d)
    }) {
        return Ok(None);
    }
    let monday = today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()));
    let message = week_message(monday);
    Ok(claim(db, &message.key, now)?.then_some(message))
}

/// Liczba dni roboczych po dniu `since` aż do `today` włącznie (dzień blokady
/// się nie liczy). Najwyżej 400 dni wstecz.
pub fn work_days_between(s: &AppSettings, since: NaiveDate, today: NaiveDate) -> u32 {
    let mut count = 0;
    let mut date = since.max(today - chrono::Duration::days(400));
    while date < today {
        let Some(next) = date.succ_opt() else { break };
        date = next;
        if crate::holidays::is_work_day(s, date) {
            count += 1;
        }
    }
    count
}

/// „1 dzień”, „3 dni”.
fn days_word(n: u32) -> String {
    if n == 1 {
        "1 dzień".into()
    } else {
        format!("{n} dni")
    }
}

/// Treść przypomnienia o blokadzie: „Czekasz na: klient — X (3 dni)”.
pub fn waiting_message(key: String, task: i64, title: &str, waiting: &str, days: u32) -> Message {
    Message {
        key,
        title: "Czekam na".into(),
        body: format!(
            "Czekasz na: {} — {} ({})",
            short(waiting),
            short(title),
            days_word(days)
        ),
        actions: vec![
            Action::new(WAITING_UNBLOCK, "Odblokuj", task),
            Action::new(WAITING_REMIND, "Przypomnij jutro", task),
        ],
        open: View {
            task: Some(task),
            section: Some("Czekam na…"),
            ..View::tasks()
        },
    }
}

/// Reguła „Czekam na” w chwili `now`: w dzień roboczy, gdy blokada trwa co
/// najmniej `waiting_notify_days` dni roboczych (0 wyłącza), raz na blokadę
/// (klucz `waiting:<zadanie>:<od kiedy>`). „Przypomnij jutro” ustawia dzień
/// kolejnego przypomnienia; wtedy przychodzi ono raz w tym dniu albo w
/// pierwszym dniu roboczym po nim (klucz z tym dniem). Nic nie zmienia się samo.
pub fn waiting_reminders<Tz: TimeZone>(
    db: &Connection,
    s: &AppSettings,
    tz: &Tz,
    now: i64,
) -> rusqlite::Result<Vec<Message>> {
    let Some(local) = tz.timestamp_millis_opt(now).earliest() else {
        return Ok(Vec::new());
    };
    let today = local.date_naive();
    if s.waiting_notify_days == 0 || !crate::holidays::is_work_day(s, today) {
        return Ok(Vec::new());
    }
    let blocked = db
        .prepare(
            "SELECT t.id, t.title, d.json FROM tasks t JOIN task_details d ON d.task_id = t.id
             LEFT JOIN task_lifecycle l ON l.task_id = t.id
             WHERE t.done = 0 AND l.deleted_at IS NULL AND COALESCE(l.archived, 0) = 0
               AND COALESCE(json_extract(d.json, '$.waiting'), '') <> ''
             ORDER BY t.id",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut messages = Vec::new();
    for (id, title, json) in blocked {
        let Ok(details) = serde_json::from_str::<crate::planner::Details>(&json) else {
            continue;
        };
        // Stara blokada bez `waitingSince` nie wie, od kiedy trwa.
        let Some(since) = tz
            .timestamp_millis_opt(details.waiting_since)
            .earliest()
            .filter(|_| details.waiting_since > 0)
        else {
            continue;
        };
        let since_day = since.date_naive();
        let days = work_days_between(s, since_day, today);
        let key = if details.waiting_remind_day.is_empty() {
            if days < s.waiting_notify_days {
                continue;
            }
            format!("waiting:{id}:{}", details.waiting_since)
        } else {
            match NaiveDate::parse_from_str(&details.waiting_remind_day, "%Y-%m-%d") {
                Ok(day) if day <= today => format!(
                    "waiting:{id}:{}:{}",
                    details.waiting_since, details.waiting_remind_day
                ),
                _ => continue,
            }
        };
        if claim(db, &key, now)? {
            messages.push(waiting_message(key, id, &title, &details.waiting, days));
        }
    }
    Ok(messages)
}

/// Dedupe przypomnień o blokadzie trwa dłużej, bo blokada może trwać tygodniami.
const WAITING_KEEP_MS: i64 = 400 * 24 * 3_600_000;

/// Usuwa wpisy dedupe starsze niż 7 dni (przypomnienia o blokadzie: 400 dni).
/// Klucz trwającej sesji zostaje, żeby wielodniowy pomiar nie dostał drugiego
/// powiadomienia o długim przedziale.
pub fn cleanup(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    let keep = active(db)?
        .map(|(id, ..)| format!("long:{id}"))
        .unwrap_or_default();
    db.execute(
        "DELETE FROM notifications_sent WHERE key <> ?2 AND (
            (key NOT LIKE 'waiting:%' AND COALESCE(at, 0) < ?1)
            OR (key LIKE 'waiting:%' AND COALESCE(at, 0) < ?3))",
        params![now - KEEP_MS, keep, now - WAITING_KEEP_MS],
    )
}

/// Kliknięcie toastu: argument przycisku (`None` = treść) i cel kliknięcia treści.
type Activation = (Option<String>, View);

/// Nadawca kanału aktywacji; odbiorca żyje w wątku z `setup`.
pub struct Activations(Mutex<mpsc::Sender<Activation>>);

/// Rejestruje kanał aktywacji i wątek wykonujący akcje z powiadomień. Wołać przed
/// `create_windows` (manage). Wątek czeka na kanale, nie zależy od okna `main`.
pub fn setup(app: &tauri::AppHandle) {
    let (tx, rx) = mpsc::channel::<Activation>();
    app.manage(Activations(Mutex::new(tx)));
    let app = app.clone();
    std::thread::spawn(move || {
        while let Ok((argument, open)) = rx.recv() {
            activate(&app, command(argument.as_deref(), &open));
        }
    });
}

/// Wykonuje polecenie z powiadomienia na `AppHandle`: zmiana danych → `data-changed`,
/// widok → główne okno na wierzchu i `navigate`, błąd → okno z komunikatem.
fn activate(app: &tauri::AppHandle, command: Command) {
    if let Command::JiraMove(task) = command {
        crate::jira_move::activate(app, task);
        return;
    }
    let result = if matches!(command, Command::FocusBreak(_) | Command::FocusNext(_)) {
        crate::focus::activate(app, command)
    } else {
        app.state::<Database>()
            .0
            .lock()
            .map_err(|e| e.to_string())
            .and_then(|mut db| {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                perform(&mut db, command, &today, crate::db::now())
            })
    };
    match result {
        Ok(outcome) => {
            if outcome.changed {
                crate::reminder::reset(app);
                let _ = app.emit("data-changed", ());
                crate::suggest::refresh(app, crate::db::now());
                // Stop albo start z powiadomienia może przerwać blok skupienia.
                crate::focus::sync(app);
            }
            if let Some(view) = outcome.navigate {
                crate::show(app);
                let _ = app.emit_to("main", "navigate", view);
            }
        }
        Err(e) => {
            crate::show(app);
            let _ = app.emit("storage-error", e);
        }
    }
}

/// AUMID jak w tauri-plugin-notification 2.5.0: identyfikator aplikacji dla
/// zainstalowanego exe, AUMID PowerShella dla kompilacji z `target\debug|release`.
#[cfg(windows)]
fn app_id(app: &tauri::AppHandle) -> String {
    let dev = tauri::utils::platform::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.display().to_string()))
        .is_some_and(|dir| dir.ends_with(r"\target\debug") || dir.ends_with(r"\target\release"));
    if dev {
        tauri_winrt_notification::Toast::POWERSHELL_APP_ID.to_owned()
    } else {
        app.config().identifier.clone()
    }
}

/// Wysyła powiadomienie. W czasie bloku skupienia trafia ono do kolejki
/// (`focus::hold`) i przychodzi po bloku w jednym zbiorczym.
pub fn show(app: &tauri::AppHandle, message: &Message) {
    if !crate::focus::hold(app, message) {
        display(app, message);
    }
}

/// Wysyła toast Windows z przyciskami od razu, z pominięciem kolejki trybu
/// skupienia. Błąd (np. brak AUMID) jest pomijany.
#[cfg(windows)]
pub fn display(app: &tauri::AppHandle, message: &Message) {
    let sender = app
        .try_state::<Activations>()
        .and_then(|s| s.0.lock().ok().map(|tx| tx.clone()));
    let app_id = app_id(app);
    let message = message.clone();
    // Jak w pluginie: wysyłka poza wątkiem wywołującym (np. obsługą skrótu globalnego).
    std::thread::spawn(move || {
        let mut toast = tauri_winrt_notification::Toast::new(&app_id)
            .title(&message.title)
            .text1(&message.body)
            .sound(None);
        for action in &message.actions {
            toast = toast.add_button(&action.label, &action.argument());
        }
        if let Some(tx) = sender {
            let open = message.open.clone();
            toast = toast.on_activated(move |argument| {
                let _ = tx.send((argument, open.clone()));
                Ok(())
            });
        }
        let _ = toast.show();
    });
}

/// Poza Windows zostaje plugin (bez przycisków i kliknięć).
#[cfg(not(windows))]
pub fn display(app: &tauri::AppHandle, message: &Message) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(&message.title)
        .body(&message.body)
        .show();
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
    // Odpowiedź na własne naciśnięcie skrótu: od razu, także w czasie bloku.
    if !visible(crate::widget::LABEL) && !main_front {
        display(
            app,
            &Message {
                key: String::new(),
                title: "Todo Track".into(),
                body: text.into(),
                actions: Vec::new(),
                open: View::default(),
            },
        );
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
                    let now = crate::db::now();
                    let mut messages = clock.tick(&db, &s, &chrono::Local, now).unwrap_or_default();
                    if let Ok(Some(m)) = morning_review(&db, &s, &chrono::Local, now) {
                        messages.push(m);
                    }
                    if let Ok(Some(m)) = week_review(&db, &s, &chrono::Local, now) {
                        messages.push(m);
                    }
                    if let Ok(waiting) = waiting_reminders(&db, &s, &chrono::Local, now) {
                        messages.extend(waiting);
                    }
                    messages
                }
                Err(_) => Vec::new(),
            };
            for m in messages {
                show(&app, &m);
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
    fn target_is_reduced_by_meetings_and_all_day_out_of_office() {
        let mut db = setup();
        let s = AppSettings {
            target_day_minutes: 180,
            ..only("target")
        };
        let event = |id: &str, start: i64, end: i64, show_as: &str, all_day: bool| {
            crate::calendar::CalendarEvent {
                id: id.into(),
                start,
                end,
                subject: id.into(),
                show_as: show_as.into(),
                all_day,
                cancelled: false,
                response: "accepted".into(),
            }
        };
        let (fri_from, fri_to) = crate::stats::day_bounds(
            &Warsaw,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(),
        );
        crate::calendar::replace_range(
            &mut db,
            at("2026-10-12 00:00"),
            at("2026-10-19 00:00"),
            &[
                event(
                    "m",
                    at("2026-10-14 12:00"),
                    at("2026-10-14 13:00"),
                    "tentative",
                    false,
                ),
                event("u", fri_from, fri_to, "oof", true),
            ],
            at("2026-10-14 07:00"),
        )
        .unwrap();
        crate::calendar::set_test_email(&db, "jan@firma.example");
        let mut clock = NotifyClock::default();
        session(&db, 1, at("2026-10-14 08:00"), Some(at("2026-10-14 10:00")));
        // Cel 3:00 minus spotkanie 1:00 = 2:00 mierzonego czasu.
        let m = clock
            .tick(&db, &s, &Warsaw, at("2026-10-14 10:01"))
            .unwrap();
        assert_eq!(keys(&m), ["target:2026-10-14"]);
        assert_eq!(m[0].body, "Dzisiaj 2:00 / 2:00");
        // Urlop całodniowy: brak celu, brak powiadomienia.
        session(&db, 1, at("2026-10-16 08:00"), Some(at("2026-10-16 12:00")));
        assert!(clock
            .tick(&db, &s, &Warsaw, at("2026-10-16 13:00"))
            .unwrap()
            .is_empty());
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

    #[test]
    fn rules_map_to_messages_with_actions_and_targets() {
        let db = setup();
        let s = AppSettings {
            notify_target: true,
            target_day_minutes: 60,
            ..AppSettings::default()
        };
        details(&db, 1, json!({"dueDay": "2026-10-14"}));
        let id = session(&db, 2, at("2026-10-14 08:00"), None);
        let m = NotifyClock::default()
            .tick(&db, &s, &Warsaw, at("2026-10-14 18:00"))
            .unwrap();
        assert_eq!(m.len(), 4);
        // Termin: Zadania z filtrem „Dzisiaj”, bez przycisków.
        assert!(m[0].actions.is_empty());
        assert_eq!(
            m[0].open,
            View {
                filter: Some("due"),
                ..View::tasks()
            }
        );
        // Długi pomiar: rozwinięte bieżące zadanie, „Zatrzymaj” i „Podziel…”.
        assert_eq!(
            m[1].actions,
            [
                Action::new(STOP, "Zatrzymaj", id),
                Action::new(SPLIT, "Podziel…", id)
            ]
        );
        assert_eq!(
            m[1].actions
                .iter()
                .map(Action::argument)
                .collect::<Vec<_>>(),
            [format!("stop:{id}"), format!("split:{id}")]
        );
        assert_eq!(
            m[1].open,
            View {
                task: Some(2),
                ..View::tasks()
            }
        );
        // Wieczór: Zamknięcie dnia i „Zatrzymaj teraz”.
        assert_eq!(m[2].actions, [Action::new(STOP, "Zatrzymaj teraz", id)]);
        assert_eq!(m[2].open, View::day_close());
        // Cel: Zamknięcie dnia, bez przycisków.
        assert!(m[3].actions.is_empty());
        assert_eq!(m[3].open, View::day_close());
        // Ładunek zdarzenia `navigate` dla frontendu.
        assert_eq!(
            serde_json::to_value(&m[1].open).unwrap(),
            json!({"view": "tasks", "filter": null, "task": 2, "split": null,
                   "section": null, "dayClose": false})
        );
        assert_eq!(
            serde_json::to_value(View::day_close()).unwrap()["dayClose"],
            json!(true)
        );
        // Zakładka Ustawienia na wybranej sekcji: `section` to identyfikator sekcji.
        let settings = View {
            view: Some("settings"),
            section: Some("notifications"),
            ..View::default()
        };
        assert_eq!(
            serde_json::to_value(&settings).unwrap(),
            json!({"view": "settings", "filter": null, "task": null, "split": null,
                   "section": "notifications", "dayClose": false})
        );
    }

    #[test]
    fn unknown_action_only_shows_main_window() {
        let mut db = setup();
        let id = session(&db, 1, at("2026-10-14 08:00"), None);
        let open = View {
            task: Some(1),
            ..View::tasks()
        };
        // Kliknięcie treści: cel z powiadomienia.
        assert_eq!(command(None, &open), Command::Open(open.clone()));
        assert_eq!(command(Some(""), &open), Command::Open(open.clone()));
        for bad in [
            "bogus",
            "bogus:1",
            "stop",
            "stop:abc",
            "stop:-1",
            "split:",
            "plan-today:1,x",
            "jira-start:0",
        ] {
            assert_eq!(
                command(Some(bad), &open),
                Command::Open(View::default()),
                "{bad}"
            );
        }
        let before = db::snapshot(&db).unwrap();
        let out = perform(
            &mut db,
            command(Some("bogus:1"), &open),
            "2026-10-14",
            at("2026-10-14 09:00"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: false,
                navigate: Some(View::default())
            }
        );
        let after = db::snapshot(&db).unwrap();
        assert_eq!(after.sessions.len(), before.sessions.len());
        assert!(after.sessions.iter().any(|s| s.ended_at.is_none()));
        // „Zatrzymaj” ze starego toastu nie zatrzymuje późniejszego pomiaru.
        let out = perform(
            &mut db,
            Command::Stop(id + 100),
            "2026-10-14",
            at("2026-10-14 09:00"),
        )
        .unwrap();
        assert!(!out.changed);
        assert!(active(&db).unwrap().is_some());
        let out = perform(
            &mut db,
            command(Some(&format!("stop:{id}")), &open),
            "2026-10-14",
            at("2026-10-14 09:00"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: true,
                navigate: None
            }
        );
        assert!(active(&db).unwrap().is_none());
    }

    #[test]
    fn split_action_opens_editor_on_current_measurement() {
        let mut db = setup();
        let first = session(&db, 1, at("2026-10-14 08:00"), None);
        db::split_session(
            &mut db,
            first,
            at("2026-10-14 09:00"),
            None,
            at("2026-10-14 12:00"),
        )
        .unwrap();
        let (current, ..) = active(&db).unwrap().unwrap();
        assert_ne!(current, first);
        let out = perform(
            &mut db,
            command(Some(&format!("split:{first}")), &View::default()),
            "2026-10-14",
            at("2026-10-14 12:00"),
        )
        .unwrap();
        assert!(!out.changed);
        assert_eq!(
            out.navigate,
            Some(View {
                task: Some(1),
                split: Some(current),
                ..View::tasks()
            })
        );
        // Po Stop: edytor zakończonego przedziału z toastu.
        db::stop(&db, at("2026-10-14 12:30")).unwrap();
        let out = perform(
            &mut db,
            Command::Split(first),
            "2026-10-14",
            at("2026-10-14 13:00"),
        )
        .unwrap();
        assert_eq!(out.navigate.unwrap().split, Some(first));
    }

    #[test]
    fn split_action_from_stale_toast_ignores_other_task_measurement() {
        let mut db = setup();
        // Toast o długim pomiarze zadania A (1); potem użytkownik przechodzi na B (2).
        let a = session(&db, 1, at("2026-10-14 08:00"), Some(at("2026-10-14 12:00")));
        let b = session(&db, 2, at("2026-10-14 12:00"), None);
        let out = perform(
            &mut db,
            command(Some(&format!("split:{a}")), &View::default()),
            "2026-10-14",
            at("2026-10-14 12:30"),
        )
        .unwrap();
        assert_eq!(
            out.navigate,
            Some(View {
                task: Some(1),
                split: Some(a),
                ..View::tasks()
            })
        );
        assert_eq!(active(&db).unwrap().unwrap().0, b);
        // Ponowny start A później to nowy pomiar, nie dalszy ciąg toastu.
        db::stop(&db, at("2026-10-14 13:00")).unwrap();
        let again = session(&db, 1, at("2026-10-14 13:05"), None);
        let out = perform(
            &mut db,
            Command::Split(a),
            "2026-10-14",
            at("2026-10-14 13:10"),
        )
        .unwrap();
        assert_eq!(out.navigate.unwrap().split, Some(a));
        let out = perform(
            &mut db,
            Command::Stop(a),
            "2026-10-14",
            at("2026-10-14 13:10"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: false,
                navigate: Some(View::tasks())
            }
        );
        assert_eq!(active(&db).unwrap().unwrap().0, again);
        // Przedziału z toastu już nie ma: sama lista zadań, bez edytora.
        db.execute("DELETE FROM sessions WHERE id=?1", [a]).unwrap();
        let out = perform(
            &mut db,
            Command::Split(a),
            "2026-10-14",
            at("2026-10-14 13:10"),
        )
        .unwrap();
        assert_eq!(out.navigate, Some(View::tasks()));
    }

    #[test]
    fn stop_action_after_split_stops_continued_measurement() {
        let mut db = setup();
        let first = session(&db, 1, at("2026-10-14 08:00"), None);
        // Dwa podziały: pomiar trwa w trzecim wierszu tego samego zadania.
        db::split_session(
            &mut db,
            first,
            at("2026-10-14 09:00"),
            None,
            at("2026-10-14 12:00"),
        )
        .unwrap();
        let (second, ..) = active(&db).unwrap().unwrap();
        db::split_session(
            &mut db,
            second,
            at("2026-10-14 10:00"),
            None,
            at("2026-10-14 12:00"),
        )
        .unwrap();
        let (third, ..) = active(&db).unwrap().unwrap();
        assert!(third != first && third != second);
        let out = perform(
            &mut db,
            command(Some(&format!("split:{first}")), &View::default()),
            "2026-10-14",
            at("2026-10-14 12:00"),
        )
        .unwrap();
        assert_eq!(out.navigate.unwrap().split, Some(third));
        let out = perform(
            &mut db,
            command(Some(&format!("stop:{first}")), &View::default()),
            "2026-10-14",
            at("2026-10-14 12:30"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: true,
                navigate: None
            }
        );
        assert!(active(&db).unwrap().is_none());
        // Drugie kliknięcie: nic do zatrzymania, okno pokazuje listę zadań.
        let out = perform(
            &mut db,
            Command::Stop(first),
            "2026-10-14",
            at("2026-10-14 12:31"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: false,
                navigate: Some(View::tasks())
            }
        );
    }

    fn morning(db: &Connection, s: &AppSettings, when: &str) -> Option<Message> {
        morning_review(db, s, &Warsaw, at(when)).unwrap()
    }

    #[test]
    fn morning_review_fires_once_after_day_start_on_work_day() {
        let db = setup();
        let s = AppSettings::default();
        // Czwartek 15.10.2026: wczoraj niedokończone A, zaległy termin B, wzorzec
        // cyklu C (pomijany), ukończone D.
        details(&db, 1, json!({"plannedDay": "2026-10-14"}));
        details(&db, 2, json!({"dueDay": "2026-10-15"}));
        details(
            &db,
            3,
            json!({"plannedDay": "2026-10-12", "repeatDays": [1, 2, 3, 4, 5]}),
        );
        details(&db, 4, json!({"plannedDay": "2026-10-14"}));
        db.execute("UPDATE tasks SET done=1 WHERE id=4", [])
            .unwrap();
        // Wzorzec reguły miesięcznej (P3) też nie jest niedokończonym planem.
        details(
            &db,
            5,
            json!({"plannedDay": "2026-10-01", "repeatRule": {"kind": "monthDay", "day": 1}}),
        );
        // Przed początkiem dnia (08:00): nic i bez klucza.
        assert!(morning(&db, &s, "2026-10-15 07:59").is_none());
        assert!(!sent(&db).contains(&"morning:2026-10-15".to_owned()));
        let m = morning(&db, &s, "2026-10-15 08:00").unwrap();
        assert_eq!(m.key, "morning:2026-10-15");
        assert_eq!(m.title, "Przegląd dnia");
        assert_eq!(m.body, "Niedokończone: 1 zadanie · terminy: 1 zadanie");
        assert!(m.actions.is_empty());
        assert_eq!(m.open, View::morning_review());
        assert_eq!(
            serde_json::to_value(&m.open).unwrap(),
            json!({"view": "journal", "filter": null, "task": null, "split": null,
                   "section": "morning", "dayClose": false})
        );
        // Drugi raz tego dnia nie przychodzi, także po restarcie.
        assert!(morning(&db, &s, "2026-10-15 08:00").is_none());
        assert!(morning(&db, &s, "2026-10-15 15:00").is_none());
        // Własny początek dnia: następny dzień roboczy o 09:30.
        let late = AppSettings {
            day_start: "09:30".into(),
            ..s.clone()
        };
        assert!(morning(&db, &late, "2026-10-16 09:29").is_none());
        assert!(morning(&db, &late, "2026-10-16 09:30").is_some());
    }

    #[test]
    fn morning_review_skips_free_days_holidays_and_reviewed_days() {
        let mut db = setup();
        let s = AppSettings::default();
        // Sobota i niedziela: dni wolne.
        assert!(morning(&db, &s, "2026-10-17 10:00").is_none());
        assert!(morning(&db, &s, "2026-10-18 10:00").is_none());
        // Święto w dzień roboczy (środa 11.11.2026), chyba że święta nie są
        // uwzględniane.
        assert!(morning(&db, &s, "2026-11-11 10:00").is_none());
        let no_holidays = AppSettings {
            holidays_in_target: false,
            ..s.clone()
        };
        assert!(morning(&db, &no_holidays, "2026-11-11 10:00").is_some());
        // Sobota jako dzień roboczy z ustawień.
        let saturday = AppSettings {
            work_days: vec![6],
            ..s.clone()
        };
        assert!(morning(&db, &saturday, "2026-10-17 10:00").is_some());
        // Już przejrzane dziś: nic. Przegląd z wczoraj nie liczy się dziś.
        let reviewed = AppSettings {
            morning_review_day: "2026-10-19".into(),
            ..s.clone()
        };
        assert!(morning(&db, &reviewed, "2026-10-19 10:00").is_none());
        assert!(!sent(&db).contains(&"morning:2026-10-19".to_owned()));
        let yesterday = AppSettings {
            morning_review_day: "2026-10-19".into(),
            ..s.clone()
        };
        assert!(morning(&db, &yesterday, "2026-10-20 10:00").is_some());
        // Wyłącznik w Ustawieniach → Powiadomienia.
        let off = AppSettings {
            notify_morning_review: false,
            ..s.clone()
        };
        assert!(morning(&db, &off, "2026-10-21 10:00").is_none());
        assert!(!sent(&db).contains(&"morning:2026-10-21".to_owned()));
        // Urlop całodniowy z kalendarza: dzień wolny.
        let (from, to) = crate::stats::day_bounds(
            &Warsaw,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap(),
        );
        crate::calendar::replace_range(
            &mut db,
            at("2026-10-19 00:00"),
            at("2026-10-26 00:00"),
            &[crate::calendar::CalendarEvent {
                id: "u".into(),
                start: from,
                end: to,
                subject: "Urlop".into(),
                show_as: "oof".into(),
                all_day: true,
                cancelled: false,
                response: "accepted".into(),
            }],
            at("2026-10-19 07:00"),
        )
        .unwrap();
        crate::calendar::set_test_email(&db, "jan@firma.example");
        assert!(morning(&db, &s, "2026-10-22 10:00").is_none());
        assert!(morning(&db, &s, "2026-10-23 10:00").is_some());
    }

    #[test]
    fn morning_review_counts_task_planned_yesterday_and_due_today_once() {
        let db = setup();
        let s = AppSettings::default();
        // A: plan z wczoraj i termin dziś — tylko w „Niedokończone”, jak w widoku.
        details(
            &db,
            1,
            json!({"plannedDay": "2026-10-14", "dueDay": "2026-10-15"}),
        );
        let m = morning(&db, &s, "2026-10-15 08:00").unwrap();
        assert_eq!(m.body, "Niedokończone: 1 zadanie");
        // B: wzorzec cyklu z terminem dziś nie trafia do kroku 1, więc liczy się
        // w terminach; C: zaplanowane dziś z terminem dziś — też termin.
        details(
            &db,
            2,
            json!({"plannedDay": "2026-10-12", "repeatDays": [1, 2, 3, 4, 5],
                   "dueDay": "2026-10-16"}),
        );
        details(
            &db,
            3,
            json!({"plannedDay": "2026-10-16", "dueDay": "2026-10-16"}),
        );
        let m = morning(&db, &s, "2026-10-16 08:00").unwrap();
        assert_eq!(m.body, "Niedokończone: 1 zadanie · terminy: 2 zadania");
    }

    #[test]
    fn morning_message_without_work_left_invites_to_plan() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        assert_eq!(
            morning_message(day, 0, 0).body,
            "Zaplanuj dzień: spotkania, plan i pojemność."
        );
        assert_eq!(morning_message(day, 3, 0).body, "Niedokończone: 3 zadania");
        assert_eq!(morning_message(day, 0, 5).body, "Terminy: 5 zadań");
    }

    fn week(db: &Connection, s: &AppSettings, when: &str) -> Option<Message> {
        week_review(db, s, &Warsaw, at(when)).unwrap()
    }

    #[test]
    fn week_review_fires_once_on_last_work_day_after_set_time() {
        let db = setup();
        let s = AppSettings::default();
        // Czwartek 15.10.2026 to nie ostatni dzień roboczy tygodnia.
        assert!(week(&db, &s, "2026-10-15 16:00").is_none());
        // Piątek przed 15:00: nic i bez klucza.
        assert!(week(&db, &s, "2026-10-16 14:59").is_none());
        assert!(!sent(&db).contains(&"week:2026-10-12".to_owned()));
        let m = week(&db, &s, "2026-10-16 15:00").unwrap();
        assert_eq!(m.key, "week:2026-10-12");
        assert_eq!(m.title, "Przegląd tygodnia");
        assert!(m.actions.is_empty());
        assert_eq!(m.open, View::week_review());
        assert_eq!(
            serde_json::to_value(&m.open).unwrap(),
            json!({"view": "journal", "filter": null, "task": null, "split": null,
                   "section": "week-review", "dayClose": false})
        );
        // Drugi raz w tym tygodniu nie przychodzi, także po restarcie; weekend nic.
        assert!(week(&db, &s, "2026-10-16 17:00").is_none());
        assert!(week(&db, &s, "2026-10-17 15:00").is_none());
        assert!(week(&db, &s, "2026-10-18 15:00").is_none());
        // Własna godzina i wyłącznik.
        let late = AppSettings {
            week_review_time: "16:30".into(),
            ..s.clone()
        };
        assert!(week(&db, &late, "2026-10-23 16:29").is_none());
        assert!(week(&db, &late, "2026-10-23 16:30").is_some());
        let off = AppSettings {
            notify_week_review: false,
            ..s.clone()
        };
        assert!(week(&db, &off, "2026-10-30 15:00").is_none());
        assert!(!sent(&db).contains(&"week:2026-10-26".to_owned()));
        // Sobota jako dzień roboczy: ostatni jest sobota, nie piątek.
        let saturday = AppSettings {
            work_days: vec![1, 2, 3, 4, 5, 6],
            ..s.clone()
        };
        assert!(week(&db, &saturday, "2026-11-06 15:00").is_none());
        assert!(week(&db, &saturday, "2026-11-07 15:00").is_some());
    }

    #[test]
    fn week_review_moves_to_earlier_day_before_holiday() {
        let db = setup();
        let s = AppSettings::default();
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let none = |_: NaiveDate| false;
        // Piątek 1.05.2026 to Święto Pracy: ostatni dzień roboczy to czwartek 30.04.
        assert!(last_work_day_of_week(&s, day(2026, 4, 30), none));
        assert!(!last_work_day_of_week(&s, day(2026, 5, 1), none));
        assert!(week(&db, &s, "2026-05-01 15:00").is_none());
        let m = week(&db, &s, "2026-04-30 15:00").unwrap();
        assert_eq!(m.key, "week:2026-04-27");
        // Wigilia i Boże Narodzenie (czwartek i piątek): środa 23.12.
        assert!(last_work_day_of_week(&s, day(2026, 12, 23), none));
        assert!(!last_work_day_of_week(&s, day(2026, 12, 22), none));
        assert!(week(&db, &s, "2026-12-23 15:00").is_some());
        // Bez świąt w celu piątek 1.05 jest zwykłym ostatnim dniem roboczym.
        let no_holidays = AppSettings {
            holidays_in_target: false,
            ..s.clone()
        };
        assert!(!last_work_day_of_week(&no_holidays, day(2026, 4, 30), none));
        assert!(last_work_day_of_week(&no_holidays, day(2026, 5, 1), none));
        // Bez dni roboczych w ustawieniach nie ma ostatniego dnia roboczego.
        let nothing = AppSettings {
            work_days: Vec::new(),
            ..s.clone()
        };
        assert!(!last_work_day_of_week(&nothing, day(2026, 10, 16), none));
    }

    #[test]
    fn week_review_skips_calendar_off_days() {
        let mut db = setup();
        let s = AppSettings::default();
        // Urlop całodniowy w piątek 23.10: przegląd w czwartek 22.10.
        let (from, to) = crate::stats::day_bounds(
            &Warsaw,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        );
        crate::calendar::replace_range(
            &mut db,
            at("2026-10-19 00:00"),
            at("2026-10-26 00:00"),
            &[crate::calendar::CalendarEvent {
                id: "u".into(),
                start: from,
                end: to,
                subject: "Urlop".into(),
                show_as: "oof".into(),
                all_day: true,
                cancelled: false,
                response: "accepted".into(),
            }],
            at("2026-10-19 07:00"),
        )
        .unwrap();
        crate::calendar::set_test_email(&db, "jan@firma.example");
        assert!(week(&db, &s, "2026-10-23 15:00").is_none());
        let m = week(&db, &s, "2026-10-22 15:00").unwrap();
        assert_eq!(m.key, "week:2026-10-19");
    }

    fn waiting(db: &Connection, s: &AppSettings, when: &str) -> Vec<Message> {
        waiting_reminders(db, s, &Warsaw, at(when)).unwrap()
    }

    #[test]
    fn waiting_reminder_after_work_days_once_per_block() {
        let db = setup();
        let s = AppSettings::default();
        // Blokada od czwartku 15.10.2026 10:00. Dni robocze po niej: pt 16,
        // pn 19, wt 20 (weekend się nie liczy), więc po 3 dniach we wtorek.
        let since = at("2026-10-15 10:00");
        details(&db, 1, json!({"waiting": "klient", "waitingSince": since}));
        // Stara blokada bez `waitingSince`, ukończone i zablokowane bez czasu: pomijane.
        details(&db, 2, json!({"waiting": "dostawca"}));
        details(&db, 3, json!({"waiting": "x", "waitingSince": since}));
        db.execute("UPDATE tasks SET done=1 WHERE id=3", [])
            .unwrap();
        assert!(waiting(&db, &s, "2026-10-19 12:00").is_empty());
        // Sobota i niedziela: nie dni robocze.
        assert!(waiting(&db, &s, "2026-10-18 12:00").is_empty());
        let m = waiting(&db, &s, "2026-10-20 09:00");
        assert_eq!(keys(&m), [format!("waiting:1:{since}").as_str()]);
        assert_eq!(m[0].title, "Czekam na");
        assert_eq!(m[0].body, "Czekasz na: klient — A (3 dni)");
        assert_eq!(
            m[0].actions,
            [
                Action::new(WAITING_UNBLOCK, "Odblokuj", 1),
                Action::new(WAITING_REMIND, "Przypomnij jutro", 1)
            ]
        );
        assert_eq!(
            m[0].actions
                .iter()
                .map(Action::argument)
                .collect::<Vec<_>>(),
            ["waiting-unblock:1", "waiting-remind:1"]
        );
        assert_eq!(m[0].open.task, Some(1));
        assert_eq!(m[0].open.section, Some("Czekam na…"));
        // Raz na blokadę, także po restarcie i po 7-dniowym sprzątaniu.
        assert!(waiting(&db, &s, "2026-10-21 09:00").is_empty());
        cleanup(&db, at("2026-11-30 09:00")).unwrap();
        assert!(waiting(&db, &s, "2026-12-01 09:00").is_empty());
        // Wyłączone (0) albo wyższy próg.
        let db = setup();
        details(&db, 1, json!({"waiting": "klient", "waitingSince": since}));
        let off = AppSettings {
            waiting_notify_days: 0,
            ..AppSettings::default()
        };
        assert!(waiting(&db, &off, "2026-10-30 09:00").is_empty());
        let five = AppSettings {
            waiting_notify_days: 5,
            ..AppSettings::default()
        };
        assert!(waiting(&db, &five, "2026-10-21 09:00").is_empty());
        assert_eq!(waiting(&db, &five, "2026-10-22 09:00").len(), 1);
    }

    #[test]
    fn waiting_reminder_counts_holidays_from_settings() {
        // Blokada od piątku 6.11.2026: pn 9, wt 10, (śr 11.11 święto), czw 12,
        // więc trzeci dzień roboczy to czwartek.
        let db = setup();
        let since = at("2026-11-06 10:00");
        details(&db, 1, json!({"waiting": "klient", "waitingSince": since}));
        let s = AppSettings::default();
        // Święto: dzień wolny, brak przypomnienia.
        assert!(waiting(&db, &s, "2026-11-11 10:00").is_empty());
        assert_eq!(waiting(&db, &s, "2026-11-12 10:00").len(), 1);
        let db = setup();
        details(&db, 1, json!({"waiting": "klient", "waitingSince": since}));
        let no_holidays = AppSettings {
            holidays_in_target: false,
            ..AppSettings::default()
        };
        assert_eq!(waiting(&db, &no_holidays, "2026-11-11 10:00").len(), 1);
        assert_eq!(
            work_days_between(
                &s,
                NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
            ),
            3
        );
    }

    #[test]
    fn waiting_actions_unblock_and_remind_tomorrow() {
        let mut db = setup();
        let s = AppSettings::default();
        let since = at("2026-10-12 10:00");
        details(&db, 1, json!({"waiting": "klient", "waitingSince": since}));
        details(
            &db,
            2,
            json!({"waiting": "dostawca", "waitingSince": since}),
        );
        assert_eq!(waiting(&db, &s, "2026-10-15 09:00").len(), 2);
        // „Przypomnij jutro” z toastu zadania 1 w czwartek → piątek.
        let out = perform(
            &mut db,
            command(Some("waiting-remind:1"), &View::default()),
            "2026-10-15",
            at("2026-10-15 09:01"),
        )
        .unwrap();
        assert!(out.changed && out.navigate.is_none());
        assert!(waiting(&db, &s, "2026-10-15 15:00").is_empty());
        let m = waiting(&db, &s, "2026-10-16 09:00");
        assert_eq!(keys(&m), [format!("waiting:1:{since}:2026-10-16").as_str()]);
        assert_eq!(m[0].body, "Czekasz na: klient — A (4 dni)");
        // Ponownie „Przypomnij jutro” w piątek → sobota; przychodzi w poniedziałek.
        perform(
            &mut db,
            command(Some("waiting-remind:1"), &View::default()),
            "2026-10-16",
            at("2026-10-16 09:01"),
        )
        .unwrap();
        assert!(waiting(&db, &s, "2026-10-17 09:00").is_empty());
        assert_eq!(waiting(&db, &s, "2026-10-19 09:00").len(), 1);
        // „Odblokuj” zdejmuje blokadę zadania 2, timer bez zmian.
        let out = perform(
            &mut db,
            command(Some("waiting-unblock:2"), &View::default()),
            "2026-10-19",
            at("2026-10-19 09:02"),
        )
        .unwrap();
        assert!(out.changed);
        let snap = db::snapshot(&db).unwrap();
        let two = snap.tasks.iter().find(|t| t.id == 2).unwrap();
        assert_eq!(two.details.waiting, "");
        assert_eq!(two.details.waiting_since, 0);
        assert!(snap.sessions.is_empty());
        // Zadanie, którego już nie ma: tylko lista zadań.
        let out = perform(
            &mut db,
            command(Some("waiting-unblock:99"), &View::default()),
            "2026-10-19",
            at("2026-10-19 09:03"),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                changed: false,
                navigate: Some(View::tasks())
            }
        );
    }

    #[test]
    fn split_to_other_task_is_not_continuation_of_toast() {
        let mut db = setup();
        let first = session(&db, 1, at("2026-10-14 08:00"), None);
        db::split_session(
            &mut db,
            first,
            at("2026-10-14 09:00"),
            Some(2),
            at("2026-10-14 12:00"),
        )
        .unwrap();
        let out = perform(
            &mut db,
            Command::Stop(first),
            "2026-10-14",
            at("2026-10-14 12:00"),
        )
        .unwrap();
        assert!(!out.changed);
        assert!(active(&db).unwrap().is_some());
    }
}
