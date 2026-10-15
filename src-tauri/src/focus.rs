//! Tryb skupienia (PLAN_1.3.md, etap 6).
//!
//! Blok skupienia to wiersz `focus_blocks(id, task_id, start, end, completed)`:
//! trwający blok ma `completed IS NULL`, a `end` to planowany koniec. Koniec liczymy
//! z zegara ściennego (`now >= end`), nie z odliczania, więc blok kończy się dobrze
//! także po uśpieniu komputera: pierwsze sprawdzenie po wybudzeniu zamyka go z końcem
//! równym planowanemu.
//!
//! - Start bloku uruchamia pomiar wybranego zadania, jeśli nie trwa (poprzedni staje).
//! - Stop albo pomiar innego zadania kończy blok jako nieukończony (`completed = 0`,
//!   `end` = chwila przerwania), bez pytań.
//! - W czasie bloku powiadomienia inne niż „koniec bloku” trafiają do kolejki
//!   (`notify::show` → `hold`) i przychodzą po bloku jako jedno zbiorcze. Kolejka
//!   jest też w tabeli `focus_queue` (klucz, tytuł, treść), bo `notifications_sent`
//!   oznacza je jako wysłane już przy odkładaniu: po zamknięciu aplikacji w trakcie
//!   bloku wracają po starcie (bez przycisków) i przychodzą po bloku.
//! - Koniec bloku: powiadomienie z „Przerwa” i „Kolejny blok”. Przerwa domyślnie
//!   zatrzymuje pomiar (`focus_break_stops`), a po niej przychodzi „Wracasz do X?”
//!   z „Rozpocznij”.
//!
//! Timer nigdy nie przełącza się sam: pomiar zmieniają tylko kliknięcia użytkownika
//! (Skupienie, Przerwa, Kolejny blok, Rozpocznij). Zakończony blok, przerwa
//! i „Wracasz do X?” żyją w pamięci (po restarcie aplikacji ich nie ma); trwający
//! blok jest w bazie i przeżywa restart.
use crate::notify::{self, Action, Command, Message, Outcome, View};
use crate::settings::AppSettings;
use crate::{db, stats, Database};
use chrono::{Datelike, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Co ile wątek sprawdza koniec bloku i przerwy.
const CHECK: Duration = Duration::from_secs(2);
const MIN: i64 = 60_000;
/// Tyle tytułów z kolejki wymieniamy w zbiorczym powiadomieniu.
const SUMMARY_TITLES: usize = 4;

/// Blok skupienia (trwający albo zakończony).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: i64,
    pub task_id: i64,
    pub title: String,
    pub start: i64,
    pub end: i64,
}

/// Przerwa po bloku; `stops`: przerwa zatrzymała pomiar.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pause {
    pub task_id: i64,
    pub title: String,
    pub start: i64,
    pub until: i64,
    pub stops: bool,
}

/// „Wracasz do X?” po przerwie.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Return {
    pub task_id: i64,
    pub title: String,
}

/// Stan dla frontendu (komenda `focus_status`, zdarzenie `focus-changed`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub block: Option<Block>,
    /// Ukończony blok czekający na „Przerwa” albo „Kolejny blok”.
    pub ended: Option<Block>,
    pub pause: Option<Pause>,
    pub resume: Option<Return>,
    /// Powiadomienia czekające na koniec bloku.
    pub queued: usize,
    /// Ukończone bloki dziś i w tym tygodniu (od poniedziałku).
    pub today: i64,
    pub week: i64,
}

/// Wynik sprawdzenia: czy stan się zmienił i co wysłać (już poza kolejką).
#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    pub changed: bool,
    pub messages: Vec<Message>,
}

/// Stan trybu skupienia w pamięci.
#[derive(Debug, Default)]
pub struct Focus {
    /// Kopia trwającego bloku z bazy, żeby `hold` nie potrzebował bazy.
    running: Option<Block>,
    ended: Option<Block>,
    pause: Option<Pause>,
    resume: Option<Return>,
    queue: Vec<Message>,
}

