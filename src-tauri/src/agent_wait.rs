//! Czas, w którym sesje agentów czekały na użytkownika (tablica sesji, etap 3).
//!
//! Pętla agentów (`agents::start`) przekazuje czekające sesje z tablicy
//! (`agent_board`) do [`record`]: jedno wejście w czekanie to jeden wiersz
//! `agent_waits` (klucz: agent, sesja, chwila wejścia), którego koniec rośnie z
//! każdą obserwacją. Gdy sesja przestaje czekać, nic go już nie przedłuża, więc
//! koniec zostaje na ostatniej obserwacji (dokładność około 5 s).
//!
//! [`query`] podaje dla zakresu: czas, w którym czekała co najmniej jedna sesja
//! (bez liczenia dwa razy), liczbę czekań i najdłuższe czekanie. Czekanie nie
//! wchodzi do celu dnia, raportów ani TS.
use rusqlite::{params, Connection};
use serde::Serialize;

/// Wiersze starsze niż tyle są usuwane przy starcie (jak `agent_time::KEEP`).
pub const KEEP: i64 = crate::agent_time::KEEP;

/// Jedno czekanie sesji widziane w tej chwili.
#[derive(Clone, Debug, PartialEq)]
pub struct Wait {
    pub agent: String,
    pub session: String,
    pub folder: String,
    pub task_id: Option<i64>,
    /// Wejście w czekanie (`hooks::Session::phase_at`).
    pub start: i64,
}

impl Wait {
    /// Czekające wpisy tablicy sesji.
    pub fn from_board(list: &[crate::agent_board::Entry]) -> Vec<Wait> {
        list.iter()
            .filter(|e| e.phase == crate::hooks::Phase::Waiting.as_str())
            .map(|e| Wait {
                agent: e.agent.kind.clone(),
                session: e.id.clone(),
                folder: e.agent.cwd.clone(),
                task_id: e.agent.task.as_ref().and_then(|t| t.task_id),
                start: e.phase_at,
            })
            .collect()
    }
}

/// Zapisuje obserwację z chwili `now`: nowe czekanie albo dłuższy koniec.
pub fn record(db: &Connection, waits: &[Wait], now: i64) -> rusqlite::Result<()> {
    if waits.is_empty() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            // Zadanie usunięte między dopasowaniem a zapisem: NULL zamiast błędu
            // klucza obcego, który wycofałby zapis wszystkich czekań.
            "INSERT INTO agent_waits(agent,session,folder,task_id,start,\"end\")
             VALUES(?1,?2,?3,(SELECT id FROM tasks WHERE id=?4),?5,?6)
             ON CONFLICT(agent,session,start) DO UPDATE SET
               \"end\"=MAX(\"end\",excluded.\"end\"),
               task_id=COALESCE(task_id,excluded.task_id)",
        )?;
        for w in waits {
            stmt.execute(params![
                w.agent,
                w.session,
                w.folder,
                w.task_id,
                w.start,
                now.max(w.start)
            ])?;
        }
    }
    tx.commit()
}

/// Usuwa czekania starsze niż [`KEEP`] (przy starcie aplikacji).
pub fn prune(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    db.execute("DELETE FROM agent_waits WHERE \"end\" < ?1", [now - KEEP])
}

/// Czekanie na użytkownika w zakresie `[from, to)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitTime {
    /// Czas, w którym czekała co najmniej jedna sesja.
    pub total_ms: i64,
    /// Liczba czekań, które nachodzą na zakres.
    pub count: usize,
    /// Najdłuższe czekanie (przycięte do zakresu).
    pub longest_ms: i64,
}

/// Sumy dla przedziałów czekania przyciętych do `[from, to)`.
pub fn summarize(spans: &[(i64, i64)], from: i64, to: i64) -> WaitTime {
    let clipped: Vec<(i64, i64)> = spans
        .iter()
        .map(|(a, b)| ((*a).max(from), (*b).min(to)))
        .filter(|(a, b)| b > a)
        .collect();
    let total_ms = crate::agent_time::union(clipped.clone())
        .iter()
        .map(|(a, b)| b - a)
        .sum();
    WaitTime {
        total_ms,
        count: clipped.len(),
        longest_ms: clipped.iter().map(|(a, b)| b - a).max().unwrap_or(0),
    }
}

pub fn query(db: &Connection, from: i64, to: i64) -> rusqlite::Result<WaitTime> {
    let spans: Vec<(i64, i64)> = db
        .prepare("SELECT start,\"end\" FROM agent_waits WHERE start < ?2 AND \"end\" > ?1")?
        .query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(summarize(&spans, from, to))
}

