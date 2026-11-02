//! Przerwa w pracy aplikacji: wyłączony albo zrestartowany komputer, zamknięty
//! proces. Pętla `idle::start` zapisuje puls życia (`app_heartbeat`) najrzadziej
//! co 30 s, a przy starcie (`main.rs`, przed `manage(Database)`) trwający pomiar
//! po co najmniej 10 min przerwy kończy się na ostatnim pulsie. Baner
//! `GapBanner.vue` pyta potem, co dalej (`gap_resolve`). To nie „Luka w zapisie”
//! z osi dnia (`db::fill_gap`) ani bezczynność (`idle.rs`): tamte działają tylko
//! przy uruchomionej aplikacji.
//!
//! Ograniczenie: gdy zapis pulsu zawodzi (np. zablokowana albo uszkodzona
//! baza), w bazie zostaje ostatni udany puls. Aplikacja zgłasza to raz
//! (`storage-error`, `Beat::record`) aż do udanego zapisu, ale `recover` po
//! przerwie zakończy wtedy pomiar na starym pulsie, czyli za wcześnie; brakujący
//! czas trzeba poprawić na osi dnia albo „Przywróć cały czas”.
use crate::idle::BREAK_MIN;
use crate::{db, Database};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Najrzadszy zapis pulsu (pętla `idle::start` chodzi co 2 s).
const BEAT_EVERY: i64 = 30_000;
/// Tyle nieudanych zapisów pulsu z rzędu (ok. 30 s) uznajemy za trwały błąd.
/// Nie pierwszy: pierwszy puls idzie 2 s po starcie, zanim okno nasłuchuje
/// `storage-error`, a pojedynczy błąd (chwilowa blokada bazy) nie wymaga zgłoszenia.
const REPORT_AFTER_FAILURES: u32 = 2;

/// Pomiar zatrzymany przy starcie: `stopped_at` to jego nowy koniec (ostatni
/// puls, nie wcześniej niż start pomiaru), `restarted_at` to start aplikacji.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GapNotice {
    pub session_id: i64,
    pub task_id: i64,
    pub stopped_at: i64,
    pub restarted_at: i64,
}

/// Notatka do banera; osobno od `idle::State`, bo `idle::Clock` jest zerowany
/// przy wyłączonej bezczynności i w `idle::clear`.
pub struct State(Mutex<Option<GapNotice>>);

/// Rejestruje notatkę z `recover`; wołać przed `create_windows`.
pub fn setup(app: &tauri::AppHandle, notice: Option<GapNotice>) {
    app.manage(State(Mutex::new(notice)));
}

/// Chwila ostatniego zapisu pulsu w pętli `idle::start`.
#[derive(Default)]
pub struct Beat {
    last: Option<i64>,
    /// Nieudane zapisy z rzędu; zero po udanym.
    failures: u32,
}
impl Beat {
    /// Czy zapisać puls teraz: pierwszy raz, po 30 s albo po cofnięciu zegara.
    fn due(&mut self, now: i64) -> bool {
        let due = self
            .last
            .map_or(true, |last| now - last >= BEAT_EVERY || now < last);
        if due {
            self.last = Some(now);
        }
        due
    }

    /// Wynik zapisu pulsu: komunikat do `storage-error` raz na serię błędów
    /// (przy `REPORT_AFTER_FAILURES`), seria kończy się udanym zapisem.
    fn record(&mut self, result: Result<(), String>) -> Option<String> {
        match result {
            Ok(()) => {
                self.failures = 0;
                None
            }
            Err(e) => {
                self.failures = self.failures.saturating_add(1);
                (self.failures == REPORT_AFTER_FAILURES).then(|| {
                    format!(
                        "Nie zapisano znaku życia aplikacji ({e}). Po wyłączeniu komputera pomiar może skończyć się za wcześnie."
                    )
                })
            }
        }
    }
}

/// Jeden wiersz `app_heartbeat` z chwilą `now`.
pub fn beat_at(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_heartbeat(id,at) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET at=excluded.at",
        [now],
    )?;
    Ok(())
}

/// Puls z pętli `idle::start`. Zapis jest zawsze, nie tylko przy trwającym
/// pomiarze: puls znaczy wtedy po prostu „aplikacja działa”, a pętla nie musi
/// co 2 s pytać bazy o pomiar. Koszt to jeden UPSERT na 30 s. Błąd zapisu nie
/// przerywa pętli; następna próba po 30 s, a trwały błąd zgłasza raz
/// `storage-error` (`Beat::record`).
pub(crate) fn tick(app: &tauri::AppHandle, beat: &mut Beat, now: i64) {
    if !beat.due(now) {
        return;
    }
    let result = app
        .state::<Database>()
        .0
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|conn| beat_at(&conn, now).map_err(|e| e.to_string()));
    if let Err(e) = &result {
        eprintln!("Puls aplikacji: {e}");
    }
    if let Some(message) = beat.record(result) {
        let _ = app.emit("storage-error", message);
    }
}

