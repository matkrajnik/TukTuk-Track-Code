//! Rozpocznij, zakończ i wznów dzień (PLAN_BACKLOG.md, B13).
//!
//! Historia dni w tabeli `day_marks(day, started_at, ended_at, resumed)`
//! (`CREATE TABLE IF NOT EXISTS` w `db::init`, kopia SQLite obejmuje ją, bo
//! kopiuje całą bazę). Stan dnia: brak wiersza (albo wiersz bez `ended_at` i bez
//! `started_at`) = nie rozpoczęty, `ended_at` puste = rozpoczęty, `ended_at`
//! ustawione = zakończony. Komendy działają tylko na dzisiejszym dniu: dzień
//! w przyszłości i dzień miniony są odrzucane, więc „Wznów dzień” po północy
//! nie ma już czego wznowić.
//!
//! „Rozpocznij dzień” zapisuje też `morning_review_day` / `morning_review_at`
//! (od nich zależą powiadomienie „Przegląd dnia” i „nowe” zgłoszenia Jiry).
//! „Zakończ dzień” przy trwającym pomiarze wymaga jawnego `stop_measurement`
//! z dialogu: bez niego komenda zwraca błąd `MEASURING` i niczego nie zmienia.
//! Po zakończeniu dnia do północy milczą przypomnienie o braku pomiaru
//! (`reminder`), powiadomienie o celu (`notify`) i propozycje startu
//! (`suggest`); timer zmienia się wyłącznie po kliknięciu.
use crate::{db, settings, stats, Database};
use chrono::{NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Błąd „Zakończ dzień” przy trwającym pomiarze bez potwierdzenia zatrzymania.
pub const MEASURING: &str = "Trwa pomiar. Potwierdź zatrzymanie pomiaru, żeby zakończyć dzień.";

/// Wiersz `day_marks` (w migawce jako `dayMarks`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayMark {
    pub day: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub resumed: i64,
}

/// Stan dnia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayState {
    NotStarted,
    Started,
    Ended,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<DayMark> {
    Ok(DayMark {
        day: r.get(0)?,
        started_at: r.get(1)?,
        ended_at: r.get(2)?,
        resumed: r.get(3)?,
    })
}

/// Wszystkie dni w kolejności (migawka: Raport dnia, Zamknięcie dnia, Miesiąc).
pub fn all(db: &Connection) -> rusqlite::Result<Vec<DayMark>> {
    db.prepare("SELECT day,started_at,ended_at,resumed FROM day_marks ORDER BY day")?
        .query_map([], row)?
        .collect()
}

pub fn get(db: &Connection, day: &str) -> rusqlite::Result<Option<DayMark>> {
    db.query_row(
        "SELECT day,started_at,ended_at,resumed FROM day_marks WHERE day=?1",
        [day],
        row,
    )
    .optional()
}

pub fn state_of(mark: Option<&DayMark>) -> DayState {
    match mark {
        Some(m) if m.ended_at.is_some() => DayState::Ended,
        Some(m) if m.started_at.is_some() || m.resumed > 0 => DayState::Started,
        _ => DayState::NotStarted,
    }
}

/// Dzień `day` musi być dzisiejszym (`today`); przyszły i miniony są odrzucane.
fn check_today(day: &str, today: NaiveDate) -> Result<(), String> {
    if !crate::planner::date_valid(day) {
        return Err("Niepoprawny dzień.".into());
    }
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| "Niepoprawny dzień.")?;
    if date > today {
        return Err("Nie można zmienić stanu dnia w przyszłości.".into());
    }
    if date < today {
        return Err("Rozpocząć, zakończyć i wznowić można tylko dzisiejszy dzień.".into());
    }
    Ok(())
}

