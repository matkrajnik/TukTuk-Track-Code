//! Wykrywanie agentów AI przez hooki Claude Code i Codexa (PLAN_BACKLOG.md, B8).
//!
//! Przepływ: agent uruchamia `tuktuk-track-hook.exe` przy każdym zdarzeniu → potok
//! `\\.\pipe\tuktuk-track-hooks` → wątek w tym module (`start`) → [`Sessions`] (maszyna
//! stanów sesji) w `agents::Watch` → pętla `agents::start` liczy z niej listę
//! pracujących agentów. Stan nie jest skanowany z plików: po restarcie aplikacji
//! lista jest pusta i zapełnia się zdarzeniami (`PreToolUse` przychodzi co kilka
//! sekund).
//!
//! Maszyna stanów sesji (klucz: agent + `session_id`):
//! - `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SubagentStart` = praca;
//! - `PermissionRequest`, `Notification` (`permission_prompt`, `agent_needs_input`,
//!   `idle_prompt` w trakcie pracy) i `PreToolUse` agenta głównego z
//!   `AskUserQuestion` albo `ExitPlanMode` = czekanie na użytkownika, nie praca;
//!   powód (nazwa narzędzia albo komunikat agenta) trafia do tablicy sesji;
//! - `Stop` = koniec tury, ale dopóki trwają subagenci, sesja nadal pracuje;
//!   `Interrupt` = koniec; `SessionEnd` usuwa sesję;
//! - zdarzenia z `agent_id` to subagenci, liczeni do sesji nadrzędnej; subagent
//!   bez zdarzeń przez [`SUBAGENT_SILENCE`] przestaje się liczyć (zgubiony
//!   `SubagentStop`);
//! - sesja bez zdarzeń przez [`SILENCE`] kończy się sama (ubity proces); w trakcie
//!   wywołania narzędzia (`PreToolUse` bez `PostToolUse`) dopiero po [`TOOL_SILENCE`],
//!   a czekająca na użytkownika po [`WAIT_SILENCE`].
//!
//! Tablica sesji (`agent_board`, [`Sessions::board`]): każda zmiana fazy zapisuje
//! chwilę wejścia; czekanie i koniec są nieprzejrzane, dopóki użytkownik ich nie
//! oznaczy albo sesja nie wróci do pracy.
//!
//! Rejestracja hooków (`hooks_config`) jest zgodna z ustawieniem `agent_hooks`:
//! przy starcie i co minutę (po aktualizacji instalatorem ścieżka exe się zmienia),
//! przy zmianie ustawienia i po przywróceniu kopii.
use crate::hooks_config::{self, Dirs, Registration};
use crate::{agents, db, hook_wire, settings};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// Sesja bez zdarzeń tak długo kończy się sama.
pub const SILENCE: i64 = 10 * 60_000;
/// Limit ciszy sesji w trakcie wywołania narzędzia (`PreToolUse` bez
/// `PostToolUse`): długi build, testy albo subagent pracują bez zdarzeń.
pub const TOOL_SILENCE: i64 = 60 * 60_000;
/// Subagent bez zdarzeń tak długo przestaje trzymać sesję w pracy (zgubiony
/// `SubagentStop`: przerwanie, zalany potok); w trakcie narzędzia [`TOOL_SILENCE`].
pub const SUBAGENT_SILENCE: i64 = 3 * 60_000;
/// Sesja czekająca na użytkownika kończy się sama dopiero po takim czasie bez
/// zdarzeń: agent nie wysyła nic, dopóki ktoś nie odpowie, a tablica sesji ma
/// pokazywać czekanie, nie „koniec”.
pub const WAIT_SILENCE: i64 = 2 * 3_600_000;
/// Powód zakończenia sesji przez brak zdarzeń (ubity proces, zgubione zdarzenie).
pub const TIMEOUT_REASON: &str = "brak zdarzeń";
/// Zakończona sesja jest zapominana po takim czasie bez zdarzeń.
pub const FORGET: i64 = 12 * 3_600_000;
/// Najwięcej pamiętanych sesji (ochrona pamięci przed zalewem zdarzeń).
const MAX_SESSIONS: usize = 128;
/// Ile ostatnich zdarzeń pokazuje diagnostyka w Ustawieniach.
const RECENT: usize = 30;
/// Co ile ponawiamy sprawdzenie rejestracji w plikach agentów.
const RESYNC: i64 = 60_000;

