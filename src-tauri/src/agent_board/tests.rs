use super::*;
use crate::agents::Match;
use crate::hooks::{HookEvent, Sessions};

const T0: i64 = 1_000_000;

fn ev(id: &str, event: &str) -> HookEvent {
    HookEvent {
        agent: "claude".into(),
        session_id: id.into(),
        cwd: format!("C:\\Projekty\\{id}"),
        event: event.into(),
        ..HookEvent::default()
    }
}

fn board(s: &Sessions) -> Vec<Entry> {
    entries(s.board())
}

#[test]
fn entries_carry_folder_name_phase_and_reason() {
    let mut s = Sessions::default();
    s.apply(&ev("wms", "UserPromptSubmit"), T0);
    s.apply(
        &HookEvent {
            tool_name: Some("Bash".into()),
            ..ev("wms", "PermissionRequest")
        },
        T0 + 1_000,
    );
    let list = board(&s);
    assert_eq!(list.len(), 1);
    let e = &list[0];
    assert_eq!(
        (e.agent.kind.as_str(), e.agent.name.as_str(), e.id.as_str()),
        ("claude", "wms", "wms")
    );
    assert_eq!(
        (e.phase, e.phase_at, e.seen),
        ("waiting", T0 + 1_000, false)
    );
    assert_eq!(e.reason.as_deref(), Some("zgoda: Bash"));
    assert!(!e.timed_out);
    let json = serde_json::to_value(e).unwrap();
    assert_eq!(json["kind"], "claude");
    assert_eq!(json["phaseAt"], T0 + 1_000);
    assert_eq!(json["timedOut"], false);
    assert!(json.get("sessionId").is_none());
}

#[test]
fn counts_and_tray_line_skip_seen_and_working() {
    let mut s = Sessions::default();
    for id in ["a", "b", "c", "d"] {
        s.apply(&ev(id, "UserPromptSubmit"), T0);
    }
    s.apply(&ev("a", "PermissionRequest"), T0 + 1);
    s.apply(&ev("b", "PermissionRequest"), T0 + 1);
    s.apply(&ev("c", "Stop"), T0 + 1);
    s.mark_seen("claude", "b");
    let c = counts(&board(&s));
    assert_eq!(
        c,
        Counts {
            waiting: 1,
            done: 1
        }
    );
    assert_eq!(
        tray_line(c).as_deref(),
        Some("Agenci: czeka 1 · skończone 1")
    );
    assert_eq!(
        tray_line(Counts {
            waiting: 0,
            done: 2
        })
        .as_deref(),
        Some("Agenci: skończone 2")
    );
    assert_eq!(tray_line(Counts::default()), None);
}

#[test]
fn memory_reports_each_waiting_and_end_once() {
    let mut s = Sessions::default();
    let mut m = Memory::default();
    s.apply(&ev("a", "UserPromptSubmit"), T0);
    let (changed, fresh) = m.update(board(&s));
    assert!(changed && fresh.is_empty(), "praca nie jest powiadamiana");
    s.apply(&ev("a", "PermissionRequest"), T0 + 1_000);
    let (changed, fresh) = m.update(board(&s));
    assert!(changed);
    assert_eq!(fresh.len(), 1);
    let (changed, fresh) = m.update(board(&s));
    assert!(!changed && fresh.is_empty());
    // Zgoda, praca, koniec: nowy koniec jest zgłaszany.
    s.apply(&ev("a", "PostToolUse"), T0 + 2_000);
    m.update(board(&s));
    s.apply(&ev("a", "Stop"), T0 + 3_000);
    let (_, fresh) = m.update(board(&s));
    assert_eq!(fresh[0].phase, "done");
    // Kolejne czekanie tej samej sesji to nowe powiadomienie.
    s.apply(&ev("a", "UserPromptSubmit"), T0 + 4_000);
    m.update(board(&s));
    s.apply(&ev("a", "PermissionRequest"), T0 + 5_000);
    assert_eq!(m.update(board(&s)).1.len(), 1);
}

#[test]
fn timed_out_and_seen_sessions_are_not_reported() {
    let mut s = Sessions::default();
    let mut m = Memory::default();
    s.apply(&ev("a", "UserPromptSubmit"), T0);
    m.update(board(&s));
    s.expire(T0 + crate::hooks::SILENCE + 1);
    let (changed, fresh) = m.update(board(&s));
    assert!(changed);
    assert!(fresh.is_empty());
    assert!(m.entries()[0].timed_out);

    let mut s = Sessions::default();
    s.apply(&ev("b", "UserPromptSubmit"), T0);
    s.apply(&ev("b", "PermissionRequest"), T0 + 1);
    s.mark_seen("claude", "b");
    assert!(Memory::default().update(board(&s)).1.is_empty());
}

