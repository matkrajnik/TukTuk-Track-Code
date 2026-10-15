//! Czas agentów jako osobny tor (PLAN_1.7.md, W1).
//!
//! Pętla agentów (`agents::start`, co 5 s) zapisuje obserwacje do tabeli
//! `agent_sessions(id, agent, folder, task_id, start, "end")`:
//! - kolejne obserwacje tego samego agenta (`claude`/`codex`) i folderu z przerwą
//!   krótszą niż `GAP` przedłużają ostatni okres (`end` = chwila obserwacji);
//! - okres jest „otwarty” tylko w tym sensie, że jego `end` rośnie z każdą
//!   obserwacją. Po utracie aktywności albo zamknięciu aplikacji nic go już nie
//!   przedłuża, więc kończy się na ostatniej obserwacji; kolejna praca po `GAP`
//!   zaczyna nowy okres;
//! - `task_id` pochodzi z dopasowania z 1.5 (`agents::resolve`); zmiana zadania
//!   w trakcie (np. nowa gałąź) zaczyna nowy okres. Przypisanie folderu
//!   (`agents::bind`) uzupełnia okresy z ostatnich `BACKFILL` ms bez zadania.
//!
//! Czas agentów nie wchodzi do celu dnia, raportu TS ani eksportu: to osobna
//! informacja na osi dnia, w rozwiniętym kafelku i w zakładce Miesiąc.
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::BTreeMap;

/// Przerwa, poniżej której kolejne obserwacje łączą się w jeden okres.
pub const GAP: i64 = 2 * 60_000;
/// Jak daleko wstecz przypisanie folderu uzupełnia okresy bez zadania.
pub const BACKFILL: i64 = 7 * 86_400_000;
/// Okresy kończące się wcześniej są usuwane przy starcie.
pub const KEEP: i64 = 400 * 86_400_000;
/// Nowy okres nie zaczyna się wcześniej niż tyle przed pierwszą obserwacją, nawet
/// gdy agent podaje starszy początek pracy (np. aplikację uruchomiono w trakcie).
const START_LIMIT: i64 = 15 * 60_000;

/// Jedna obserwacja pracującego agenta.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub agent: String,
    pub folder: String,
    pub task_id: Option<i64>,
    /// Od kiedy agent pracuje według jego plików stanu (ms).
    pub since: i64,
}

impl Observation {
    pub fn of(agent: &crate::agents::Agent) -> Self {
        Self {
            agent: agent.kind.clone(),
            folder: agent.cwd.clone(),
            task_id: agent.task.as_ref().and_then(|m| m.task_id),
            since: agent.since,
        }
    }
}

/// Zapisuje obserwacje z chwili `now`: przedłuża albo zaczyna okresy.
pub fn record(db: &Connection, observations: &[Observation], now: i64) -> rusqlite::Result<()> {
    // Kilka sesji tego samego agenta w jednym folderze to jeden okres.
    let mut unique: BTreeMap<(String, String), Observation> = BTreeMap::new();
    for o in observations {
        let key = (o.agent.clone(), crate::agents::norm_folder(&o.folder));
        match unique.get_mut(&key) {
            Some(seen) => {
                seen.since = seen.since.min(o.since);
                seen.task_id = seen.task_id.or(o.task_id);
            }
            None => {
                unique.insert(key, o.clone());
            }
        }
    }
    if unique.is_empty() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    for o in unique.into_values() {
        record_one(&tx, &o, now)?;
    }
    tx.commit()
}

/// Ostatni okres tego agenta w tym samym folderze. Foldery porównujemy po
/// normalizacji (`agents::norm_folder`: wielkość liter, separatory, ukośnik na
/// końcu), bo sesje w jednym folderze potrafią podawać ścieżkę różnie. Okresy
/// zakończone wcześniej niż `START_LIMIT` przed `now` nie wpływają ani na
/// łączenie (`GAP` < `START_LIMIT`), ani na początek nowego okresu, więc ich nie czytamy.
fn latest_period(
    db: &Connection,
    o: &Observation,
    now: i64,
) -> rusqlite::Result<Option<(i64, Option<i64>, i64)>> {
    let folder = crate::agents::norm_folder(&o.folder);
    let mut stmt = db.prepare_cached(
        "SELECT id,task_id,\"end\",folder FROM agent_sessions WHERE agent=?1 AND \"end\">=?2 ORDER BY \"end\" DESC, id DESC",
    )?;
    let mut rows = stmt.query(params![o.agent, now - START_LIMIT])?;
    while let Some(r) = rows.next()? {
        let stored: String = r.get(3)?;
        if crate::agents::norm_folder(&stored) == folder {
            return Ok(Some((r.get(0)?, r.get(1)?, r.get(2)?)));
        }
    }
    Ok(None)
}

