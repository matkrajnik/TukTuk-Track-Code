//! Pytanie o przesunięcie zgłoszenia w Jirze przy starcie pracy (PLAN_1.7.md, J1).
//!
//! Wątek co 2 s patrzy na aktywny pomiar. Nowy pomiar innego zadania niż
//! poprzedni (każda droga startu: przycisk, szybkie przełączanie, skrót,
//! powiadomienie, „Przełącz” z propozycji) dla zadania powiązanego ze zgłoszeniem
//! sprawdza w Jirze status. Gdy zgłoszenie jest w kategorii „Do zrobienia”
//! (`statusCategory.key = new`), powstaje pytanie „Przesunąć KEY do …?”: baner
//! w głównym oknie i linia w widgecie (zdarzenie `jira-move-prompts`), a przy
//! schowanym oknie powiadomienie z przyciskiem „Przesuń” (w trybie skupienia
//! czeka w kolejce bloku, baner wraca po bloku).
//!
//! Pomiar startuje od razu, a zgłoszenie nie przesuwa się samo: tylko
//! `jira_transition` (kliknięcie) wykonuje `POST …/transitions` przez wspólną
//! ścieżkę zapisu `jira::write` z głównym przełącznikiem `jira_write`.
use crate::jira::{self, Transition};
use crate::notify::{self, Action, Message, View};
use crate::settings::AppSettings;
use crate::{db, Database};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Co ile wątek sprawdza aktywny pomiar.
const TICK: Duration = Duration::from_secs(2);
/// Zdarzenie z listą pytań dla banera i widgetu.
pub const EVENT: &str = "jira-move-prompts";

/// Pytanie o przesunięcie zgłoszenia zadania.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub task_id: i64,
    pub key: String,
    pub title: String,
    /// Bieżący status w Jirze.
    pub status: String,
    pub url: String,
    /// Status z ustawień (tytuł pytania, gdy przejście nie jest wybrane).
    pub target: String,
    /// Przejścia do statusu „w toku” (kategoria `indeterminate`).
    pub transitions: Vec<Transition>,
    /// Wybrane przejście; `None` = baner pokazuje listę do wyboru.
    pub transition_id: Option<String>,
}

impl Prompt {
    /// Nazwa statusu docelowego w pytaniu.
    pub fn target_name(&self) -> String {
        self.transition_id
            .as_ref()
            .and_then(|id| self.transitions.iter().find(|t| &t.id == id))
            .map(|t| t.to_status.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.target.clone())
    }
}

/// Czy pytać: zapisy włączone, pytanie włączone, zadanie bez „Nie pytaj”
/// i zgłoszenie w kategorii „Do zrobienia”.
pub fn should_ask(s: &AppSettings, no_ask: bool, category: &str) -> bool {
    s.jira_write && s.jira_ask_move && !no_ask && category == "new"
}

/// Przejścia do kategorii `indeterminate` i wybór: jedno wygrywa samo, przy kilku
/// to z nazwą z ustawień (status docelowy, a potem nazwa przejścia, bez względu na
/// wielkość liter), bez dopasowania `None` (lista do wyboru).
pub fn choose(all: &[Transition], preferred: &str) -> (Vec<Transition>, Option<String>) {
    let candidates: Vec<Transition> = all
        .iter()
        .filter(|t| t.to_category == "indeterminate")
        .cloned()
        .collect();
    let wanted = preferred.trim().to_lowercase();
    let by = |pick: fn(&Transition) -> &str| {
        candidates
            .iter()
            .find(|t| !wanted.is_empty() && pick(t).trim().to_lowercase() == wanted)
            .map(|t| t.id.clone())
    };
    let chosen = match candidates.as_slice() {
        [] => None,
        [one] => Some(one.id.clone()),
        _ => by(|t| &t.to_status).or_else(|| by(|t| &t.name)),
    };
    (candidates, chosen)
}

/// Aktywny pomiar: (przedział, zadanie).
type Active = Option<(i64, i64)>;

/// Zadanie nowo rozpoczętego pomiaru: inny przedział i inne zadanie niż poprzednio
/// widziane. Podział przedziału (ten sam task) i brak pomiaru nie są startem.
pub fn started(previous: Active, current: Active) -> Option<i64> {
    match (previous, current) {
        (_, None) => None,
        (Some((ps, pt)), Some((s, t))) if ps == s || pt == t => None,
        (_, Some((_, task))) => Some(task),
    }
}