#[test]
fn memory_mark_seen_drops_finished_and_keeps_waiting() {
    let mut s = Sessions::default();
    let mut m = Memory::default();
    for id in ["a", "b", "c"] {
        s.apply(&ev(id, "UserPromptSubmit"), T0);
    }
    s.apply(&ev("a", "Stop"), T0 + 1);
    s.apply(&ev("b", "PermissionRequest"), T0 + 1);
    m.update(board(&s));
    m.mark_seen("claude", Some("a"));
    let ids: Vec<_> = m.entries().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["b", "c"]);
    m.mark_seen("", None);
    let waiting = m.entries().iter().find(|e| e.id == "b").unwrap();
    assert!(waiting.seen);
    // Pracująca sesja nie zmienia się.
    assert!(m.entries().iter().find(|e| e.id == "c").unwrap().seen);
    assert_eq!(counts(m.entries()), Counts::default());
}

#[test]
fn stale_loop_list_does_not_undo_seen() {
    let mut s = Sessions::default();
    let mut m = Memory::default();
    for id in ["a", "b"] {
        s.apply(&ev(id, "UserPromptSubmit"), T0);
    }
    s.apply(&ev("a", "Stop"), T0 + 1);
    s.apply(&ev("b", "PermissionRequest"), T0 + 1);
    m.update(board(&s));
    // Pętla odczytała tablicę, potem użytkownik kliknął „Przejrzane”.
    let stale = board(&s);
    s.mark_all_seen();
    m.mark_seen("", None);
    let (changed, fresh) = m.update(stale);
    assert!(!changed, "{:?}", m.entries());
    assert!(fresh.is_empty());
    assert_eq!(counts(m.entries()), Counts::default());
    // Kolejna (aktualna) lista z pętli niczego nie zmienia.
    assert!(!m.update(board(&s)).0);
}

#[test]
fn timed_out_end_is_listed_but_not_counted() {
    let mut s = Sessions::default();
    s.apply(&ev("a", "UserPromptSubmit"), T0);
    s.expire(T0 + crate::hooks::SILENCE + 1);
    let list = board(&s);
    assert_eq!(list.len(), 1);
    assert_eq!(counts(&list), Counts::default());
}

fn entry(id: &str, phase: &'static str, reason: Option<&str>) -> Entry {
    Entry {
        agent: Agent {
            kind: "claude".into(),
            name: id.into(),
            ..Agent::default()
        },
        id: id.into(),
        phase,
        phase_at: T0,
        seen: false,
        reason: reason.map(str::to_string),
        subagents: 0,
        timed_out: false,
    }
}

#[test]
fn message_follows_settings_and_groups_many_sessions() {
    let on = AppSettings {
        agent_notify_waiting: true,
        agent_notify_done: true,
        ..AppSettings::default()
    };
    let wait = entry("wms", "waiting", Some("zgoda: Bash"));
    let m = message(std::slice::from_ref(&wait), &on).unwrap();
    assert_eq!(m.title, "Claude czeka na Ciebie");
    assert_eq!(m.body, "wms · zgoda: Bash");
    assert_eq!(m.open.view, Some("agents"));
    assert!(m.actions.is_empty());

    let mut done = entry("api", "done", None);
    done.agent.kind = "codex".into();
    done.agent.task = Some(Match {
        task_id: Some(1),
        title: "Eksport do Excela".into(),
        key: None,
        source: "folder".into(),
        done: false,
    });
    let m = message(std::slice::from_ref(&done), &on).unwrap();
    assert_eq!(m.title, "Codex skończył pracę");
    assert_eq!(m.body, "api · Eksport do Excela");

    let many = [
        wait.clone(),
        done.clone(),
        entry("c", "waiting", None),
        entry("d", "waiting", None),
    ];
    let m = message(&many, &on).unwrap();
    assert_eq!(m.title, "Sesje agentów: czeka 3, skończone 1");
    assert_eq!(m.body, "wms, api · Eksport do Excela, c, i 1 więcej");

    // Domyślnie: czekanie tak, koniec nie.
    let defaults = AppSettings::default();
    assert!(defaults.agent_notify_waiting && !defaults.agent_notify_done);
    assert!(message(std::slice::from_ref(&done), &defaults).is_none());
    let m = message(&[wait, done], &defaults).unwrap();
    assert_eq!(m.title, "Claude czeka na Ciebie");
    let off = AppSettings {
        agent_notify_waiting: false,
        ..AppSettings::default()
    };
    assert!(message(&many, &off).is_none());
}
