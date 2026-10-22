//! Testy A5: zebranie danych dnia (baza w pamięci i repozytorium git w katalogu
//! tymczasowym, bez autorów w wyjściu), schemat i mapowanie propozycji, brak
//! wywołania modelu dla pustego dnia i szczelność filtra osobowego. Żaden test
//! nie uruchamia claude ani codex: `ai::run` działa na podstawionym `Runner`.
use super::*;
use crate::ai::{Cli, Config, Detected, Output, RunError, Runner, Spec, State as AiState};
use crate::ai_privacy::Dictionary;
use chrono_tz::Europe::Warsaw;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const DAY: &str = "2026-10-15";

fn at(s: &str) -> i64 {
    Warsaw
        .from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis()
}

fn now() -> i64 {
    at("2026-10-15 17:00")
}

fn date() -> NaiveDate {
    NaiveDate::parse_from_str(DAY, "%Y-%m-%d").unwrap()
}

/// Dzień pełny danych: pomiary z lukami (też krótką), zadanie z kosza, czas agentów
/// z przypisaniem i bez, ręczne spotkanie z nazwiskiem, notatka, przypisanie
/// folderu i zmiany statusów (jedna z innego dnia).
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO tasks(id,title,done,created_at) VALUES
           (1,'Eksport CSV',0,1),(2,'Code review',0,2),(3,'Rozmowa z Janem Kowalskim',0,3),
           (4,'Usunięte',0,4),(5,'Stare',1,5);
         INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(4,10);
         INSERT INTO task_details(task_id,json) VALUES
           (1,'{{\"project\":\"WMS\"}}'),
           (2,'{{\"project\":\"API\",\"notes\":{{\"{DAY}\":\"Przegląd PR\",\"2026-10-14\":\"Wczoraj\"}}}}');
         INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-12');
         INSERT INTO jira_issues(site,issue_key,status) VALUES('https://x.atlassian.net','WMS-12','Do review');
         INSERT INTO agent_bindings(folder,task_id) VALUES('C:\\Projekty\\api',1);
         INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES
           ('claude','C:\\Projekty\\api',1,{a1},{a2}),('codex','C:\\Projekty\\narzedzia',NULL,{a3},{a4});
         INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,created_at,updated_at)
           VALUES('manual:1',{m1},{m2},'Spotkanie z Janem Kowalskim','busy',0,0,0);
         INSERT INTO status_changes(system,issue,from_status,to_status,at) VALUES
           ('jira','WMS-12','W toku','Do review',{s1}),('jira','WMS-12','Nowe','W toku',{s2});",
        a1 = at("2026-10-15 08:00"),
        a2 = at("2026-10-15 08:40"),
        a3 = at("2026-10-15 09:10"),
        a4 = at("2026-10-15 09:40"),
        m1 = at("2026-10-15 10:00"),
        m2 = at("2026-10-15 10:30"),
        s1 = at("2026-10-15 10:40"),
        s2 = at("2026-10-14 10:40"),
    ))
    .unwrap();
    for (task, from, to) in [
        (1, "2026-10-15 08:00", "2026-10-15 09:00"),
        (2, "2026-10-15 09:30", "2026-10-15 10:00"),
        (1, "2026-10-15 10:30", "2026-10-15 11:00"),
        // Przerwa 2 minuty: krótsza niż GAP_MIN_MS, więc nie jest luką dla modelu.
        (2, "2026-10-15 11:02", "2026-10-15 11:30"),
        // Zadanie z kosza i pomiar z innego dnia: poza dniem.
        (4, "2026-10-15 12:00", "2026-10-15 12:30"),
        (1, "2026-10-14 08:00", "2026-10-14 09:00"),
    ] {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            params![task, at(from), at(to)],
        )
        .unwrap();
    }
    db
}

fn settings() -> AppSettings {
    AppSettings::default()
}