/// Zdarzenie hooka po odchudzeniu (`hook_wire::envelope`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookEvent {
    /// `claude` albo `codex`.
    pub agent: String,
    pub session_id: String,
    pub cwd: String,
    pub event: String,
    /// Obecne tylko w zdarzeniach subagentów.
    pub agent_id: Option<String>,
    pub notification_type: Option<String>,
    pub prompt: Option<String>,
    /// Nazwa narzędzia (`PreToolUse`, `PermissionRequest`), bez argumentów.
    pub tool_name: Option<String>,
    /// Komunikat powiadomienia agenta (`Notification`).
    pub message: Option<String>,
}

/// Koperta z potoku → zdarzenie. `None` dla niepoprawnej koperty, nieznanego agenta
/// albo zdarzenia bez `session_id` i nazwy.
pub fn parse(body: &[u8]) -> Option<HookEvent> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let agent = value.get("agent")?.as_str()?;
    if agent != "claude" && agent != "codex" {
        return None;
    }
    let event = value.get("event")?.as_object()?;
    let text = |key: &str| {
        event
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let session_id = text("session_id")?;
    let name = text("hook_event_name")?;
    if session_id.chars().count() > 200 || name.chars().count() > 64 {
        return None;
    }
    Some(HookEvent {
        agent: agent.into(),
        session_id: session_id.into(),
        cwd: text("cwd")
            .map(|c| c.chars().take(1024).collect())
            .unwrap_or_default(),
        event: name.into(),
        agent_id: text("agent_id").map(|s| s.chars().take(200).collect()),
        notification_type: text("notification_type").map(str::to_string),
        prompt: text("prompt").map(|p| hook_wire::clip(p, hook_wire::PROMPT_MAX)),
        tool_name: text("tool_name").map(|t| hook_wire::clip(t, hook_wire::TOOL_MAX)),
        // Komunikat trafia do powiadomienia i tablicy: jedna linia.
        message: text("message").map(|m| {
            let line = m.split_whitespace().collect::<Vec<_>>().join(" ");
            hook_wire::clip(&line, hook_wire::MESSAGE_MAX)
        }),
    })
}

/// Narzędzia, którymi agent pyta użytkownika (czekanie, nie praca), z powodem.
fn asks_user(tool: &str) -> Option<&'static str> {
    match tool {
        "AskUserQuestion" => Some("pytanie"),
        "ExitPlanMode" => Some("plan do akceptacji"),
        _ => None,
    }
}

/// Faza sesji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Working,
    /// Agent czeka na użytkownika (zgoda, pytanie); to nie jest praca.
    Waiting,
    Done,
}

impl Phase {
    /// Nazwa fazy w tablicy sesji (`working`, `waiting`, `done`).
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Working => "working",
            Phase::Waiting => "waiting",
            Phase::Done => "done",
        }
    }
}

/// Sesja na tablicy sesji: pracuje, czeka albo skończyła się i nie jest przejrzana.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardSession {
    pub agent: String,
    pub session_id: String,
    pub cwd: String,
    pub phase: Phase,
    /// Wejście w bieżącą fazę (ms).
    pub phase_at: i64,
    /// Użytkownik widział czekanie albo koniec („Przejrzane”, kolejny prompt).
    pub seen: bool,
    /// Na co czeka sesja; przy końcu tylko [`TIMEOUT_REASON`].
    pub reason: Option<String>,
    /// Pracujący subagenci.
    pub subagents: usize,
}

