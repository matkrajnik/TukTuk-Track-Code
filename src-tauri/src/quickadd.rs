//! Szybkie dodawanie ze składnią (`src/quickadd.ts`): zadanie z polami, opcjonalnym
//! powiązaniem z Jirą **albo** z Redmine (PLAN_1.9.md, R2) i startem pomiaru
//! w jednej transakcji.
use crate::db::{self, Snapshot};
use crate::planner::{self, Details};
use crate::templates;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

/// Wynik parsera z frontendu. Pusty tytuł jest dozwolony tylko z kluczem Jiry
/// albo numerem Redmine, wtedy tytuł pochodzi ze zgłoszenia.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NewTask {
    pub title: String,
    pub jira_key: Option<String>,
    /// Numer zgłoszenia Redmine (`#1234`); razem z `jira_key` to błąd.
    pub redmine_id: Option<u64>,
    pub details: Option<Details>,
    /// „Utwórz i rozpocznij” z szybkiego przełączania: poprzedni pomiar staje.
    pub start: bool,
    /// `/nazwa`: szablon z `task_templates`; jawne pola z linii wygrywają.
    pub template: Option<String>,
}

pub const ONE_ISSUE: &str = "Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.";

/// Tworzy zadanie bez źródła Redmine (numer Redmine w `task` jest wtedy
/// odrzucany jak klucz Jiry bez połączonego konta). `summary` podaje tytuł
/// zgłoszenia znanego z listy Jiry. `site` to adres połączonej Jiry; `None`,
/// gdy konto nie jest połączone (klucz Jiry jest wtedy odrzucany zamiast
/// wiązać zadanie z domyślnym adresem).
pub fn create(
    db: &mut Connection,
    site: Option<&str>,
    summary: impl FnOnce(&str) -> Option<String>,
    task: NewTask,
    now: i64,
) -> Result<Snapshot, String> {
    create_linked(db, site, summary, None, |_| None, task, now)
}