fn str_err(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Trwający blok (z tytułem zadania).
pub fn running(db: &Connection) -> rusqlite::Result<Option<Block>> {
    db.query_row(
        r#"SELECT f.id, f.task_id, COALESCE(t.title, ''), f.start, f."end"
           FROM focus_blocks f LEFT JOIN tasks t ON t.id = f.task_id
           WHERE f.completed IS NULL ORDER BY f.id DESC LIMIT 1"#,
        [],
        |r| {
            Ok(Block {
                id: r.get(0)?,
                task_id: r.get(1)?,
                title: r.get(2)?,
                start: r.get(3)?,
                end: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Bieżący pomiar: zadanie i początek przedziału.
fn active(db: &Connection) -> rusqlite::Result<Option<(i64, i64)>> {
    db.query_row(
        "SELECT task_id, started_at FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

fn title(db: &Connection, task: i64) -> rusqlite::Result<String> {
    Ok(db
        .query_row("SELECT title FROM tasks WHERE id=?1", [task], |r| r.get(0))
        .optional()?
        .unwrap_or_default())
}

/// Ostatni koniec przedziału zadania od początku bloku (chwila Stop).
fn last_end(db: &Connection, task: i64, since: i64) -> rusqlite::Result<Option<i64>> {
    db.query_row(
        "SELECT MAX(ended_at) FROM sessions WHERE task_id=?1 AND ended_at>=?2",
        params![task, since],
        |r| r.get(0),
    )
}

/// Kolejka zapisana w `focus_queue` (bez przycisków: zostają klucz, tytuł i treść).
fn saved_queue(db: &Connection) -> rusqlite::Result<Vec<Message>> {
    db.prepare("SELECT key, title, body FROM focus_queue ORDER BY id")?
        .query_map([], |r| {
            Ok(Message {
                key: r.get(0)?,
                title: r.get(1)?,
                body: r.get(2)?,
                actions: Vec::new(),
                open: View::default(),
            })
        })?
        .collect()
}

/// Zamyka blok: ukończony z planowanym końcem albo przerwany w chwili `at`.
fn close(db: &Connection, block: &Block, at: i64, completed: bool) -> rusqlite::Result<()> {
    let end = if completed {
        block.end
    } else {
        at.max(block.start).min(block.end)
    };
    db.execute(
        r#"UPDATE focus_blocks SET "end"=?1, completed=?2 WHERE id=?3"#,
        params![end, completed, block.id],
    )?;
    Ok(())
}

/// „1 powiadomienie”, „3 powiadomienia”, „5 powiadomień”.
pub fn notifications_word(n: usize) -> String {
    let word = if n == 1 {
        "powiadomienie"
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "powiadomienia"
    } else {
        "powiadomień"
    };
    format!("{n} {word}")
}

/// Kolejka po bloku: jedno powiadomienie przechodzi bez zmian (z przyciskami),
/// kilka składa się w jedno zbiorcze z tytułami.
pub fn summary(queue: Vec<Message>) -> Vec<Message> {
    if queue.len() <= 1 {
        return queue;
    }
    let mut lines: Vec<String> = queue
        .iter()
        .take(SUMMARY_TITLES)
        .map(|m| format!("• {}", notify::short(&m.title)))
        .collect();
    if queue.len() > SUMMARY_TITLES {
        lines.push(format!("i jeszcze {}", queue.len() - SUMMARY_TITLES));
    }
    vec![Message {
        key: String::new(),
        title: format!("Podczas skupienia: {}", notifications_word(queue.len())),
        body: lines.join("\n"),
        actions: Vec::new(),
        open: View::default(),
    }]
}

/// Powiadomienie o końcu bloku z „Przerwa” i „Kolejny blok”.
pub fn end_message(block: &Block) -> Message {
    Message {
        key: String::new(),
        title: "Koniec bloku skupienia".into(),
        body: format!(
            "{} · {} min. Przerwa czy kolejny blok?",
            notify::short(&block.title),
            (block.end - block.start) / MIN
        ),
        actions: vec![
            Action::new(notify::FOCUS_BREAK, "Przerwa", block.id),
            Action::new(notify::FOCUS_NEXT, "Kolejny blok", block.task_id),
        ],
        open: View::tasks(),
    }
}

/// Koniec przerwy: „Wracasz do X?” z „Rozpocznij”, a gdy pomiar trwał przez
/// przerwę — tylko „Kolejny blok”.
pub fn pause_message(pause: &Pause, measuring: bool) -> Message {
    if measuring {
        Message {
            key: String::new(),
            title: "Koniec przerwy".into(),
            body: format!("{} · pomiar trwa", notify::short(&pause.title)),
            actions: vec![Action::new(
                notify::FOCUS_NEXT,
                "Kolejny blok",
                pause.task_id,
            )],
            open: View::tasks(),
        }
    } else {
        Message {
            key: String::new(),
            title: format!("Wracasz do {}?", notify::short(&pause.title)),
            body: "Przerwa minęła.".into(),
            actions: vec![Action::new(
                notify::FOCUS_RESUME,
                "Rozpocznij",
                pause.task_id,
            )],
            open: View::tasks(),
        }
    }
}

impl Focus {
    /// Stan po starcie aplikacji: trwający blok i kolejka z bazy.
    pub fn load(db: &Connection) -> Self {
        Focus {
            running: running(db).ok().flatten(),
            queue: saved_queue(db).unwrap_or_default(),
            ..Focus::default()
        }
    }

    /// Czy trwa blok (powiadomienia czekają w kolejce).
    pub fn holds(&self) -> bool {
        self.running.is_some()
    }

    /// Blok albo przerwa: przypomnienie o braku pomiaru milczy.
    pub fn quiet(&self) -> bool {
        self.running.is_some() || self.pause.is_some()
    }

    /// Odkłada powiadomienie tylko w pamięci.
    pub fn enqueue(&mut self, message: Message) {
        self.queue.push(message);
    }

    /// Odkłada powiadomienie w pamięci i w `focus_queue` (przeżywa restart).
    pub fn enqueue_saved(&mut self, db: &Connection, message: Message) -> rusqlite::Result<()> {
        db.execute(
            "INSERT INTO focus_queue(key, title, body) VALUES(?1, ?2, ?3)",
            params![message.key, message.title, message.body],
        )?;
        self.queue.push(message);
        Ok(())
    }

    /// Opróżnia kolejkę (pamięć i tabelę) i zwraca ją jako zbiorcze powiadomienie.
    fn flush(&mut self, db: &Connection) -> rusqlite::Result<Vec<Message>> {
        db.execute("DELETE FROM focus_queue", [])?;
        Ok(summary(std::mem::take(&mut self.queue)))
    }

    /// Start bloku na zadaniu `task` (domyślnie mierzonym). Pomiar startuje, jeśli
    /// nie trwa; trwający blok innego zadania kończy się jako przerwany.
    pub fn start(
        &mut self,
        db: &mut Connection,
        task: Option<i64>,
        s: &AppSettings,
        now: i64,
    ) -> Result<(), String> {
        let current = active(db).map_err(str_err)?;
        let task = task
            .or(current.map(|(t, _)| t))
            .ok_or("Uruchom pomiar albo wybierz zadanie, żeby zacząć blok skupienia.")?;
        let previous = running(db).map_err(str_err)?;
        if let Some(block) = &previous {
            if block.task_id == task && current.is_some_and(|(t, _)| t == task) {
                self.running = previous;
                return Ok(());
            }
        }
        // Najpierw pomiar: gdy zadania nie da się uruchomić (np. zrobione), trwający
        // blok i pomiar zostają bez zmian.
        if current.map(|(t, _)| t) != Some(task) {
            db::change(db, "start", Some(task), None, now)?;
        }
        if let Some(block) = &previous {
            close(db, block, now, false).map_err(str_err)?;
        }
        db.execute(
            r#"INSERT INTO focus_blocks(task_id, start, "end") VALUES(?1, ?2, ?3)"#,
            params![
                task,
                now,
                now + i64::from(s.focus_block_minutes.max(1)) * MIN
            ],
        )
        .map_err(str_err)?;
        self.ended = None;
        self.pause = None;
        self.resume = None;
        self.running = running(db).map_err(str_err)?;
        Ok(())
    }

    /// „Zakończ blok”: blok przerwany teraz, pomiar trwa dalej. Zwraca kolejkę.
    pub fn stop(&mut self, db: &Connection, now: i64) -> Result<Vec<Message>, String> {
        if let Some(block) = running(db).map_err(str_err)? {
            close(db, &block, now, false).map_err(str_err)?;
        }
        self.running = None;
        self.flush(db).map_err(str_err)
    }

    /// „Przerwa” po ukończonym bloku (`block`: id z powiadomienia; stary toast
    /// innego bloku nic nie robi). Domyślnie zatrzymuje pomiar.
    pub fn take_break(
        &mut self,
        db: &Connection,
        s: &AppSettings,
        block: Option<i64>,
        now: i64,
    ) -> Result<bool, String> {
        let Some(ended) = self
            .ended
            .clone()
            .filter(|b| block.is_none_or(|id| id == b.id))
        else {
            return Ok(false);
        };
        if s.focus_break_stops {
            db::stop(db, now).map_err(str_err)?;
        }
        self.pause = Some(Pause {
            task_id: ended.task_id,
            title: ended.title,
            start: now,
            until: now + i64::from(s.focus_break_minutes.max(1)) * MIN,
            stops: s.focus_break_stops,
        });
        self.ended = None;
        self.resume = None;
        Ok(true)
    }

    /// „Kolejny blok”: nowy blok na zadaniu z powiadomienia, zakończonego bloku
    /// albo przerwy.
    pub fn next(
        &mut self,
        db: &mut Connection,
        s: &AppSettings,
        task: Option<i64>,
        now: i64,
    ) -> Result<(), String> {
        let task = task
            .or(self.ended.as_ref().map(|b| b.task_id))
            .or(self.resume.as_ref().map(|r| r.task_id))
            .or(self.pause.as_ref().map(|p| p.task_id));
        self.start(db, task, s, now)
    }

    /// „Zakończ przerwę” przed czasem.
    pub fn end_break(&mut self, db: &Connection) -> Result<(), String> {
        if let Some(pause) = self.pause.take() {
            let measuring = active(db)
                .map_err(str_err)?
                .is_some_and(|(t, _)| t == pause.task_id);
            if !measuring {
                self.resume = Some(Return {
                    task_id: pause.task_id,
                    title: pause.title,
                });
            }
        }
        Ok(())
    }

    /// „Rozpocznij” po przerwie: start pomiaru zadania (bez nowego bloku).
    pub fn resume(
        &mut self,
        db: &mut Connection,
        task: Option<i64>,
        now: i64,
    ) -> Result<(), String> {
        let task = task
            .or(self.resume.as_ref().map(|r| r.task_id))
            .ok_or("Nie ma zadania do wznowienia.")?;
        db::change(db, "start", Some(task), None, now)?;
        self.resume = None;
        self.pause = None;
        Ok(())
    }

    /// „Zamknij” / „Nie teraz”: chowa pytanie po bloku albo po przerwie.
    pub fn dismiss(&mut self) {
        self.ended = None;
        self.resume = None;
    }

    /// Jedno sprawdzenie w chwili `now`: koniec bloku z zegara, przerwanie przez
    /// Stop lub inne zadanie, koniec przerwy.
    pub fn tick(&mut self, db: &Connection, now: i64) -> rusqlite::Result<Tick> {
        let mut tick = Tick::default();
        let current = active(db)?;
        if let Some(block) = running(db)? {
            let stopped_at = match current {
                Some((task, _)) if task == block.task_id => None,
                // Inne zadanie: blok kończy się w chwili przełączenia.
                Some((_, started)) => Some(started),
                // Stop: chwila zatrzymania pomiaru zadania.
                None => Some(last_end(db, block.task_id, block.start)?.unwrap_or(now)),
            };
            let finished = match stopped_at {
                None => (now >= block.end).then_some(true),
                Some(at) => Some(at.max(block.start).min(now) >= block.end),
            };
            if let Some(completed) = finished {
                close(db, &block, stopped_at.unwrap_or(now).min(now), completed)?;
                tick.changed = true;
                if completed {
                    tick.messages.push(end_message(&block));
                    self.ended = Some(block);
                }
                tick.messages.extend(self.flush(db)?);
            }
        }
        let still = running(db)?;
        if still != self.running {
            tick.changed = true;
        }
        self.running = still;
        // Blok zniknął inaczej (przywrócenie kopii, usunięte zadanie) albo kolejka
        // wróciła z bazy bez bloku: odłożone powiadomienia wychodzą teraz.
        if self.running.is_none() && !self.queue.is_empty() {
            tick.changed = true;
            tick.messages.extend(self.flush(db)?);
        }
        // Pytanie po bloku przestaje dotyczyć, gdy mierzymy inne zadanie.
        if let (Some(ended), Some((task, _))) = (&self.ended, current) {
            if ended.task_id != task {
                self.ended = None;
                tick.changed = true;
            }
        }
        if let Some(pause) = self.pause.clone() {
            let measuring = current.is_some_and(|(t, _)| t == pause.task_id);
            if pause.stops && current.is_some() {
                // Pomiar wrócił w czasie przerwy: przerwa skończona bez pytania.
                self.pause = None;
                tick.changed = true;
            } else if now >= pause.until {
                self.pause = None;
                tick.changed = true;
                tick.messages.push(pause_message(&pause, measuring));
                if !measuring {
                    self.resume = Some(Return {
                        task_id: pause.task_id,
                        title: pause.title,
                    });
                }
            }
        }
        if self.resume.is_some() && current.is_some() {
            self.resume = None;
            tick.changed = true;
        }
        Ok(tick)
    }

    /// Stan dla frontendu; liczniki w strefie `tz`.
    pub fn status<Tz: TimeZone>(
        &self,
        db: &Connection,
        tz: &Tz,
        now: i64,
    ) -> rusqlite::Result<Status> {
        let (today, week) = match tz.timestamp_millis_opt(now).earliest() {
            Some(local) => {
                let date = local.date_naive();
                let monday =
                    date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()));
                (
                    stats::day_bounds(tz, date).0,
                    stats::day_bounds(tz, monday).0,
                )
            }
            None => (now, now),
        };
        let count = |from: i64| -> rusqlite::Result<i64> {
            db.query_row(
                "SELECT COUNT(*) FROM focus_blocks WHERE completed=1 AND start>=?1",
                [from],
                |r| r.get(0),
            )
        };
        let mut ended = self.ended.clone();
        if let Some(b) = ended.as_mut() {
            b.title = title(db, b.task_id)?;
        }
        Ok(Status {
            block: running(db)?,
            ended,
            pause: self.pause.clone(),
            resume: self.resume.clone(),
            queued: self.queue.len(),
            today: count(today)?,
            week: count(week)?,
        })
    }

    /// Kliknięcie „Przerwa” lub „Kolejny blok” w powiadomieniu.
    pub fn perform(
        &mut self,
        db: &mut Connection,
        s: &AppSettings,
        command: Command,
        now: i64,
    ) -> Result<Outcome, String> {
        match command {
            Command::FocusBreak(block) => {
                if self.take_break(db, s, Some(block), now)? {
                    Ok(Outcome {
                        changed: true,
                        navigate: None,
                    })
                } else {
                    // Stary toast: przerwa nie dotyczy już tego bloku.
                    Ok(Outcome {
                        changed: false,
                        navigate: Some(View::tasks()),
                    })
                }
            }
            Command::FocusNext(task) => {
                self.next(db, s, Some(task), now)?;
                Ok(Outcome {
                    changed: true,
                    navigate: None,
                })
            }
            other => notify::perform(db, other, "", now),
        }
    }
}

/// Stan w aplikacji.
pub struct State(Mutex<Focus>);

/// Rejestruje stan (trwający blok z bazy); wołać po `manage(Database)`, przed oknami.
pub fn setup(app: &tauri::AppHandle) {
    let focus = app
        .state::<Database>()
        .0
        .lock()
        .map(|db| Focus::load(&db))
        .unwrap_or_default();
    app.manage(State(Mutex::new(focus)));
}

/// W czasie bloku odkłada powiadomienie do kolejki i do `focus_queue` (`true`);
/// inaczej `false`. Blokady jak w `with`: baza, potem stan skupienia; wołający
/// (`notify::show`) nie trzyma żadnej z nich.
pub fn hold(app: &tauri::AppHandle, message: &Message) -> bool {
    let Some(state) = app.try_state::<State>() else {
        return false;
    };
    let database = app.try_state::<Database>();
    let db = database.as_ref().and_then(|d| d.0.lock().ok());
    let Ok(mut f) = state.0.lock() else {
        return false;
    };
    if !f.holds() {
        return false;
    }
    let saved = db
        .as_deref()
        .is_some_and(|db| f.enqueue_saved(db, message.clone()).is_ok());
    if !saved {
        f.enqueue(message.clone());
    }
    true
}

/// Czy trwa blok (propozycje agentów czekają).
pub fn holding(app: &tauri::AppHandle) -> bool {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|f| f.holds()))
        .unwrap_or(false)
}

/// Blok albo przerwa (przypomnienie w tle milczy).
pub fn quiet(app: &tauri::AppHandle) -> bool {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|f| f.quiet()))
        .unwrap_or(false)
}

/// Baza, potem stan skupienia (zawsze w tej kolejności) i ustawienia.
fn with<R>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&mut Connection, &mut Focus, &AppSettings) -> Result<R, String>,
) -> Result<R, String> {
    let database = app.state::<Database>();
    let mut db = database.0.lock().map_err(|e| e.to_string())?;
    let state = app
        .try_state::<State>()
        .ok_or("Brak stanu trybu skupienia.")?;
    let mut focus = state.0.lock().map_err(|e| e.to_string())?;
    let settings = crate::settings::load(&db);
    f(&mut db, &mut focus, &settings)
}

