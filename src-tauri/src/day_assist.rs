//! Zamknięcie dnia z asystentem (PLAN_BACKLOG.md, A5).
//!
//! Sekcja „Asystent” w Zamknięciu dnia (przełącznik `ai_day_close`, wybrany silnik)
//! analizuje dzień **na kliknięcie** „Przeanalizuj dzień” (nigdy sama): zbiera dane
//! dnia z bazy i z folderów w `agent_bindings`, wysyła je do modelu przez
//! `ai::ask` (filtr osobowy `ai_privacy::redact` działa wewnątrz `ai::run`) i
//! mapuje odpowiedź na propozycje. Model tylko proponuje: zapisów (czas, notatka,
//! przypisanie folderu) dokonuje dopiero kliknięcie w interfejsie, istniejącymi
//! komendami.
//!
//! **Dane dnia** ([`DayInput`]):
//! - oś dnia: pomiary (bez zadań z kosza), luki między pomiarami od
//!   [`GAP_MIN_MS`], godziny „Rozpocznij / Zakończ dzień” (B13),
//! - czas agentów z przypisaniami (`agent_sessions`; okres bez zadania to kandydat
//!   do „Przypisz”),
//! - spotkania (zajmujące czas); tytuły tylko przy `ai_send_meeting_titles`,
//! - commity z dnia z folderów `agent_bindings`: `git log --since --until
//!   --format=%H%x09%ad%x09%s`, **bez autora w wyjściu** (do modelu idzie godzina,
//!   nazwa repozytorium i temat); do repozytorium z `user.email` wchodzą tylko
//!   commity tej osoby, więc cudze commity z wspólnego repozytorium nie trafiają
//!   do dnia,
//! - zmiany statusów zgłoszeń Jiry i Redmine z dnia (tabela `status_changes`,
//!   zapisywana przez wyzwalacze przy każdej obserwowanej zmianie statusu),
//! - notatki dnia.
//!
//! **Wynik** to `{ "proposals": [ { kind: gap|unassigned|note|ts_description, from,
//! to, taskId, text, reason } ] }`; [`parse`] przepuszcza tylko propozycje zgodne
//! z danymi wejściowymi (zadanie z listy, luka z osi dnia, okres agenta bez
//! zadania) i dokłada to, czego model nie zna: dokładne chwile, folder do
//! przypisania i możliwość `fill_gap` (B3), gdy luka leży między pomiarami tego
//! samego zadania.
use crate::agents;
use crate::ai::{self, Tier};
use crate::settings::AppSettings;
use crate::{agent_time, assign, calendar, days, db, stats, timesheet};
use chrono::{NaiveDate, NaiveTime, TimeZone};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use tauri::Manager;

/// Rodzaj wywołania w dzienniku „Ostatnie wywołania”.
pub const KIND: &str = "day_close";
/// Najkrótsza luka, o której pyta się model (krótsze to zwykłe przerwy).
pub const GAP_MIN_MS: i64 = 5 * 60_000;
/// Ile repozytoriów (folderów z przypisaniem) czyta `git log`.
pub const REPOS_MAX: usize = 20;
/// Ile commitów z jednego repozytorium i łącznie trafia do modelu.
pub const COMMITS_REPO_MAX: usize = 50;
pub const COMMITS_MAX: usize = 100;
/// Najdłuższy temat commitu, notatka dnia i tekst propozycji (znaki).
pub const SUBJECT_MAX: usize = 160;
pub const NOTE_MAX: usize = 1000;
pub const TEXT_MAX: usize = 1000;
pub const REASON_MAX: usize = 300;
/// Ile propozycji pokazuje interfejs.
pub const PROPOSALS_MAX: usize = 30;

pub const OFF_MESSAGE: &str =
    "Asystent w Zamknięciu dnia jest wyłączony w Ustawieniach → Asystent AI.";
pub const EMPTY_DAY: &str = "Brak danych z tego dnia do analizy.";
pub const NOTHING_FOUND: &str = "Asystent nie znalazł nic do zaproponowania.";

// ---------------------------------------------------------------------------
// Dane dnia

/// Zadanie na liście dla modelu (jak `assign::OpenTask`).
pub type TaskRef = assign::OpenTask;