/// „Rozpocznij dzień”: `started_at` = `now`. Zwraca `true`, gdy dzień właśnie
/// się zaczął (wtedy wywołujący zapisuje przegląd dnia w ustawieniach), `false`
/// dla dnia już rozpoczętego. Zakończony dzień wznawia się „Wznów dzień”.
pub fn start(db: &Connection, day: &str, today: NaiveDate, now: i64) -> Result<bool, String> {
    check_today(day, today)?;
    let mark = get(db, day).map_err(|e| e.to_string())?;
    match state_of(mark.as_ref()) {
        DayState::Ended => Err("Dzień jest już zakończony. Użyj „Wznów dzień”.".into()),
        DayState::Started => Ok(false),
        DayState::NotStarted => {
            db.execute(
                "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES(?1,?2,NULL,0)
                 ON CONFLICT(day) DO UPDATE SET started_at=excluded.started_at",
                params![day, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(true)
        }
    }
}

/// Ustawienia zapisywane przez „Rozpocznij dzień” (dawne „Zaczynam dzień”).
pub fn review_patch(day: &str, now: i64) -> serde_json::Value {
    serde_json::json!({"morningReviewDay": day, "morningReviewAt": now})
}

/// Początek pierwszego pomiaru dnia (przedział sprzed północy liczy się od
/// północy), bez zadań w koszu.
fn first_measurement<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    today: NaiveDate,
) -> rusqlite::Result<Option<i64>> {
    let (from, to) = stats::day_bounds(tz, today);
    db.query_row(
        "SELECT MIN(MAX(s.started_at, ?1)) FROM sessions s
         WHERE s.started_at < ?2 AND (s.ended_at IS NULL OR s.ended_at > ?1)
           AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)",
        params![from, to],
        |r| r.get(0),
    )
}