pub fn status(app: &tauri::AppHandle) -> Result<Status, String> {
    with(app, |db, focus, _| {
        focus.status(db, &chrono::Local, db::now()).map_err(str_err)
    })
}

/// Zdarzenie `focus-changed` dla okien.
pub fn emit(app: &tauri::AppHandle) {
    if let Ok(status) = status(app) {
        let _ = app.emit("focus-changed", status);
    }
}

/// Po zmianie stanu: okna, propozycje agentów (wracają po bloku) i dane.
fn changed(app: &tauri::AppHandle, data: bool) {
    if data {
        crate::reminder::reset(app);
        let _ = app.emit("data-changed", ());
    }
    emit(app);
    crate::suggest::refresh(app, db::now());
}

/// Sprawdzenie: koniec bloku, przerwanie, koniec przerwy; wysyła powiadomienia.
pub fn sync(app: &tauri::AppHandle) {
    let now = db::now();
    let Ok(tick) = with(app, |db, focus, _| focus.tick(db, now).map_err(str_err)) else {
        return;
    };
    if tick.changed {
        changed(app, false);
    }
    for m in &tick.messages {
        notify::display(app, m);
    }
}

/// Kliknięcie „Przerwa” / „Kolejny blok” w powiadomieniu (`notify::activate`).
pub fn activate(app: &tauri::AppHandle, command: Command) -> Result<Outcome, String> {
    let now = db::now();
    let outcome = with(app, |db, focus, s| focus.perform(db, s, command, now))?;
    if outcome.changed {
        emit(app);
    }
    Ok(outcome)
}