/// Przedział pomiaru przycięty do dnia (trwający kończy się w `now`).
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub session_id: i64,
    pub task_id: i64,
    pub from: i64,
    pub to: i64,
}

/// Luka między pomiarami: pomiar kończący się najpóźniej przed nią i pomiar po niej.
#[derive(Clone, Debug, PartialEq)]
pub struct Gap {
    pub from: i64,
    pub to: i64,
    pub before: Span,
    pub after: Span,
}

/// Okres pracy agenta (`agent_sessions`) przycięty do dnia.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentSpan {
    pub agent: String,
    /// Pełna ścieżka folderu (do modelu idzie sama nazwa folderu).
    pub folder: String,
    pub task_id: Option<i64>,
    pub from: i64,
    pub to: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub from: i64,
    pub to: i64,
    /// Tytuł tylko przy `ai_send_meeting_titles`.
    pub title: Option<String>,
}

/// Commit z dnia (bez autora).
#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    /// Nazwa repozytorium (ostatni człon folderu).
    pub repo: String,
    pub at: i64,
    pub subject: String,
    /// Zadanie przypisane do folderu repozytorium.
    pub task_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusChange {
    /// `Jira` albo `Redmine`.
    pub system: String,
    /// Klucz Jiry (`WMS-12`) albo `#numer` Redmine.
    pub key: String,
    pub from: String,
    pub to: String,
    pub at: i64,
    pub task_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub task_id: i64,
    pub text: String,
}

/// Folder z przypisaniem do zadania, z którego czytamy commity.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    pub folder: String,
    pub task_id: i64,
}

/// Wszystko, co asystent wie o dniu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayInput {
    pub day: String,
    pub from: i64,
    pub to: i64,
    pub tasks: Vec<TaskRef>,
    pub measurements: Vec<Span>,
    pub gaps: Vec<Gap>,
    pub agents: Vec<AgentSpan>,
    pub meetings: Vec<Meeting>,
    pub repos: Vec<Repo>,
    pub commits: Vec<Commit>,
    pub status_changes: Vec<StatusChange>,
    pub notes: Vec<Note>,
    /// Godziny „Rozpocznij / Zakończ dzień” (B13).
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

impl DayInput {
    /// Czy dzień nie ma nic, o co warto pytać model.
    pub fn is_empty(&self) -> bool {
        self.measurements.is_empty()
            && self.agents.is_empty()
            && self.commits.is_empty()
            && self.status_changes.is_empty()
            && self.notes.is_empty()
    }
}

/// `git log` jednego repozytorium w zakresie dat (podstawiany w testach).
pub trait Git {
    /// Wyjście `git log --format=%H%x09%ad%x09%s --date=unix` bez autorów.
    fn log(&self, folder: &str, since: &str, until: &str) -> Result<String, String>;
}

/// Prawdziwy `git` z `PATH`.
pub struct SystemGit;