fn record_one(db: &Connection, o: &Observation, now: i64) -> rusqlite::Result<()> {
    let last = latest_period(db, o, now)?;
    if let Some((id, task, end)) = last {
        if now >= end && now - end < GAP {
            let same = task.is_none() || o.task_id.is_none() || task == o.task_id;
            if same {
                db.execute(
                    "UPDATE agent_sessions SET \"end\"=MAX(\"end\",?2), task_id=COALESCE(task_id,?3) WHERE id=?1",
                    params![id, now, o.task_id],
                )?;
                return Ok(());
            }
            // Inne zadanie bez przerwy: nowy okres zaczyna się tam, gdzie skończył poprzedni.
            db.execute(
                "INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES(?1,?2,?3,?4,?5)",
                params![o.agent, o.folder, o.task_id, end, now],
            )?;
            return Ok(());
        }
    }
    let floor = last.map(|(_, _, end)| end).unwrap_or(i64::MIN);
    let start = o.since.max(now - START_LIMIT).max(floor).min(now);
    db.execute(
        "INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES(?1,?2,?3,?4,?5)",
        params![o.agent, o.folder, o.task_id, start, now],
    )?;
    Ok(())
}

/// Po przypisaniu folderu: okresy z ostatnich `BACKFILL` ms bez zadania dostają
/// zadanie z przypisań (`agents::bound_task`, z podfolderami i najdłuższym
/// dopasowaniem). Zwraca liczbę uzupełnionych okresów.
pub fn backfill(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    let rows: Vec<(i64, String)> = db
        .prepare("SELECT id,folder FROM agent_sessions WHERE task_id IS NULL AND \"end\" >= ?1")?
        .query_map([now - BACKFILL], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut filled = 0;
    for (id, folder) in rows {
        if let Some(task) = crate::agents::bound_task(db, &folder)?.and_then(|m| m.task_id) {
            filled += db.execute(
                "UPDATE agent_sessions SET task_id=?2 WHERE id=?1 AND task_id IS NULL",
                params![id, task],
            )?;
        }
    }
    Ok(filled)
}

/// Usuwa okresy starsze niż 400 dni (przy starcie aplikacji).
pub fn prune(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    db.execute(
        "DELETE FROM agent_sessions WHERE \"end\" < ?1",
        [now - KEEP],
    )
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub id: i64,
    pub agent: String,
    pub folder: String,
    pub task_id: Option<i64>,
    pub start: i64,
    pub end: i64,
}

/// Czas agentów nad zadaniem i część nakładająca się na Twój pomiar tego zadania.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Total {
    pub task_id: i64,
    pub agent_ms: i64,
    pub parallel_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTime {
    pub periods: Vec<Period>,
    pub totals: Vec<Total>,
    /// Czas agentów bez dopasowanego zadania (suma przedziałów bez nakładania).
    pub unmatched_ms: i64,
}

/// Suma przedziałów: posortowane, bez nakładania, puste pominięte.
pub fn union(mut spans: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    spans.retain(|(a, b)| b > a);
    spans.sort_unstable();
    let mut out: Vec<(i64, i64)> = Vec::new();
    for (a, b) in spans {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

fn length(spans: &[(i64, i64)]) -> i64 {
    spans.iter().map(|(a, b)| b - a).sum()
}

/// Długość części wspólnej dwóch sum przedziałów (wynik `union`).
pub fn overlap(a: &[(i64, i64)], b: &[(i64, i64)]) -> i64 {
    let (mut i, mut j, mut sum) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        let from = a[i].0.max(b[j].0);
        let to = a[i].1.min(b[j].1);
        if to > from {
            sum += to - from;
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    sum
}

/// Sumy według zadania w `[from, to)`: agenci (bez liczenia dwa razy, gdy dwóch
/// agentów pracuje równocześnie) i część równoległa z przedziałami pomiaru
/// (`sessions`: zadanie, początek, koniec; trwający pomiar kończy się w `now`).
pub fn totals(
    periods: &[Period],
    sessions: &[(i64, i64, Option<i64>)],
    from: i64,
    to: i64,
    now: i64,
) -> (Vec<Total>, i64) {
    let clip = |a: i64, b: i64| (a.max(from), b.min(to));
    let mut agent: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    let mut unmatched = Vec::new();
    for p in periods {
        match p.task_id {
            Some(task) => agent.entry(task).or_default().push(clip(p.start, p.end)),
            None => unmatched.push(clip(p.start, p.end)),
        }
    }
    let mut out = Vec::new();
    for (task, spans) in agent {
        let spans = union(spans);
        let mine = union(
            sessions
                .iter()
                .filter(|(t, _, _)| *t == task)
                .map(|(_, a, b)| clip(*a, b.unwrap_or(now).max(*a)))
                .collect(),
        );
        let agent_ms = length(&spans);
        if agent_ms > 0 {
            out.push(Total {
                task_id: task,
                agent_ms,
                parallel_ms: overlap(&spans, &mine),
            });
        }
    }
    (out, length(&union(unmatched)))
}

/// Okresy nakładające się na `[from, to)` z sumami według zadania.
pub fn query(db: &Connection, from: i64, to: i64, now: i64) -> rusqlite::Result<AgentTime> {
    let periods: Vec<Period> = db
        .prepare(
            "SELECT id,agent,folder,task_id,start,\"end\" FROM agent_sessions WHERE start < ?2 AND \"end\" > ?1 ORDER BY start, id",
        )?
        .query_map(params![from, to], |r| {
            Ok(Period {
                id: r.get(0)?,
                agent: r.get(1)?,
                folder: r.get(2)?,
                task_id: r.get(3)?,
                start: r.get(4)?,
                end: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let sessions: Vec<(i64, i64, Option<i64>)> = db
        .prepare(
            "SELECT task_id,started_at,ended_at FROM sessions WHERE started_at < ?2 AND COALESCE(ended_at,?3) > ?1",
        )?
        .query_map(params![from, to, now], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let (totals, unmatched_ms) = totals(&periods, &sessions, from, to, now);
    Ok(AgentTime {
        periods,
        totals,
        unmatched_ms,
    })
}

/// Zapis z pętli agentów; błąd bazy nie zatrzymuje pętli.
pub fn observe(app: &tauri::AppHandle, agents: &[crate::agents::Agent], now: i64) {
    use tauri::Manager;
    let observations: Vec<Observation> = agents.iter().map(Observation::of).collect();
    if observations.is_empty() {
        return;
    }
    let state = app.state::<crate::Database>();
    if let Ok(db) = state.0.lock() {
        let _ = record(&db, &observations, now);
    };
}

#[tauri::command]
pub fn agent_time(
    state: tauri::State<crate::Database>,
    from: i64,
    to: i64,
) -> Result<AgentTime, String> {
    if to < from {
        return Err("Niepoprawny zakres dat.".into());
    }
    let db = state.0.lock().map_err(|e| e.to_string())?;
    query(&db, from, to, crate::db::now()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const T: i64 = 1_800_000_000_000;
    const S: i64 = 1_000;
    const MIN: i64 = 60_000;

    fn open() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["Analiza", "Eksport"] {
            db.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db
    }
    fn obs(agent: &str, folder: &str, task: Option<i64>, since: i64) -> Observation {
        Observation {
            agent: agent.into(),
            folder: folder.into(),
            task_id: task,
            since,
        }
    }
    fn rows(db: &Connection) -> Vec<(String, Option<i64>, i64, i64)> {
        db.prepare("SELECT agent,task_id,start,\"end\" FROM agent_sessions ORDER BY start, id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn observations_with_short_gaps_merge_into_one_period() {
        let db = open();
        let a = obs("claude", "C:\\repo", Some(1), T);
        record(&db, &[a.clone()], T).unwrap();
        record(&db, &[a.clone()], T + 5 * S).unwrap();
        // Przerwa poniżej 2 min: ten sam okres.
        record(&db, &[a.clone()], T + 5 * S + GAP - S).unwrap();
        assert_eq!(
            rows(&db),
            vec![("claude".into(), Some(1), T, T + 4 * S + GAP)]
        );
        // Dwie sesje tego samego agenta w tym samym folderze (inne separatory) to jeden okres.
        let twin = obs("claude", "c:/repo/", Some(1), T);
        record(&db, &[a.clone(), twin], T + GAP + 10 * S).unwrap();
        assert_eq!(rows(&db).len(), 1);
        // Inny agent albo inny folder ma własny okres.
        record(
            &db,
            &[
                obs("codex", "C:\\repo", None, T + GAP),
                obs("claude", "C:\\inne", None, T + GAP),
            ],
            T + GAP + 15 * S,
        )
        .unwrap();
        assert_eq!(rows(&db).len(), 3);
    }

    #[test]
    fn folder_spelled_differently_in_next_observation_extends_same_period() {
        let db = open();
        record(&db, &[obs("claude", "C:\\Projekty\\x", Some(1), T)], T).unwrap();
        // Kolejne odczyty: inna wielkość litery dysku, ukośnik na końcu, inne separatory.
        record(
            &db,
            &[obs("claude", "c:\\Projekty\\x\\", Some(1), T)],
            T + 5 * S,
        )
        .unwrap();
        record(&db, &[obs("claude", "c:/projekty/X", None, T)], T + 10 * S).unwrap();
        assert_eq!(rows(&db), vec![("claude".into(), Some(1), T, T + 10 * S)]);
        // Po przerwie nowy okres zaczyna się nie wcześniej niż koniec poprzedniego,
        // także gdy folder zapisano inaczej.
        record(
            &db,
            &[obs("claude", "C:/Projekty/x/", Some(1), T - 3_600 * S)],
            T + 10 * S + GAP,
        )
        .unwrap();
        assert_eq!(
            rows(&db),
            vec![
                ("claude".into(), Some(1), T, T + 10 * S),
                ("claude".into(), Some(1), T + 10 * S, T + 10 * S + GAP),
            ]
        );
    }

    #[test]
    fn period_closes_when_agent_stops_and_next_work_starts_new_one() {
        let db = open();
        let a = obs("codex", "C:\\api", Some(2), T);
        record(&db, &[a.clone()], T).unwrap();
        record(&db, &[a.clone()], T + 5 * S).unwrap();
        // Utrata aktywności (albo zamknięta aplikacja): brak obserwacji nic nie przedłuża.
        record(&db, &[], T + 10 * S).unwrap();
        record(&db, &[], T + 60 * S).unwrap();
        assert_eq!(rows(&db), vec![("codex".into(), Some(2), T, T + 5 * S)]);
        // Praca po przerwie dłuższej niż 2 min to nowy okres, który nie sięga
        // przed koniec poprzedniego ani dalej niż 15 min wstecz.
        let back = obs("codex", "C:\\api", Some(2), T - 3_600 * S);
        record(&db, &[back], T + 5 * S + GAP).unwrap();
        assert_eq!(
            rows(&db),
            vec![
                ("codex".into(), Some(2), T, T + 5 * S),
                ("codex".into(), Some(2), T + 5 * S, T + 5 * S + GAP),
            ]
        );
        let later = T + 3_600 * S;
        record(&db, &[obs("codex", "C:\\api", Some(2), T)], later).unwrap();
        assert_eq!(
            rows(&db)[2],
            ("codex".into(), Some(2), later - 15 * MIN, later)
        );
    }

    #[test]
    fn task_change_starts_new_period_and_missing_task_is_filled() {
        let db = open();
        record(&db, &[obs("claude", "C:\\repo", None, T)], T).unwrap();
        // Dopasowanie pojawiło się w trakcie: okres dostaje zadanie.
        record(&db, &[obs("claude", "C:\\repo", Some(1), T)], T + 5 * S).unwrap();
        // Chwilowy brak dopasowania nie zmienia zadania okresu.
        record(&db, &[obs("claude", "C:\\repo", None, T)], T + 10 * S).unwrap();
        // Nowa gałąź z innym zadaniem: nowy okres od końca poprzedniego.
        record(&db, &[obs("claude", "C:\\repo", Some(2), T)], T + 15 * S).unwrap();
        assert_eq!(
            rows(&db),
            vec![
                ("claude".into(), Some(1), T, T + 10 * S),
                ("claude".into(), Some(2), T + 10 * S, T + 15 * S),
            ]
        );
    }

    #[test]
    fn folder_binding_fills_task_of_recent_unmatched_periods() {
        let db = open();
        let now = db::now();
        let old = now - BACKFILL - 60 * MIN;
        db.execute_batch(&format!(
            "INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES
               ('claude','C:\\Projekty\\wms\\src',NULL,{a},{b}),
               ('claude','C:\\Projekty\\wms',NULL,{old},{old_end}),
               ('codex','C:\\Projekty\\wms',2,{a},{b}),
               ('codex','C:\\Projekty\\inne',NULL,{a},{b});",
            a = now - 3_600 * S,
            b = now - 1_800 * S,
            old_end = old + 10 * MIN,
        ))
        .unwrap();
        crate::agents::bind(&db, "C:/Projekty/WMS", 1).unwrap();
        let tasks: Vec<(String, Option<i64>)> = db
            .prepare("SELECT folder,task_id FROM agent_sessions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            tasks.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
            // Podfolder z ostatnich 7 dni uzupełniony; starszy okres, okres z innym
            // zadaniem i inny folder bez zmian.
            vec![Some(1), None, Some(2), None]
        );
        assert_eq!(backfill(&db, now).unwrap(), 0);
    }

    #[test]
    fn parallel_time_is_overlap_with_own_measurement_of_same_task() {
        let p = |id, task, start, end| Period {
            id,
            agent: "claude".into(),
            folder: "C:\\repo".into(),
            task_id: task,
            start,
            end,
        };
        let periods = [
            // Dwóch agentów nad zadaniem 1 nakłada się 10:30–11:00: liczone raz.
            p(1, Some(1), 10 * 60 * MIN, 11 * 60 * MIN),
            p(2, Some(1), 10 * 60 * MIN + 30 * MIN, 12 * 60 * MIN),
            p(3, Some(2), 9 * 60 * MIN, 10 * 60 * MIN),
            p(4, None, 8 * 60 * MIN, 8 * 60 * MIN + 20 * MIN),
        ];
        let sessions = [
            // Pomiar zadania 1 11:30–12:30: równolegle 30 min.
            (1, 11 * 60 * MIN + 30 * MIN, Some(12 * 60 * MIN + 30 * MIN)),
            // Pomiar innego zadania w czasie agenta zadania 1 nie jest równoległy.
            (2, 10 * 60 * MIN, Some(11 * 60 * MIN)),
            // Trwający pomiar zadania 2 od 9:45 do „teraz” 10:15.
            (2, 9 * 60 * MIN + 45 * MIN, None),
        ];
        let (totals, unmatched) = totals(
            &periods,
            &sessions,
            0,
            24 * 60 * MIN,
            10 * 60 * MIN + 15 * MIN,
        );
        assert_eq!(
            totals,
            vec![
                Total {
                    task_id: 1,
                    agent_ms: 120 * MIN,
                    parallel_ms: 30 * MIN
                },
                Total {
                    task_id: 2,
                    agent_ms: 60 * MIN,
                    parallel_ms: 15 * MIN
                },
            ]
        );
        assert_eq!(unmatched, 20 * MIN);
        // Zakres przycina okresy i pomiary.
        let (totals, _) = totals_in(&periods, &sessions, 11 * 60 * MIN + 45 * MIN);
        assert_eq!(
            totals[0],
            Total {
                task_id: 1,
                agent_ms: 15 * MIN,
                parallel_ms: 15 * MIN
            }
        );
    }

    fn totals_in(
        periods: &[Period],
        sessions: &[(i64, i64, Option<i64>)],
        from: i64,
    ) -> (Vec<Total>, i64) {
        totals(periods, sessions, from, 24 * 60 * MIN, 0)
    }

    #[test]
    fn query_reads_range_and_prune_drops_old_periods() {
        let db = open();
        let now = T;
        db.execute_batch(&format!(
            "INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES
               ('claude','C:\\a',1,{a},{b}),
               ('codex','C:\\b',NULL,{old},{old_end});
             INSERT INTO sessions(task_id,started_at,ended_at) VALUES(1,{a},NULL);",
            a = now - 60 * MIN,
            b = now - 30 * MIN,
            old = now - KEEP - 60 * MIN,
            old_end = now - KEEP - 1,
        ))
        .unwrap();
        let time = query(&db, now - 120 * MIN, now, now).unwrap();
        assert_eq!(time.periods.len(), 1);
        assert_eq!(
            time.totals,
            vec![Total {
                task_id: 1,
                agent_ms: 30 * MIN,
                parallel_ms: 30 * MIN
            }]
        );
        assert_eq!(query(&db, 0, now, now).unwrap().unmatched_ms, 60 * MIN - 1);
        assert_eq!(prune(&db, now).unwrap(), 1);
        assert_eq!(query(&db, 0, now, now).unwrap().periods.len(), 1);
        // Trwałe usunięcie zadania zostawia czas agenta bez zadania.
        db.execute("DELETE FROM sessions", []).unwrap();
        db.execute("DELETE FROM tasks WHERE id=1", []).unwrap();
        assert_eq!(query(&db, 0, now, now).unwrap().periods[0].task_id, None);
    }
}