impl BoardSession {
    /// Sesja zakończona przez brak zdarzeń, nie przez `Stop`.
    pub fn timed_out(&self) -> bool {
        self.phase == Phase::Done && self.reason.as_deref() == Some(TIMEOUT_REASON)
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub agent: String,
    pub cwd: String,
    pub phase: Phase,
    /// Początek bieżącej pracy (ms); zaczyna się od nowa po czekaniu.
    pub since: i64,
    /// Ostatnie zdarzenie (ms).
    pub last: i64,
    /// Subagenci, którzy zaczęli pracę i jeszcze się nie zakończyli, z czasem
    /// ich ostatniego zdarzenia (zgubiony `SubagentStop` nie trzyma sesji w pracy).
    pub subagents: BTreeMap<String, i64>,
    /// Rozpoczęte i niezakończone wywołania narzędzi (`PreToolUse` bez
    /// `PostToolUse`) według wykonawcy: `""` to agent główny, inaczej `agent_id`.
    pub tools: BTreeMap<String, u32>,
    /// Agent główny zakończył turę (`Stop`), ale subagenci jeszcze pracują.
    pub stopped: bool,
    /// Pierwszy prompt sesji (do 500 znaków), tylko w pamięci; pod asystenta (A4),
    /// zapisywany tylko przy [`Sessions::keep_prompts`].
    pub first_prompt: Option<String>,
    /// Wejście w bieżącą fazę (ms).
    pub phase_at: i64,
    /// Czekanie albo koniec widziane przez użytkownika; praca jest zawsze „widziana”.
    pub seen: bool,
    /// Na co czeka sesja (zgoda na narzędzie, pytanie) albo [`TIMEOUT_REASON`].
    pub reason: Option<String>,
}

impl Session {
    /// Zmiana fazy: nowa chwila wejścia, czekanie i koniec jako nieprzejrzane,
    /// powód od nowa. Ta sama faza niczego nie zmienia.
    fn enter(&mut self, phase: Phase, now: i64) {
        if self.phase == phase {
            return;
        }
        self.phase = phase;
        self.phase_at = now;
        self.seen = phase == Phase::Working;
        self.reason = None;
    }

    /// Czekanie na użytkownika; pierwszy znany powód zostaje (powiadomienie po
    /// `PermissionRequest` nie zastępuje nazwy narzędzia).
    fn wait(&mut self, reason: String, now: i64) {
        self.enter(Phase::Waiting, now);
        if self.reason.is_none() {
            self.reason = Some(reason);
        }
    }

    /// Trwa jakieś wywołanie narzędzia (długi build, testy, subagent z `Task`).
    fn in_tool(&self) -> bool {
        self.tools.values().any(|&n| n > 0)
    }

    /// Usuwa subagentów bez zdarzeń dłużej niż [`SUBAGENT_SILENCE`] (albo
    /// [`TOOL_SILENCE`], gdy subagent jest w trakcie wywołania narzędzia).
    fn prune_subagents(&mut self, now: i64) {
        let tools = &mut self.tools;
        self.subagents.retain(|id, last| {
            let busy = tools.get(id).is_some_and(|&n| n > 0);
            let limit = if busy { TOOL_SILENCE } else { SUBAGENT_SILENCE };
            let keep = now >= *last && now - *last <= limit;
            if !keep {
                tools.remove(id);
            }
            keep
        });
    }
}

/// Wpis diagnostyki: ostatnie zdarzenia hooków (bez treści promptów).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub at: i64,
    pub agent: String,
    pub event: String,
    /// Początek identyfikatora sesji.
    pub session: String,
    pub cwd: String,
    /// Subagent, notification_type albo faza po zdarzeniu.
    pub detail: String,
}

/// Wszystkie sesje agentów znane z hooków.
#[derive(Default)]
pub struct Sessions {
    map: HashMap<(String, String), Session>,
    recent: VecDeque<Recent>,
    /// Zapamiętywanie pierwszego promptu sesji; ustawiane z `ai_assign_suggest`
    /// (A4, [`Sessions::set_keep_prompts`]), domyślnie wyłączone.
    pub keep_prompts: bool,
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Working => "pracuje",
        Phase::Waiting => "czeka",
        Phase::Done => "koniec",
    }
}

impl Sessions {
    /// Włącza albo wyłącza zapamiętywanie pierwszego promptu; wyłączenie czyści
    /// prompty zapamiętane w trwających sesjach.
    pub fn set_keep_prompts(&mut self, keep: bool) {
        self.keep_prompts = keep;
        if !keep {
            for s in self.map.values_mut() {
                s.first_prompt = None;
            }
        }
    }

