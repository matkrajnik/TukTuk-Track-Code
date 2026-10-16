//! Propozycje przełączenia timera na zadanie, nad którym pracuje agent AI
//! (PLAN_1.3.md, etap 4 „Kiedy proponować”).
//!
//! Propozycja (agent, X) pojawia się, gdy agent pracuje co najmniej `MIN_WORK` nad
//! dopasowanym zadaniem X (`agents::resolve`), timer stoi albo mierzy inne zadanie,
//! folder agenta nie jest wyciszony w ustawieniach (`agent_muted_folders`, z
//! podfolderami), funkcja jest włączona (`agent_suggest`) i para nie została
//! odrzucona przyciskiem „Nie teraz”. Kilku agentów nad tym samym X daje jedną
//! propozycję; różne zadania dają listę.
//!
//! „Nie teraz” wycisza parę do końca pracy agenta: pamięć trzyma ją, dopóki agent
//! (rodzaj i folder) pracuje nad X, i zapomina po `GRACE` bez jego pracy.
//! Gdy główne okno jest schowane, nowa propozycja przychodzi raz jako powiadomienie
//! z przyciskiem „Przełącz” (`notify::AGENT_SWITCH`).
//!
//! Sygnał z aktywnego okna (W3, `foreground`) wchodzi tu jako „pseudoagent”
//! z `kind` równym źródłu (`window`, w W4 `browser`): ta sama reguła 2 minut,
//! „Nie teraz”, wyciszone foldery i powiadomienie, ale własny wyłącznik
//! (`window_suggest`, dla przeglądarki `browser_extension`) i treść ze źródłem
//! „Okno” albo „Przeglądarka”.
//!
//! Timer nigdy nie przełącza się sam: zmienia go tylko `agent_switch` (kliknięcie).
use crate::agents::{self, Agent, Match};
use crate::foreground;
use crate::notify::{self, Action, Message, View};
use crate::settings::AppSettings;
use crate::{db, Database};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Tyle musi trwać praca agenta, zanim zaproponujemy przełączenie.
pub const MIN_WORK: i64 = 2 * 60_000;
/// Po takim czasie bez pracy agenta nad X zapominamy „Nie teraz” i powiadomienie.
pub const GRACE: i64 = 10 * 60_000;

/// Agent w propozycji.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
    pub kind: String,
    pub name: String,
    pub cwd: String,
    pub since: i64,
}

/// Propozycja przełączenia na zadanie, nad którym pracują agenci.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub task: Match,
    pub agents: Vec<Worker>,
}

/// Cel propozycji: lokalne zadanie `t:<id>`, zgłoszenie bez zadania `k:<klucz>`.
pub fn target(task_id: Option<i64>, key: Option<&str>) -> String {
    match (task_id, key) {
        (Some(id), _) => format!("t:{id}"),
        (None, Some(k)) if !k.is_empty() => format!("k:{k}"),
        _ => String::new(),
    }
}
fn target_of(m: &Match) -> String {
    target(m.task_id, m.key.as_deref())
}
/// Para (agent, zadanie): rodzaj agenta, jego folder i cel.
fn pair(kind: &str, cwd: &str, target: &str) -> String {
    format!("{kind}|{}|{target}", agents::norm_folder(cwd))
}

/// Czy folder agenta (albo folder nadrzędny) jest na liście „Nie proponuj”.
pub fn muted(folders: &[String], cwd: &str) -> bool {
    let cwd = agents::norm_folder(cwd);
    !cwd.is_empty()
        && folders
            .iter()
            .map(|f| agents::norm_folder(f))
            .any(|f| !f.is_empty() && (cwd == f || cwd.starts_with(&format!("{f}/"))))
}

/// Pamięć odrzuceń („Nie teraz”) i wysłanych powiadomień: para → ostatnia chwila,
/// w której agent nad nią pracował.
#[derive(Debug, Default)]
pub struct Memory {
    dismissed: HashMap<String, i64>,
    notified: HashMap<String, i64>,
}

