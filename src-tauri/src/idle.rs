use crate::agents::Match;
use crate::{db, Database};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
/// Jak długo pamiętamy, nad czym pracowali agenci (do przypisania przerwy).
const SEEN_KEEP: i64 = 24 * 3_600_000;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub session_id: i64,
    pub task_id: i64,
    pub idle_at: i64,
    /// Ostatnie wejście użytkownika (nie wcześniej niż początek pomiaru). Przy
    /// pracy agentów liczonej jako aktywność bywa wcześniejsze niż `idle_at`:
    /// od tej chwili liczy się „Przypisz przerwę do X”.
    pub away_since: i64,
    pub returned_at: i64,
    /// Zadania (inne niż mierzone), nad którymi agenci pracowali podczas
    /// nieobecności: trzecia odpowiedź „Przypisz przerwę do X”. Najpierw ostatnie.
    pub agent_tasks: Vec<Match>,
}
#[derive(Default)]
pub struct State(Mutex<Clock>);
#[derive(Default)]
struct Clock {
    candidate: Option<(i64, i64, i64)>,
    pending: Option<Notice>,
    /// Ostatnie wejście użytkownika przed przerwą-kandydatem.
    away_since: Option<i64>,
    /// Zadania z lokalnym id, nad którymi pracowali agenci, z chwilą ostatniej pracy.
    seen: Vec<(i64, Match)>,
}
impl Clock {
    /// Zapamiętuje zadania pracujących agentów inne niż mierzone (`active`).
    fn see(&mut self, working: &[Match], active: Option<i64>, now: i64) {
        self.seen.retain(|(at, _)| now - at <= SEEN_KEEP);
        for m in working {
            // Ukończonemu zadaniu nie da się przypisać trwającej części pomiaru.
            let Some(id) = m
                .task_id
                .filter(|id| Some(*id) != active)
                .filter(|_| !m.done)
            else {
                continue;
            };
            self.seen.retain(|(_, s)| s.task_id != Some(id));
            self.seen.push((now, m.clone()));
        }
    }
    /// Zadania agentów od `since`, bez `task`, najpierw ostatnie.
    fn agent_tasks(&self, since: i64, task: i64) -> Vec<Match> {
        let mut found: Vec<&(i64, Match)> = self
            .seen
            .iter()
            .filter(|(at, m)| *at >= since && m.task_id != Some(task))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found.into_iter().map(|(_, m)| m.clone()).collect()
    }
    /// `user_idle`: czas od ostatniego wejścia z klawiatury/myszy, `agent_idle`: od
    /// ostatniej pracy agenta (`agents`). Praca agenta liczy się jako aktywność:
    /// przerwa trwa, gdy oba są dłuższe niż 10 min, a jej początek to późniejsza z
    /// tych chwil. Pytanie pada tylko po powrocie użytkownika; gdy przerwę kończy
    /// agent, zapominamy ją bez pytania.
    fn observe(
        &mut self,
        session: Option<(i64, i64, i64)>,
        now: i64,
        user_idle: u32,
        agent_idle: u32,
    ) -> bool {
        let Some((id, task, start)) = session else {
            self.candidate = None;
            return false;
        };
        let idle_ms = user_idle.min(agent_idle);
        if idle_ms >= 600_000 {
            if self.candidate.as_ref().map(|v| v.0) != Some(id) {
                self.candidate = Some((id, task, (now - i64::from(idle_ms)).max(start)));
                self.away_since = Some((now - i64::from(user_idle)).max(start));
            }
        } else if user_idle >= 600_000 {
            self.candidate = None;
        } else if let Some((old_id, task, at)) = self.candidate.take() {
            if old_id == id && self.pending.is_none() && now - at >= 600_000 {
                let since = self.away_since.unwrap_or(at).min(at);
                self.pending = Some(Notice {
                    session_id: id,
                    task_id: task,
                    idle_at: at,
                    away_since: since,
                    returned_at: now,
                    agent_tasks: self.agent_tasks(since, task),
                });
                return true;
            }
        }
        false
    }
}
#[cfg(windows)]
fn idle_ms() -> Option<u32> {
    #[repr(C)]
    struct LastInput {
        size: u32,
        time: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(info: *mut LastInput) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTickCount() -> u32;
    }
    let mut input = LastInput {
        size: std::mem::size_of::<LastInput>() as u32,
        time: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut input) == 0 {
            None
        } else {
            Some(GetTickCount().wrapping_sub(input.time))
        }
    }
}
#[cfg(not(windows))]
fn idle_ms() -> Option<u32> {
    None
}