/// Podstawione `git log`: zapisane wyjście dla folderu; zapamiętuje zakres dat.
struct FakeGit {
    logs: HashMap<String, String>,
    calls: Mutex<Vec<(String, String, String)>>,
}
impl FakeGit {
    fn new(logs: &[(&str, String)]) -> Self {
        Self {
            logs: logs
                .iter()
                .map(|(f, l)| (f.to_string(), l.clone()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }
}
impl Git for FakeGit {
    fn log(&self, folder: &str, since: &str, until: &str) -> Result<String, String> {
        self.calls
            .lock()
            .unwrap()
            .push((folder.into(), since.into(), until.into()));
        self.logs
            .get(folder)
            .cloned()
            .ok_or_else(|| "brak".to_string())
    }
}

fn unix(s: &str) -> i64 {
    at(s) / 1000
}

/// Wyjście `git log --format=%H%x09%ad%x09%s --date=unix`: dwa commity z dnia,
/// powtórzony skrót, commit z wczoraj (poza zakresem) i śmieci.
fn api_log() -> String {
    format!(
        "{h1}\t{t1}\tfix eksportu CSV\n{h2}\t{t2}\tdodaj test eksportu\n{h1}\t{t1}\tfix eksportu CSV\n{h3}\t{t3}\tstary commit\nnie-commit\n\n{h4}\tnie-liczba\ttemat\n",
        h1 = "a".repeat(40),
        h2 = "b".repeat(40),
        h3 = "c".repeat(40),
        h4 = "d".repeat(40),
        t1 = unix("2026-10-15 08:20"),
        t2 = unix("2026-10-15 08:35"),
        t3 = unix("2026-10-14 16:00"),
    )
}

fn collected(settings: &AppSettings) -> DayInput {
    let git = FakeGit::new(&[(r"C:\Projekty\api", api_log())]);
    collect(&database(), &Warsaw, settings, date(), now(), &git).unwrap()
}

// ---------------------------------------------------------------------------
// Zebranie danych dnia

#[test]
fn collects_measurements_and_gaps_without_trash_other_days_and_short_breaks() {
    let input = collected(&settings());
    let spans: Vec<(i64, i64, i64)> = input
        .measurements
        .iter()
        .map(|s| (s.task_id, s.from, s.to))
        .collect();
    assert_eq!(
        spans,
        vec![
            (1, at("2026-10-15 08:00"), at("2026-10-15 09:00")),
            (2, at("2026-10-15 09:30"), at("2026-10-15 10:00")),
            (1, at("2026-10-15 10:30"), at("2026-10-15 11:00")),
            (2, at("2026-10-15 11:02"), at("2026-10-15 11:30")),
        ]
    );
    // Dwie luki od 5 minut; przerwa 11:00–11:02 i zadanie z kosza nie liczą się.
    let gaps: Vec<(i64, i64, i64, i64)> = input
        .gaps
        .iter()
        .map(|g| (g.from, g.to, g.before.task_id, g.after.task_id))
        .collect();
    assert_eq!(
        gaps,
        vec![
            (at("2026-10-15 09:00"), at("2026-10-15 09:30"), 1, 2),
            (at("2026-10-15 10:00"), at("2026-10-15 10:30"), 2, 1),
        ]
    );
    assert_eq!(input.gaps[0].before.session_id, 1);
    assert_eq!(input.gaps[0].after.session_id, 2);
    assert!(!input.is_empty());
}

#[test]
fn running_measurement_counts_up_to_now_and_no_gap_after_the_last_one() {
    let db = database();
    db.execute(
        "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(2,?1,NULL)",
        [at("2026-10-15 16:00")],
    )
    .unwrap();
    let git = FakeGit::new(&[]);
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &git).unwrap();
    let last = input.measurements.last().unwrap();
    assert_eq!((last.from, last.to), (at("2026-10-15 16:00"), now()));
    // Luka między 11:30 a 16:00 jest, po ostatnim pomiarze nic nie ma.
    let last_gap = input.gaps.last().unwrap();
    assert_eq!(
        (last_gap.from, last_gap.to),
        (at("2026-10-15 11:30"), at("2026-10-15 16:00"))
    );
}

#[test]
fn gaps_keep_the_measurement_that_ends_latest_as_the_one_before() {
    let span = |id, task, from: &str, to: &str| Span {
        session_id: id,
        task_id: task,
        from: at(from),
        to: at(to),
    };
    // Pomiar 2 leży w pomiarze 1: luka po obu zaczyna się na końcu pomiaru 1.
    let list = vec![
        span(1, 1, "2026-10-15 08:00", "2026-10-15 10:00"),
        span(2, 2, "2026-10-15 08:30", "2026-10-15 09:00"),
        span(3, 3, "2026-10-15 10:30", "2026-10-15 11:00"),
    ];
    let found = gaps(&list);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].before.session_id, 1);
    assert_eq!(found[0].after.session_id, 3);
    assert!(gaps(&[]).is_empty());
}