/// Zapis z pętli agentów; błąd bazy nie zatrzymuje pętli.
pub fn observe(app: &tauri::AppHandle, board: &[crate::agent_board::Entry], now: i64) {
    use tauri::Manager;
    let waits = Wait::from_board(board);
    if waits.is_empty() {
        return;
    }
    let state = app.state::<crate::Database>();
    if let Ok(db) = state.0.lock() {
        if let Err(e) = record(&db, &waits, now) {
            eprintln!("Czas czekania agentów: zapis nieudany ({e}).");
        }
    };
}

#[tauri::command]
pub fn agent_wait_time(
    state: tauri::State<crate::Database>,
    from: i64,
    to: i64,
) -> Result<WaitTime, String> {
    if to < from {
        return Err("Niepoprawny zakres dat.".into());
    }
    let db = state.0.lock().map_err(|e| e.to_string())?;
    query(&db, from, to).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const T: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init(&c).unwrap();
        c
    }

    fn wait(session: &str, start: i64) -> Wait {
        Wait {
            agent: "claude".into(),
            session: session.into(),
            folder: r"C:\Projekty\app".into(),
            task_id: None,
            start,
        }
    }

    #[test]
    fn observations_extend_one_row_per_entry_into_waiting() {
        let db = mem();
        record(&db, &[wait("a", T)], T + MIN).unwrap();
        record(&db, &[wait("a", T)], T + 3 * MIN).unwrap();
        // Spóźniona obserwacja nie skraca końca.
        record(&db, &[wait("a", T)], T + 2 * MIN).unwrap();
        // Kolejne czekanie tej samej sesji to nowy wiersz.
        record(&db, &[wait("a", T + 10 * MIN)], T + 11 * MIN).unwrap();
        let rows: Vec<(i64, i64)> = db
            .prepare("SELECT start,\"end\" FROM agent_waits ORDER BY start")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, [(T, T + 3 * MIN), (T + 10 * MIN, T + 11 * MIN)]);
    }

    #[test]
    fn query_counts_overlap_once_and_clips_to_range() {
        let db = mem();
        record(&db, &[wait("a", T), wait("b", T + MIN)], T + 4 * MIN).unwrap();
        record(&db, &[wait("c", T + 20 * MIN)], T + 22 * MIN).unwrap();
        let all = query(&db, T, T + 60 * MIN).unwrap();
        assert_eq!(
            all,
            WaitTime {
                total_ms: 6 * MIN,
                count: 3,
                longest_ms: 4 * MIN,
            }
        );
        // Zakres ucina czekanie „a” po 2 minutach i pomija „c”.
        let part = query(&db, T + 2 * MIN, T + 10 * MIN).unwrap();
        assert_eq!(
            part,
            WaitTime {
                total_ms: 2 * MIN,
                count: 2,
                longest_ms: 2 * MIN,
            }
        );
        assert_eq!(query(&db, 0, T).unwrap(), WaitTime::default());
    }

    #[test]
    fn missing_task_does_not_drop_other_waits() {
        let db = mem();
        let gone = Wait {
            task_id: Some(999),
            ..wait("a", T)
        };
        record(&db, &[gone, wait("b", T)], T + MIN).unwrap();
        let tasks: Vec<Option<i64>> = db
            .prepare("SELECT task_id FROM agent_waits ORDER BY session")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(tasks, [None, None]);
    }

    #[test]
    fn prune_drops_waits_older_than_keep() {
        let db = mem();
        record(&db, &[wait("old", T)], T + MIN).unwrap();
        record(&db, &[wait("new", T + KEEP)], T + KEEP + MIN).unwrap();
        assert_eq!(prune(&db, T + KEEP + 2 * MIN).unwrap(), 1);
        assert_eq!(query(&db, 0, i64::MAX).unwrap().count, 1);
    }

    #[test]
    fn board_waiting_entries_become_waits() {
        use crate::hooks::{HookEvent, Sessions};
        let mut s = Sessions::default();
        let ev = |id: &str, event: &str| HookEvent {
            agent: "codex".into(),
            session_id: id.into(),
            cwd: r"C:\Projekty\api".into(),
            event: event.into(),
            ..HookEvent::default()
        };
        s.apply(&ev("w", "UserPromptSubmit"), T);
        s.apply(&ev("k", "UserPromptSubmit"), T);
        s.apply(&ev("w", "PermissionRequest"), T + MIN);
        let waits = Wait::from_board(&crate::agent_board::entries(s.board()));
        assert_eq!(
            waits,
            [Wait {
                agent: "codex".into(),
                session: "w".into(),
                folder: r"C:\Projekty\api".into(),
                task_id: None,
                start: T + MIN,
            }]
        );
    }
}
