//! Szybkie dodawanie ze składnią (`src/quickadd.ts`): zadanie z polami, opcjonalnym
//! powiązaniem z Jirą i startem pomiaru w jednej transakcji.
use crate::db::{self, Snapshot};
use crate::planner::{self, Details};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

/// Wynik parsera z frontendu. Pusty tytuł jest dozwolony tylko z kluczem Jiry,
/// wtedy tytuł pochodzi ze zgłoszenia.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NewTask {
    pub title: String,
    pub jira_key: Option<String>,
    pub details: Option<Details>,
    /// „Utwórz i rozpocznij” z szybkiego przełączania: poprzedni pomiar staje.
    pub start: bool,
}

/// Tworzy zadanie. `summary` podaje tytuł zgłoszenia znanego z listy Jiry.
/// `site` to adres połączonej Jiry; `None`, gdy konto nie jest połączone
/// (klucz Jiry jest wtedy odrzucany zamiast wiązać zadanie z domyślnym adresem).
pub fn create(
    db: &mut Connection,
    site: Option<&str>,
    summary: impl FnOnce(&str) -> Option<String>,
    task: NewTask,
    now: i64,
) -> Result<Snapshot, String> {
    let key = task
        .jira_key
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty());
    if let Some(key) = &key {
        if !crate::jira::valid_key(key) {
            return Err("Niepoprawny klucz zgłoszenia Jiry.".into());
        }
    }
    let site = match (&key, site) {
        (Some(key), None) => {
            return Err(format!(
                "Jira nie jest połączona, więc {key} nie zostanie powiązane. Połącz konto w widoku Jira albo poprzedź klucz znakiem \\."
            ))
        }
        (_, site) => site.unwrap_or_default(),
    };
    let mut title = task.title.trim().to_owned();
    if title.is_empty() {
        if let Some(key) = &key {
            title = summary(key)
                .map(|s| s.trim().chars().take(240).collect())
                .filter(|s: &String| !s.is_empty())
                .ok_or(format!(
                    "Nie znam tytułu {key}. Wpisz tytuł albo odśwież listę zgłoszeń w widoku Jira."
                ))?;
        }
    }
    if title.is_empty() || title.chars().count() > 240 {
        return Err("Tytuł musi mieć od 1 do 240 znaków.".into());
    }
    let details = match task.details {
        Some(mut details) => {
            planner::validate(&mut details)?;
            Some(details)
        }
        None => None,
    };
    let tx = db.transaction().map_err(|e| e.to_string())?;
    if let Some(key) = &key {
        let existing: Option<(String, bool)> = tx
            .query_row(
                "SELECT t.title, EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=t.id AND l.deleted_at IS NOT NULL)
                 FROM jira_tasks j JOIN tasks t ON t.id=j.task_id WHERE j.site=?1 AND j.issue_key=?2",
                params![site, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((existing, deleted)) = existing {
            return Err(format!(
                "{key} ma już zadanie „{existing}”{}.",
                if deleted { " (w koszu)" } else { "" }
            ));
        }
    }
    tx.execute(
        "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
        params![title, now],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    if let Some(details) = &details {
        tx.execute(
            "INSERT INTO task_details(task_id,json) VALUES(?1,?2)",
            params![
                id,
                serde_json::to_string(details).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(key) = &key {
        tx.execute(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(?1,?2,?3)",
            params![id, site, key],
        )
        .map_err(|e| e.to_string())?;
    }
    if task.start {
        db::stop(&tx, now).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sessions(task_id,started_at) VALUES(?1,?2)",
            params![id, now],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SITE: &str = "https://example.atlassian.net";

    fn open() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db
    }

    #[test]
    fn creates_task_with_details_without_touching_timer() {
        let mut db = open();
        db::change(&mut db, "add", None, Some("Old".into()), 0).unwrap();
        db::change(&mut db, "start", Some(1), None, 10).unwrap();
        let snap = create(
            &mut db,
            Some(SITE),
            |_| None,
            NewTask {
                title: "  Raport  ".into(),
                details: Some(Details {
                    priority: 1,
                    planned_day: "2026-10-16".into(),
                    due_day: "2026-10-20".into(),
                    project: " WMS ".into(),
                    estimate_minutes: 45,
                    ..Details::default()
                }),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let task = snap.tasks.iter().find(|t| t.title == "Raport").unwrap();
        assert_eq!(task.details.project, "WMS");
        assert_eq!(task.details.estimate_minutes, 45);
        assert_eq!(task.details.due_day, "2026-10-20");
        assert!(task.jira_key.is_none());
        let open: Vec<_> = snap
            .sessions
            .iter()
            .filter(|s| s.ended_at.is_none())
            .collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].task_id, 1);
    }

    #[test]
    fn start_stops_previous_measurement() {
        let mut db = open();
        db::change(&mut db, "add", None, Some("Old".into()), 0).unwrap();
        db::change(&mut db, "start", Some(1), None, 10).unwrap();
        let snap = create(
            &mut db,
            Some(SITE),
            |_| None,
            NewTask {
                title: "Nowe".into(),
                start: true,
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let new_id = snap.tasks.iter().find(|t| t.title == "Nowe").unwrap().id;
        let old = snap.sessions.iter().find(|s| s.task_id == 1).unwrap();
        assert_eq!(old.ended_at, Some(100));
        let new = snap.sessions.iter().find(|s| s.task_id == new_id).unwrap();
        assert_eq!((new.started_at, new.ended_at), (100, None));
    }

    #[test]
    fn jira_key_links_task_and_takes_title_from_issue_list() {
        let mut db = open();
        let snap = create(
            &mut db,
            Some(SITE),
            |key| (key == "WMS-123").then(|| "Eksport etykiet".to_owned()),
            NewTask {
                jira_key: Some("WMS-123".into()),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        assert_eq!(snap.tasks[0].title, "Eksport etykiet");
        assert_eq!(snap.tasks[0].jira_key.as_deref(), Some("WMS-123"));
        assert_eq!(
            snap.tasks[0].jira_url.as_deref(),
            Some("https://example.atlassian.net/browse/WMS-123")
        );
        // Ten sam klucz drugi raz nie tworzy duplikatu.
        let err = create(
            &mut db,
            Some(SITE),
            |_| None,
            NewTask {
                title: "Inny".into(),
                jira_key: Some("WMS-123".into()),
                ..NewTask::default()
            },
            200,
        )
        .err()
        .unwrap();
        assert!(err.contains("Eksport etykiet"), "{err}");
        // Nieznane zgłoszenie bez tytułu: czytelny błąd, nic nie powstaje.
        let err = create(
            &mut db,
            Some(SITE),
            |_| None,
            NewTask {
                jira_key: Some("WMS-9".into()),
                ..NewTask::default()
            },
            300,
        )
        .err()
        .unwrap();
        assert!(err.contains("WMS-9"), "{err}");
        assert_eq!(db::snapshot(&db).unwrap().tasks.len(), 1);
    }

    #[test]
    fn rejects_invalid_input_atomically() {
        let mut db = open();
        for task in [
            NewTask::default(),
            NewTask {
                title: "x".into(),
                jira_key: Some("wms-1".into()),
                ..NewTask::default()
            },
            NewTask {
                title: "x".into(),
                details: Some(Details {
                    planned_day: "2026-02-31".into(),
                    ..Details::default()
                }),
                ..NewTask::default()
            },
        ] {
            assert!(create(&mut db, Some(SITE), |_| None, task, 1).is_err());
        }
        assert!(db::snapshot(&db).unwrap().tasks.is_empty());
    }

    #[test]
    fn jira_key_without_connected_account_is_rejected() {
        let mut db = open();
        let err = create(
            &mut db,
            None,
            |_| Some("Raport".into()),
            NewTask {
                title: "raport".into(),
                jira_key: Some("COVID-19".into()),
                ..NewTask::default()
            },
            1,
        )
        .err()
        .unwrap();
        assert!(err.contains("COVID-19"), "{err}");
        assert!(db::snapshot(&db).unwrap().tasks.is_empty());
        // Bez klucza zadanie powstaje zwyczajnie.
        let snap = create(
            &mut db,
            None,
            |_| None,
            NewTask {
                title: "COVID-19 raport".into(),
                ..NewTask::default()
            },
            2,
        )
        .unwrap();
        assert_eq!(snap.tasks[0].title, "COVID-19 raport");
        assert_eq!(snap.tasks[0].jira_key, None);
    }
}