#[test]
fn collects_agent_time_with_assignments_meetings_notes_status_changes_and_marks() {
    let input = collected(&settings());
    assert_eq!(
        input
            .agents
            .iter()
            .map(|a| (a.agent.as_str(), a.task_id, a.from, a.to))
            .collect::<Vec<_>>(),
        vec![
            (
                "claude",
                Some(1),
                at("2026-10-15 08:00"),
                at("2026-10-15 08:40")
            ),
            (
                "codex",
                None,
                at("2026-10-15 09:10"),
                at("2026-10-15 09:40")
            ),
        ]
    );
    assert_eq!(
        input.notes,
        vec![Note {
            task_id: 2,
            text: "Przegląd PR".into()
        }]
    );
    // Tylko zmiana z dnia, ze zgłoszeniem zadania.
    assert_eq!(
        input.status_changes,
        vec![StatusChange {
            system: "Jira".into(),
            key: "WMS-12".into(),
            from: "W toku".into(),
            to: "Do review".into(),
            at: at("2026-10-15 10:40"),
            task_id: Some(1),
        }]
    );
    assert_eq!(input.meetings.len(), 1);
    assert_eq!(
        input.repos,
        vec![Repo {
            folder: r"C:\Projekty\api".into(),
            task_id: 1
        }]
    );
    // Lista zadań dla modelu: bez zadania z kosza, z kluczem Jiry.
    assert!(input.tasks.iter().all(|t| t.id != 4));
    assert_eq!(
        input.tasks.iter().find(|t| t.id == 1).unwrap(),
        &TaskRef {
            id: 1,
            title: "Eksport CSV".into(),
            project: "WMS".into(),
            key: "WMS-12".into()
        }
    );
}

#[test]
fn day_marks_are_part_of_the_timeline() {
    let db = database();
    db.execute(
        "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES(?1,?2,?3,0)",
        params![DAY, at("2026-10-15 07:55"), at("2026-10-15 16:30")],
    )
    .unwrap();
    let git = FakeGit::new(&[]);
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &git).unwrap();
    assert_eq!(input.started_at, Some(at("2026-10-15 07:55")));
    let value = input_json(&input, &Warsaw);
    assert_eq!(value["dayStartedAt"], "07:55");
    assert_eq!(value["dayEndedAt"], "16:30");
}

#[test]
fn meeting_titles_leave_only_when_the_switch_is_on() {
    let off = collected(&settings());
    assert_eq!(off.meetings[0].title, None);
    let json_off = input_json(&off, &Warsaw).to_string();
    assert!(!json_off.contains("Spotkanie"));
    assert_eq!(
        input_json(&off, &Warsaw)["meetings"],
        json!([{"from": "10:00", "to": "10:30"}])
    );
    let mut on = settings();
    on.ai_send_meeting_titles = true;
    let with = collected(&on);
    assert_eq!(
        with.meetings[0].title.as_deref(),
        Some("Spotkanie z Janem Kowalskim")
    );
    assert_eq!(
        input_json(&with, &Warsaw)["meetings"][0]["title"],
        "Spotkanie z Janem Kowalskim"
    );
}

#[test]
fn declined_cancelled_and_all_day_meetings_are_not_sent() {
    let db = database();
    db.execute_batch(&format!(
        "INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,created_at,updated_at) VALUES
           ('manual:2',{f},{t},'Wolne','free',0,0,0),
           ('manual:3',{day},{next},'Cały dzień','busy',1,0,0);",
        f = at("2026-10-15 13:00"),
        t = at("2026-10-15 14:00"),
        day = at("2026-10-15 00:00"),
        next = at("2026-10-16 00:00"),
    ))
    .unwrap();
    let git = FakeGit::new(&[]);
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &git).unwrap();
    assert_eq!(input.meetings.len(), 1);
}

#[test]
fn status_change_triggers_record_only_real_changes() {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    db.execute_batch(
        "INSERT INTO jira_issues(site,issue_key,status) VALUES('s','WMS-1','');
         INSERT INTO redmine_issues(site,issue_id,status) VALUES('r',1234,'Nowe');",
    )
    .unwrap();
    let count = |db: &Connection| -> i64 {
        db.query_row("SELECT COUNT(*) FROM status_changes", [], |r| r.get(0))
            .unwrap()
    };
    // Pierwszy status zgłoszenia i ten sam status to nie zmiana.
    db.execute(
        "UPDATE jira_issues SET status='W toku' WHERE issue_key='WMS-1'",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE jira_issues SET status='W toku' WHERE issue_key='WMS-1'",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE redmine_issues SET status='Nowe' WHERE issue_id=1234",
        [],
    )
    .unwrap();
    assert_eq!(count(&db), 0);
    // Zapis jak z obserwacji Jiry (UPSERT) i Redmine.
    let tx = db.unchecked_transaction().unwrap();
    crate::jira::save_statuses(&tx, "s", [("WMS-1", "Gotowe")].into_iter()).unwrap();
    tx.commit().unwrap();
    db.execute(
        "UPDATE redmine_issues SET status='W toku' WHERE issue_id=1234",
        [],
    )
    .unwrap();
    let rows: Vec<(String, String, String, String)> = db
        .prepare("SELECT system,issue,from_status,to_status FROM status_changes ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "jira".into(),
                "WMS-1".into(),
                "W toku".into(),
                "Gotowe".into()
            ),
            (
                "redmine".into(),
                "1234".into(),
                "Nowe".into(),
                "W toku".into()
            )
        ]
    );
}