/// Przy starcie: trwający pomiar, puls i co najmniej `BREAK_MIN` (10 min, ten
/// sam próg co bezczynność) od pulsu, to
/// koniec pomiaru na pulsie (nie wcześniej niż jego start). Bez wiersza pulsu
/// (pierwszy start po aktualizacji) i przy cofniętym zegarze nic się nie dzieje.
pub fn recover(conn: &Connection, now: i64) -> Result<Option<GapNotice>, String> {
    let active: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT id,task_id,started_at FROM sessions WHERE ended_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let heartbeat: Option<i64> = conn
        .query_row("SELECT at FROM app_heartbeat WHERE id=1", [], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    let (Some((session_id, task_id, started_at)), Some(beat)) = (active, heartbeat) else {
        return Ok(None);
    };
    if now - beat < BREAK_MIN {
        return Ok(None);
    }
    let stopped_at = started_at.max(beat);
    conn.execute(
        "UPDATE sessions SET ended_at=?1 WHERE id=?2 AND ended_at IS NULL",
        rusqlite::params![stopped_at, session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(GapNotice {
        session_id,
        task_id,
        stopped_at,
        restarted_at: now,
    }))
}

/// „Przywróć cały czas”: ten sam przedział trwa dalej, jakby aplikacja działała.
/// Odmowa, gdy przedział zmieniono po zatrzymaniu, zadanie jest w koszu albo
/// ukończone, trwa inny pomiar (indeks `one_active_session`) albo po
/// zatrzymaniu zapisano inny pomiar (przedziały z kosza też się liczą, jak w
/// sprawdzaniu konfliktów czasu). Jak Start zdejmuje archiwum i blokadę „Czekam
/// na” (`planner::clear_waiting`).
fn restore(conn: &mut Connection, notice: &GapNotice) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let (ended_at, done, deleted): (Option<i64>, bool, bool) = tx
        .query_row(
            "SELECT s.ended_at,t.done,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=s.task_id AND l.deleted_at IS NOT NULL) FROM sessions s JOIN tasks t ON t.id=s.task_id WHERE s.id=?1",
            [notice.session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "Nie znaleziono przedziału.".to_owned(),
            e => e.to_string(),
        })?;
    if deleted {
        return Err("Przedział należy do zadania w koszu.".into());
    }
    if done {
        return Err("Najpierw przywróć zadanie do zrobienia.".into());
    }
    if ended_at != Some(notice.stopped_at) {
        return Err("Przedział zmieniono po zatrzymaniu. Popraw go na osi dnia.".into());
    }
    let running: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if running {
        return Err("Trwa inny pomiar. Zatrzymaj go, aby przywrócić cały czas.".into());
    }
    let later: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id<>?1 AND ended_at>?2)",
            rusqlite::params![notice.session_id, notice.stopped_at],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if later {
        return Err("Po zatrzymaniu zapisano inny pomiar. Przywrócenie nałożyłoby czas.".into());
    }
    tx.execute(
        "UPDATE sessions SET ended_at=NULL WHERE id=?1",
        [notice.session_id],
    )
    .map_err(|e| e.to_string())?;
    // Jak start w db::change: trwający pomiar nie zostaje w archiwum ani z blokadą „Czekam na”.
    tx.execute(
        "UPDATE task_lifecycle SET archived=0 WHERE task_id=?1",
        [notice.task_id],
    )
    .map_err(|e| e.to_string())?;
    crate::planner::clear_waiting(&tx, notice.task_id).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// Odpowiedź z banera: `keep` („OK”, zatrzymanie zostaje), `restore`
/// („Przywróć cały czas”), `resume` („Wznów pomiar”: to samo zadanie od teraz,
/// jak Start w `db::change`).
pub fn resolve(
    conn: &mut Connection,
    notice: &GapNotice,
    action: &str,
    now: i64,
) -> Result<db::Snapshot, String> {
    match action {
        "keep" => {}
        "restore" => restore(conn, notice)?,
        "resume" => return db::change(conn, "start", Some(notice.task_id), None, now),
        _ => return Err("Nieznana odpowiedź.".into()),
    }
    db::snapshot(conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gap_notice(app: tauri::AppHandle) -> Option<GapNotice> {
    app.state::<State>().0.lock().ok().and_then(|n| n.clone())
}

/// Jak `idle::idle_resolve`: notatka znika tylko po udanej odpowiedzi.
#[tauri::command]
pub fn gap_resolve(app: tauri::AppHandle, action: String) -> Result<db::Snapshot, String> {
    let state = app.state::<State>();
    let mut pending = state.0.lock().map_err(|e| e.to_string())?;
    let notice = pending
        .clone()
        .ok_or("Brak zgłoszonej przerwy w pracy aplikacji.")?;
    let database = app.state::<Database>();
    let mut conn = database.0.lock().map_err(|e| e.to_string())?;
    let result = resolve(&mut conn, &notice, &action, db::now())?;
    *pending = None;
    drop(conn);
    drop(pending);
    if action != "keep" {
        crate::reminder::reset(&app);
    }
    let _ = app.emit("data-changed", ());
    let _ = app.emit("gap-notice", ());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MIN: i64 = 60_000;

    /// Zadania 1 „Mierzone” i 2 „Inne”, pomiar zadania 1 od `start`, puls opcjonalnie.
    fn running(start: i64, heartbeat: Option<i64>) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        for title in ["Mierzone", "Inne"] {
            conn.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db::change(&mut conn, "start", Some(1), None, start).unwrap();
        if let Some(at) = heartbeat {
            beat_at(&conn, at).unwrap();
        }
        conn
    }
    fn rows(conn: &Connection) -> Vec<(i64, i64, Option<i64>)> {
        let mut q = conn
            .prepare("SELECT task_id,started_at,ended_at FROM sessions ORDER BY id")
            .unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
    fn stopped(start: i64) -> (Connection, GapNotice) {
        let conn = running(start, Some(30 * MIN));
        let notice = recover(&conn, 40 * MIN).unwrap().unwrap();
        (conn, notice)
    }
    fn run(
        conn: &mut Connection,
        notice: &GapNotice,
        action: &str,
        now: i64,
    ) -> Result<(), String> {
        resolve(conn, notice, action, now).map(|_| ())
    }

    #[test]
    fn recovery_stops_measurement_at_last_heartbeat() {
        let conn = running(0, Some(30 * MIN));
        let notice = recover(&conn, 30 * MIN + BREAK_MIN).unwrap();
        assert_eq!(
            notice,
            Some(GapNotice {
                session_id: 1,
                task_id: 1,
                stopped_at: 30 * MIN,
                restarted_at: 40 * MIN
            })
        );
        assert_eq!(rows(&conn), vec![(1, 0, Some(30 * MIN))]);
        assert_eq!(recover(&conn, 50 * MIN).unwrap(), None);
    }

    #[test]
    fn recovery_leaves_measurement_after_short_gap_or_without_heartbeat() {
        let conn = running(0, Some(30 * MIN));
        assert_eq!(recover(&conn, 30 * MIN + BREAK_MIN - 1).unwrap(), None);
        assert_eq!(rows(&conn), vec![(1, 0, None)]);
        let conn = running(0, None);
        assert_eq!(recover(&conn, 1000 * MIN).unwrap(), None);
        assert_eq!(rows(&conn), vec![(1, 0, None)]);
        let conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        beat_at(&conn, 0).unwrap();
        assert_eq!(recover(&conn, 1000 * MIN).unwrap(), None);
        let conn = running(0, Some(50 * MIN));
        assert_eq!(recover(&conn, 40 * MIN).unwrap(), None);
        assert_eq!(rows(&conn), vec![(1, 0, None)]);
    }

    #[test]
    fn recovery_never_ends_before_measurement_start() {
        let conn = running(50 * MIN, Some(30 * MIN));
        let notice = recover(&conn, 100 * MIN).unwrap().unwrap();
        assert_eq!(notice.stopped_at, 50 * MIN);
        assert_eq!(rows(&conn), vec![(1, 50 * MIN, Some(50 * MIN))]);
    }

    #[test]
    fn restore_refuses_while_another_measurement_runs() {
        let (mut conn, notice) = stopped(0);
        db::change(&mut conn, "start", Some(2), None, 41 * MIN).unwrap();
        let err = run(&mut conn, &notice, "restore", 42 * MIN).unwrap_err();
        assert!(err.contains("Trwa inny pomiar"), "{err}");
        assert_eq!(rows(&conn)[0], (1, 0, Some(30 * MIN)));
    }

    #[test]
    fn restore_reopens_measurement_when_nothing_else_runs() {
        let (mut conn, notice) = stopped(0);
        crate::planner::set_waiting(
            &mut conn,
            1,
            &crate::planner::WaitingChange {
                action: "block".into(),
                text: "Klient".into(),
            },
            41 * MIN,
        )
        .unwrap();
        db::change(&mut conn, "archive", Some(1), None, 41 * MIN).unwrap();
        let state = |conn: &Connection| -> (bool, String) {
            conn.query_row(
                "SELECT COALESCE(l.archived,0),COALESCE(json_extract(d.json,'$.waiting'),'') FROM tasks t LEFT JOIN task_lifecycle l ON l.task_id=t.id LEFT JOIN task_details d ON d.task_id=t.id WHERE t.id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(state(&conn), (true, "Klient".to_owned()));
        let snapshot = resolve(&mut conn, &notice, "restore", 42 * MIN).unwrap();
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].ended_at, None);
        assert_eq!(rows(&conn), vec![(1, 0, None)]);
        assert_eq!(state(&conn), (false, String::new()));
    }

    #[test]
    fn restore_reports_missing_interval_and_other_sqlite_errors() {
        let (mut conn, notice) = stopped(0);
        let missing = GapNotice {
            session_id: 99,
            ..notice.clone()
        };
        assert_eq!(
            run(&mut conn, &missing, "restore", 42 * MIN).unwrap_err(),
            "Nie znaleziono przedziału."
        );
        // Wartość, której nie da się odczytać jako bool: błąd SQLite inny niż brak wiersza.
        conn.execute("UPDATE tasks SET done='nie' WHERE id=1", [])
            .unwrap();
        let err = run(&mut conn, &notice, "restore", 42 * MIN).unwrap_err();
        assert_ne!(err, "Nie znaleziono przedziału.");
        assert!(err.contains("done"), "{err}");
    }

    #[test]
    fn heartbeat_failure_is_reported_once_until_a_write_succeeds() {
        let mut beat = Beat::default();
        assert_eq!(beat.record(Err("blokada".into())), None);
        let message = beat.record(Err("blokada".into())).unwrap();
        assert!(
            message.starts_with("Nie zapisano znaku życia aplikacji (blokada)."),
            "{message}"
        );
        assert_eq!(beat.record(Err("blokada".into())), None);
        assert_eq!(beat.record(Ok(())), None);
        assert_eq!(beat.record(Err("x".into())), None);
        assert!(beat.record(Err("x".into())).is_some());
    }

    #[test]
    fn restore_refuses_later_measurement_edited_interval_and_done_task() {
        let (mut conn, notice) = stopped(0);
        db::change(&mut conn, "start", Some(2), None, 41 * MIN).unwrap();
        db::change(&mut conn, "stop", None, None, 42 * MIN).unwrap();
        let err = run(&mut conn, &notice, "restore", 43 * MIN).unwrap_err();
        assert!(err.contains("Po zatrzymaniu zapisano inny pomiar"), "{err}");

        let (mut conn, notice) = stopped(0);
        conn.execute("UPDATE sessions SET ended_at=?1 WHERE id=1", [35 * MIN])
            .unwrap();
        let err = run(&mut conn, &notice, "restore", 43 * MIN).unwrap_err();
        assert!(err.contains("Przedział zmieniono po zatrzymaniu"), "{err}");

        let (mut conn, notice) = stopped(0);
        db::change(&mut conn, "toggle", Some(1), None, 41 * MIN).unwrap();
        let err = run(&mut conn, &notice, "restore", 43 * MIN).unwrap_err();
        assert_eq!(err, "Najpierw przywróć zadanie do zrobienia.");
        assert_eq!(rows(&conn)[0].2, Some(30 * MIN));

        let (mut conn, notice) = stopped(0);
        db::change(&mut conn, "delete", Some(1), None, 41 * MIN).unwrap();
        let err = run(&mut conn, &notice, "restore", 43 * MIN).unwrap_err();
        assert_eq!(err, "Przedział należy do zadania w koszu.");
        assert_eq!(rows(&conn)[0].2, Some(30 * MIN));
    }

    #[test]
    fn resume_starts_the_same_task_now() {
        let (mut conn, notice) = stopped(0);
        run(&mut conn, &notice, "resume", 45 * MIN).unwrap();
        assert_eq!(
            rows(&conn),
            vec![(1, 0, Some(30 * MIN)), (1, 45 * MIN, None)]
        );
    }

    #[test]
    fn keep_leaves_the_stop_and_unknown_action_fails() {
        let (mut conn, notice) = stopped(0);
        run(&mut conn, &notice, "keep", 45 * MIN).unwrap();
        assert_eq!(rows(&conn), vec![(1, 0, Some(30 * MIN))]);
        assert_eq!(
            run(&mut conn, &notice, "x", 45 * MIN).unwrap_err(),
            "Nieznana odpowiedź."
        );
    }

    #[test]
    fn heartbeat_is_one_row_written_at_most_every_30_s() {
        let mut beat = Beat::default();
        assert!(beat.due(0));
        assert!(!beat.due(29_999));
        assert!(beat.due(30_000));
        assert!(beat.due(10_000));
        let conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        beat_at(&conn, 1).unwrap();
        beat_at(&conn, 2).unwrap();
        let (count, at): (i64, i64) = conn
            .query_row("SELECT COUNT(*),MAX(at) FROM app_heartbeat", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((count, at), (1, 2));
        db::init(&conn).unwrap();
    }
}