    /// Stosuje zdarzenie. Zwraca `true`, gdy zmieniło się to, co widać w liście
    /// pracujących agentów (faza, folder, pojawienie się albo usunięcie sesji);
    /// wtedy pętla agentów budzi się od razu.
    pub fn apply(&mut self, ev: &HookEvent, now: i64) -> bool {
        let key = (ev.agent.clone(), ev.session_id.clone());
        let before = self.visible(&key);
        let mut detail = ev
            .notification_type
            .clone()
            .or_else(|| ev.agent_id.clone())
            .unwrap_or_default();
        if ev.event == "SessionEnd" {
            self.map.remove(&key);
        } else {
            if !self.map.contains_key(&key) {
                self.make_room(now);
            }
            let keep_prompts = self.keep_prompts;
            let s = self.map.entry(key.clone()).or_insert_with(|| Session {
                agent: ev.agent.clone(),
                cwd: ev.cwd.clone(),
                phase: Phase::Done,
                since: now,
                last: now,
                subagents: BTreeMap::new(),
                tools: BTreeMap::new(),
                stopped: false,
                first_prompt: None,
                phase_at: now,
                // Nowa sesja bez tury nie ma czego przeglądać.
                seen: true,
                reason: None,
            });
            s.last = now;
            if !ev.cwd.is_empty() {
                s.cwd = ev.cwd.clone();
            }
            Self::transition(s, ev, now, keep_prompts);
            if detail.is_empty() {
                detail = phase_name(s.phase).into();
            }
        }
        self.recent.push_back(Recent {
            at: now,
            agent: ev.agent.clone(),
            event: ev.event.clone(),
            session: ev.session_id.chars().take(8).collect(),
            cwd: ev.cwd.clone(),
            detail,
        });
        while self.recent.len() > RECENT {
            self.recent.pop_front();
        }
        before != self.visible(&key)
    }