// ---------------------------------------------------------------------------
// Commity

#[test]
fn parse_log_reads_hash_time_subject_dedupes_and_clips_to_the_day() {
    let (from, to) = (at("2026-10-15 00:00"), at("2026-10-16 00:00"));
    let commits = parse_log(&api_log(), "api", Some(1), from, to);
    assert_eq!(
        commits,
        vec![
            Commit {
                repo: "api".into(),
                at: unix("2026-10-15 08:20") * 1000,
                subject: "fix eksportu CSV".into(),
                task_id: Some(1)
            },
            Commit {
                repo: "api".into(),
                at: unix("2026-10-15 08:35") * 1000,
                subject: "dodaj test eksportu".into(),
                task_id: Some(1)
            },
        ]
    );
    let long = format!(
        "{}\t{}\t{}",
        "e".repeat(40),
        unix("2026-10-15 09:00"),
        "ż".repeat(400)
    );
    assert_eq!(
        parse_log(&long, "api", None, from, to)[0]
            .subject
            .chars()
            .count(),
        SUBJECT_MAX
    );
}

#[test]
fn commits_come_from_bound_folders_with_the_day_range_in_the_users_time_zone() {
    let git = FakeGit::new(&[(r"C:\Projekty\api", api_log())]);
    let input = collect(&database(), &Warsaw, &settings(), date(), now(), &git).unwrap();
    assert_eq!(input.commits.len(), 2);
    assert_eq!(input.commits[0].repo, "api");
    assert_eq!(input.commits[0].task_id, Some(1));
    let calls = git.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(
            r"C:\Projekty\api".to_string(),
            "2026-10-15 00:00:00 +0200".to_string(),
            "2026-10-16 00:00:00 +0200".to_string()
        )]
    );
}

#[test]
fn a_folder_without_a_repository_is_skipped_not_fatal() {
    let git = FakeGit::new(&[]);
    let input = collect(&database(), &Warsaw, &settings(), date(), now(), &git).unwrap();
    assert!(input.commits.is_empty());
    assert!(!input.measurements.is_empty());
}

#[test]
fn commit_count_is_capped_per_repository_and_in_total() {
    let db = database();
    db.execute_batch(
        "INSERT INTO tasks(id,title,done,created_at) VALUES(10,'A',0,1),(11,'B',0,1),(12,'C',0,1);
         INSERT INTO agent_bindings(folder,task_id) VALUES('C:\\r\\a',10),('C:\\r\\b',11),('C:\\r\\c',12);",
    )
    .unwrap();
    let log: String = (0..80)
        .map(|i| {
            format!(
                "{:040x}\t{}\ttemat {i}\n",
                i + 1,
                unix("2026-10-15 09:00") + i
            )
        })
        .collect();
    let git = FakeGit::new(&[
        (r"C:\r\a", log.clone()),
        (r"C:\r\b", log.replace("temat", "inny")),
        (r"C:\r\c", log.replace("temat", "trzeci")),
    ]);
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &git).unwrap();
    assert_eq!(input.commits.len(), COMMITS_MAX);
    assert_eq!(
        input.commits.iter().filter(|c| c.repo == "a").count(),
        COMMITS_REPO_MAX
    );
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git(dir: &std::path::Path, args: &[&str], date: Option<&str>) {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(dir).args(args);
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Repozytorium w katalogu tymczasowym: dwa commity użytkownika z dnia, commit
/// innej osoby z tego samego dnia i commit z poprzedniego dnia. Autorzy mają
/// nazwiska i adresy, których nie wolno znaleźć w wyniku.
fn make_repo() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tt-day-assist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"], None);
    git(&dir, &["config", "user.email", "ja@firma.example"], None);
    git(&dir, &["config", "user.name", "Mateusz Krajnik"], None);
    git(&dir, &["config", "commit.gpgsign", "false"], None);
    let commit = |name: &str, message: &str, date: &str, author: Option<&str>| {
        std::fs::write(dir.join(name), message).unwrap();
        git(&dir, &["add", name], None);
        let mut args = vec!["commit", "-q", "--no-verify", "-m", message];
        if let Some(author) = author {
            args.push("--author");
            args.push(author);
        }
        git(&dir, &args, Some(date));
    };
    commit("a", "wczoraj", "2026-10-14T16:00:00+0200", None);
    commit("b", "fix eksportu CSV", "2026-10-15T08:20:00+0200", None);
    commit(
        "c",
        "cudzy commit",
        "2026-10-15T08:30:00+0200",
        Some("Anna Nowak <anna.nowak@firma.example>"),
    );
    commit("d", "dodaj test eksportu", "2026-10-15T08:35:00+0200", None);
    dir
}