impl SystemGit {
    fn run(folder: &str, args: &[&str]) -> Result<String, String> {
        use std::process::{Command, Stdio};
        let mut command = Command::new("git");
        command
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(folder)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: bez migającego okna konsoli.
            command.creation_flags(0x0800_0000);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("git zakończył się błędem".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Git for SystemGit {
    fn log(&self, folder: &str, since: &str, until: &str) -> Result<String, String> {
        if !std::path::Path::new(folder).is_dir() {
            return Err("Brak folderu.".into());
        }
        let since = format!("--since={since}");
        let until = format!("--until={until}");
        let mut args = vec![
            "-c",
            "core.quotepath=off",
            "log",
            "--branches",
            "--no-merges",
            "--regexp-ignore-case",
            "--fixed-strings",
        ];
        // Tylko commity tej osoby (adres z konfiguracji repozytorium); sam adres
        // ani nazwa autora nie trafiają do wyniku.
        let email = Self::run(folder, &["config", "user.email"])
            .map(|e| e.trim().to_string())
            .unwrap_or_default();
        let author = format!("--author={email}");
        if !email.is_empty() {
            args.push(&author);
        }
        args.extend([
            since.as_str(),
            until.as_str(),
            "--date=unix",
            "--format=%H%x09%ad%x09%s",
        ]);
        Self::run(folder, &args)
    }
}

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Odczyt dnia z bazy: oś dnia, agenci, spotkania, statusy, notatki, lista zadań
/// i foldery z przypisaniem (commity dokłada [`add_commits`], poza blokadą bazy).
pub fn collect_db<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    settings: &AppSettings,
    date: NaiveDate,
    now: i64,
) -> Result<DayInput, String> {
    let day = date.format("%Y-%m-%d").to_string();
    let (from, to) = stats::day_bounds(tz, date);
    let mut input = DayInput {
        day: day.clone(),
        from,
        to,
        ..DayInput::default()
    };
    let snapshot = db::snapshot(db).map_err(|e| e.to_string())?;
    // Pomiary dnia (bez zadań z kosza), trwający liczony do `now`.
    let mut stmt = db
        .prepare(
            "SELECT s.id,s.task_id,s.started_at,s.ended_at FROM sessions s
             WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
               AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)
             ORDER BY s.started_at, s.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to, now], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (session_id, task_id, start, end) = row.map_err(|e| e.to_string())?;
        let span = Span {
            session_id,
            task_id,
            from: start.max(from),
            to: end.unwrap_or(now).min(to),
        };
        if span.to > span.from {
            input.measurements.push(span);
        }
    }
    input.gaps = gaps(&input.measurements);
    // Czas agentów z przypisaniami.
    let agents = agent_time::query(db, from, to, now).map_err(|e| e.to_string())?;
    for p in agents.periods {
        let span = AgentSpan {
            agent: p.agent,
            folder: p.folder,
            task_id: p.task_id,
            from: p.start.max(from),
            to: p.end.min(to),
        };
        if span.to > span.from {
            input.agents.push(span);
        }
    }
    // Spotkania, które zajmują czas; tytuł tylko przy przełączniku.
    for e in calendar::meetings_between(db, from, to)? {
        if e.all_day || !calendar::occupies(&e) {
            continue;
        }
        let (a, b) = (e.start.max(from), e.end.min(to));
        if b > a {
            input.meetings.push(Meeting {
                from: a,
                to: b,
                title: settings
                    .ai_send_meeting_titles
                    .then(|| clip(&e.subject, SUBJECT_MAX))
                    .filter(|t| !t.is_empty()),
            });
        }
    }
    // Notatki dnia.
    for t in &snapshot.tasks {
        if let Some(note) = t.details.notes.get(&day) {
            let text = clip(note, NOTE_MAX);
            if !text.is_empty() {
                input.notes.push(Note {
                    task_id: t.id,
                    text,
                });
            }
        }
    }
    // Zmiany statusów zgłoszeń z obserwacji.
    input.status_changes = status_changes(db, from, to, &snapshot.tasks)?;
    // Foldery z przypisaniem (commity czytane później).
    let mut seen = HashSet::new();
    for b in agents::bindings(db).map_err(|e| e.to_string())? {
        if input.repos.len() < REPOS_MAX && seen.insert(agents::norm_folder(&b.folder)) {
            input.repos.push(Repo {
                folder: b.folder,
                task_id: b.task_id,
            });
        }
    }
    // Godziny „Rozpocznij / Zakończ dzień” (B13).
    if let Some(mark) = days::get(db, &day).map_err(|e| e.to_string())? {
        input.started_at = mark.started_at;
        input.ended_at = mark.ended_at;
    }
    input.tasks = task_list(&input, &snapshot.tasks);
    Ok(input)
}

/// Luki między pomiarami (jak w Zamknięciu dnia): od pierwszego do ostatniego
/// pomiaru dnia, od [`GAP_MIN_MS`]. `before` to pomiar kończący się najpóźniej.
pub fn gaps(spans: &[Span]) -> Vec<Gap> {
    let mut sorted: Vec<&Span> = spans.iter().collect();
    sorted.sort_by_key(|s| (s.from, s.session_id));
    let mut out = Vec::new();
    let mut last: Option<&Span> = None;
    for span in sorted {
        if let Some(before) = last {
            if span.from - before.to >= GAP_MIN_MS {
                out.push(Gap {
                    from: before.to,
                    to: span.from,
                    before: before.clone(),
                    after: span.clone(),
                });
            }
        }
        if last.is_none_or(|l| span.to >= l.to) {
            last = Some(span);
        }
    }
    out
}