/// „Zakończ dzień”: `ended_at` = `now`. Trwający pomiar zatrzymuje tylko jawne
/// `stop_measurement` (potwierdzenie w dialogu); bez niego `Err(MEASURING)`.
/// Nie rozpoczęty dzień też można zakończyć: `started_at` = przegląd dnia
/// zapisany dziś (`review_at`), a bez niego początek pierwszego pomiaru dnia
/// albo puste. Zwraca, czy pomiar został zatrzymany.
pub fn end<Tz: TimeZone>(
    db: &mut Connection,
    tz: &Tz,
    day: &str,
    today: NaiveDate,
    now: i64,
    review_at: Option<i64>,
    stop_measurement: bool,
) -> Result<bool, String> {
    check_today(day, today)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let mark = get(&tx, day).map_err(|e| e.to_string())?;
    if state_of(mark.as_ref()) == DayState::Ended {
        return Err("Dzień jest już zakończony.".into());
    }
    let measuring: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if measuring && !stop_measurement {
        return Err(MEASURING.into());
    }
    if measuring {
        db::stop(&tx, now).map_err(|e| e.to_string())?;
    }
    let started = match mark.as_ref().and_then(|m| m.started_at) {
        Some(at) => Some(at),
        None => match review_at {
            Some(at) => Some(at),
            None => first_measurement(&tx, tz, today).map_err(|e| e.to_string())?,
        },
    };
    tx.execute(
        "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES(?1,?2,?3,0)
         ON CONFLICT(day) DO UPDATE SET started_at=excluded.started_at, ended_at=excluded.ended_at",
        params![day, started, now],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(measuring)
}

/// „Wznów dzień”: tylko zakończony dzisiejszy dzień; czyści `ended_at`
/// i zwiększa `resumed`. Pomiaru nie wznawia (to osobne kliknięcie).
pub fn resume(db: &Connection, day: &str, today: NaiveDate) -> Result<(), String> {
    check_today(day, today)?;
    let mark = get(db, day).map_err(|e| e.to_string())?;
    if state_of(mark.as_ref()) != DayState::Ended {
        return Err("Dzień nie jest zakończony.".into());
    }
    db.execute(
        "UPDATE day_marks SET ended_at=NULL, resumed=resumed+1 WHERE day=?1",
        [day],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Dzień `now` (w strefie `tz`) jest zakończony: wyciszenie do północy. Po
/// północy liczy się nowy dzień bez wiersza, więc wyciszenie mija samo.
pub fn ended_at<Tz: TimeZone>(db: &Connection, tz: &Tz, now: i64) -> Option<i64> {
    let today = tz.timestamp_millis_opt(now).earliest()?.date_naive();
    get(db, &today.format("%Y-%m-%d").to_string())
        .ok()
        .flatten()
        .and_then(|m| m.ended_at)
}

/// `ended_at` w lokalnej strefie (wątki przypomnień, traya i propozycji).
pub fn ended_now(db: &Connection) -> bool {
    ended_at(db, &chrono::Local, db::now()).is_some()
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Po zmianie stanu dnia: przypomnienie od nowa, propozycje, tray i okna.
fn changed(app: &tauri::AppHandle, window: Option<&tauri::Window>) {
    crate::reminder::reset(app);
    match window {
        Some(w) => crate::data_changed(app, w),
        None => {
            let _ = app.emit("data-changed", ());
        }
    }
    crate::suggest::refresh(app, db::now());
}

#[tauri::command]
pub fn day_start(
    app: tauri::AppHandle,
    window: tauri::Window,
    day: String,
) -> Result<db::Snapshot, String> {
    let (snapshot, saved) = {
        let state = app.state::<Database>();
        let mut connection = state.0.lock().map_err(|e| e.to_string())?;
        let now = db::now();
        let first = start(&connection, &day, today(), now)?;
        let saved = if first {
            Some(settings::save_cached(
                &app,
                &mut connection,
                &review_patch(&day, now),
            )?)
        } else {
            None
        };
        (db::snapshot(&connection).map_err(|e| e.to_string())?, saved)
    };
    if let Some(s) = saved {
        let _ = app.emit("settings-changed", s);
    }
    changed(&app, Some(&window));
    Ok(snapshot)
}

fn end_today(
    app: &tauri::AppHandle,
    day: &str,
    stop_measurement: bool,
) -> Result<db::Snapshot, String> {
    let state = app.state::<Database>();
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let s = settings::cached(app, &connection);
    let review_at =
        (s.morning_review_day == day && s.morning_review_at > 0).then_some(s.morning_review_at);
    end(
        &mut connection,
        &chrono::Local,
        day,
        today(),
        db::now(),
        review_at,
        stop_measurement,
    )?;
    db::snapshot(&connection).map_err(|e| e.to_string())
}

/// „Zakończ dzień” z Rano, Zamknięcia dnia i paska (dialog w głównym oknie);
/// `stop_measurement` przychodzi tylko z potwierdzenia „Zakończyć dzień
/// i zatrzymać pomiar »X«?”.
#[tauri::command]
pub fn day_end(
    app: tauri::AppHandle,
    window: tauri::Window,
    day: String,
    stop_measurement: bool,
) -> Result<db::Snapshot, String> {
    let snapshot = end_today(&app, &day, stop_measurement)?;
    changed(&app, Some(&window));
    Ok(snapshot)
}

#[tauri::command]
pub fn day_resume(
    app: tauri::AppHandle,
    window: tauri::Window,
    day: String,
) -> Result<db::Snapshot, String> {
    let snapshot = {
        let state = app.state::<Database>();
        let connection = state.0.lock().map_err(|e| e.to_string())?;
        resume(&connection, &day, today())?;
        db::snapshot(&connection).map_err(|e| e.to_string())?
    };
    changed(&app, Some(&window));
    Ok(snapshot)
}

/// Zdarzenie do głównego okna: „Zakończ dzień” z traya albo widgetu. Główne
/// okno pokazuje dialog zatrzymania pomiaru (gdy trwa) i Zamknięcie dnia.
pub const END_REQUEST: &str = "day-end-request";

/// „Zakończ dzień” spoza głównego okna: pokazuje okno i prosi je o dialog,
/// bo zatrzymanie pomiaru wymaga potwierdzenia.
pub fn request_end(app: &tauri::AppHandle) {
    crate::show(app);
    let _ = app.emit_to("main", END_REQUEST, ());
}

/// „Wznów dzień” z traya: bez dialogu, pomiar zostaje zatrzymany.
pub fn resume_now(app: &tauri::AppHandle) {
    let day = today().format("%Y-%m-%d").to_string();
    let result = app
        .state::<Database>()
        .0
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|db| resume(&db, &day, today()));
    match result {
        Ok(()) => changed(app, None),
        Err(e) => {
            crate::show(app);
            let _ = app.emit("storage-error", e);
        }
    }
}

/// „Zakończ dzień” z widgetu (jak z traya).
#[tauri::command]
pub fn day_end_request(app: tauri::AppHandle) {
    request_end(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
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
        for title in ["A", "B"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        db
    }
    fn session(db: &Connection, task: i64, start: i64, end: Option<i64>) {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            params![task, start, end],
        )
        .unwrap();
    }
    fn active(db: &Connection) -> bool {
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }
    const DAY: &str = "2026-10-15";

    #[test]
    fn init_creates_table_idempotently_and_snapshot_lists_marks() {
        let db = setup();
        db::init(&db).unwrap();
        assert!(db::snapshot(&db).unwrap().day_marks.is_empty());
        start(&db, DAY, date(DAY), at("2026-10-15 08:05")).unwrap();
        let marks = db::snapshot(&db).unwrap().day_marks;
        assert_eq!(
            marks,
            vec![DayMark {
                day: DAY.into(),
                started_at: Some(at("2026-10-15 08:05")),
                ended_at: None,
                resumed: 0,
            }]
        );
    }

    #[test]
    fn states_follow_start_end_resume_order() {
        let mut db = setup();
        let today = date(DAY);
        assert_eq!(
            state_of(get(&db, DAY).unwrap().as_ref()),
            DayState::NotStarted
        );
        // Wznowić można tylko zakończony dzień.
        assert_eq!(
            resume(&db, DAY, today).unwrap_err(),
            "Dzień nie jest zakończony."
        );
        assert!(start(&db, DAY, today, at("2026-10-15 08:05")).unwrap());
        assert_eq!(state_of(get(&db, DAY).unwrap().as_ref()), DayState::Started);
        // Ponowne „Rozpocznij” nie przesuwa początku dnia.
        assert!(!start(&db, DAY, today, at("2026-10-15 09:00")).unwrap());
        assert_eq!(
            get(&db, DAY).unwrap().unwrap().started_at,
            Some(at("2026-10-15 08:05"))
        );
        assert_eq!(
            resume(&db, DAY, today).unwrap_err(),
            "Dzień nie jest zakończony."
        );

        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:40"),
            None,
            false,
        )
        .unwrap();
        let mark = get(&db, DAY).unwrap().unwrap();
        assert_eq!(state_of(Some(&mark)), DayState::Ended);
        assert_eq!(mark.started_at, Some(at("2026-10-15 08:05")));
        assert_eq!(mark.ended_at, Some(at("2026-10-15 16:40")));
        assert_eq!(
            end(
                &mut db,
                &Warsaw,
                DAY,
                today,
                at("2026-10-15 16:41"),
                None,
                false
            )
            .unwrap_err(),
            "Dzień jest już zakończony."
        );
        assert!(start(&db, DAY, today, at("2026-10-15 16:42"))
            .unwrap_err()
            .contains("Wznów dzień"));

        resume(&db, DAY, today).unwrap();
        let mark = get(&db, DAY).unwrap().unwrap();
        assert_eq!(state_of(Some(&mark)), DayState::Started);
        assert_eq!(mark.ended_at, None);
        assert_eq!(mark.resumed, 1);
        assert_eq!(mark.started_at, Some(at("2026-10-15 08:05")));
        // Drugie zakończenie i wznowienie liczą się dalej.
        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 18:00"),
            None,
            false,
        )
        .unwrap();
        resume(&db, DAY, today).unwrap();
        assert_eq!(get(&db, DAY).unwrap().unwrap().resumed, 2);
    }

    #[test]
    fn only_today_is_accepted() {
        let mut db = setup();
        let today = date(DAY);
        for day in ["2026-10-16", "2027-01-01"] {
            assert_eq!(
                start(&db, day, today, 0).unwrap_err(),
                "Nie można zmienić stanu dnia w przyszłości."
            );
            assert_eq!(
                end(&mut db, &Warsaw, day, today, 0, None, false).unwrap_err(),
                "Nie można zmienić stanu dnia w przyszłości."
            );
            assert_eq!(
                resume(&db, day, today).unwrap_err(),
                "Nie można zmienić stanu dnia w przyszłości."
            );
        }
        assert!(start(&db, "2026-10-14", today, 0)
            .unwrap_err()
            .contains("tylko dzisiejszy"));
        for bad in ["", "15.10.2026", "2026-02-30"] {
            assert_eq!(start(&db, bad, today, 0).unwrap_err(), "Niepoprawny dzień.");
        }
        assert!(db::snapshot(&db).unwrap().day_marks.is_empty());
    }

    /// „Wznów dzień” tylko dla dzisiaj: po północy wczorajszy zakończony dzień
    /// zostaje zakończony, a wyciszenie mija.
    #[test]
    fn resume_only_today_and_quiet_ends_at_midnight() {
        let mut db = setup();
        end(
            &mut db,
            &Warsaw,
            DAY,
            date(DAY),
            at("2026-10-15 16:40"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            ended_at(&db, &Warsaw, at("2026-10-15 23:59")),
            Some(at("2026-10-15 16:40"))
        );
        assert_eq!(ended_at(&db, &Warsaw, at("2026-10-16 00:00")), None);
        assert!(resume(&db, DAY, date("2026-10-16"))
            .unwrap_err()
            .contains("tylko dzisiejszy"));
        assert_eq!(state_of(get(&db, DAY).unwrap().as_ref()), DayState::Ended);
    }

    /// Trwający pomiar zatrzymuje tylko jawny parametr z dialogu.
    #[test]
    fn measurement_stops_only_with_explicit_confirmation() {
        let mut db = setup();
        let today = date(DAY);
        start(&db, DAY, today, at("2026-10-15 08:00")).unwrap();
        session(&db, 1, at("2026-10-15 15:00"), None);
        assert_eq!(
            end(
                &mut db,
                &Warsaw,
                DAY,
                today,
                at("2026-10-15 16:40"),
                None,
                false
            )
            .unwrap_err(),
            MEASURING
        );
        assert!(active(&db));
        assert_eq!(state_of(get(&db, DAY).unwrap().as_ref()), DayState::Started);
        assert!(end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:40"),
            None,
            true
        )
        .unwrap());
        assert!(!active(&db));
        let ended: i64 = db
            .query_row("SELECT ended_at FROM sessions WHERE task_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ended, at("2026-10-15 16:40"));
        // Bez pomiaru potwierdzenie niczego nie zatrzymuje.
        resume(&db, DAY, today).unwrap();
        assert!(!end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 17:00"),
            None,
            true
        )
        .unwrap());
    }

    /// Nie rozpoczęty dzień też można zakończyć: początek z przeglądu dnia,
    /// a bez niego z pierwszego pomiaru dnia (od północy), albo pusty.
    #[test]
    fn end_without_start_uses_review_or_first_measurement() {
        let mut db = setup();
        let today = date(DAY);
        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:00"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(get(&db, DAY).unwrap().unwrap().started_at, None);

        let mut db = setup();
        session(&db, 1, at("2026-10-14 23:30"), Some(at("2026-10-15 00:30")));
        session(&db, 2, at("2026-10-15 09:00"), Some(at("2026-10-15 10:00")));
        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:00"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            get(&db, DAY).unwrap().unwrap().started_at,
            Some(at("2026-10-15 00:00"))
        );

        let mut db = setup();
        session(&db, 2, at("2026-10-15 09:00"), Some(at("2026-10-15 10:00")));
        let review = at("2026-10-15 08:05");
        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:00"),
            Some(review),
            false,
        )
        .unwrap();
        assert_eq!(get(&db, DAY).unwrap().unwrap().started_at, Some(review));
        // Pomiary zadań z kosza się nie liczą.
        let mut db = setup();
        session(&db, 1, at("2026-10-15 07:00"), Some(at("2026-10-15 07:30")));
        session(&db, 2, at("2026-10-15 09:00"), Some(at("2026-10-15 10:00")));
        db::change(&mut db, "delete", Some(1), None, at("2026-10-15 11:00")).unwrap();
        end(
            &mut db,
            &Warsaw,
            DAY,
            today,
            at("2026-10-15 16:00"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            get(&db, DAY).unwrap().unwrap().started_at,
            Some(at("2026-10-15 09:00"))
        );
    }

    #[test]
    fn start_saves_morning_review_settings() {
        let mut db = setup();
        let now = at("2026-10-15 08:05");
        assert!(start(&db, DAY, date(DAY), now).unwrap());
        settings::save(&mut db, &review_patch(DAY, now)).unwrap();
        let s = settings::load(&db);
        assert_eq!(s.morning_review_day, DAY);
        assert_eq!(s.morning_review_at, now);
    }
}