    fn transition(s: &mut Session, ev: &HookEvent, now: i64, keep_prompts: bool) {
        let work = |s: &mut Session| {
            if s.phase != Phase::Working {
                s.since = now;
            }
            s.enter(Phase::Working, now);
        };
        // Powód czekania: komunikat agenta albo opis rodzaju powiadomienia.
        let told = |fallback: &str| ev.message.clone().unwrap_or_else(|| fallback.into());
        // Wykonawca zdarzenia: subagent (`agent_id`) albo agent główny (`""`).
        let actor = ev.agent_id.clone().unwrap_or_default();
        if let Some(id) = &ev.agent_id {
            if ev.event != "SubagentStop" {
                s.subagents.insert(id.clone(), now);
            }
        }
        match ev.event.as_str() {
            "UserPromptSubmit" => {
                work(s);
                s.stopped = false;
                if keep_prompts && s.first_prompt.is_none() {
                    s.first_prompt = ev.prompt.clone();
                }
            }
            "PreToolUse" => {
                // Pytanie albo plan do akceptacji od agenta głównego: czeka na
                // użytkownika do `PostToolUse`.
                let asks = ev
                    .tool_name
                    .as_deref()
                    .filter(|_| actor.is_empty())
                    .and_then(asks_user);
                *s.tools.entry(actor).or_default() += 1;
                match asks {
                    Some(reason) => s.wait(reason.into(), now),
                    None => work(s),
                }
            }
            "PostToolUse" => {
                if let Some(n) = s.tools.get_mut(&actor) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        s.tools.remove(&actor);
                    }
                }
                work(s);
            }
            "SubagentStart" => work(s),
            "SubagentStop" => {
                if let Some(id) = &ev.agent_id {
                    s.subagents.remove(id);
                    s.tools.remove(id);
                }
            }
            "PermissionRequest" => {
                let reason = ev
                    .tool_name
                    .as_deref()
                    .map_or_else(|| "prośba o zgodę".into(), |t| format!("zgoda: {t}"));
                s.wait(reason, now);
            }
            "Notification" => match ev.notification_type.as_deref() {
                Some("permission_prompt") => s.wait(told("prośba o zgodę"), now),
                Some("agent_needs_input" | "elicitation_dialog") => s.wait(told("pytanie"), now),
                // Claude przypomina o bezczynności także po zakończonej turze.
                Some("idle_prompt") if s.phase == Phase::Working => {
                    s.wait(told("czeka na odpowiedź"), now)
                }
                _ => {}
            },
            "Stop" => {
                // Koniec tury: wywołania narzędzi agenta głównego są zakończone
                // (także te bez `PostToolUse`, np. nieudane), a subagenci bez
                // zdarzeń od dawna nie trzymają już sesji w pracy.
                s.tools.remove("");
                s.prune_subagents(now);
                if s.subagents.is_empty() {
                    s.enter(Phase::Done, now);
                    s.stopped = false;
                } else {
                    s.stopped = true;
                }
            }
            "Interrupt" => {
                s.subagents.clear();
                s.tools.clear();
                s.stopped = false;
                s.enter(Phase::Done, now);
                // Przerwał użytkownik: widział, jak sesja się kończy.
                s.seen = true;
            }
            // SessionStart zakłada sesję (bezczynną); pozostałe zdarzenia tylko
            // odświeżają „ostatnie zdarzenie”.
            _ => {}
        }
    }

    /// Faza i folder sesji widoczne w liście (praca) albo `None`.
    fn visible(&self, key: &(String, String)) -> Option<(String, i64)> {
        self.map
            .get(key)
            .filter(|s| s.phase == Phase::Working)
            .map(|s| (s.cwd.clone(), s.since))
    }

    /// Zwalnia miejsce na nową sesję: najpierw zakończone, potem najstarsze.
    fn make_room(&mut self, now: i64) {
        self.expire(now);
        while self.map.len() >= MAX_SESSIONS {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, s)| (s.phase != Phase::Done, s.last))
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => self.map.remove(&k),
                None => break,
            };
        }
    }

    /// Timeout braku zdarzeń: praca kończy się po [`SILENCE`] (po [`TOOL_SILENCE`],
    /// gdy trwa wywołanie narzędzia), czekanie na użytkownika po [`WAIT_SILENCE`],
    /// sesja po `Stop` czekająca tylko na subagentów kończy się, gdy ich zabraknie
    /// (zgubiony `SubagentStop`), a zakończone sesje są zapominane po [`FORGET`].
    /// Koniec z braku zdarzeń ma powód [`TIMEOUT_REASON`]. Zwraca `true`, gdy
    /// ubyła pracująca sesja.
    pub fn expire(&mut self, now: i64) -> bool {
        let mut changed = false;
        for s in self.map.values_mut() {
            if s.phase == Phase::Done {
                continue;
            }
            let limit = match (s.phase, s.in_tool()) {
                (Phase::Waiting, _) => WAIT_SILENCE,
                (_, true) => TOOL_SILENCE,
                _ => SILENCE,
            };
            let silent = now - s.last > limit || now < s.last;
            let orphaned = s.stopped && {
                s.prune_subagents(now);
                s.subagents.is_empty() && now - s.last > SUBAGENT_SILENCE
            };
            if silent || orphaned {
                changed |= s.phase == Phase::Working;
                // Koniec liczy się od ostatniego zdarzenia, nie od chwili wykrycia ciszy.
                s.enter(Phase::Done, s.last.min(now));
                // Sesja po `Stop`, która czekała tylko na subagentów, skończyła
                // turę normalnie; cisza to raczej ubity proces.
                if silent && !orphaned {
                    s.reason = Some(TIMEOUT_REASON.into());
                }
                s.subagents.clear();
                s.tools.clear();
                s.stopped = false;
            }
        }
        self.map
            .retain(|_, s| s.phase != Phase::Done || (now >= s.last && now - s.last <= FORGET));
        changed
    }

    /// Pracujące sesje jako agenci (jedna sesja = jeden agent, subagenci w niej).
    pub fn working(&self) -> Vec<agents::Agent> {
        let mut list: Vec<agents::Agent> = self
            .map
            .iter()
            .filter(|(_, s)| s.phase == Phase::Working)
            .map(|((_, session_id), s)| {
                let name = agents::folder(&s.cwd);
                agents::Agent {
                    session_id: session_id.clone(),
                    first_prompt: s.first_prompt.clone(),
                    kind: s.agent.clone(),
                    name: if name.is_empty() {
                        "sesja".into()
                    } else {
                        name
                    },
                    cwd: s.cwd.clone(),
                    since: s.since,
                    ..agents::Agent::default()
                }
            })
            .collect();
        list.sort_by(|a, b| (a.since, &a.kind, &a.cwd).cmp(&(b.since, &b.kind, &b.cwd)));
        list
    }

    /// Tablica sesji: pracujące, czekające i zakończone nieprzejrzane (sesja bez
    /// tury i przejrzany koniec nie są pokazywane). Kolejność: czekające od
    /// najdłużej czekającej, potem pracujące, potem zakończone od najstarszej.
    pub fn board(&self) -> Vec<BoardSession> {
        let rank = |p: Phase| match p {
            Phase::Waiting => 0,
            Phase::Working => 1,
            Phase::Done => 2,
        };
        let mut list: Vec<BoardSession> = self
            .map
            .iter()
            .filter(|(_, s)| s.phase != Phase::Done || !s.seen)
            .map(|((_, session_id), s)| BoardSession {
                agent: s.agent.clone(),
                session_id: session_id.clone(),
                cwd: s.cwd.clone(),
                phase: s.phase,
                phase_at: s.phase_at,
                seen: s.seen,
                reason: s.reason.clone(),
                subagents: s.subagents.len(),
            })
            .collect();
        list.sort_by(|a, b| {
            (rank(a.phase), a.phase_at, &a.agent, &a.session_id).cmp(&(
                rank(b.phase),
                b.phase_at,
                &b.agent,
                &b.session_id,
            ))
        });
        list
    }

    /// Faza sesji i „przejrzane” (budzenie pętli tablicy po zmianie).
    pub fn board_state(&self, agent: &str, id: &str) -> Option<(Phase, bool)> {
        self.session(agent, id).map(|s| (s.phase, s.seen))
    }

    /// „Przejrzane” dla czekającej albo zakończonej sesji; `true`, gdy coś zmieniło.
    pub fn mark_seen(&mut self, agent: &str, id: &str) -> bool {
        match self.map.get_mut(&(agent.to_string(), id.to_string())) {
            Some(s) if s.phase != Phase::Working && !s.seen => {
                s.seen = true;
                true
            }
            _ => false,
        }
    }

    /// „Przejrzane” dla wszystkich czekających i zakończonych sesji.
    pub fn mark_all_seen(&mut self) -> bool {
        let mut changed = false;
        for s in self.map.values_mut() {
            if s.phase != Phase::Working && !s.seen {
                s.seen = true;
                changed = true;
            }
        }
        changed
    }

    pub fn session(&self, agent: &str, id: &str) -> Option<&Session> {
        self.map.get(&(agent.to_string(), id.to_string()))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.recent.clear();
    }

    /// Ostatnie zdarzenia, od najnowszego.
    pub fn recent(&self) -> Vec<Recent> {
        self.recent.iter().rev().cloned().collect()
    }
}

