//! Tablica sesji agentów: które sesje Claude Code i Codexa czekają na użytkownika,
//! które pracują, a które skończyły się i nikt ich jeszcze nie przejrzał.
//!
//! Źródłem jest maszyna stanów hooków (`hooks::Sessions::board`). Pętla agentów
//! (`agents::start`) co 5 s i po zmianie fazy sesji rozpoznaje gałąź i zadanie
//! każdej sesji (jak dla pracujących agentów, bez asystenta przypisań) i oddaje
//! listę tutaj ([`publish`]). Ten moduł trzyma ostatnią listę, emituje
//! [`EVENT`] przy zmianie, wysyła powiadomienia o czekaniu i o końcu (osobne
//! wyłączniki w ustawieniach) i podaje liczniki dla traya.
//!
//! Wszystko jest w pamięci: po restarcie aplikacji tablica jest pusta, tak jak
//! lista pracujących agentów.
use crate::agents::{self, Agent};
use crate::hooks::{BoardSession, Phase};
use crate::notify::{self, Message, View};
use crate::settings::AppSettings;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Zdarzenie z nową tablicą sesji dla okna.
pub const EVENT: &str = "agent-board-changed";
/// Ile sesji wymienia zbiorcze powiadomienie.
const NAMES: usize = 3;

/// Sesja na tablicy: agent (folder, gałąź, klucz, zadanie) i jego stan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(flatten)]
    pub agent: Agent,
    /// Identyfikator sesji (do „Przejrzane”).
    pub id: String,
    /// `working`, `waiting` albo `done`.
    pub phase: &'static str,
    /// Wejście w bieżącą fazę (ms).
    pub phase_at: i64,
    pub seen: bool,
    /// Na co czeka sesja albo `brak zdarzeń` przy końcu z ciszy.
    pub reason: Option<String>,
    /// Pracujący subagenci.
    pub subagents: usize,
    /// Koniec z braku zdarzeń (ubity proces), nie po `Stop`.
    pub timed_out: bool,
}

impl Entry {
    fn waiting(&self) -> bool {
        self.phase == Phase::Waiting.as_str()
    }
    fn done(&self) -> bool {
        self.phase == Phase::Done.as_str()
    }
}

/// Sesje z maszyny stanów jako wpisy tablicy (przed rozpoznaniem gałęzi i zadania).
pub fn entries(board: Vec<BoardSession>) -> Vec<Entry> {
    board
        .into_iter()
        .map(|b| {
            let name = agents::folder(&b.cwd);
            let timed_out = b.timed_out();
            Entry {
                agent: Agent {
                    kind: b.agent,
                    name: if name.is_empty() {
                        "sesja".into()
                    } else {
                        name
                    },
                    cwd: b.cwd,
                    since: b.phase_at,
                    session_id: b.session_id.clone(),
                    ..Agent::default()
                },
                id: b.session_id,
                phase: b.phase.as_str(),
                phase_at: b.phase_at,
                seen: b.seen,
                reason: b.reason,
                subagents: b.subagents,
                timed_out,
            }
        })
        .collect()
}

/// Liczniki do traya i nawigacji: nieprzejrzane czekające i zakończone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub waiting: usize,
    pub done: usize,
}

/// Koniec z braku zdarzeń (ubity proces) jest na tablicy, ale nie w licznikach:
/// nikt na niego nie czeka i nie ma o nim powiadomienia.
pub fn counts(list: &[Entry]) -> Counts {
    list.iter()
        .filter(|e| !e.seen && !e.timed_out)
        .fold(Counts::default(), |c, e| Counts {
            waiting: c.waiting + usize::from(e.waiting()),
            done: c.done + usize::from(e.done()),
        })
}

/// Wiersz traya: „Agenci: czeka 2 · skończone 1”; `None`, gdy nic nie czeka.
pub fn tray_line(c: Counts) -> Option<String> {
    let parts: Vec<String> = [("czeka", c.waiting), ("skończone", c.done)]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(label, n)| format!("{label} {n}"))
        .collect();
    (!parts.is_empty()).then(|| format!("Agenci: {}", parts.join(" · ")))
}

/// Klucz powiadomienia: sesja i chwila wejścia w fazę (jedno na czekanie lub koniec).
fn notice_key(e: &Entry) -> String {
    format!("{}|{}|{}|{}", e.agent.kind, e.id, e.phase, e.phase_at)
}

/// Pamięć tablicy: ostatnia lista i sesje, o których już powiadomiliśmy.
#[derive(Default)]
pub struct Memory {
    entries: Vec<Entry>,
    notified: HashSet<String>,
    /// Przejrzane końce zdjęte z listy od ostatniego `update`.
    dismissed: Vec<String>,
}

impl Memory {
    /// Zapisuje listę; zwraca `true`, gdy się zmieniła, i nowe wpisy do
    /// powiadomienia (nieprzejrzane czekanie albo koniec po `Stop`, każdy raz).
    pub fn update(&mut self, mut list: Vec<Entry>) -> (bool, Vec<Entry>) {
        // Lista z pętli powstaje przed rozpoznaniem zadań; „Przejrzane” kliknięte
        // w tym czasie (ta sama faza) nie może się cofnąć.
        let seen: HashSet<String> = self
            .entries
            .iter()
            .filter(|e| e.seen)
            .map(notice_key)
            .chain(self.dismissed.iter().cloned())
            .collect();
        for e in list.iter_mut() {
            if e.phase != Phase::Working.as_str() && seen.contains(&notice_key(e)) {
                e.seen = true;
            }
        }
        list.retain(|e| !(e.done() && e.seen));
        let fresh: Vec<Entry> = list
            .iter()
            .filter(|e| !e.seen && (e.waiting() || (e.done() && !e.timed_out)))
            .filter(|e| !self.notified.contains(&notice_key(e)))
            .cloned()
            .collect();
        // Pamiętamy tylko klucze sesji, które nadal są na tablicy.
        let current: HashSet<String> = list.iter().map(notice_key).collect();
        self.notified.retain(|k| current.contains(k));
        self.notified.extend(fresh.iter().map(notice_key));
        self.dismissed.clear();
        let changed = list != self.entries;
        self.entries = list;
        (changed, fresh)
    }