#[test]
fn real_git_log_has_no_authors_and_only_the_users_commits_of_the_day() {
    if !git_available() {
        eprintln!("git niedostępny: test pominięty");
        return;
    }
    let dir = make_repo();
    let folder = dir.to_string_lossy().to_string();
    let output = SystemGit
        .log(
            &folder,
            "2026-10-15 00:00:00 +0200",
            "2026-10-16 00:00:00 +0200",
        )
        .unwrap();
    for forbidden in ["Krajnik", "Nowak", "firma.example", "ja@"] {
        assert!(!output.contains(forbidden), "autor w wyniku: {forbidden}");
    }
    let commits = parse_log(
        &output,
        "repo",
        None,
        at("2026-10-15 00:00"),
        at("2026-10-16 00:00"),
    );
    let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
    // Od najnowszego, bez cudzego commitu i bez commitu z wczoraj.
    assert_eq!(subjects, vec!["dodaj test eksportu", "fix eksportu CSV"]);
    // Całość: z bazą z przypisaniem folderu, dane dla modelu bez autorów i ścieżki.
    let db = database();
    db.execute(
        "INSERT INTO agent_bindings(folder,task_id) VALUES(?1,2)",
        [&folder],
    )
    .unwrap();
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &SystemGit).unwrap();
    let from_repo: Vec<&str> = input
        .commits
        .iter()
        .filter(|c| c.task_id == Some(2))
        .map(|c| c.subject.as_str())
        .collect();
    assert_eq!(from_repo, vec!["fix eksportu CSV", "dodaj test eksportu"]);
    let text = prompt(&input, &Warsaw);
    for forbidden in ["Krajnik", "Nowak", "firma.example"] {
        assert!(!text.contains(forbidden), "autor w zapytaniu: {forbidden}");
    }
    assert!(
        !text.contains(&folder)
            && !text.contains(&dir.parent().unwrap().to_string_lossy().to_string()),
        "pełna ścieżka repozytorium w zapytaniu"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_folder_is_an_error_for_system_git() {
    assert!(SystemGit
        .log(
            r"C:\na\pewno\nie\istnieje-tt",
            "2026-10-15 00:00:00 +0200",
            "2026-10-16 00:00:00 +0200"
        )
        .is_err());
}

// ---------------------------------------------------------------------------
// Zapytanie

fn data_of(text: &str) -> Value {
    serde_json::from_str(text.split("Dane:\n").nth(1).expect("dane")).unwrap()
}

#[test]
fn model_input_has_hours_folder_names_commits_status_changes_and_notes_without_paths() {
    let input = collected(&settings());
    let value = data_of(&prompt(&input, &Warsaw));
    assert_eq!(value["day"], DAY);
    assert_eq!(
        value["measurements"][0],
        json!({"taskId": 1, "from": "08:00", "to": "09:00"})
    );
    assert_eq!(
        value["gaps"][0],
        json!({"from": "09:00", "to": "09:30", "minutes": 30, "beforeTaskId": 1, "afterTaskId": 2})
    );
    assert_eq!(
        value["agentWork"][1],
        json!({"agent": "codex", "folder": "narzedzia", "from": "09:10", "to": "09:40", "taskId": null})
    );
    assert_eq!(
        value["commits"][0],
        json!({"time": "08:20", "repo": "api", "subject": "fix eksportu CSV", "taskId": 1})
    );
    assert_eq!(
        value["statusChanges"][0],
        json!({"time": "10:40", "system": "Jira", "key": "WMS-12", "from": "W toku", "to": "Do review", "taskId": 1})
    );
    assert_eq!(
        value["notes"][0],
        json!({"taskId": 2, "note": "Przegląd PR"})
    );
    assert!(!value.to_string().contains("C:\\\\"));
}

#[test]
fn schema_accepts_the_four_kinds_and_rejects_others() {
    let ok = json!({"proposals": [
        {"kind": "gap", "from": "09:00", "to": "09:30", "taskId": 1, "text": "", "reason": "r"},
        {"kind": "note", "from": "", "to": "", "taskId": null, "text": "x", "reason": "r"}
    ]});
    assert!(ai::validate(&schema(), &ok).is_ok());
    assert!(ai::validate(&schema(), &json!({"proposals": []})).is_ok());
    let bad = json!({"proposals": [
        {"kind": "delete", "from": "", "to": "", "taskId": 1, "text": "", "reason": ""}
    ]});
    assert!(ai::validate(&schema(), &bad).is_err());
    let missing = json!({"proposals": [{"kind": "note", "taskId": 1}]});
    assert!(ai::validate(&schema(), &missing).is_err());
    assert!(ai::validate(&schema(), &json!({})).is_err());
}

// ---------------------------------------------------------------------------
// Mapowanie propozycji

fn proposal(kind: &str, from: &str, to: &str, task: Option<i64>, text: &str) -> Value {
    json!({"kind": kind, "from": from, "to": to, "taskId": task, "text": text, "reason": "powód"})
}

fn parsed(items: Vec<Value>) -> Vec<DayProposal> {
    parse(
        &json!({"proposals": items}),
        &collected(&settings()),
        &Warsaw,
    )
}

#[test]
fn a_gap_proposal_gets_exact_times_and_fill_when_it_joins_a_neighbour() {
    let found = parsed(vec![
        // Cała luka 09:00–09:30 między zadaniem 1 (przed) a 2 (po).
        proposal("gap", "09:00", "09:30", Some(2), ""),
        // Fragment luki: tylko nowy przedział, bez fill_gap.
        proposal("gap", "10:10", "10:20", Some(1), ""),
        // Cała luka z zadaniem spoza sąsiadów: tylko ręczny czas.
        proposal("gap", "10:00", "10:30", Some(3), ""),
    ]);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].kind, "gap");
    assert_eq!(found[0].from, "09:00");
    assert_eq!(found[0].from_ms, Some(at("2026-10-15 09:00")));
    assert_eq!(found[0].to_ms, Some(at("2026-10-15 09:30")));
    assert_eq!(found[0].task_title, "Code review");
    assert_eq!(found[0].reason, "powód");
    assert_eq!(
        found[0].fill,
        Some(GapFill {
            before_id: 1,
            after_id: 2,
            mode: "next".into()
        })
    );
    assert_eq!(found[1].fill, None);
    assert_eq!(found[1].from_ms, Some(at("2026-10-15 10:10")));
    assert_eq!(found[2].fill, None);
}