/// Lista zadań dla modelu: najpierw te z danych dnia (pomiary, agenci, notatki,
/// commity, statusy), potem najnowsze otwarte do [`assign::OPEN_MAX`].
fn task_list(input: &DayInput, tasks: &[db::Task]) -> Vec<TaskRef> {
    let mut wanted: Vec<i64> = Vec::new();
    let mut push = |id: i64| {
        if !wanted.contains(&id) {
            wanted.push(id);
        }
    };
    for s in &input.measurements {
        push(s.task_id);
    }
    for a in &input.agents {
        if let Some(id) = a.task_id {
            push(id);
        }
    }
    for n in &input.notes {
        push(n.task_id);
    }
    for c in &input.status_changes {
        if let Some(id) = c.task_id {
            push(id);
        }
    }
    for r in &input.repos {
        push(r.task_id);
    }
    let named = |t: &db::Task| TaskRef {
        id: t.id,
        title: t.title.clone(),
        project: t.details.project.clone(),
        key: t
            .jira_key
            .clone()
            .or_else(|| t.redmine_id.map(|id| format!("#{id}")))
            .unwrap_or_default(),
    };
    let mut list: Vec<TaskRef> = wanted
        .iter()
        .filter_map(|id| tasks.iter().find(|t| t.id == *id))
        .map(named)
        .collect();
    let limit = list.len() + assign::OPEN_MAX;
    for t in tasks.iter().filter(|t| !t.done && !t.archived) {
        if list.len() >= limit {
            break;
        }
        if !list.iter().any(|x| x.id == t.id) {
            list.push(named(t));
        }
    }
    list
}

/// Zmiany statusów z `status_changes` w `[from, to)`, z zadaniem powiązanym ze zgłoszeniem.
fn status_changes(
    db: &Connection,
    from: i64,
    to: i64,
    tasks: &[db::Task],
) -> Result<Vec<StatusChange>, String> {
    let mut stmt = db
        .prepare(
            "SELECT system,issue,from_status,to_status,at FROM status_changes
             WHERE at >= ?1 AND at < ?2 ORDER BY at, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (system, issue, from_status, to_status, at) = row.map_err(|e| e.to_string())?;
        let (label, key) = if system == "redmine" {
            ("Redmine", format!("#{issue}"))
        } else {
            ("Jira", issue)
        };
        let task_id = tasks
            .iter()
            .find(|t| {
                if system == "redmine" {
                    t.redmine_id.map(|id| format!("#{id}")).as_deref() == Some(key.as_str())
                } else {
                    t.jira_key.as_deref() == Some(key.as_str())
                }
            })
            .map(|t| t.id);
        if out.len() < 100 {
            out.push(StatusChange {
                system: label.into(),
                key,
                from: from_status,
                to: to_status,
                at,
                task_id,
            });
        }
    }
    Ok(out)
}

/// Dokłada commity dnia z folderów `agent_bindings` (`input.repos`). Błąd jednego
/// repozytorium (brak folderu, to nie repozytorium) pomija je, nie całą analizę.
pub fn add_commits<Tz: TimeZone>(input: &mut DayInput, tz: &Tz, git: &dyn Git)
where
    Tz::Offset: std::fmt::Display,
{
    let stamp = |ms: i64| {
        tz.timestamp_millis_opt(ms)
            .single()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S %z").to_string())
    };
    let (Some(since), Some(until)) = (stamp(input.from), stamp(input.to)) else {
        return;
    };
    for repo in input.repos.clone() {
        let Ok(text) = git.log(&repo.folder, &since, &until) else {
            continue;
        };
        let name = agents::folder(&repo.folder);
        for commit in parse_log(&text, &name, Some(repo.task_id), input.from, input.to)
            .into_iter()
            .take(COMMITS_REPO_MAX)
        {
            if input.commits.len() >= COMMITS_MAX {
                break;
            }
            input.commits.push(commit);
        }
    }
    input.commits.sort_by_key(|c| c.at);
}