impl Memory {
    /// Odświeża pary, nad którymi agenci nadal pracują, i zapomina pozostałe po `GRACE`.
    fn touch(&mut self, agents: &[Agent], now: i64) {
        let live: Vec<String> = agents
            .iter()
            .filter_map(|a| {
                a.task
                    .as_ref()
                    .map(|m| pair(&a.kind, &a.cwd, &target_of(m)))
            })
            .collect();
        for map in [&mut self.dismissed, &mut self.notified] {
            for (key, at) in map.iter_mut() {
                if live.contains(key) {
                    *at = now;
                }
            }
            map.retain(|_, at| now - *at <= GRACE);
        }
    }

    /// Reguła propozycji; `active` to zadanie bieżącego pomiaru.
    pub fn pending(
        &mut self,
        agents: &[Agent],
        active: Option<i64>,
        s: &AppSettings,
        now: i64,
    ) -> Vec<Suggestion> {
        self.touch(agents, now);
        let mut out: Vec<Suggestion> = Vec::new();
        for agent in agents {
            let Some(m) = &agent.task else {
                continue;
            };
            let goal = target_of(m);
            // Ukończone zadanie: „Przełącz” wymagałoby najpierw przywrócenia go.
            // Agenci AI (`agent_suggest`) i sygnały z okna (`window_suggest`, W3)
            // mają osobne wyłączniki, ale tę samą regułę.
            if goal.is_empty()
                || !foreground::enabled(&agent.kind, s)
                || m.done
                || now - agent.since < MIN_WORK
                || (m.task_id.is_some() && m.task_id == active)
                || muted(&s.agent_muted_folders, &agent.cwd)
                || self
                    .dismissed
                    .contains_key(&pair(&agent.kind, &agent.cwd, &goal))
            {
                continue;
            }
            let worker = Worker {
                kind: agent.kind.clone(),
                name: agent.name.clone(),
                cwd: agent.cwd.clone(),
                since: agent.since,
            };
            match out.iter_mut().find(|x| target_of(&x.task) == goal) {
                Some(found) => found.agents.push(worker),
                None => out.push(Suggestion {
                    task: m.clone(),
                    agents: vec![worker],
                }),
            }
        }
        out
    }

    /// „Nie teraz”: wycisza X dla wszystkich agentów, którzy teraz nad nim pracują.
    pub fn dismiss(&mut self, agents: &[Agent], goal: &str, now: i64) -> bool {
        let mut any = false;
        for agent in agents {
            if agent.task.as_ref().map(target_of).as_deref() == Some(goal) && !goal.is_empty() {
                self.dismissed
                    .insert(pair(&agent.kind, &agent.cwd, goal), now);
                any = true;
            }
        }
        any
    }

    /// Propozycje z agentami, o których nie było jeszcze powiadomienia; zaznacza je.
    pub fn unnotified(&mut self, list: &[Suggestion], now: i64) -> Vec<Suggestion> {
        list.iter()
            .filter_map(|s| {
                let goal = target_of(&s.task);
                let fresh: Vec<Worker> = s
                    .agents
                    .iter()
                    .filter(|w| {
                        self.notified
                            .insert(pair(&w.kind, &w.cwd, &goal), now)
                            .is_none()
                    })
                    .cloned()
                    .collect();
                (!fresh.is_empty()).then(|| Suggestion {
                    task: s.task.clone(),
                    agents: fresh,
                })
            })
            .collect()
    }
}

/// Nazwa zadania w propozycji: tytuł, a bez niego klucz zgłoszenia.
fn title(s: &Suggestion) -> String {
    let t = s.task.title.trim();
    match (&s.task.key, t.is_empty()) {
        (Some(key), true) => key.clone(),
        (Some(key), false) if s.task.task_id.is_none() => format!("{key}: {t}"),
        _ => t.to_string(),
    }
}

/// Propozycja wyłącznie z sygnału okna albo przeglądarki (bez agenta AI).
fn signal_only(s: &Suggestion) -> bool {
    !s.agents.is_empty() && s.agents.iter().all(|w| foreground::is_signal(&w.kind))
}