/// Rejestruje stan bezczynności; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Pętla co 2 s sprawdzająca bezczynność. Kończy się, gdy okno main zniknie,
/// więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let Some(_) = app.get_webview_window("main") else {
            break;
        };
        let Some(settings) = app
            .state::<Database>()
            .0
            .lock()
            .ok()
            .map(|db| crate::settings::cached(&app, &db))
        else {
            continue;
        };
        if !settings.idle_enabled {
            if let Ok(mut state) = app.state::<State>().0.lock() {
                *state = Clock::default();
            }
            continue;
        }
        let Some(idle) = idle_ms() else {
            continue;
        };
        let now = db::now();
        let agent_idle = if settings.idle_agents {
            crate::agents::idle_ms(&app, now)
        } else {
            u32::MAX
        };
        let session = app.state::<Database>().0.lock().ok().and_then(|db| {
            db.query_row(
                "SELECT id,task_id,started_at FROM sessions WHERE ended_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()
        });
        // Zadania pracujących agentów: do „Przypisz przerwę do X”.
        let working: Vec<Match> = crate::agents::current(&app)
            .into_iter()
            .filter_map(|a| a.task)
            .collect();
        let due = app
            .state::<State>()
            .0
            .lock()
            .map(|mut c| {
                c.see(&working, session.map(|(_, task, _)| task), now);
                c.observe(session, now, idle, agent_idle)
            })
            .unwrap_or(false);
        if due {
            let _ = app.emit("idle-notice", ());
            crate::show(&app);
        }
    });
}
#[tauri::command]
pub fn idle_notice(app: tauri::AppHandle) -> Option<Notice> {
    app.state::<State>()
        .0
        .lock()
        .ok()
        .and_then(|c| c.pending.clone())
}
pub fn clear(app: &tauri::AppHandle) {
    if let Ok(mut state) = app.state::<State>().0.lock() {
        *state = Clock::default();
    }
    let _ = app.emit("idle-notice", ());
}
/// „Przypisz przerwę do X”: podział pomiaru z banera w chwili wyjścia użytkownika
/// (`away_since`, a nie `idle_at`: praca agenta nad X podczas nieobecności też
/// należy do X), druga część (do końca albo dalej trwająca) należy do X. Ten sam
/// mechanizm co zakładka Podziel (`db::split_session`). Gdy użytkownik wyszedł
/// zaraz po starcie, cały pomiar przechodzi na X (`db::move_session`). X musi
/// być zadaniem agenta z banera.
pub fn assign_break(
    conn: &mut rusqlite::Connection,
    notice: &Notice,
    task: i64,
    now: i64,
) -> Result<db::Snapshot, String> {
    if !notice.agent_tasks.iter().any(|m| m.task_id == Some(task)) {
        return Err("To zadanie nie jest pracą agenta z tej przerwy.".into());
    }
    let started_at: i64 = conn
        .query_row(
            "SELECT started_at FROM sessions WHERE id=?1",
            [notice.session_id],
            |r| r.get(0),
        )
        .map_err(|_| "Nie znaleziono przedziału.".to_owned())?;
    if notice.away_since <= started_at {
        db::move_session(conn, notice.session_id, task)
    } else {
        db::split_session(conn, notice.session_id, notice.away_since, Some(task), now)
    }
}
/// „Zatrzymaj na początku przerwy”: koniec pomiaru na `idle_at` (nie wcześniej
/// niż jego start). Pomiar zakończony przed przerwą zostaje bez zmian.
fn trim_break(conn: &rusqlite::Connection, notice: &Notice) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE id=?2 AND (ended_at IS NULL OR ended_at>?1)",
        rusqlite::params![notice.idle_at, notice.session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// „Przytnij i wznów”: z pomiaru wypada tylko przerwa `idle_at`–`returned_at`.
/// Pierwsza część kończy się na `idle_at`, druga zaczyna na `returned_at` przy
/// tym samym zadaniu i kończy tam, gdzie pomiar (trwa dalej, jeśli trwał).
/// Gdy pomiar zatrzymano przed powrotem, to samo co `trim_break`; gdy przerwa
/// zaczęła się razem z pomiarem, przesuwa się tylko jego początek. Timer nie
/// przełącza się na inne zadanie.
fn trim_and_resume(conn: &mut rusqlite::Connection, notice: &Notice) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let (task, started_at, ended_at, deleted): (i64, i64, Option<i64>, bool) = tx
        .query_row(
            "SELECT s.task_id,s.started_at,s.ended_at,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=s.task_id AND l.deleted_at IS NOT NULL) FROM sessions s WHERE s.id=?1",
            [notice.session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| "Nie znaleziono przedziału.".to_owned())?;
    if deleted {
        return Err("Przedział należy do zadania w koszu.".into());
    }
    if ended_at.is_some_and(|end| end <= notice.returned_at) {
        trim_break(&tx, notice)?;
    } else if notice.idle_at <= started_at {
        tx.execute(
            "UPDATE sessions SET started_at=?1 WHERE id=?2",
            rusqlite::params![notice.returned_at, notice.session_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        // Najpierw zamknięcie, potem wstawienie (indeks one_active_session).
        tx.execute(
            "UPDATE sessions SET ended_at=?1 WHERE id=?2",
            rusqlite::params![notice.idle_at, notice.session_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            rusqlite::params![task, notice.returned_at, ended_at],
        )
        .map_err(|e| e.to_string())?;
        if ended_at.is_none() {
            // Jak w db::split_session: bez drugiego powiadomienia o długim pomiarze.
            let new_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT OR IGNORE INTO notifications_sent(key,at)
                 SELECT ?1, at FROM notifications_sent WHERE key=?2",
                rusqlite::params![
                    format!("long:{new_id}"),
                    format!("long:{}", notice.session_id)
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}
/// Odpowiedź z banera: `assign` („Przypisz przerwę do X”) ma pierwszeństwo,
/// potem `trim` z `resume` („Przytnij i wznów”), sam `trim` („Zatrzymaj na
/// początku przerwy”); bez nich „Zachowaj cały czas”.
pub fn resolve(
    conn: &mut rusqlite::Connection,
    notice: &Notice,
    trim: bool,
    assign: Option<i64>,
    resume: bool,
    now: i64,
) -> Result<db::Snapshot, String> {
    if let Some(task) = assign {
        assign_break(conn, notice, task, now)?;
    } else if trim && resume {
        trim_and_resume(conn, notice)?;
    } else if trim {
        trim_break(conn, notice)?;
    }
    db::snapshot(conn).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn idle_resolve(
    app: tauri::AppHandle,
    trim: bool,
    assign: Option<i64>,
    resume: Option<bool>,
) -> Result<db::Snapshot, String> {
    let state = app.state::<State>();
    let mut clock = state.0.lock().map_err(|e| e.to_string())?;
    let notice = clock
        .pending
        .clone()
        .ok_or("Brak zgłoszonej bezczynności.")?;
    let database = app.state::<Database>();
    let mut conn = database.0.lock().map_err(|e| e.to_string())?;
    let resume = resume.unwrap_or(false);
    let result = resolve(&mut conn, &notice, trim, assign, resume, db::now())?;
    clock.pending = None;
    drop(conn);
    drop(clock);
    if assign.is_some() || (trim && resume) {
        crate::reminder::reset(&app);
    }
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Żaden agent nie pracował.
    const N: u32 = u32::MAX;
    #[test]
    fn only_prompts_after_return_without_mutating_time() {
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 2, 100)), 700_000, 650_000, N));
        assert!(c.observe(Some((1, 2, 100)), 800_000, 0, N));
        assert_eq!(c.pending.as_ref().unwrap().idle_at, 50_000);
        assert!(!c.observe(Some((1, 2, 100)), 800_010, 10, N));
    }
    #[test]
    fn short_break_and_switching_sessions_do_not_prompt() {
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 2, 0)), 500_000, 499_999, N));
        assert!(!c.observe(Some((1, 2, 0)), 500_001, 0, N));
        assert!(c.pending.is_none());
        c.observe(Some((1, 2, 0)), 700_000, 650_000, N);
        assert!(!c.observe(Some((2, 3, 700_000)), 800_000, 0, N));
        assert!(c.pending.is_none());
        c.observe(Some((2, 3, 700_000)), 1_400_000, 650_000, N);
        c.observe(None, 1_500_000, 0, N);
        assert!(!c.observe(Some((3, 4, 1_500_000)), 1_500_100, 0, N));
    }
    #[test]
    fn agent_work_counts_as_activity() {
        // Agent pracuje przez całą nieobecność: brak przerwy.
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 2, 0)), 1_000_000, 900_000, 1_000));
        assert!(!c.observe(Some((1, 2, 0)), 1_000_100, 0, 1_000));
        assert!(c.pending.is_none());
        // Agent skończył 700 s temu, użytkownika nie ma 900 s: przerwa od końca pracy agenta.
        assert!(!c.observe(Some((1, 2, 0)), 2_000_000, 900_000, 700_000));
        assert!(c.observe(Some((1, 2, 0)), 2_000_100, 0, 700_100));
        assert_eq!(c.pending.as_ref().unwrap().idle_at, 1_300_000);
        // Agent rusza podczas nieobecności: przerwa zapomniana, bez pytania po powrocie.
        let mut c = Clock::default();
        c.observe(Some((1, 2, 0)), 1_000_000, 700_000, N);
        assert!(!c.observe(Some((1, 2, 0)), 1_100_000, 800_000, 0));
        assert!(!c.observe(Some((1, 2, 0)), 1_200_000, 0, 0));
        assert!(c.pending.is_none());
    }
    fn task(id: i64) -> Match {
        Match {
            task_id: Some(id),
            title: format!("Zadanie {id}"),
            key: None,
            source: "task".into(),
            done: false,
        }
    }
    #[test]
    fn agent_work_during_absence_is_offered_for_the_break() {
        let mut c = Clock::default();
        // Przed wyjściem użytkownika agent pracował nad zadaniem 7: to nie ta przerwa.
        c.see(&[task(7)], Some(2), 50_000);
        // Użytkownik wychodzi o 100 000; agent pracuje nad 3 (i nad mierzonym 2).
        c.see(&[task(3), task(2)], Some(2), 400_000);
        assert!(!c.observe(Some((1, 2, 0)), 400_000, 300_000, 0));
        c.see(&[task(3)], Some(2), 500_000);
        // Agent kończy o 500 000; przerwa od 500 000, powrót o 1 200 000.
        assert!(!c.observe(Some((1, 2, 0)), 1_100_000, 1_000_000, 600_000));
        assert!(c.observe(Some((1, 2, 0)), 1_200_000, 0, 700_000));
        let notice = c.pending.clone().unwrap();
        assert_eq!(notice.idle_at, 500_000);
        // Przypisanie liczy się od wyjścia użytkownika, nie od końca pracy agenta.
        assert_eq!(notice.away_since, 100_000);
        let ids: Vec<_> = notice.agent_tasks.iter().map(|m| m.task_id).collect();
        assert_eq!(ids, vec![Some(3)]);
        // Bez pracy agentów trzeciej odpowiedzi nie ma.
        let mut c = Clock::default();
        c.observe(Some((1, 2, 0)), 700_000, 650_000, N);
        assert!(c.observe(Some((1, 2, 0)), 800_000, 0, N));
        assert!(c.pending.unwrap().agent_tasks.is_empty());
    }
    #[test]
    fn assigning_break_splits_measurement_at_its_start() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        for title in ["Mierzone", "Agent"] {
            conn.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db::change(&mut conn, "start", Some(1), None, 1_000).unwrap();
        let session: i64 = conn
            .query_row("SELECT id FROM sessions", [], |r| r.get(0))
            .unwrap();
        let notice = Notice {
            session_id: session,
            task_id: 1,
            idle_at: 5_000,
            away_since: 5_000,
            returned_at: 9_000,
            agent_tasks: vec![task(2)],
        };
        assert!(assign_break(&mut conn, &notice, 1, 9_000).is_err());
        let snap = assign_break(&mut conn, &notice, 2, 9_000).unwrap();
        let mut rows: Vec<_> = snap
            .sessions
            .iter()
            .map(|s| (s.task_id, s.started_at, s.ended_at))
            .collect();
        rows.sort();
        assert_eq!(rows, vec![(1, 1_000, Some(5_000)), (2, 5_000, None)]);
    }
    #[test]
    fn agent_work_on_x_during_absence_goes_to_x() {
        // Użytkownik wychodzi o 10:00 (0), agent pracuje nad X do 10:30, powrót 11:00.
        const MIN: i64 = 60_000;
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        for title in ["Mierzone", "Agent"] {
            conn.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db::change(&mut conn, "start", Some(1), None, -30 * MIN).unwrap();
        let session: i64 = conn
            .query_row("SELECT id FROM sessions", [], |r| r.get(0))
            .unwrap();
        let mut c = Clock::default();
        let s = Some((session, 1, -30 * MIN));
        c.see(&[task(2)], Some(1), 20 * MIN);
        assert!(!c.observe(s, 20 * MIN, (20 * MIN) as u32, 0));
        c.see(&[task(2)], Some(1), 30 * MIN);
        assert!(!c.observe(s, 45 * MIN, (45 * MIN) as u32, (15 * MIN) as u32));
        assert!(c.observe(s, 60 * MIN, 0, (30 * MIN) as u32));
        let notice = c.pending.clone().unwrap();
        assert_eq!((notice.idle_at, notice.away_since), (30 * MIN, 0));
        let snap = assign_break(&mut conn, &notice, 2, 60 * MIN).unwrap();
        let mut rows: Vec<_> = snap
            .sessions
            .iter()
            .map(|s| (s.task_id, s.started_at, s.ended_at))
            .collect();
        rows.sort();
        assert_eq!(rows, vec![(1, -30 * MIN, Some(0)), (2, 0, None)]);
    }
    #[test]
    fn break_from_measurement_start_moves_whole_measurement() {
        // Start i od razu wyjście: początek przerwy (przycięty do startu) = start pomiaru.
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 1, 100_000)), 800_000, 750_000, N));
        assert!(c.observe(Some((1, 1, 100_000)), 900_000, 0, N));
        let notice = c.pending.clone().unwrap();
        assert_eq!((notice.idle_at, notice.away_since), (100_000, 100_000));

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        for title in ["Mierzone", "Agent", "Ukończone"] {
            conn.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        conn.execute("UPDATE tasks SET done=1 WHERE id=3", [])
            .unwrap();
        db::change(&mut conn, "start", Some(1), None, 100_000).unwrap();
        let session: i64 = conn
            .query_row("SELECT id FROM sessions", [], |r| r.get(0))
            .unwrap();
        let notice = Notice {
            session_id: session,
            agent_tasks: vec![task(2), task(3)],
            ..notice
        };
        // Ukończone zadanie nie przejmuje trwającego pomiaru.
        assert!(assign_break(&mut conn, &notice, 3, 900_000).is_err());
        let snap = assign_break(&mut conn, &notice, 2, 900_000).unwrap();
        let rows: Vec<_> = snap
            .sessions
            .iter()
            .map(|s| (s.task_id, s.started_at, s.ended_at))
            .collect();
        assert_eq!(rows, vec![(2, 100_000, None)]);
    }
    /// Baza z zadaniem 1 i pomiarem od `start` (trwającym albo do `end`).
    fn measured(start: i64, end: Option<i64>) -> (rusqlite::Connection, Notice) {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        conn.execute(
            "INSERT INTO tasks(title,created_at) VALUES('Mierzone',0)",
            [],
        )
        .unwrap();
        db::change(&mut conn, "start", Some(1), None, start).unwrap();
        if let Some(end) = end {
            db::change(&mut conn, "stop", None, None, end).unwrap();
        }
        let session: i64 = conn
            .query_row("SELECT id FROM sessions", [], |r| r.get(0))
            .unwrap();
        let notice = Notice {
            session_id: session,
            task_id: 1,
            idle_at: 30_000,
            away_since: 30_000,
            returned_at: 60_000,
            agent_tasks: vec![],
        };
        (conn, notice)
    }
    fn rows(snap: &db::Snapshot) -> Vec<(i64, i64, Option<i64>)> {
        let mut rows: Vec<_> = snap
            .sessions
            .iter()
            .map(|s| (s.task_id, s.started_at, s.ended_at))
            .collect();
        rows.sort();
        rows
    }
    #[test]
    fn trim_ends_measurement_at_start_of_break() {
        // Trwający pomiar: koniec na idle_at.
        let (mut conn, notice) = measured(10_000, None);
        let snap = resolve(&mut conn, &notice, true, None, false, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 10_000, Some(30_000))]);
        // Pomiar zatrzymany po powrocie: też przycięty do idle_at.
        let (mut conn, notice) = measured(10_000, Some(65_000));
        let snap = resolve(&mut conn, &notice, true, None, false, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 10_000, Some(30_000))]);
        // Zakończony przed przerwą: bez zmian.
        let (mut conn, notice) = measured(10_000, Some(20_000));
        let snap = resolve(&mut conn, &notice, true, None, false, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 10_000, Some(20_000))]);
        // Przerwa od startu: pusty przedział (koniec = start), nigdy przed startem.
        let (mut conn, notice) = measured(40_000, None);
        let snap = resolve(&mut conn, &notice, true, None, false, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 40_000, Some(40_000))]);
        // „Zachowaj cały czas”: bez zmian.
        let (mut conn, notice) = measured(10_000, None);
        let snap = resolve(&mut conn, &notice, false, None, false, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 10_000, None)]);
    }
    #[test]
    fn trim_and_resume_cuts_out_only_the_break() {
        // Trwający pomiar: przycięty do idle_at, nowy od returned_at trwa dalej.
        let (mut conn, notice) = measured(10_000, None);
        conn.execute(
            "INSERT INTO notifications_sent(key,at) VALUES(?1,5)",
            [format!("long:{}", notice.session_id)],
        )
        .unwrap();
        let snap = resolve(&mut conn, &notice, true, None, true, 70_000).unwrap();
        assert_eq!(
            rows(&snap),
            vec![(1, 10_000, Some(30_000)), (1, 60_000, None)]
        );
        let resumed = snap.sessions.iter().find(|s| s.ended_at.is_none()).unwrap();
        let moved: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notifications_sent WHERE key=?1",
                [format!("long:{}", resumed.id)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(moved, 1);
        // Pomiar zatrzymany (albo przełączony) po powrocie: druga część do tego końca.
        let (mut conn, notice) = measured(10_000, Some(65_000));
        let snap = resolve(&mut conn, &notice, true, None, true, 70_000).unwrap();
        assert_eq!(
            rows(&snap),
            vec![(1, 10_000, Some(30_000)), (1, 60_000, Some(65_000))]
        );
        // Zatrzymany przed powrotem: tylko przycięcie, bez wznowienia.
        let (mut conn, notice) = measured(10_000, Some(50_000));
        let snap = resolve(&mut conn, &notice, true, None, true, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 10_000, Some(30_000))]);
        // Przerwa od startu pomiaru: przesuwa się tylko początek.
        let (mut conn, notice) = measured(40_000, None);
        let snap = resolve(&mut conn, &notice, true, None, true, 70_000).unwrap();
        assert_eq!(rows(&snap), vec![(1, 60_000, None)]);
        // Zadanie w koszu: błąd, bez zmian.
        let (mut conn, notice) = measured(10_000, None);
        db::change(&mut conn, "delete", Some(1), None, 65_000).unwrap();
        assert!(resolve(&mut conn, &notice, true, None, true, 70_000).is_err());
        let ended: Option<i64> = conn
            .query_row("SELECT ended_at FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ended, Some(65_000));
    }
    #[test]
    fn done_agent_tasks_are_not_offered_for_the_break() {
        let mut c = Clock::default();
        let mut done = task(3);
        done.done = true;
        c.see(&[done, task(4)], Some(2), 100_000);
        let ids: Vec<_> = c.agent_tasks(0, 2).iter().map(|m| m.task_id).collect();
        assert_eq!(ids, vec![Some(4)]);
    }
}