/// Wyjście `git log --format=%H%x09%ad%x09%s --date=unix` jako commity z `[from, to)`;
/// powtórzone skróty (ten sam commit na kilku gałęziach) liczą się raz.
pub fn parse_log(text: &str, repo: &str, task_id: Option<i64>, from: i64, to: i64) -> Vec<Commit> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(hash), Some(date), Some(subject)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let hash = hash.trim();
        if hash.len() < 7 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(secs) = date.trim().parse::<i64>() else {
            continue;
        };
        let at = secs * 1000;
        let subject = clip(subject, SUBJECT_MAX);
        if at < from || at >= to || subject.is_empty() || !seen.insert(hash.to_string()) {
            continue;
        }
        out.push(Commit {
            repo: repo.to_string(),
            at,
            subject,
            task_id,
        });
    }
    out
}

/// Odczyt dnia z bazy i z `git` (aplikacja robi to w dwóch krokach, żeby nie
/// trzymać blokady bazy podczas `git log`; testy używają obu naraz).
#[cfg(test)]
pub fn collect<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    settings: &AppSettings,
    date: NaiveDate,
    now: i64,
    git: &dyn Git,
) -> Result<DayInput, String>
where
    Tz::Offset: std::fmt::Display,
{
    let mut input = collect_db(db, tz, settings, date, now)?;
    add_commits(&mut input, tz, git);
    Ok(input)
}

// ---------------------------------------------------------------------------
// Zapytanie i schemat

fn hm<Tz: TimeZone>(tz: &Tz, ms: i64) -> String {
    tz.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local().format("%H:%M").to_string())
        .unwrap_or_default()
}

fn put(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        map.insert(key.into(), json!(value.trim()));
    }
}

fn minutes(ms: i64) -> i64 {
    (ms.max(0) + 30_000) / 60_000
}

/// Schemat odpowiedzi: `{ "proposals": [ { kind, from, to, taskId, text, reason } ] }`.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "proposals": {
                "type": "array",
                "maxItems": PROPOSALS_MAX,
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["gap", "unassigned", "note", "ts_description"]
                        },
                        "from": { "type": "string" },
                        "to": { "type": "string" },
                        "taskId": { "type": ["integer", "null"] },
                        "text": { "type": "string" },
                        "reason": { "type": "string" }
                    },
                    "required": ["kind", "from", "to", "taskId", "text", "reason"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["proposals"],
        "additionalProperties": false
    })
}