/// Tworzy zadanie z opcjonalnym powiązaniem z Jirą (`jira_site`, `summary`)
/// albo z Redmine (`redmine_site`, `subject` z `redmine::cached_subject`).
/// `None` jako adres oznacza niepołączone konto: zapis jest odrzucany zamiast
/// wiązać zadanie z nieznanym adresem. Zadanie ma najwyżej jedno zgłoszenie.
pub fn create_linked(
    db: &mut Connection,
    jira_site: Option<&str>,
    summary: impl FnOnce(&str) -> Option<String>,
    redmine_site: Option<&str>,
    subject: impl FnOnce(u64) -> Option<String>,
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
    let redmine_id = task.redmine_id;
    if let Some(id) = redmine_id {
        if !crate::redmine::valid_issue_id(id) {
            return Err("Niepoprawny numer zgłoszenia Redmine.".into());
        }
    }
    if key.is_some() && redmine_id.is_some() {
        return Err(ONE_ISSUE.into());
    }
    let site = match (&key, jira_site) {
        (Some(key), None) => {
            return Err(format!(
                "Jira nie jest połączona, więc {key} nie zostanie powiązane. Połącz konto w widoku Jira albo poprzedź klucz znakiem \\."
            ))
        }
        (_, site) => site.unwrap_or_default(),
    };
    let redmine_site = match (redmine_id, redmine_site) {
        (Some(id), None) => {
            return Err(format!(
                "Redmine nie jest połączone, więc #{id} nie zostanie powiązane. Połącz konto w Ustawieniach → Redmine albo poprzedź # znakiem \\."
            ))
        }
        (_, site) => site.unwrap_or_default(),
    };
    // Etykieta zgłoszenia do szablonu (`{klucz}`) i komunikatów: klucz Jiry
    // albo `#numer`; tytuł zgłoszenia z właściwego źródła.
    let label = key
        .clone()
        .or_else(|| redmine_id.map(|id| format!("#{id}")));
    let mut lookup = Some(move |label: &str| -> Option<String> {
        match label.strip_prefix('#').and_then(|n| n.parse::<u64>().ok()) {
            Some(id) => subject(id),
            None => summary(label),
        }
    });
    let template = match task.template.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => Some(templates::find(db, name)?.ok_or(format!(
            "Nie ma szablonu „{name}”. Dodaj go w Ustawieniach → Szablony."
        ))?),
        _ => None,
    };
    let mut title = match &template {
        Some(t) => templates::render_title(&t.body.title, label.as_deref(), &task.title, |k| {
            lookup.take().and_then(|f| f(k))
        })?
        // Długi tytuł zgłoszenia nie blokuje szablonu, jak przy samym kluczu.
        .chars()
        .take(240)
        .collect(),
        None => task.title.trim().to_owned(),
    };
    let lookup = lookup.take();
    if title.is_empty() {
        if let Some(label) = &label {
            title = lookup
                .and_then(|f| f(label))
                .map(|s| s.trim().chars().take(240).collect())
                .filter(|s: &String| !s.is_empty())
                .ok_or_else(|| templates::unknown_title(label))?;
        }
    }
    if title.is_empty() || title.chars().count() > 240 {
        return Err("Tytuł musi mieć od 1 do 240 znaków.".into());
    }
    let line_details = match &template {
        Some(t) => templates::merge_details(task.details, &t.body),
        None => task.details,
    };
    let details = match line_details {
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
    if let Some(id) = redmine_id {
        let existing: Option<(String, bool)> = tx
            .query_row(
                "SELECT t.title, EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=t.id AND l.deleted_at IS NOT NULL)
                 FROM redmine_tasks r JOIN tasks t ON t.id=r.task_id WHERE r.site=?1 AND r.issue_id=?2",
                params![redmine_site, id as i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((existing, deleted)) = existing {
            return Err(format!(
                "#{id} ma już zadanie „{existing}”{}.",
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
    if let Some(issue) = redmine_id {
        tx.execute(
            "INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(?1,?2,?3)",
            params![id, redmine_site, issue as i64],
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

    fn review(db: &mut Connection) {
        templates::save(
            db,
            None,
            "review",
            templates::Body {
                title: "Code review {klucz}: {tytuł}".into(),
                estimate_minutes: 45,
                steps: vec!["Przeczytaj opis".into(), "Sprawdź testy".into()],
                project: "WMS".into(),
                priority: 1,
            },
        )
        .unwrap();
    }

    #[test]
    fn template_with_jira_key_takes_issue_title_steps_and_estimate() {
        let mut db = open();
        review(&mut db);
        let snap = create(
            &mut db,
            Some(SITE),
            |key| (key == "WMS-123").then(|| "Eksport etykiet".to_owned()),
            NewTask {
                jira_key: Some("WMS-123".into()),
                template: Some("Review".into()),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let task = &snap.tasks[0];
        assert_eq!(task.title, "Code review WMS-123: Eksport etykiet");
        assert_eq!(task.jira_key.as_deref(), Some("WMS-123"));
        assert_eq!(task.details.estimate_minutes, 45);
        assert_eq!(task.details.project, "WMS");
        assert_eq!(task.details.priority, 1);
        let steps: Vec<_> = task
            .details
            .checklist
            .iter()
            .map(|c| (c.text.as_str(), c.done))
            .collect();
        assert_eq!(
            steps,
            [("Przeczytaj opis", false), ("Sprawdź testy", false)]
        );
        // Szablon nie uruchamia pomiaru.
        assert!(snap.sessions.is_empty());
    }

    #[test]
    fn explicit_line_fields_win_over_template() {
        let mut db = open();
        review(&mut db);
        let snap = create(
            &mut db,
            Some(SITE),
            |_| Some("Z Jiry".into()),
            NewTask {
                title: "Poprawki".into(),
                jira_key: Some("WMS-5".into()),
                template: Some("review".into()),
                details: Some(Details {
                    estimate_minutes: 90,
                    priority: -1,
                    project: "Foundation".into(),
                    planned_day: "2026-10-16".into(),
                    ..Details::default()
                }),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let task = &snap.tasks[0];
        assert_eq!(task.title, "Code review WMS-5: Poprawki");
        assert_eq!(task.details.estimate_minutes, 90);
        assert_eq!(task.details.priority, -1);
        assert_eq!(task.details.project, "Foundation");
        assert_eq!(task.details.planned_day, "2026-10-16");
        assert_eq!(task.details.checklist.len(), 2);
    }

    #[test]
    fn unknown_template_or_issue_creates_nothing() {
        let mut db = open();
        review(&mut db);
        for task in [
            NewTask {
                title: "Raport".into(),
                template: Some("brak".into()),
                ..NewTask::default()
            },
            NewTask {
                jira_key: Some("WMS-9".into()),
                template: Some("review".into()),
                ..NewTask::default()
            },
        ] {
            assert!(create(&mut db, Some(SITE), |_| None, task, 1).is_err());
        }
        assert!(db::snapshot(&db).unwrap().tasks.is_empty());
    }

    const REDMINE: &str = "https://redmine.example.pl";

    /// `create_linked` z połączoną Jirą i Redmine; temat #1234 znany z listy.
    fn linked(db: &mut Connection, task: NewTask, now: i64) -> Result<Snapshot, String> {
        create_linked(
            db,
            Some(SITE),
            |key| (key == "WMS-123").then(|| "Eksport etykiet".to_owned()),
            Some(REDMINE),
            |id| (id == 1234).then(|| "  Import CSV  ".to_owned()),
            task,
            now,
        )
    }

    #[test]
    fn redmine_number_links_task_and_takes_subject_from_cache() {
        let mut db = open();
        let snap = linked(
            &mut db,
            NewTask {
                redmine_id: Some(1234),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let task = &snap.tasks[0];
        assert_eq!(task.title, "Import CSV");
        assert_eq!(task.redmine_id, Some(1234));
        assert_eq!(
            task.redmine_url.as_deref(),
            Some("https://redmine.example.pl/issues/1234")
        );
        assert!(task.jira_key.is_none());
        // Ten sam numer drugi raz nie tworzy duplikatu (także z kosza).
        let err = linked(
            &mut db,
            NewTask {
                title: "Inny".into(),
                redmine_id: Some(1234),
                ..NewTask::default()
            },
            200,
        )
        .err()
        .unwrap();
        assert_eq!(err, "#1234 ma już zadanie „Import CSV”.");
        db::change(&mut db, "delete", Some(task.id), None, 250).unwrap();
        let err = linked(
            &mut db,
            NewTask {
                title: "Inny".into(),
                redmine_id: Some(1234),
                ..NewTask::default()
            },
            260,
        )
        .err()
        .unwrap();
        assert_eq!(err, "#1234 ma już zadanie „Import CSV” (w koszu).");
        // Nieznany temat bez tytułu: czytelny błąd, nic nie powstaje.
        let err = linked(
            &mut db,
            NewTask {
                redmine_id: Some(77),
                ..NewTask::default()
            },
            300,
        )
        .err()
        .unwrap();
        assert_eq!(
            err,
            "Nie znam tematu #77. Wpisz tytuł albo odśwież listę w widoku Redmine."
        );
        // Z tytułem powstaje bez tematu z listy; start zatrzymuje poprzedni pomiar.
        let snap = linked(
            &mut db,
            NewTask {
                title: "Nowe zgłoszenie".into(),
                redmine_id: Some(77),
                start: true,
                ..NewTask::default()
            },
            400,
        )
        .unwrap();
        let task = snap
            .tasks
            .iter()
            .find(|t| t.title == "Nowe zgłoszenie")
            .unwrap();
        assert_eq!(task.redmine_id, Some(77));
        assert_eq!(snap.sessions[0].task_id, task.id);
        assert_eq!(db::snapshot(&db).unwrap().tasks.len(), 1);
    }

    #[test]
    fn task_has_one_issue_jira_or_redmine() {
        let mut db = open();
        let err = linked(
            &mut db,
            NewTask {
                title: "Oba".into(),
                jira_key: Some("WMS-123".into()),
                redmine_id: Some(1234),
                ..NewTask::default()
            },
            1,
        )
        .err()
        .unwrap();
        assert_eq!(err, ONE_ISSUE);
        // Niepoprawny numer (0) też nic nie tworzy.
        assert!(linked(
            &mut db,
            NewTask {
                title: "Zero".into(),
                redmine_id: Some(0),
                ..NewTask::default()
            },
            1,
        )
        .is_err());
        assert!(db::snapshot(&db).unwrap().tasks.is_empty());
        // Klucz Jiry działa jak dotąd obok źródła Redmine.
        let snap = linked(
            &mut db,
            NewTask {
                jira_key: Some("WMS-123".into()),
                ..NewTask::default()
            },
            2,
        )
        .unwrap();
        assert_eq!(snap.tasks[0].title, "Eksport etykiet");
        assert!(snap.tasks[0].redmine_id.is_none());
    }

    #[test]
    fn redmine_number_without_connected_account_is_rejected() {
        let mut db = open();
        // `create` (bez źródła Redmine) i jawne `None` odrzucają numer.
        let err = create(
            &mut db,
            Some(SITE),
            |_| None,
            NewTask {
                title: "Import".into(),
                redmine_id: Some(1234),
                ..NewTask::default()
            },
            1,
        )
        .err()
        .unwrap();
        assert!(
            err.contains("#1234") && err.contains("Redmine nie jest połączone"),
            "{err}"
        );
        assert!(db::snapshot(&db).unwrap().tasks.is_empty());
    }

    #[test]
    fn template_with_redmine_number_fills_key_and_subject() {
        let mut db = open();
        review(&mut db);
        let snap = linked(
            &mut db,
            NewTask {
                redmine_id: Some(1234),
                template: Some("review".into()),
                ..NewTask::default()
            },
            100,
        )
        .unwrap();
        let task = &snap.tasks[0];
        assert_eq!(task.title, "Code review #1234: Import CSV");
        assert_eq!(task.redmine_id, Some(1234));
        assert_eq!(task.details.estimate_minutes, 45);
        // Nieznany temat w szablonie: komunikat o Redmine.
        let err = linked(
            &mut db,
            NewTask {
                redmine_id: Some(5),
                template: Some("review".into()),
                ..NewTask::default()
            },
            200,
        )
        .err()
        .unwrap();
        assert!(err.contains("#5") && err.contains("Redmine"), "{err}");
    }
}