/// Wątek sprawdzający co 2 s; kończy się razem z oknem main.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(CHECK);
        if app.get_webview_window("main").is_none() {
            break;
        }
        sync(&app);
    });
}

#[tauri::command]
pub fn focus_status(app: tauri::AppHandle) -> Result<Status, String> {
    sync(&app);
    status(&app)
}

/// Akcje z okien: `start` (opcjonalnie `task_id`), `stop`, `break`, `next`,
/// `end-break`, `resume`, `dismiss`.
#[tauri::command]
pub fn focus_action(
    app: tauri::AppHandle,
    action: String,
    task_id: Option<i64>,
) -> Result<Status, String> {
    let now = db::now();
    let (data, messages) = with(&app, |db, focus, s| match action.as_str() {
        "start" => focus.start(db, task_id, s, now).map(|_| (true, Vec::new())),
        "stop" => focus.stop(db, now).map(|m| (false, m)),
        "break" => focus
            .take_break(db, s, None, now)
            .map(|taken| (taken && s.focus_break_stops, Vec::new())),
        "next" => focus.next(db, s, task_id, now).map(|_| (true, Vec::new())),
        "end-break" => focus.end_break(db).map(|_| (false, Vec::new())),
        "resume" => focus.resume(db, task_id, now).map(|_| (true, Vec::new())),
        "dismiss" => {
            focus.dismiss();
            Ok((false, Vec::new()))
        }
        _ => Err("Nieznana akcja trybu skupienia.".into()),
    })?;
    changed(&app, data);
    for m in &messages {
        notify::display(&app, m);
    }
    status(&app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;

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
        for title in ["Analiza", "Poczta"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        db
    }
    fn message(title: &str) -> Message {
        Message {
            key: String::new(),
            title: title.into(),
            body: String::new(),
            actions: Vec::new(),
            open: View::default(),
        }
    }
    fn blocks(db: &Connection) -> Vec<(i64, i64, i64, Option<bool>)> {
        db.prepare(r#"SELECT task_id, start, "end", completed FROM focus_blocks ORDER BY id"#)
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }
    fn measuring(db: &Connection) -> Option<i64> {
        active(db).unwrap().map(|(t, _)| t)
    }

    #[test]
    fn start_begins_measurement_and_block_from_settings() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        // Bez pomiaru i bez zadania nie ma czego mierzyć.
        assert!(f.start(&mut db, None, &s, at("2026-10-14 10:00")).is_err());
        assert!(!f.holds());
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        assert_eq!(measuring(&db), Some(1));
        assert!(f.holds());
        assert_eq!(
            blocks(&db),
            [(1, at("2026-10-14 10:00"), at("2026-10-14 10:25"), None)]
        );
        // Ponowny start tego samego zadania nie tworzy drugiego bloku.
        f.start(&mut db, None, &s, at("2026-10-14 10:01")).unwrap();
        assert_eq!(blocks(&db).len(), 1);
        // Własna długość bloku z ustawień.
        let long = AppSettings {
            focus_block_minutes: 50,
            ..AppSettings::default()
        };
        f.start(&mut db, Some(2), &long, at("2026-10-14 10:05"))
            .unwrap();
        assert_eq!(measuring(&db), Some(2));
        let rows = blocks(&db);
        assert_eq!(
            rows[0],
            (
                1,
                at("2026-10-14 10:00"),
                at("2026-10-14 10:05"),
                Some(false)
            )
        );
        assert_eq!(
            rows[1],
            (2, at("2026-10-14 10:05"), at("2026-10-14 10:55"), None)
        );
        // Domyślne ustawienia: 25 / 5 min, przerwa zatrzymuje pomiar.
        assert_eq!(
            (
                s.focus_block_minutes,
                s.focus_break_minutes,
                s.focus_break_stops
            ),
            (25, 5, true)
        );
    }

    #[test]
    fn notifications_wait_in_queue_and_come_as_one_summary_after_block() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        // Bez bloku nic nie czeka.
        assert!(!f.holds());
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        assert!(f.holds() && f.quiet());
        for title in [
            "Termin dziś",
            "Długi pomiar bez Stop",
            "Agent pracuje",
            "Cel",
            "Jira",
            "Wieczór",
        ] {
            f.enqueue(message(title));
        }
        let status = f.status(&db, &Warsaw, at("2026-10-14 10:10")).unwrap();
        assert_eq!(status.queued, 6);
        assert_eq!(status.block.unwrap().title, "Analiza");
        // W trakcie bloku nic nie wychodzi.
        let t = f.tick(&db, at("2026-10-14 10:24")).unwrap();
        assert_eq!(t, Tick::default());
        // Koniec bloku: powiadomienie o końcu i jedno zbiorcze.
        let t = f.tick(&db, at("2026-10-14 10:25")).unwrap();
        assert!(t.changed);
        assert_eq!(t.messages.len(), 2);
        assert_eq!(t.messages[0].title, "Koniec bloku skupienia");
        assert_eq!(t.messages[1].title, "Podczas skupienia: 6 powiadomień");
        assert_eq!(
            t.messages[1].body,
            "• Termin dziś\n• Długi pomiar bez Stop\n• Agent pracuje\n• Cel\ni jeszcze 2"
        );
        assert!(!f.holds());
        assert_eq!(
            f.status(&db, &Warsaw, at("2026-10-14 10:25"))
                .unwrap()
                .queued,
            0
        );
        // Jedno powiadomienie w kolejce przychodzi bez zmian (z przyciskami).
        let single = Message {
            actions: vec![Action::new(notify::STOP, "Zatrzymaj", 1)],
            ..message("Długi pomiar bez Stop")
        };
        assert_eq!(summary(vec![single.clone()]), vec![single]);
        assert!(summary(Vec::new()).is_empty());
        assert_eq!(notifications_word(3), "3 powiadomienia");
        assert_eq!(notifications_word(1), "1 powiadomienie");
    }

    #[test]
    fn block_end_after_sleep_is_counted_from_wall_clock() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        assert!(!f.tick(&db, at("2026-10-14 10:05")).unwrap().changed);
        // Komputer spał od 10:05 do 11:40; pomiar trwał (obejmuje uśpienie).
        let t = f.tick(&db, at("2026-10-14 11:40")).unwrap();
        assert_eq!(t.messages.len(), 1);
        let end = &t.messages[0];
        assert_eq!(end.body, "Analiza · 25 min. Przerwa czy kolejny blok?");
        let block = running(&db).unwrap();
        assert!(block.is_none());
        // Koniec bloku równy planowanemu, nie chwili wybudzenia.
        assert_eq!(
            blocks(&db),
            [(
                1,
                at("2026-10-14 10:00"),
                at("2026-10-14 10:25"),
                Some(true)
            )]
        );
        let ended = f
            .status(&db, &Warsaw, at("2026-10-14 11:40"))
            .unwrap()
            .ended
            .unwrap();
        assert_eq!(
            end.actions.iter().map(Action::argument).collect::<Vec<_>>(),
            [format!("focus-break:{}", ended.id), "focus-next:1".into()]
        );
        // Pomiar nadal trwa: timer nie przełącza się sam.
        assert_eq!(measuring(&db), Some(1));
        // Drugie sprawdzenie nic nie powtarza.
        assert_eq!(
            f.tick(&db, at("2026-10-14 11:41")).unwrap(),
            Tick::default()
        );
        let status = f.status(&db, &Warsaw, at("2026-10-14 11:41")).unwrap();
        assert_eq!((status.today, status.week), (1, 1));
    }

    #[test]
    fn stop_or_other_task_ends_block_as_not_completed() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        f.enqueue(message("Termin dziś"));
        db::change(&mut db, "stop", None, None, at("2026-10-14 10:07")).unwrap();
        let t = f.tick(&db, at("2026-10-14 10:08")).unwrap();
        assert!(t.changed);
        // Bez „Koniec bloku”; zaległe powiadomienie przychodzi od razu.
        assert_eq!(
            t.messages
                .iter()
                .map(|m| m.title.as_str())
                .collect::<Vec<_>>(),
            ["Termin dziś"]
        );
        assert_eq!(
            blocks(&db),
            [(
                1,
                at("2026-10-14 10:00"),
                at("2026-10-14 10:07"),
                Some(false)
            )]
        );
        assert!(!f.holds());
        let status = f.status(&db, &Warsaw, at("2026-10-14 10:08")).unwrap();
        assert!(status.ended.is_none() && status.block.is_none());
        assert_eq!(status.today, 0);
        // Zmiana zadania w trakcie bloku: koniec w chwili przełączenia.
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:10"))
            .unwrap();
        db::change(&mut db, "start", Some(2), None, at("2026-10-14 10:12")).unwrap();
        f.tick(&db, at("2026-10-14 10:13")).unwrap();
        assert_eq!(
            blocks(&db)[1],
            (
                1,
                at("2026-10-14 10:10"),
                at("2026-10-14 10:12"),
                Some(false)
            )
        );
        // „Zakończ blok”: przerwany, pomiar trwa dalej.
        f.start(&mut db, None, &s, at("2026-10-14 10:20")).unwrap();
        let flushed = f.stop(&db, at("2026-10-14 10:30")).unwrap();
        assert!(flushed.is_empty());
        assert_eq!(
            blocks(&db)[2],
            (
                2,
                at("2026-10-14 10:20"),
                at("2026-10-14 10:30"),
                Some(false)
            )
        );
        assert_eq!(measuring(&db), Some(2));
    }

    #[test]
    fn break_stops_measurement_and_asks_to_resume() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        f.tick(&db, at("2026-10-14 10:25")).unwrap();
        let ended = f.ended.clone().unwrap();
        // Stary toast innego bloku: bez zmian, tylko okno.
        let stale = f
            .perform(
                &mut db,
                &s,
                Command::FocusBreak(ended.id + 1),
                at("2026-10-14 10:26"),
            )
            .unwrap();
        assert_eq!(stale.navigate, Some(View::tasks()));
        assert_eq!(measuring(&db), Some(1));
        // „Przerwa” z powiadomienia zatrzymuje pomiar.
        let out = f
            .perform(
                &mut db,
                &s,
                notify::command(Some(&format!("focus-break:{}", ended.id)), &View::default()),
                at("2026-10-14 10:26"),
            )
            .unwrap();
        assert!(out.changed);
        assert_eq!(measuring(&db), None);
        assert!(f.quiet() && !f.holds());
        let pause = f
            .status(&db, &Warsaw, at("2026-10-14 10:27"))
            .unwrap()
            .pause
            .unwrap();
        assert_eq!(pause.until, at("2026-10-14 10:31"));
        assert!(f
            .tick(&db, at("2026-10-14 10:30"))
            .unwrap()
            .messages
            .is_empty());
        // Po przerwie: „Wracasz do Analiza?” z „Rozpocznij”.
        let t = f.tick(&db, at("2026-10-14 10:31")).unwrap();
        assert_eq!(t.messages.len(), 1);
        assert_eq!(t.messages[0].title, "Wracasz do Analiza?");
        assert_eq!(t.messages[0].actions[0].argument(), "focus-resume:1");
        assert_eq!(
            f.status(&db, &Warsaw, at("2026-10-14 10:31"))
                .unwrap()
                .resume,
            Some(Return {
                task_id: 1,
                title: "Analiza".into()
            })
        );
        // Timer stoi, dopóki użytkownik nie kliknie.
        assert!(f
            .tick(&db, at("2026-10-14 10:40"))
            .unwrap()
            .messages
            .is_empty());
        assert_eq!(measuring(&db), None);
        let out = notify::perform(
            &mut db,
            notify::command(Some("focus-resume:1"), &View::default()),
            "2026-10-14",
            at("2026-10-14 10:41"),
        )
        .unwrap();
        assert!(out.changed);
        assert_eq!(measuring(&db), Some(1));
        assert!(f.tick(&db, at("2026-10-14 10:41")).unwrap().changed);
        assert!(f.resume.is_none());
    }

    #[test]
    fn break_can_keep_measuring_and_next_block_continues() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings {
            focus_break_stops: false,
            focus_break_minutes: 10,
            ..AppSettings::default()
        };
        f.start(&mut db, Some(2), &s, at("2026-10-14 10:00"))
            .unwrap();
        f.tick(&db, at("2026-10-14 10:30")).unwrap();
        assert!(f.take_break(&db, &s, None, at("2026-10-14 10:30")).unwrap());
        assert_eq!(measuring(&db), Some(2));
        let t = f.tick(&db, at("2026-10-14 10:40")).unwrap();
        assert_eq!(t.messages[0].title, "Koniec przerwy");
        assert_eq!(t.messages[0].actions[0].argument(), "focus-next:2");
        assert!(f.resume.is_none());
        // „Kolejny blok” z powiadomienia: nowy blok na tym samym zadaniu.
        let out = f
            .perform(&mut db, &s, Command::FocusNext(2), at("2026-10-14 10:41"))
            .unwrap();
        assert!(out.changed);
        assert!(f.holds());
        assert_eq!(blocks(&db).len(), 2);
        // Zatrzymany pomiar i „Kolejny blok” uruchamia go znowu.
        f.stop(&db, at("2026-10-14 10:42")).unwrap();
        db::change(&mut db, "stop", None, None, at("2026-10-14 10:43")).unwrap();
        f.next(&mut db, &s, Some(1), at("2026-10-14 10:44"))
            .unwrap();
        assert_eq!(measuring(&db), Some(1));
        let status = f.status(&db, &Warsaw, at("2026-10-14 10:44")).unwrap();
        assert_eq!(status.block.unwrap().task_id, 1);
        assert_eq!((status.today, status.week), (1, 1));
    }

    #[test]
    fn counts_completed_blocks_today_and_this_week() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        // Poniedziałek 12.10, środa 14.10 dwa razy, niedziela 11.10 (poprzedni tydzień).
        for start in [
            "2026-10-11 09:00",
            "2026-10-12 09:00",
            "2026-10-14 08:00",
            "2026-10-14 09:00",
        ] {
            f.start(&mut db, Some(1), &s, at(start)).unwrap();
            f.tick(&db, at(start) + 26 * MIN).unwrap();
            db::change(&mut db, "stop", None, None, at(start) + 30 * MIN).unwrap();
        }
        // Przerwany blok się nie liczy.
        f.start(&mut db, Some(1), &s, at("2026-10-14 11:00"))
            .unwrap();
        f.stop(&db, at("2026-10-14 11:05")).unwrap();
        let status = f.status(&db, &Warsaw, at("2026-10-14 12:00")).unwrap();
        assert_eq!((status.today, status.week), (2, 3));
        let json = serde_json::to_value(&status).unwrap();
        assert!(json.get("today").is_some() && json.get("resume").is_some());
    }

    #[test]
    fn queue_survives_restart_and_comes_after_block() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        for title in ["Termin dziś", "Cel"] {
            f.enqueue_saved(&db, message(title)).unwrap();
        }
        // Zamknięcie aplikacji w trakcie bloku: stan w pamięci znika.
        drop(f);
        let mut f = Focus::load(&db);
        assert!(f.holds());
        assert_eq!(
            f.status(&db, &Warsaw, at("2026-10-14 10:10"))
                .unwrap()
                .queued,
            2
        );
        let t = f.tick(&db, at("2026-10-14 10:25")).unwrap();
        assert_eq!(
            t.messages
                .iter()
                .map(|m| m.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Koniec bloku skupienia",
                "Podczas skupienia: 2 powiadomienia"
            ]
        );
        // Kolejka wyszła: po kolejnym restarcie nic nie wraca.
        assert_eq!(Focus::load(&db).queue.len(), 0);
    }

    #[test]
    fn start_on_task_that_cannot_start_keeps_running_block() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        db::change(&mut db, "toggle", Some(2), None, at("2026-10-14 09:00")).unwrap();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        // Stary toast „Kolejny blok” dla zrobionego zadania.
        assert!(f
            .start(&mut db, Some(2), &s, at("2026-10-14 10:05"))
            .is_err());
        assert_eq!(
            blocks(&db),
            [(1, at("2026-10-14 10:00"), at("2026-10-14 10:25"), None)]
        );
        assert_eq!(measuring(&db), Some(1));
        assert!(f.holds());
        assert!(!f.tick(&db, at("2026-10-14 10:06")).unwrap().changed);
    }

    #[test]
    fn queue_comes_out_when_block_disappears() {
        let mut db = setup();
        let mut f = Focus::default();
        let s = AppSettings::default();
        f.start(&mut db, Some(1), &s, at("2026-10-14 10:00"))
            .unwrap();
        f.enqueue_saved(&db, message("Termin dziś")).unwrap();
        // Np. przywrócenie kopii bez trwającego bloku.
        db.execute("DELETE FROM focus_blocks", []).unwrap();
        let t = f.tick(&db, at("2026-10-14 10:05")).unwrap();
        assert!(t.changed);
        assert_eq!(
            t.messages
                .iter()
                .map(|m| m.title.as_str())
                .collect::<Vec<_>>(),
            ["Termin dziś"]
        );
        assert!(!f.holds());
        assert_eq!(saved_queue(&db).unwrap().len(), 0);
    }
}