/// Dane dnia jako JSON dla modelu: godziny „HH:MM” w strefie użytkownika, foldery
/// jako same nazwy, bez autorów i bez tytułów spotkań (gdy wyłączone). Filtr
/// osobowy dokłada `ai::run`.
pub fn input_json<Tz: TimeZone>(input: &DayInput, tz: &Tz) -> Value {
    let mut out = Map::new();
    out.insert("day".into(), json!(input.day));
    if let Some(at) = input.started_at {
        out.insert("dayStartedAt".into(), json!(hm(tz, at)));
    }
    if let Some(at) = input.ended_at {
        out.insert("dayEndedAt".into(), json!(hm(tz, at)));
    }
    out.insert(
        "tasks".into(),
        json!(input
            .tasks
            .iter()
            .map(|t| {
                let mut m = Map::new();
                m.insert("id".into(), json!(t.id));
                put(&mut m, "title", &t.title);
                put(&mut m, "project", &t.project);
                put(&mut m, "key", &t.key);
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "measurements".into(),
        json!(input
            .measurements
            .iter()
            .map(|s| json!({"taskId": s.task_id, "from": hm(tz, s.from), "to": hm(tz, s.to)}))
            .collect::<Vec<_>>()),
    );
    out.insert(
        "gaps".into(),
        json!(input
            .gaps
            .iter()
            .map(|g| json!({
                "from": hm(tz, g.from),
                "to": hm(tz, g.to),
                "minutes": minutes(g.to - g.from),
                "beforeTaskId": g.before.task_id,
                "afterTaskId": g.after.task_id,
            }))
            .collect::<Vec<_>>()),
    );
    out.insert(
        "agentWork".into(),
        json!(input
            .agents
            .iter()
            .map(|a| {
                let mut m = Map::new();
                m.insert("agent".into(), json!(a.agent));
                put(&mut m, "folder", &agents::folder(&a.folder));
                m.insert("from".into(), json!(hm(tz, a.from)));
                m.insert("to".into(), json!(hm(tz, a.to)));
                m.insert("taskId".into(), json!(a.task_id));
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "meetings".into(),
        json!(input
            .meetings
            .iter()
            .map(|e| {
                let mut m = Map::new();
                m.insert("from".into(), json!(hm(tz, e.from)));
                m.insert("to".into(), json!(hm(tz, e.to)));
                if let Some(title) = &e.title {
                    put(&mut m, "title", title);
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "commits".into(),
        json!(input
            .commits
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("time".into(), json!(hm(tz, c.at)));
                put(&mut m, "repo", &c.repo);
                put(&mut m, "subject", &c.subject);
                if let Some(id) = c.task_id {
                    m.insert("taskId".into(), json!(id));
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "statusChanges".into(),
        json!(input
            .status_changes
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("time".into(), json!(hm(tz, c.at)));
                put(&mut m, "system", &c.system);
                put(&mut m, "key", &c.key);
                put(&mut m, "from", &c.from);
                put(&mut m, "to", &c.to);
                if let Some(id) = c.task_id {
                    m.insert("taskId".into(), json!(id));
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "notes".into(),
        json!(input
            .notes
            .iter()
            .map(|n| json!({"taskId": n.task_id, "note": n.text}))
            .collect::<Vec<_>>()),
    );
    Value::Object(out)
}

/// Treść zapytania „Przeanalizuj dzień” przed filtrem osobowym; tę samą treść
/// pokazuje „Pokaż, co zostanie wysłane”.
pub fn prompt<Tz: TimeZone>(input: &DayInput, tz: &Tz) -> String {
    format!(
        "Aplikacja do śledzenia czasu pracy prosi o przegląd jednego dnia przed jego zamknięciem. \
         Na podstawie danych zaproponuj, co użytkownik może uzupełnić. Dozwolone rodzaje propozycji:\n\
         - \"gap\": luka z listy \"gaps\" (albo jej fragment) do zapisania jako czas zadania; \
         \"from\" i \"to\" to godziny HH:MM mieszczące się w luce, \"taskId\" zadanie z listy \"tasks\", \
         \"text\" pusty;\n\
         - \"unassigned\": praca agenta AI bez zadania (\"agentWork\" z taskId null); \"from\" i \"to\" \
         jak w danych, \"taskId\" zadanie z listy, \"text\" pusty;\n\
         - \"note\": jedno zdanie po polsku do notatki dnia zadania (\"taskId\"), oparte na commitach, \
         pracy agentów i zmianach statusów; \"from\" i \"to\" puste;\n\
         - \"ts_description\": krótki opis wpisu w Timesheet dla zadania (\"taskId\") po polsku, \
         zwłaszcza gdy zadanie nie ma notatki dnia; \"from\" i \"to\" puste.\n\
         Do każdej propozycji dodaj w \"reason\" jedno krótkie zdanie po polsku z konkretnym powodem \
         (np. godzina i temat commitu). Używaj wyłącznie id zadań z listy. Nie zgaduj: gdy dane nie \
         uzasadniają propozycji, jej nie dawaj. Odpowiedz wyłącznie obiektem JSON \
         {{\"proposals\": [{{\"kind\": …, \"from\": …, \"to\": …, \"taskId\": …, \"text\": …, \
         \"reason\": …}}]}}; pusta lista jest poprawną odpowiedzią.\n\nDane:\n{}",
        input_json(input, tz)
    )
}

// ---------------------------------------------------------------------------
// Propozycje

/// `fill_gap` (B3): luka leży między pomiarami i jest przypisywana całym przedziałem
/// pomiaru przed nią (`previous`) albo po niej (`next`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GapFill {
    pub before_id: i64,
    pub after_id: i64,
    pub mode: String,
}

/// Propozycja dla interfejsu.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayProposal {
    /// Numer w odpowiedzi (stabilny klucz karty).
    pub id: usize,
    /// `gap`, `unassigned`, `note`, `ts_description`.
    pub kind: String,
    /// Godziny HH:MM (puste przy `note` i `ts_description`).
    pub from: String,
    pub to: String,
    /// Chwile dla `gap` (nowy przedział) i `unassigned`.
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub task_id: i64,
    pub task_title: String,
    pub text: String,
    pub reason: String,
    /// `unassigned`: pełny folder do przypisania (`agent_bind`).
    pub folder: Option<String>,
    /// `gap` obejmująca całą lukę między pomiarami tego zadania.
    pub fill: Option<GapFill>,
}

/// Wynik „Przeanalizuj dzień”.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayAnalysis {
    pub day: String,
    pub proposals: Vec<DayProposal>,
    /// Powód braku propozycji (pusty, gdy są).
    pub message: String,
}

fn time_ms<Tz: TimeZone>(tz: &Tz, date: NaiveDate, text: &str) -> Option<i64> {
    let time = NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()?;
    tz.from_local_datetime(&date.and_time(time))
        .earliest()
        .map(|t| t.timestamp_millis())
}

/// Chwila z odpowiedzi modelu: godzina równa początkowi albo końcowi luki
/// (z dokładnością do minuty) przyjmuje jej dokładną chwilę.
fn snap<Tz: TimeZone>(tz: &Tz, date: NaiveDate, text: &str, exact: &[i64]) -> Option<i64> {
    let ms = time_ms(tz, date, text)?;
    Some(
        exact
            .iter()
            .copied()
            .find(|e| hm(tz, *e) == hm(tz, ms))
            .unwrap_or(ms),
    )
}

/// Odpowiedź modelu jako propozycje: tylko zgodne z danymi wejściowymi. Pozycje
/// niepoprawne (zadanie spoza listy, luka spoza osi dnia, pusty tekst notatki)
/// odpadają po cichu; powtórzenia (ta sama notatka, ten sam folder, ta sama luka)
/// liczą się raz.
pub fn parse<Tz: TimeZone>(value: &Value, input: &DayInput, tz: &Tz) -> Vec<DayProposal> {
    let Ok(date) = NaiveDate::parse_from_str(&input.day, "%Y-%m-%d") else {
        return Vec::new();
    };
    let Some(items) = value.get("proposals").and_then(Value::as_array) else {
        return Vec::new();
    };
    let title_of = |id: i64| {
        input
            .tasks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.title.clone())
    };
    let mut out: Vec<DayProposal> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (index, item) in items.iter().enumerate() {
        if out.len() >= PROPOSALS_MAX {
            break;
        }
        let text_of = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
        let kind = text_of("kind");
        let Some(task_id) = item.get("taskId").and_then(Value::as_i64) else {
            continue;
        };
        let Some(task_title) = title_of(task_id) else {
            continue;
        };
        let reason = clip(text_of("reason"), REASON_MAX);
        let mut proposal = DayProposal {
            id: index,
            kind: kind.to_string(),
            from: String::new(),
            to: String::new(),
            from_ms: None,
            to_ms: None,
            task_id,
            task_title,
            text: String::new(),
            reason,
            folder: None,
            fill: None,
        };
        match kind {
            "gap" => {
                let starts: Vec<i64> = input.gaps.iter().map(|g| g.from).collect();
                let ends: Vec<i64> = input.gaps.iter().map(|g| g.to).collect();
                let (Some(a), Some(b)) = (
                    snap(tz, date, text_of("from"), &starts),
                    snap(tz, date, text_of("to"), &ends),
                ) else {
                    continue;
                };
                let Some(gap) = input
                    .gaps
                    .iter()
                    .find(|g| a >= g.from && b <= g.to && a < b)
                else {
                    continue;
                };
                if !seen.insert(format!("gap:{a}:{b}")) {
                    continue;
                }
                proposal.from = hm(tz, a);
                proposal.to = hm(tz, b);
                proposal.from_ms = Some(a);
                proposal.to_ms = Some(b);
                if (a, b) == (gap.from, gap.to) {
                    let mode = if gap.before.task_id == task_id {
                        Some("previous")
                    } else if gap.after.task_id == task_id {
                        Some("next")
                    } else {
                        None
                    };
                    proposal.fill = mode.map(|mode| GapFill {
                        before_id: gap.before.session_id,
                        after_id: gap.after.session_id,
                        mode: mode.into(),
                    });
                }
            }
            "unassigned" => {
                let Some(from) = time_ms(tz, date, text_of("from")) else {
                    continue;
                };
                // Okres agenta bez zadania, który obejmuje podaną godzinę
                // (z dokładnością do minuty początku okresu).
                let Some(period) = input.agents.iter().find(|p| {
                    p.task_id.is_none()
                        && ((from >= p.from && from < p.to) || hm(tz, p.from) == hm(tz, from))
                }) else {
                    continue;
                };
                if !seen.insert(format!(
                    "unassigned:{}",
                    agents::norm_folder(&period.folder)
                )) {
                    continue;
                }
                proposal.from = hm(tz, period.from);
                proposal.to = hm(tz, period.to);
                proposal.from_ms = Some(period.from);
                proposal.to_ms = Some(period.to);
                proposal.folder = Some(period.folder.clone());
            }
            "note" | "ts_description" => {
                let text = clip(text_of("text"), TEXT_MAX);
                if text.is_empty() {
                    continue;
                }
                let key = if kind == "note" {
                    format!("note:{task_id}:{text}")
                } else {
                    format!("ts:{task_id}")
                };
                if !seen.insert(key) {
                    continue;
                }
                proposal.text = text;
            }
            _ => continue,
        }
        out.push(proposal);
    }
    out
}

/// Analiza na dowolnym `ask` (testowalny rdzeń komendy). Dzień bez danych nie
/// woła modelu; błąd wywołania wraca do interfejsu.
pub fn analyze<Tz: TimeZone>(
    ask: &dyn Fn(&str, &str, &Value) -> Result<Value, String>,
    input: &DayInput,
    tz: &Tz,
) -> Result<DayAnalysis, String> {
    if input.is_empty() {
        return Ok(DayAnalysis {
            day: input.day.clone(),
            proposals: Vec::new(),
            message: EMPTY_DAY.into(),
        });
    }
    let answer = ask(KIND, &prompt(input, tz), &schema())?;
    let proposals = parse(&answer, input, tz);
    let message = if proposals.is_empty() {
        NOTHING_FOUND.to_string()
    } else {
        String::new()
    };
    Ok(DayAnalysis {
        day: input.day.clone(),
        proposals,
        message,
    })
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

/// Sprawdza dzień, przełącznik i silnik, potem zbiera dane dnia (baza pod krótką
/// blokadą, `git` już bez niej).
fn request(app: &tauri::AppHandle, day: &str) -> Result<(DayInput, AppSettings), String> {
    timesheet::check_date(day)?;
    let settings = crate::settings::current(app);
    if !settings.ai_day_close {
        return Err(OFF_MESSAGE.to_string());
    }
    if ai::Engine::parse(&settings.ai_engine).is_none() {
        return Err(ai::OFF.to_string());
    }
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let mut input = {
        let database = app.state::<crate::Database>();
        let conn = database.0.lock().map_err(|e| e.to_string())?;
        collect_db(&conn, &chrono::Local, &settings, date, db::now())?
    };
    add_commits(&mut input, &chrono::Local, &SystemGit);
    Ok((input, settings))
}

/// „Przeanalizuj dzień”: jedno wywołanie modelu (poziom „główny”) na kliknięcie.
/// Zwraca propozycje; niczego nie zapisuje.
#[tauri::command]
pub async fn ai_day_analyze(app: tauri::AppHandle, day: String) -> Result<DayAnalysis, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, _) = request(&app, &day)?;
        analyze(
            &|kind, prompt, schema| ai::ask(&app, kind, Tier::Main, prompt, schema),
            &input,
            &chrono::Local,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// „Pokaż, co zostanie wysłane”: treść zapytania po filtrze danych osobowych, ta
/// sama co w [`ai_day_analyze`]; nic nie wywołuje i nic nie wysyła.
#[tauri::command]
pub async fn ai_day_preview(app: tauri::AppHandle, day: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, settings) = request(&app, &day)?;
        let dict = ai::dictionary(&app, &settings);
        Ok(crate::ai_privacy::redact(&prompt(&input, &chrono::Local), &dict).0)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