/// Pytanie w propozycji z samego sygnału: z rozszerzenia przeglądarki (W4) z kluczem
/// zgłoszenia („Pracujesz teraz przy WMS-123?”), inaczej ogólne.
fn signal_title(s: &Suggestion) -> String {
    match (
        &s.task.key,
        s.agents.iter().any(|w| w.kind == foreground::BROWSER),
    ) {
        (Some(key), true) => format!("Pracujesz teraz przy {key}?"),
        _ => "Pracujesz teraz przy innym zadaniu?".into(),
    }
}

/// Nazwa źródła sygnału w interfejsie.
pub fn source_label(kind: &str) -> &'static str {
    match kind {
        foreground::WINDOW => "Okno",
        foreground::BROWSER => "Przeglądarka",
        _ => "Agent",
    }
}

/// Agenci AI i sygnał z aktywnego okna (W3) dla jednej reguły propozycji.
fn workers(app: &tauri::AppHandle, now: i64) -> Vec<Agent> {
    let mut list = agents::current(app);
    list.extend(foreground::current(app, now));
    list
}

/// Powiadomienie o nowych propozycjach (główne okno schowane). Jedna propozycja
/// z lokalnym zadaniem ma przycisk „Przełącz”; kliknięcie treści pokazuje główne
/// okno z banerem.
pub fn message(list: &[Suggestion]) -> Option<Message> {
    let names = |s: &Suggestion| {
        s.agents
            .iter()
            .map(|w| w.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let switch = |s: &Suggestion| {
        s.task
            .task_id
            .map(|id| vec![Action::new(notify::AGENT_SWITCH, "Przełącz", id)])
            .unwrap_or_default()
    };
    match list {
        [] => None,
        // Sam sygnał z okna albo przeglądarki (W3, W4): źródło w treści.
        [one] if signal_only(one) => Some(Message {
            key: String::new(),
            title: signal_title(one),
            body: format!(
                "{}: {} · {}",
                source_label(&one.agents[0].kind),
                names(one),
                notify::short(&title(one))
            ),
            actions: switch(one),
            open: View::default(),
        }),
        [one] => Some(Message {
            key: String::new(),
            title: "Agent pracuje nad innym zadaniem".into(),
            body: format!("{} · {}", names(one), notify::short(&title(one))),
            actions: switch(one),
            open: View::default(),
        }),
        many => Some(Message {
            key: String::new(),
            title: format!(
                "{}: {} do przełączenia",
                if many
                    .iter()
                    .flat_map(|s| &s.agents)
                    .any(|w| foreground::is_signal(&w.kind))
                {
                    "Propozycje"
                } else {
                    "Agenci AI"
                },
                notify::tasks_word(many.len())
            ),
            body: many
                .iter()
                .map(|s| format!("• {}", notify::short(&title(s))))
                .collect::<Vec<_>>()
                .join("\n"),
            actions: Vec::new(),
            open: View::default(),
        }),
    }
}

/// Pamięć i ostatnia wysłana lista.
#[derive(Default)]
pub struct State(Mutex<(Memory, Vec<Suggestion>)>);

/// Rejestruje stan propozycji; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

fn main_hidden(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| !w.is_visible().unwrap_or(true) || w.is_minimized().unwrap_or(false))
        .unwrap_or(true)
}

/// Przelicza propozycje: zmiana → `agent-suggestions`, nowe przy schowanym
/// głównym oknie → powiadomienie. Blokady bierze po kolei, nigdy dwie naraz.
pub fn refresh(app: &tauri::AppHandle, now: i64) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let agents = workers(app, now);
    let read = app.state::<Database>().0.lock().ok().and_then(|db| {
        let active: Option<i64> = db
            .query_row(
                "SELECT task_id FROM sessions WHERE ended_at IS NULL LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()?;
        Some((active, crate::settings::load(&db)))
    });
    let Some((active, settings)) = read else {
        return;
    };
    let hidden = main_hidden(app);
    // W czasie bloku skupienia baner i linia w widgecie czekają, a powiadomienie
    // trafia do kolejki bloku (`notify::show`); po bloku baner wraca sam.
    let focus = crate::focus::holding(app);
    let Some((changed, list, fresh)) = state.0.lock().ok().map(|mut g| {
        let mut list = g.0.pending(&agents, active, &settings, now);
        // Propozycje widoczne w banerze też liczą się jako zgłoszone, więc schowanie
        // okna później nie wysyła o nich powiadomienia.
        let fresh = g.0.unnotified(&list, now);
        if focus {
            list.clear();
        }
        let changed = list != g.1;
        g.1 = list.clone();
        (changed, list, fresh)
    }) else {
        return;
    };
    if changed {
        let _ = app.emit("agent-suggestions", list);
    }
    if hidden || focus {
        if let Some(m) = message(&fresh) {
            notify::show(app, &m);
        }
    }
}

fn current(app: &tauri::AppHandle) -> Vec<Suggestion> {
    app.state::<State>()
        .0
        .lock()
        .map(|g| g.1.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn agent_suggestions(app: tauri::AppHandle) -> Vec<Suggestion> {
    refresh(&app, db::now());
    current(&app)
}

/// „Przełącz”: start X (poprzedni pomiar staje). Zgłoszenie bez zadania przechodzi
/// ścieżką „Rozpocznij” z widoku Jira, więc zadanie powstaje dopiero teraz; gdy
/// listy zgłoszeń nie pobrano od startu aplikacji, tytuł przychodzi wprost z Jiry
/// (`jira::start_by_key`). Asynchroniczne, bo może czekać na sieć.
#[tauri::command]
pub async fn agent_switch(
    app: tauri::AppHandle,
    task_id: Option<i64>,
    key: Option<String>,
) -> Result<db::Snapshot, String> {
    let result = match (task_id, key) {
        (Some(id), _) => {
            let result = {
                let state = app.state::<Database>();
                let mut connection = state.0.lock().map_err(|e| e.to_string())?;
                db::change(&mut connection, "start", Some(id), None, db::now())?
            };
            crate::reminder::reset(&app);
            let _ = app.emit("data-changed", ());
            result
        }
        (None, Some(key)) => crate::jira::start_by_key(&app, &key).await?,
        (None, None) => return Err("Brak zadania do przełączenia.".into()),
    };
    refresh(&app, db::now());
    Ok(result)
}

/// „Nie teraz”: cisza dla par (agent, X) do końca pracy agenta.
#[tauri::command]
pub fn agent_dismiss(
    app: tauri::AppHandle,
    task_id: Option<i64>,
    key: Option<String>,
) -> Vec<Suggestion> {
    let goal = target(task_id, key.as_deref());
    let agents = workers(&app, db::now());
    if let Ok(mut g) = app.state::<State>().0.lock() {
        g.0.dismiss(&agents, &goal, db::now());
    }
    refresh(&app, db::now());
    current(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const NOW: i64 = 100 * MIN;

    fn agent(kind: &str, cwd: &str, since: i64, task: Option<i64>, key: Option<&str>) -> Agent {
        Agent {
            kind: kind.into(),
            name: format!("{kind} {cwd}"),
            cwd: cwd.into(),
            since,
            task: Some(Match {
                task_id: task,
                title: task.map(|t| format!("Zadanie {t}")).unwrap_or_default(),
                key: key.map(str::to_string),
                source: if task.is_some() { "task" } else { "jira" }.into(),
                done: false,
            }),
            ..Agent::default()
        }
    }
    fn targets(list: &[Suggestion]) -> Vec<String> {
        list.iter().map(|s| target_of(&s.task)).collect()
    }

    #[test]
    fn proposes_after_two_minutes_of_work_on_other_task() {
        let s = AppSettings::default();
        let mut m = Memory::default();
        let a = agent("claude", "C:\\wms", NOW - MIN, Some(1), Some("WMS-1"));
        // Za krótko.
        assert!(m.pending(&[a.clone()], None, &s, NOW).is_empty());
        assert!(m.pending(&[a.clone()], None, &s, NOW + MIN - 1).is_empty());
        // Dwie minuty: timer stoi.
        assert_eq!(
            targets(&m.pending(&[a.clone()], None, &s, NOW + MIN)),
            ["t:1"]
        );
        // Timer mierzy inne zadanie: nadal propozycja; mierzy X: brak.
        assert_eq!(
            targets(&m.pending(&[a.clone()], Some(2), &s, NOW + MIN)),
            ["t:1"]
        );
        assert!(m.pending(&[a.clone()], Some(1), &s, NOW + MIN).is_empty());
        // Agent bez dopasowanego zadania nie daje propozycji.
        let mut free = a.clone();
        free.task = None;
        assert!(m.pending(&[free], None, &s, NOW + MIN).is_empty());
        // Wyłącznik całej funkcji.
        let off = AppSettings {
            agent_suggest: false,
            ..AppSettings::default()
        };
        assert!(m.pending(&[a], None, &off, NOW + MIN).is_empty());
    }

    #[test]
    fn groups_agents_by_task_and_lists_different_tasks() {
        let s = AppSettings::default();
        let mut m = Memory::default();
        let list = m.pending(
            &[
                agent("claude", "C:\\wms", NOW - 5 * MIN, Some(1), None),
                agent("codex", "C:\\wms\\api", NOW - 3 * MIN, Some(1), None),
                agent("claude", "C:\\inne", NOW - 3 * MIN, None, Some("WMS-9")),
            ],
            None,
            &s,
            NOW,
        );
        assert_eq!(targets(&list), ["t:1", "k:WMS-9"]);
        assert_eq!(list[0].agents.len(), 2);
        assert_eq!(list[1].agents[0].cwd, "C:\\inne");
    }

    #[test]
    fn muted_folder_covers_subfolders() {
        let s = AppSettings {
            agent_muted_folders: vec!["c:/Projekty/WMS/".into()],
            ..AppSettings::default()
        };
        let mut m = Memory::default();
        let inside = agent(
            "claude",
            "C:\\Projekty\\wms\\src",
            NOW - 5 * MIN,
            Some(1),
            None,
        );
        let beside = agent(
            "claude",
            "C:\\Projekty\\wms-api",
            NOW - 5 * MIN,
            Some(2),
            None,
        );
        assert_eq!(
            targets(&m.pending(&[inside, beside], None, &s, NOW)),
            ["t:2"]
        );
    }

    #[test]
    fn not_now_lasts_until_agent_stops_working() {
        let s = AppSettings::default();
        let mut m = Memory::default();
        let a = agent("claude", "C:\\wms", NOW - 5 * MIN, Some(1), None);
        let other = agent("codex", "C:\\api", NOW - 5 * MIN, Some(2), None);
        assert_eq!(
            m.pending(&[a.clone(), other.clone()], None, &s, NOW).len(),
            2
        );
        assert!(m.dismiss(&[a.clone(), other.clone()], "t:1", NOW));
        assert!(!m.dismiss(&[a.clone()], "t:9", NOW));
        // Odrzucona tylko para (agent, X); drugi agent nadal ma propozycję.
        assert_eq!(
            targets(&m.pending(&[a.clone(), other.clone()], None, &s, NOW)),
            ["t:2"]
        );
        // Kolejna tura pracy tego samego agenta (nowe `since`) nadal w ciszy.
        let mut next = a.clone();
        next.since = NOW + 2 * MIN;
        assert!(m.pending(&[], None, &s, NOW + MIN).is_empty());
        assert!(m.pending(&[next], None, &s, NOW + 5 * MIN).is_empty());
        // Agent przestał pracować na dłużej niż GRACE: para zapomniana.
        let later = NOW + 5 * MIN + GRACE + 1;
        assert!(m.pending(&[], None, &s, later).is_empty());
        let mut back = a.clone();
        back.since = later;
        assert_eq!(
            targets(&m.pending(&[back], None, &s, later + MIN_WORK)),
            ["t:1"]
        );
        // Ten sam agent nad innym zadaniem nie jest objęty odrzuceniem X.
        let mut m = Memory::default();
        m.dismiss(&[a.clone()], "t:1", NOW);
        let moved = agent("claude", "C:\\wms", NOW - 5 * MIN, Some(3), None);
        assert_eq!(targets(&m.pending(&[moved], None, &s, NOW)), ["t:3"]);
    }

    #[test]
    fn notification_is_sent_once_per_pair_with_switch_button() {
        let s = AppSettings::default();
        let mut m = Memory::default();
        let a = agent("claude", "C:\\wms", NOW - 5 * MIN, Some(1), None);
        let list = m.pending(&[a.clone()], None, &s, NOW);
        let fresh = m.unnotified(&list, NOW);
        assert_eq!(fresh.len(), 1);
        assert!(m.unnotified(&list, NOW + 5_000).is_empty());
        let msg = message(&fresh).unwrap();
        assert_eq!(msg.title, "Agent pracuje nad innym zadaniem");
        assert_eq!(msg.body, "claude C:\\wms · Zadanie 1");
        assert_eq!(msg.actions.len(), 1);
        assert_eq!(msg.actions[0].argument(), "agent-switch:1");
        assert_eq!(
            notify::command(Some("agent-switch:1"), &msg.open),
            notify::Command::AgentSwitch(1)
        );
        // Zgłoszenie bez lokalnego zadania: bez przycisku, klik pokazuje okno.
        let jira = m.pending(
            &[agent("codex", "C:\\x", NOW - 5 * MIN, None, Some("WMS-9"))],
            None,
            &s,
            NOW,
        );
        let msg = message(&jira).unwrap();
        assert!(msg.actions.is_empty());
        assert_eq!(msg.body, "codex C:\\x · WMS-9");
        // Kilka zadań: zbiorcze powiadomienie z listą.
        let two = m.pending(
            &[a, agent("codex", "C:\\api", NOW - 5 * MIN, Some(2), None)],
            None,
            &s,
            NOW,
        );
        let msg = message(&two).unwrap();
        assert_eq!(msg.title, "Agenci AI: 2 zadania do przełączenia");
        assert_eq!(msg.body, "• Zadanie 1\n• Zadanie 2");
        assert!(message(&[]).is_none());
    }

    #[test]
    fn done_task_matched_by_key_is_not_proposed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tasks(title,created_at,done) VALUES('Gotowe',0,1);
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-5');",
        )
        .unwrap();
        // Jedyne zadanie z kluczem jest ukończone: dopasowanie zna ten stan.
        let m = agents::resolve(&conn, "C:\\wms", Some("WMS-5"))
            .unwrap()
            .unwrap();
        assert_eq!((m.task_id, m.done), (Some(1), true));
        let a = Agent {
            kind: "claude".into(),
            name: "wms".into(),
            cwd: "C:\\wms".into(),
            since: NOW - 5 * MIN,
            task: Some(m),
            ..Agent::default()
        };
        let mut memory = Memory::default();
        let s = AppSettings::default();
        assert!(memory.pending(&[a.clone()], None, &s, NOW).is_empty());
        // Po przywróceniu do zrobienia propozycja wraca.
        conn.execute("UPDATE tasks SET done=0", []).unwrap();
        let mut back = a;
        back.task = agents::resolve(&conn, "C:\\wms", Some("WMS-5")).unwrap();
        assert_eq!(targets(&memory.pending(&[back], None, &s, NOW)), ["t:1"]);
    }

    #[test]
    fn switch_button_starts_task_and_stops_previous_measurement() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        for title in ["Mierzone", "Agent"] {
            conn.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db::change(&mut conn, "start", Some(1), None, 1_000).unwrap();
        let out = notify::perform(
            &mut conn,
            notify::Command::AgentSwitch(2),
            "2026-10-15",
            5_000,
        )
        .unwrap();
        assert!(out.changed);
        let rows: Vec<(i64, Option<i64>)> = conn
            .prepare("SELECT task_id, ended_at FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(1, Some(5_000)), (2, None)]);
    }
}