#[test]
fn a_gap_proposal_snaps_to_the_exact_gap_edges_with_seconds() {
    let mut input = collected(&settings());
    input.gaps[0].from += 20_000;
    input.gaps[0].to -= 15_000;
    let value = json!({"proposals": [proposal("gap", "09:00", "09:29", Some(1), "")]});
    let found = parse(&value, &input, &Warsaw);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].from_ms, Some(input.gaps[0].from));
    assert_eq!(found[0].to_ms, Some(input.gaps[0].to));
    // Poprzedni pomiar tego samego zadania: „dołącz do poprzedniego”.
    assert_eq!(found[0].fill.as_ref().unwrap().mode, "previous");
}

#[test]
fn proposals_outside_the_data_are_dropped() {
    let found = parsed(vec![
        // Luka, której nie ma na osi dnia, odwrócone godziny, śmieci w godzinie.
        proposal("gap", "12:00", "12:30", Some(1), ""),
        proposal("gap", "09:20", "09:10", Some(1), ""),
        proposal("gap", "dziewiąta", "09:10", Some(1), ""),
        // Zadanie spoza listy (z kosza) i brak zadania.
        proposal("gap", "09:00", "09:30", Some(4), ""),
        proposal("gap", "09:00", "09:30", Some(999), ""),
        proposal("gap", "09:00", "09:30", None, ""),
        proposal("note", "", "", Some(999), "tekst"),
        // Pusta notatka i pusty opis TS.
        proposal("note", "", "", Some(1), "   "),
        proposal("ts_description", "", "", Some(1), ""),
        // Okres agenta, który ma już zadanie, i godzina poza okresami.
        proposal("unassigned", "08:00", "08:40", Some(2), ""),
        proposal("unassigned", "14:00", "15:00", Some(2), ""),
        // Nieznany rodzaj.
        proposal("start_timer", "", "", Some(1), "x"),
    ]);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn unassigned_agent_time_maps_to_its_period_and_full_folder_once_per_folder() {
    let found = parsed(vec![
        proposal("unassigned", "09:15", "09:30", Some(3), ""),
        proposal("unassigned", "09:10", "09:40", Some(2), ""),
    ]);
    assert_eq!(found.len(), 1, "jedna propozycja na folder");
    let p = &found[0];
    assert_eq!(p.kind, "unassigned");
    assert_eq!((p.from.as_str(), p.to.as_str()), ("09:10", "09:40"));
    assert_eq!(p.from_ms, Some(at("2026-10-15 09:10")));
    assert_eq!(p.folder.as_deref(), Some(r"C:\Projekty\narzedzia"));
    assert_eq!(p.task_id, 3);
    assert_eq!(p.task_title, "Rozmowa z Janem Kowalskim");
}

#[test]
fn notes_and_ts_descriptions_map_with_dedupe() {
    let found = parsed(vec![
        proposal("note", "", "", Some(1), "  Naprawiono eksport CSV.  "),
        proposal("note", "", "", Some(1), "Naprawiono eksport CSV."),
        proposal("note", "", "", Some(1), "Dodano test."),
        proposal(
            "ts_description",
            "",
            "",
            Some(1),
            "Eksport CSV: poprawka i test",
        ),
        proposal(
            "ts_description",
            "",
            "",
            Some(1),
            "Inny opis tego samego zadania",
        ),
        proposal("ts_description", "", "", Some(3), "Rozmowa o eksporcie"),
    ]);
    let kinds: Vec<(&str, i64, &str)> = found
        .iter()
        .map(|p| (p.kind.as_str(), p.task_id, p.text.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("note", 1, "Naprawiono eksport CSV."),
            ("note", 1, "Dodano test."),
            ("ts_description", 1, "Eksport CSV: poprawka i test"),
            ("ts_description", 3, "Rozmowa o eksporcie"),
        ]
    );
    // Numer w odpowiedzi to stabilny klucz karty.
    assert_eq!(
        found.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![0, 2, 3, 5]
    );
    assert_eq!(found[0].from, "");
    assert_eq!(found[0].from_ms, None);
    assert_eq!(found[0].folder, None);
}

#[test]
fn a_ts_description_is_only_for_tasks_without_a_day_note() {
    // Zadanie 2 ma notatkę dnia („Przegląd PR”): wiersz TS ma już z niej opis.
    let found = parsed(vec![
        proposal("ts_description", "", "", Some(2), "Przegląd PR i poprawki"),
        proposal("note", "", "", Some(2), "Dodano komentarze."),
        proposal("ts_description", "", "", Some(1), "Eksport CSV"),
    ]);
    let kinds: Vec<(&str, i64)> = found.iter().map(|p| (p.kind.as_str(), p.task_id)).collect();
    assert_eq!(kinds, vec![("note", 2), ("ts_description", 1)]);
    // Zapytanie mówi modelowi o tej zasadzie.
    assert!(
        prompt(&collected(&settings()), &Warsaw).contains("tylko gdy zadanie nie ma notatki dnia")
    );
}

#[test]
fn gap_edges_with_milliseconds_are_rounded_to_whole_seconds_inside_the_gap() {
    // Pomiar zatrzymany o 09:00:30.456, następny od 09:29:45.789: ręczny czas ma
    // dokładność sekundy, więc luka zaczyna się o 09:00:31 i kończy o 09:29:45.
    let mut input = collected(&settings());
    input.gaps[0].from += 30_456;
    input.gaps[0].to -= 14_211;
    let start = input.gaps[0].from;
    let end = input.gaps[0].to;
    let value = json!({"proposals": [
        proposal("gap", "09:00", "09:29", Some(1), ""),
        proposal("gap", "09:00", "09:10", Some(3), ""),
    ]});
    let found = parse(&value, &input, &Warsaw);
    assert_eq!(found.len(), 2);
    let a = found[0].from_ms.unwrap();
    let b = found[0].to_ms.unwrap();
    assert_eq!(a, at("2026-10-15 09:00") + 31_000);
    assert_eq!(b, at("2026-10-15 09:29") + 45_000);
    assert!(a >= start && b <= end && a % 1000 == 0 && b % 1000 == 0);
    // Cała luka (po zaokrągleniu) nadal łączy się z poprzednim pomiarem.
    assert_eq!(found[0].fill.as_ref().unwrap().mode, "previous");
    // Fragment od początku luki też zaczyna się po końcu poprzedniego pomiaru.
    assert_eq!(found[1].from_ms, Some(at("2026-10-15 09:00") + 31_000));
    assert_eq!(found[1].to_ms, Some(at("2026-10-15 09:10")));
    assert_eq!(found[1].fill, None);
}

#[test]
fn text_and_reason_are_clipped_and_the_list_is_capped() {
    let long = "ż".repeat(3000);
    let mut item = proposal("note", "", "", Some(1), &long);
    item["reason"] = json!(long);
    let found = parsed(vec![item]);
    assert_eq!(found[0].text.chars().count(), TEXT_MAX);
    assert_eq!(found[0].reason.chars().count(), REASON_MAX);
    let many: Vec<Value> = (0..PROPOSALS_MAX + 10)
        .map(|i| proposal("note", "", "", Some(1), &format!("notatka {i}")))
        .collect();
    assert_eq!(parsed(many).len(), PROPOSALS_MAX);
    assert!(parse(&json!({}), &collected(&settings()), &Warsaw).is_empty());
    assert!(parse(&json!({"proposals": "x"}), &collected(&settings()), &Warsaw).is_empty());
}

// ---------------------------------------------------------------------------
// Wywołanie: `ai::run` na podstawionym `Runner`

fn claude_reply(structured: Value) -> String {
    json!({
        "type": "result", "subtype": "success", "is_error": false, "result": "",
        "structured_output": structured
    })
    .to_string()
}

struct Fake {
    text: String,
    specs: Mutex<Vec<Spec>>,
}
impl Fake {
    fn answering(structured: Value) -> Self {
        Self {
            text: claude_reply(structured),
            specs: Mutex::new(Vec::new()),
        }
    }
    fn count(&self) -> usize {
        self.specs.lock().unwrap().len()
    }
    fn stdin(&self) -> String {
        self.specs.lock().unwrap().last().unwrap().stdin.clone()
    }
}
impl Runner for Fake {
    fn run(&self, spec: &Spec, _timeout: Duration) -> Result<Output, RunError> {
        self.specs.lock().unwrap().push(spec.clone());
        Ok(Output {
            success: true,
            stdout: self.text.clone(),
            stderr: String::new(),
        })
    }
}

fn cfg() -> Config {
    Config {
        engine: "claude".into(),
        detected: Detected {
            claude: Some(Cli {
                path: r"C:\bin\claude.exe".into(),
                version: "2.1.304".into(),
            }),
            codex: None,
        },
        model: String::new(),
        dir: std::env::temp_dir().join(format!("tt-day-assist-ai-{}", std::process::id())),
    }
}

#[test]
fn analyze_calls_the_model_once_and_maps_the_answer_without_names_in_the_input() {
    let mut on = settings();
    on.ai_send_meeting_titles = true;
    let input = collected(&on);
    let runner = Fake::answering(json!({"proposals": [
        proposal("gap", "09:00", "09:30", Some(2), ""),
        proposal("note", "", "", Some(3), "Rozmowa z Osoba 1 o eksporcie."),
        proposal("unassigned", "09:10", "09:40", Some(1), ""),
    ]}));
    let mut dict = Dictionary::new();
    dict.add_name("Jan Kowalski");
    let state = AiState::default();
    let config = cfg();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    let analysis = analyze(&ask, &input, &Warsaw).unwrap();
    assert_eq!(runner.count(), 1);
    // Szczelność (A0): ani tytuł spotkania, ani tytuł zadania z nazwiskiem w odmianie.
    let sent = runner.stdin();
    assert!(
        !sent.contains("Kowalski") && !sent.contains("Kowalskim"),
        "{sent}"
    );
    assert!(sent.contains("Osoba 1"));
    assert_eq!(analysis.day, DAY);
    assert_eq!(analysis.message, "");
    assert_eq!(analysis.proposals.len(), 3);
    // Odpowiedź wraca z prawdziwym nazwiskiem (karta pokazuje je w notatce).
    assert_eq!(
        analysis.proposals[1].text,
        "Rozmowa z Jan Kowalski o eksporcie."
    );
    assert_eq!(analysis.proposals[0].kind, "gap");
    let calls = state.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].kind, KIND);
}