// --- zaufanie hooków Codexa (`[hooks.state]` w config.toml) ---

/// Zaufanie do hooków aplikacji w Codexie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// Hooków nie ma w `hooks.json` (nic do zatwierdzania).
    None,
    /// Żadne zdarzenie nie ma zatwierdzonego hooka: wymaga `/hooks` w Codexie.
    Untrusted,
    Partial,
    Trusted,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Trust::None => "none",
            Trust::Untrusted => "untrusted",
            Trust::Partial => "partial",
            Trust::Trusted => "trusted",
        }
    }
}

/// Cudzysłowiony napis z początku tekstu (po białych znakach) i reszta.
fn quoted(text: &str) -> Option<(String, &str)> {
    let rest = text.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    out.push(n);
                }
            }
            '"' => return Some((out, &rest[i + 1..])),
            c => out.push(c),
        }
    }
    None
}

/// Wartość `trusted_hash` z linii (`trusted_hash = "sha256:…"`), jeśli niepusta.
fn trusted_hash(line: &str) -> Option<String> {
    let at = line.find("trusted_hash")?;
    let after = line[at + "trusted_hash".len()..].trim_start();
    let value = quoted(after.strip_prefix('=')?)?.0;
    (!value.trim().is_empty()).then_some(value)
}

/// Wpisy sekcji `[hooks.state]` z `config.toml` jako (klucz, czy ma `trusted_hash`).
/// Lekki parser zamiast pełnego TOML-a: obsługuje tabele `[hooks.state."klucz"]`
/// z polem `trusted_hash` oraz wpisy `"klucz" = { trusted_hash = "…" }` pod
/// `[hooks.state]`.
pub fn parse_hook_state(text: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    let mut add = |key: String, trusted: bool| {
        if let Some(e) = entries.iter_mut().find(|e| e.0 == key) {
            e.1 |= trusted;
        } else {
            entries.push((key, trusted));
        }
    };
    // `Some(None)`: w `[hooks.state]` (wpisy w liniach), `Some(Some(k))`: w tabeli klucza.
    let mut current: Option<Option<String>> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let header = line
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            current = match header.strip_prefix("hooks.state") {
                Some("") => Some(None),
                Some(rest) if rest.starts_with('.') => {
                    let key = rest.trim_start_matches('.').trim();
                    let key = quoted(key).map(|q| q.0).unwrap_or_else(|| key.to_string());
                    add(key.clone(), false);
                    Some(Some(key))
                }
                _ => None,
            };
            continue;
        }
        match &current {
            Some(Some(key)) => {
                if trusted_hash(line).is_some() {
                    add(key.clone(), true);
                }
            }
            Some(None) => {
                if let Some((key, _)) = quoted(line) {
                    add(key, trusted_hash(line).is_some());
                } else if let Some((key, rest)) = line.split_once(".trusted_hash") {
                    add(
                        key.trim().trim_matches('"').to_string(),
                        trusted_hash(&format!("trusted_hash{rest}")).is_some(),
                    );
                }
            }
            None => {}
        }
    }
    entries
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Zaufanie do hooków aplikacji: zdarzenie liczy się jako zatwierdzone, gdy klucz
/// wpisu `hooks.state` wskazuje `hooks.json` i to zdarzenie (w dowolnym zapisie,
/// np. `PreToolUse` albo `pre_tool_use`) i wpis ma `trusted_hash`.
pub fn codex_trust(registered: Registration, config: &str) -> Trust {
    if registered == Registration::Missing {
        return Trust::None;
    }
    let states = parse_hook_state(config);
    let trusted = hooks_config::CODEX_EVENTS
        .iter()
        .filter(|event| {
            let token = squash(event);
            states.iter().any(|(key, ok)| {
                let key = squash(key);
                *ok && key.contains("hooksjson") && key.contains(&token)
            })
        })
        .count();
    match trusted {
        0 => Trust::Untrusted,
        n if n == hooks_config::CODEX_EVENTS.len() => Trust::Trusted,
        _ => Trust::Partial,
    }
}