/// Zadanie powiązane ze zgłoszeniem: adres Jiry, klucz, tytuł i „Nie pytaj”.
#[derive(Clone, Debug, PartialEq)]
pub struct Linked {
    pub site: String,
    pub key: String,
    pub title: String,
    pub no_ask: bool,
}

pub fn linked(db: &Connection, task_id: i64) -> rusqlite::Result<Option<Linked>> {
    db.query_row(
        "SELECT j.site, j.issue_key, t.title,
                COALESCE(json_extract(d.json,'$.jiraNoAsk'),0)
         FROM jira_tasks j JOIN tasks t ON t.id=j.task_id
         LEFT JOIN task_details d ON d.task_id=j.task_id
         WHERE j.task_id=?1",
        [task_id],
        |r| {
            Ok(Linked {
                site: r.get(0)?,
                key: r.get(1)?,
                title: r.get(2)?,
                no_ask: r.get::<_, i64>(3)? != 0,
            })
        },
    )
    .optional()
}

/// „Nie pytaj dla tego zadania” (`jiraNoAsk` w `task_details`) i odwrócenie z kafelka.
pub fn set_no_ask(db: &Connection, task_id: i64, no_ask: bool) -> Result<(), String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [task_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Nie znaleziono zadania.".into());
    }
    let value = if no_ask { "true" } else { "false" };
    db.execute(
        "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('jiraNoAsk',json(?2)))
         ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.jiraNoAsk',json(?2))",
        params![task_id, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn active(db: &Connection) -> rusqlite::Result<Active> {
    db.query_row(
        "SELECT id, task_id FROM sessions WHERE ended_at IS NULL ORDER BY id DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// Powiadomienie przy schowanym oknie: „Przesuń” tylko przy wybranym przejściu.
pub fn message(p: &Prompt) -> Message {
    Message {
        key: String::new(),
        title: format!("Przesunąć {} do „{}”?", p.key, p.target_name()),
        body: format!(
            "{} · pomiar już trwa",
            notify::short(&format!("{} · {}", p.key, p.title))
        ),
        actions: if p.transition_id.is_some() {
            vec![Action::new(notify::JIRA_MOVE, "Przesuń", p.task_id)]
        } else {
            Vec::new()
        },
        open: View::default(),
    }
}

#[derive(Default)]
struct Inner {
    ready: bool,
    seen: Active,
    prompts: Vec<Prompt>,
    shown: Vec<Prompt>,
}

/// Stan pytań; rejestrować przed utworzeniem okien.
#[derive(Default)]
pub struct State(Mutex<Inner>);

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Wysyła listę widoczną w banerze i widgecie; w czasie bloku skupienia pusta.
fn publish(app: &tauri::AppHandle) -> Vec<Prompt> {
    let focus = crate::focus::holding(app);
    let Some(state) = app.try_state::<State>() else {
        return Vec::new();
    };
    let Ok(mut g) = state.0.lock() else {
        return Vec::new();
    };
    let visible = if focus { Vec::new() } else { g.prompts.clone() };
    if visible != g.shown {
        g.shown = visible.clone();
        let _ = app.emit(EVENT, visible.clone());
    }
    visible
}

fn remove(app: &tauri::AppHandle, task_id: i64) {
    if let Ok(mut g) = app.state::<State>().0.lock() {
        g.prompts.retain(|p| p.task_id != task_id);
    }
}

/// Jedno sprawdzenie aktywnego pomiaru; nowy start zadania ze zgłoszeniem
/// uruchamia zapytanie do Jiry w tle.
fn tick(app: &tauri::AppHandle) {
    let read = app.state::<Database>().0.lock().ok().and_then(|db| {
        let current = active(&db).ok()?;
        let info = match current {
            Some((_, task)) => linked(&db, task).ok().flatten(),
            None => None,
        };
        Some((current, info, crate::settings::load(&db)))
    });
    let Some((current, info, settings)) = read else {
        return;
    };
    let fresh = {
        let state = app.state::<State>();
        let Ok(mut g) = state.0.lock() else {
            return;
        };
        let fresh = if g.ready {
            started(g.seen, current)
        } else {
            // Pomiar trwający przy starcie aplikacji nie jest nowym startem.
            g.ready = true;
            None
        };
        g.seen = current;
        fresh
    };
    if let (Some(task), Some(info)) = (fresh, info) {
        // Szybkie odrzucenie bez sieci; kategorię sprawdza `check`.
        if settings.jira_write && settings.jira_ask_move && !info.no_ask {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = check(&app, task, info).await;
            });
        }
    }
    publish(app);
}

/// Status i przejścia z Jiry; przy kategorii „Do zrobienia” zapisuje pytanie.
async fn check(app: &tauri::AppHandle, task_id: i64, info: Linked) -> Result<(), String> {
    let c = jira::config(app)?;
    if c.email.is_empty() || c.site != info.site {
        return Ok(());
    }
    let secret = jira::token(app, &c)?;
    let client = jira::client()?;
    let (status, category) = jira::issue_status(&client, &c, &secret, &info.key).await?;
    jira::store_status(app, &info.site, &info.key, &status);
    let _ = app.emit("data-changed", ());
    let settings = crate::settings::current(app);
    if !should_ask(&settings, info.no_ask, &category) {
        return Ok(());
    }
    let all = jira::transitions(&client, &c, &secret, &info.key).await?;
    let (transitions, transition_id) = choose(&all, &settings.jira_move_status);
    if transitions.is_empty() {
        return Ok(());
    }
    let prompt = Prompt {
        task_id,
        key: info.key.clone(),
        title: info.title.clone(),
        status,
        url: format!("{}/browse/{}", info.site, info.key),
        target: settings.jira_move_status.clone(),
        transitions,
        transition_id,
    };
    {
        let state = app.state::<State>();
        let mut g = state.0.lock().map_err(|e| e.to_string())?;
        // Pomiar przeszedł w międzyczasie na inne zadanie: pytanie nieaktualne.
        if g.seen.map(|(_, t)| t) != Some(task_id) {
            return Ok(());
        }
        g.prompts = vec![prompt.clone()];
    }
    publish(app);
    if crate::suggest::main_hidden(app) || crate::focus::holding(app) {
        notify::show(app, &message(&prompt));
    }
    Ok(())
}

/// Wątek pytań; kończy się, gdy nie ma okna main (wołać po `create_windows`).
pub fn start(app: &tauri::AppHandle) {
    tick(app);
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        if app.get_webview_window("main").is_none() {
            break;
        }
        tick(&app);
    });
}