#[test]
fn an_empty_day_does_not_call_the_model() {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    let input = collect(&db, &Warsaw, &settings(), date(), now(), &FakeGit::new(&[])).unwrap();
    assert!(input.is_empty());
    let runner = Fake::answering(json!({"proposals": []}));
    let state = AiState::default();
    let config = cfg();
    let dict = Dictionary::new();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    let analysis = analyze(&ask, &input, &Warsaw).unwrap();
    assert_eq!(runner.count(), 0);
    assert!(analysis.proposals.is_empty());
    assert_eq!(analysis.message, EMPTY_DAY);
}

#[test]
fn no_proposals_and_errors_are_reported_to_the_interface() {
    let input = collected(&settings());
    let ask_empty = |_: &str, _: &str, _: &Value| Ok(json!({"proposals": []}));
    let analysis = analyze(&ask_empty, &input, &Warsaw).unwrap();
    assert!(analysis.proposals.is_empty());
    assert_eq!(analysis.message, NOTHING_FOUND);
    let ask_error =
        |_: &str, _: &str, _: &Value| -> Result<Value, String> { Err("Limit subskrypcji".into()) };
    assert_eq!(
        analyze(&ask_error, &input, &Warsaw).unwrap_err(),
        "Limit subskrypcji"
    );
}

#[test]
fn analysis_serializes_for_the_interface_in_camel_case() {
    let found = parsed(vec![proposal("gap", "09:00", "09:30", Some(2), "")]);
    let value = serde_json::to_value(&DayAnalysis {
        day: DAY.into(),
        proposals: found,
        message: String::new(),
    })
    .unwrap();
    let p = &value["proposals"][0];
    assert_eq!(p["taskId"], 2);
    assert_eq!(p["taskTitle"], "Code review");
    assert_eq!(p["fromMs"], at("2026-10-15 09:00"));
    assert_eq!(
        p["fill"],
        json!({"beforeId": 1, "afterId": 2, "mode": "next"})
    );
    assert!(p["folder"].is_null());
}