// --- stan aplikacji i rejestracja ---

#[derive(Default)]
struct Registered {
    /// Ostatnio udanie zastosowany stan wyłącznika (`None`: jeszcze nie albo błąd).
    applied: Option<bool>,
    last_try: i64,
    error: String,
}

#[derive(Default)]
pub struct State {
    registered: Mutex<Registered>,
    /// Jedna rejestracja naraz (start, zapis ustawień, kopia i ponowienie działają
    /// w różnych wątkach i piszą te same pliki).
    io: Mutex<()>,
}

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

fn exe_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(hooks_config::HOOK_EXE)))
        .filter(|p| p.is_file())
}

/// Rejestruje albo usuwa hooki w plikach agentów; błędy obu agentów w jednym tekście.
fn apply_dirs(dirs: &Dirs, exe: Option<&Path>, enabled: bool) -> Result<(), String> {
    if enabled && exe.is_none() {
        return Err(format!(
            "Brak programu {} obok aplikacji. Zainstaluj aplikację ponownie.",
            hooks_config::HOOK_EXE
        ));
    }
    let (claude, codex) = hooks_config::sync(dirs, enabled.then_some(exe).flatten());
    let errors: Vec<String> = [("Claude Code", claude), ("Codex", codex)]
        .into_iter()
        .filter(|(_, o)| !o.error.is_empty())
        .map(|(name, o)| format!("{name}: {}", o.error))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" "))
    }
}

/// Stosuje ustawienie `agent_hooks` (start, zmiana wyłącznika, przywrócenie kopii).
/// Pliki agentów (katalog domowy może być wolny albo sieciowy) czytamy i piszemy w
/// osobnym wątku, żeby nie blokować wątku głównego i okna.
pub fn sync(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || attempt(&app, db::now(), false));
}

/// Ponowne sprawdzenie rejestracji co [`RESYNC`] z pętli agentów (nowa ścieżka exe
/// po aktualizacji, plik nadpisany przez agenta albo błąd przy poprzedniej próbie).
pub fn retry(app: &tauri::AppHandle, now: i64) {
    attempt(app, now, true);
}