    /// „Przejrzane” w zapamiętanej liście (bez czekania na pętlę agentów);
    /// `id: None` = wszystkie czekające i zakończone.
    pub fn mark_seen(&mut self, kind: &str, id: Option<&str>) {
        for e in self.entries.iter_mut() {
            if !e.waiting() && !e.done() {
                continue;
            }
            if id.is_none_or(|id| e.id == id && e.agent.kind == kind) {
                e.seen = true;
            }
        }
        // Przejrzane końce znikają z listy, ale ich klucz musi przetrwać do
        // następnego `update`, żeby starsza lista z pętli ich nie przywróciła.
        let dismissed: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.done() && e.seen)
            .map(notice_key)
            .collect();
        self.dismissed.extend(dismissed);
        self.entries.retain(|e| !(e.done() && e.seen));
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// Powiadomienie o nowych wpisach według ustawień; `None`, gdy nie ma o czym.
pub fn message(fresh: &[Entry], s: &AppSettings) -> Option<Message> {
    let shown: Vec<&Entry> = fresh
        .iter()
        .filter(|e| (e.waiting() && s.agent_notify_waiting) || (e.done() && s.agent_notify_done))
        .collect();
    let label = |e: &Entry| match &e.agent.task {
        Some(task) if !task.title.is_empty() => {
            format!("{} · {}", e.agent.name, notify::short(&task.title))
        }
        _ => e.agent.name.clone(),
    };
    let message = |title: String, body: String| Message {
        key: String::new(),
        title,
        body,
        actions: Vec::new(),
        open: View {
            view: Some("agents"),
            ..View::default()
        },
    };
    match shown.as_slice() {
        [] => None,
        [one] if one.waiting() => Some(message(
            format!("{} czeka na Ciebie", kind_label(&one.agent.kind)),
            match &one.reason {
                Some(reason) => format!("{} · {reason}", label(one)),
                None => label(one),
            },
        )),
        [one] => Some(message(
            format!("{} skończył pracę", kind_label(&one.agent.kind)),
            label(one),
        )),
        many => {
            let waiting = many.iter().filter(|e| e.waiting()).count();
            let title = match (waiting, many.len() - waiting) {
                (w, 0) => format!("Sesje agentów czekają na Ciebie: {w}"),
                (0, d) => format!("Sesje agentów skończyły pracę: {d}"),
                (w, d) => format!("Sesje agentów: czeka {w}, skończone {d}"),
            };
            let mut names: Vec<String> = many.iter().take(NAMES).map(|e| label(e)).collect();
            if many.len() > NAMES {
                names.push(format!("i {} więcej", many.len() - NAMES));
            }
            Some(message(title, names.join(", ")))
        }
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "codex" => "Codex",
        _ => "Claude",
    }
}

#[derive(Default)]
pub struct State(Mutex<Memory>);

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Nowa tablica z pętli agentów: zapis, zdarzenie przy zmianie, powiadomienie.
/// Gdy główne okno ma fokus, użytkownik widzi licznik w nawigacji i toast jest
/// pomijany (sesja i tak zostaje oznaczona jako zgłoszona).
pub fn publish(app: &tauri::AppHandle, list: Vec<Entry>) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let Ok(mut memory) = state.0.lock() else {
        return;
    };
    let (changed, fresh) = memory.update(list);
    let snapshot = changed.then(|| memory.entries.clone());
    drop(memory);
    if let Some(list) = snapshot {
        let _ = app.emit(EVENT, list);
    }
    if fresh.is_empty() || main_focused(app) {
        return;
    }
    let settings = crate::settings::current(app);
    if let Some(msg) = message(&fresh, &settings) {
        notify::show(app, &msg);
    }
}

fn main_focused(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Liczniki z ostatniej tablicy (tray).
pub fn current_counts(app: &tauri::AppHandle) -> Counts {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|m| counts(m.entries())))
        .unwrap_or_default()
}

fn snapshot(app: &tauri::AppHandle) -> Vec<Entry> {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|m| m.entries.clone()))
        .unwrap_or_default()
}

/// „Przejrzane” w maszynie stanów i w zapamiętanej liście; `id: None` = wszystkie.
fn mark(app: &tauri::AppHandle, kind: &str, id: Option<&str>) -> Vec<Entry> {
    let changed = app
        .try_state::<agents::State>()
        .is_some_and(|agents| match id {
            Some(id) => agents.mark_seen(kind, id),
            None => agents.mark_all_seen(),
        });
    if changed {
        if let Some(state) = app.try_state::<State>() {
            if let Ok(mut memory) = state.0.lock() {
                memory.mark_seen(kind, id);
            }
        }
    }
    let list = snapshot(app);
    if changed {
        let _ = app.emit(EVENT, list.clone());
    }
    list
}

#[tauri::command]
pub fn agent_board(app: tauri::AppHandle) -> Vec<Entry> {
    snapshot(&app)
}

#[tauri::command]
pub fn agent_board_seen(app: tauri::AppHandle, kind: String, id: String) -> Vec<Entry> {
    mark(&app, &kind, Some(&id))
}

#[tauri::command]
pub fn agent_board_seen_all(app: tauri::AppHandle) -> Vec<Entry> {
    mark(&app, "", None)
}

#[cfg(test)]
mod tests;