fn current(app: &tauri::AppHandle) -> Vec<Prompt> {
    publish(app);
    app.state::<State>()
        .0
        .lock()
        .map(|g| g.shown.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn jira_move_prompts(app: tauri::AppHandle) -> Vec<Prompt> {
    current(&app)
}

/// „Nie teraz”: pytanie znika do kolejnego startu zadania.
#[tauri::command]
pub fn jira_move_dismiss(app: tauri::AppHandle, task_id: i64) -> Vec<Prompt> {
    remove(&app, task_id);
    current(&app)
}

/// „Nie pytaj dla tego zadania” (z banera) i „Pytaj znowu” (z kafelka).
#[tauri::command]
pub fn jira_no_ask(
    app: tauri::AppHandle,
    task_id: i64,
    no_ask: bool,
) -> Result<db::Snapshot, String> {
    let snapshot = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        set_no_ask(&db, task_id, no_ask)?;
        db::snapshot(&db).map_err(|e| e.to_string())?
    };
    if no_ask {
        remove(&app, task_id);
    }
    publish(&app);
    let _ = app.emit("data-changed", ());
    Ok(snapshot)
}

/// Zadanie z kluczem dla zapisu: zgłoszenie musi być z połączonej Jiry.
fn target(app: &tauri::AppHandle, task_id: i64) -> Result<Linked, String> {
    let info = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        linked(&db, task_id).map_err(|e| e.to_string())?
    }
    .ok_or("To zadanie nie jest powiązane ze zgłoszeniem Jiry.")?;
    let c = jira::config(app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry, żeby zmieniać zgłoszenia.".into());
    }
    if c.site != info.site {
        return Err(format!(
            "Zgłoszenie {} pochodzi z innej Jiry ({}).",
            info.key, info.site
        ));
    }
    Ok(info)
}

/// Dostępne przejścia zgłoszenia zadania („Przesuń w Jirze…”, propozycja po ukończeniu).
#[tauri::command]
pub async fn jira_transitions(
    app: tauri::AppHandle,
    task_id: i64,
) -> Result<Vec<Transition>, String> {
    jira::ensure_write(&crate::settings::current(&app))?;
    let info = target(&app, task_id)?;
    let c = jira::config(&app)?;
    let secret = jira::token(&app, &c)?;
    jira::transitions(&jira::client()?, &c, &secret, &info.key).await
}