fn attempt(app: &tauri::AppHandle, now: i64, periodic: bool) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let Ok(_io) = state.io.lock() else {
        return;
    };
    // Ustawienie czytane pod blokadą: przy szybkim przełączaniu wygrywa ostatnie.
    let enabled = settings::current(app).agent_hooks;
    {
        let Ok(mut reg) = state.registered.lock() else {
            return;
        };
        let due = if periodic {
            (enabled || reg.applied != Some(false))
                && (now < reg.last_try || now - reg.last_try >= RESYNC)
        } else {
            true
        };
        if !due {
            return;
        }
        reg.last_try = now;
    }
    let result = apply_dirs(&Dirs::detect(), exe_path().as_deref(), enabled);
    if !enabled {
        // Wyłączone: sesje z hooków znikają z listy.
        if let Some(agents) = app.try_state::<agents::State>() {
            agents.clear_sessions();
        }
    }
    if let Ok(mut reg) = state.registered.lock() {
        reg.applied = result.is_ok().then_some(enabled);
        reg.error = result.err().unwrap_or_default();
    };
}

/// Zdarzenie z potoku: do maszyny stanów, o ile hooki są włączone.
fn on_event(app: &tauri::AppHandle, body: &[u8]) -> Value {
    if let Some(ev) = parse(body) {
        let settings = settings::current(app);
        if settings.agent_hooks {
            if let Some(agents) = app.try_state::<agents::State>() {
                // Pierwszy prompt sesji tylko dla asystenta przypisań (A4).
                agents.set_keep_prompts(crate::assign::keep_prompts(&settings));
                agents.apply_hook(&ev, db::now());
            }
        }
    }
    json!({ "ok": true })
}

/// Wątek nasłuchu na potoku; wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) =
            crate::browser::pipe::serve(hook_wire::PIPE, move |body| on_event(&app, body))
        {
            eprintln!("Hooki agentów: potok niedostępny ({e}).");
        }
    });
}

// --- status do Ustawień → Agenci ---

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    /// Folder konfiguracji agenta istnieje (agent jest zainstalowany).
    pub found: bool,
    pub file: String,
    /// `missing`, `stale` albo `current`.
    pub registration: String,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// Program hooka leży obok aplikacji.
    pub exe_found: bool,
    /// Błąd ostatniej rejestracji.
    pub error: String,
    pub claude: AgentStatus,
    pub codex: AgentStatus,
    /// `none`, `untrusted`, `partial` albo `trusted`.
    pub codex_trust: String,
    pub events: Vec<Recent>,
}

fn agent_status(
    dir: Option<&Path>,
    file: Option<PathBuf>,
    events: &[&str],
    command: &str,
) -> (AgentStatus, Registration) {
    let shown = file
        .as_ref()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (found, reg, error) = hooks_config::inspect(dir, file, events, command);
    (
        AgentStatus {
            found,
            file: shown,
            registration: reg.as_str().into(),
            error,
        },
        reg,
    )
}

/// Stan rejestracji w plikach agentów (`dirs`, `exe` wstrzykiwane).
pub fn status_for(
    dirs: &Dirs,
    exe: Option<&Path>,
    enabled: bool,
    error: String,
    events: Vec<Recent>,
) -> Status {
    let exe_for_cmd = exe.unwrap_or(Path::new(hooks_config::HOOK_EXE));
    let (claude, _) = agent_status(
        dirs.claude.as_deref(),
        dirs.claude_file(),
        &hooks_config::CLAUDE_EVENTS,
        &hooks_config::hook_command(exe_for_cmd, "claude"),
    );
    let (codex, reg) = agent_status(
        dirs.codex.as_deref(),
        dirs.codex_file(),
        &hooks_config::CODEX_EVENTS,
        &hooks_config::hook_command(exe_for_cmd, "codex"),
    );
    let config = dirs
        .codex_config()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    Status {
        enabled,
        exe_found: exe.is_some(),
        error,
        claude,
        codex,
        codex_trust: codex_trust(reg, &config).as_str().into(),
        events,
    }
}

/// Asynchroniczna: czyta pliki agentów poza wątkiem głównym (Ustawienia pytają co 5 s).
#[tauri::command(async)]
pub fn agent_hooks_status(app: tauri::AppHandle) -> Status {
    let enabled = settings::current(&app).agent_hooks;
    let error = app
        .try_state::<State>()
        .and_then(|s| s.registered.lock().ok().map(|r| r.error.clone()))
        .unwrap_or_default();
    let events = app
        .try_state::<agents::State>()
        .map(|s| s.recent_hooks())
        .unwrap_or_default();
    status_for(
        &Dirs::detect(),
        exe_path().as_deref(),
        enabled,
        error,
        events,
    )
}

#[cfg(test)]
mod tests;