/// Wykonuje przejście, odświeża status w `jira_issues` i zamyka pytanie zadania.
pub async fn perform(
    app: &tauri::AppHandle,
    task_id: i64,
    transition_id: &str,
) -> Result<db::Snapshot, String> {
    jira::ensure_write(&crate::settings::current(app))?;
    let info = target(app, task_id)?;
    jira::transition(app, &info.key, transition_id).await?;
    let c = jira::config(app)?;
    let refreshed = match jira::token(app, &c) {
        Ok(secret) => match jira::client() {
            Ok(client) => jira::issue_status(&client, &c, &secret, &info.key)
                .await
                .ok(),
            Err(_) => None,
        },
        Err(_) => None,
    };
    // Bez odczytu statusu po przejściu zostaje nazwa docelowa z pytania.
    let status = refreshed.map(|(name, _)| name).or_else(|| {
        app.state::<State>().0.lock().ok().and_then(|g| {
            g.prompts
                .iter()
                .filter(|p| p.task_id == task_id)
                .flat_map(|p| &p.transitions)
                .find(|t| t.id == transition_id)
                .map(|t| t.to_status.clone())
        })
    });
    if let Some(status) = status.filter(|s| !s.is_empty()) {
        jira::store_status(app, &info.site, &info.key, &status);
    }
    remove(app, task_id);
    publish(app);
    let _ = app.emit("data-changed", ());
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db::snapshot(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jira_transition(
    app: tauri::AppHandle,
    task_id: i64,
    transition_id: String,
) -> Result<db::Snapshot, String> {
    perform(&app, task_id, &transition_id).await
}

/// „Przesuń” z powiadomienia: przejście wybrane w pytaniu. Bez pytania (np. stary
/// toast) albo przy błędzie pokazuje główne okno.
pub fn activate(app: &tauri::AppHandle, task_id: i64) {
    let chosen = app.state::<State>().0.lock().ok().and_then(|g| {
        g.prompts
            .iter()
            .find(|p| p.task_id == task_id)
            .and_then(|p| p.transition_id.clone())
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(id) = chosen else {
            crate::show(&app);
            return;
        };
        if let Err(e) = perform(&app, task_id, &id).await {
            crate::show(&app);
            let _ = app.emit("storage-error", e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, name: &str, to: &str, category: &str) -> Transition {
        Transition {
            id: id.into(),
            name: name.into(),
            to_status: to.into(),
            to_category: category.into(),
        }
    }

    #[test]
    fn single_in_progress_transition_wins() {
        let all = [
            t("11", "Do zrobienia", "Do zrobienia", "new"),
            t("21", "Rozpocznij", "W toku", "indeterminate"),
            t("31", "Zamknij", "Gotowe", "done"),
        ];
        let (list, chosen) = choose(&all, "In Progress");
        assert_eq!(list, vec![all[1].clone()]);
        assert_eq!(chosen.as_deref(), Some("21"));
    }

    #[test]
    fn several_transitions_use_preferred_name() {
        let all = [
            t("21", "Start", "In Progress", "indeterminate"),
            t("31", "Do review", "Code review", "indeterminate"),
            t("41", "Gotowe", "Done", "done"),
        ];
        let (list, chosen) = choose(&all, " in progress ");
        assert_eq!(list.len(), 2);
        assert_eq!(chosen.as_deref(), Some("21"));
        // Nazwa przejścia, gdy status docelowy się nie zgadza.
        assert_eq!(choose(&all, "do review").1.as_deref(), Some("31"));
        // Bez dopasowania: lista do wyboru.
        let (list, chosen) = choose(&all, "W realizacji");
        assert_eq!(list.len(), 2);
        assert_eq!(chosen, None);
        // Brak przejść „w toku”: nie ma o co pytać.
        let (list, chosen) = choose(&all[2..], "In Progress");
        assert!(list.is_empty() && chosen.is_none());
    }

    #[test]
    fn asks_only_for_new_category_with_switches_on() {
        let s = AppSettings::default();
        assert!(should_ask(&s, false, "new"));
        // Zgłoszenie już w toku albo zrobione: bez pytania.
        assert!(!should_ask(&s, false, "indeterminate"));
        assert!(!should_ask(&s, false, "done"));
        assert!(!should_ask(&s, false, ""));
        // „Nie pytaj dla tego zadania”.
        assert!(!should_ask(&s, true, "new"));
        // Wyłącznik pytania i główny przełącznik zapisów.
        let off = AppSettings {
            jira_ask_move: false,
            ..AppSettings::default()
        };
        assert!(!should_ask(&off, false, "new"));
        let read_only = AppSettings {
            jira_write: false,
            ..AppSettings::default()
        };
        assert!(!should_ask(&read_only, false, "new"));
        assert!(jira::ensure_write(&read_only).is_err());
        assert!(jira::ensure_write(&s).is_ok());
    }

    #[test]
    fn settings_default_to_writes_and_question_on() {
        let s = AppSettings::default();
        assert!(s.jira_write && s.jira_ask_move);
        assert_eq!(s.jira_move_status, "In Progress");
        let patched =
            crate::settings::apply(&s, &serde_json::json!({"jiraMoveStatus": "  W toku "}))
                .unwrap();
        assert_eq!(patched.jira_move_status, "W toku");
        assert!(crate::settings::apply(&s, &serde_json::json!({"jiraMoveStatus": " "})).is_err());
        assert!(
            crate::settings::apply(&s, &serde_json::json!({"jiraMoveStatus": "x".repeat(81)}))
                .is_err()
        );
    }

    #[test]
    fn start_detection_ignores_split_and_existing_measurement() {
        assert_eq!(started(None, Some((1, 5))), Some(5));
        assert_eq!(started(Some((1, 5)), Some((1, 5))), None);
        // Podział przedziału: nowy wiersz tego samego zadania.
        assert_eq!(started(Some((1, 5)), Some((2, 5))), None);
        assert_eq!(started(Some((1, 5)), Some((2, 6))), Some(6));
        assert_eq!(started(Some((1, 5)), None), None);
    }

    #[test]
    fn no_ask_is_stored_in_task_details_and_reversible() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        db::change(&mut conn, "add", None, Some("Zadanie".into()), 0).unwrap();
        conn.execute(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://a.atlassian.net','WMS-1')",
            [],
        )
        .unwrap();
        assert!(!linked(&conn, 1).unwrap().unwrap().no_ask);
        set_no_ask(&conn, 1, true).unwrap();
        let info = linked(&conn, 1).unwrap().unwrap();
        assert!(info.no_ask);
        assert_eq!(info.key, "WMS-1");
        let s = db::snapshot(&conn).unwrap();
        assert!(s.tasks[0].details.jira_no_ask);
        assert!(!should_ask(&AppSettings::default(), info.no_ask, "new"));
        // Pozostałe pola szczegółów zostają.
        conn.execute(
            "UPDATE task_details SET json=json_set(json,'$.project','WMS') WHERE task_id=1",
            [],
        )
        .unwrap();
        set_no_ask(&conn, 1, false).unwrap();
        let s = db::snapshot(&conn).unwrap();
        assert!(!s.tasks[0].details.jira_no_ask);
        assert_eq!(s.tasks[0].details.project, "WMS");
        // Zadanie bez zgłoszenia i nieistniejące.
        db::change(&mut conn, "add", None, Some("Lokalne".into()), 0).unwrap();
        assert_eq!(linked(&conn, 2).unwrap(), None);
        assert!(set_no_ask(&conn, 99, true).is_err());
    }

    #[test]
    fn notification_offers_move_only_with_chosen_transition() {
        let mut p = Prompt {
            task_id: 4,
            key: "WMS-1".into(),
            title: "Import".into(),
            status: "Do zrobienia".into(),
            url: "https://a.atlassian.net/browse/WMS-1".into(),
            target: "In Progress".into(),
            transitions: vec![
                t("21", "Start", "W toku", "indeterminate"),
                t("31", "Review", "Code review", "indeterminate"),
            ],
            transition_id: Some("21".into()),
        };
        let m = message(&p);
        assert_eq!(m.title, "Przesunąć WMS-1 do „W toku”?");
        assert_eq!(m.actions, [Action::new(notify::JIRA_MOVE, "Przesuń", 4)]);
        assert_eq!(m.actions[0].argument(), "jira-move:4");
        assert_eq!(
            notify::command(Some("jira-move:4"), &View::default()),
            notify::Command::JiraMove(4)
        );
        p.transition_id = None;
        let m = message(&p);
        assert_eq!(m.title, "Przesunąć WMS-1 do „In Progress”?");
        assert!(m.actions.is_empty());
    }
}
