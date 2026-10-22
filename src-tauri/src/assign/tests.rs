//! Testy A4: dane wejściowe asystenta przypisań bez nazwisk, próg pewności,
//! pamięć po `session_id`, brak wywołania przy dopasowaniu heurystyką albo
//! wyłączonym przełączniku oraz baner w `suggest.rs`. Żaden test nie uruchamia
//! claude ani codex: `ai::run` działa na podstawionym `Runner`.
use super::*;
use crate::ai::{Cli, Config, Detected, Output, RunError, Runner, Spec, State as AiState};
use crate::ai_privacy::Dictionary;
use std::cell::Cell;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

const MIN: i64 = 60_000;
const NOW: i64 = 100 * MIN;

fn task(id: i64, title: &str, project: &str, key: &str) -> OpenTask {
    OpenTask {
        id,
        title: title.into(),
        project: project.into(),
        key: key.into(),
    }
}

fn tasks() -> Vec<OpenTask> {
    vec![
        task(1, "Eksport CSV", "WMS", "WMS-12"),
        task(2, "Rozmowa z Janem Kowalskim", "Zespół", ""),
        task(3, "Code review", "Foundation", "#77"),
    ]
}

fn claude_reply(structured: Value) -> String {
    json!({
        "type": "result", "subtype": "success", "is_error": false, "result": "",
        "structured_output": structured
    })
    .to_string()
}

/// Zapisane odpowiedzi `claude -p`; zapamiętuje polecenia.
struct Fake {
    reply: Box<dyn Fn(&Spec) -> Result<Output, RunError> + Send + Sync>,
    specs: StdMutex<Vec<Spec>>,
}
impl Fake {
    fn answering(structured: Value) -> Self {
        let text = claude_reply(structured);
        Self::new(move |_| {
            Ok(Output {
                success: true,
                stdout: text.clone(),
                stderr: String::new(),
            })
        })
    }
    fn new(reply: impl Fn(&Spec) -> Result<Output, RunError> + Send + Sync + 'static) -> Self {
        Self {
            reply: Box::new(reply),
            specs: StdMutex::new(Vec::new()),
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
        (self.reply)(spec)
    }
}

fn dict() -> Dictionary {
    let mut d = Dictionary::new();
    d.add_name("Jan Kowalski");
    d
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
        dir: std::env::temp_dir().join(format!("tt-assign-{}", std::process::id())),
    }
}

/// `ask` jak w aplikacji: `ai::run` (filtr osobowy, schemat, pamięć wywołań).
fn ask_with<'a>(
    state: &'a AiState,
    runner: &'a Fake,
    dict: &'a Dictionary,
) -> impl Fn(&str, &str, &Value) -> Result<Value, String> + 'a {
    let config = cfg();
    move |kind, prompt, schema| ai::run(state, runner, &config, kind, prompt, schema, dict, NOW)
}

fn agent(session: &str, cwd: &str, since: i64) -> Agent {
    Agent {
        kind: "claude".into(),
        name: agents::folder(cwd),
        cwd: cwd.into(),
        since,
        session_id: session.into(),
        ..Agent::default()
    }
}

// ---------------------------------------------------------------------------
// Dane wejściowe

#[test]
fn assign_input_has_folder_name_branch_prompt_and_open_tasks_without_paths() {
    let prompt = assign_prompt(
        "api",
        Some("feature/WMS-12-csv"),
        Some("  napraw eksport CSV  "),
        &tasks(),
    );
    let data = prompt.split("Dane:\n").nth(1).expect("dane");
    let value: Value = serde_json::from_str(data).unwrap();
    assert_eq!(
        value["session"],
        json!({"folder": "api", "branch": "feature/WMS-12-csv", "firstPrompt": "napraw eksport CSV"})
    );
    assert_eq!(value["openTasks"].as_array().unwrap().len(), 3);
    assert_eq!(
        value["openTasks"][0],
        json!({"id": 1, "title": "Eksport CSV", "project": "WMS", "key": "WMS-12"})
    );
    // Zadanie bez projektu i klucza ma tylko id i tytuł.
    let bare = assign_prompt("x", None, None, &[task(9, "Samo", "", "")]);
    let bare: Value = serde_json::from_str(bare.split("Dane:\n").nth(1).unwrap()).unwrap();
    assert_eq!(bare["session"], json!({"folder": "x"}));
    assert_eq!(bare["openTasks"][0], json!({"id": 9, "title": "Samo"}));
}

#[test]
fn long_prompt_is_clipped_to_500_chars() {
    let long = "ż".repeat(900);
    let prompt = assign_prompt("api", None, Some(&long), &tasks());
    let value: Value = serde_json::from_str(prompt.split("Dane:\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        value["session"]["firstPrompt"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        TEXT_MAX
    );
}

#[test]
fn open_tasks_skip_done_and_archived_and_are_capped() {
    let mut list: Vec<db::Task> = Vec::new();
    let conn = Connection::open_in_memory().unwrap();
    db::init(&conn).unwrap();
    for i in 0..(OPEN_MAX as i64 + 20) {
        conn.execute(
            "INSERT INTO tasks(title,created_at,done) VALUES(?1,0,?2)",
            rusqlite::params![format!("Zadanie {i}"), i64::from(i == 5)],
        )
        .unwrap();
    }
    conn.execute_batch(
        "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(100,'https://x.atlassian.net','WMS-3');
         INSERT INTO task_lifecycle(task_id,archived) VALUES(99,1);",
    )
    .unwrap();
    list.extend(db::snapshot(&conn).unwrap().tasks);
    let open = open_tasks(&list);
    assert_eq!(open.len(), OPEN_MAX);
    // Najnowsze pierwsze, bez ukończonego (id 6) i zarchiwizowanego (id 99).
    assert_eq!(open[0].id, 100);
    assert_eq!(open[0].key, "WMS-3");
    assert!(open.iter().all(|t| t.id != 99 && t.id != 6));
}

/// Kryterium A0: do modelu nie trafia żadne nazwisko ze słownika, a odpowiedź
/// wraca z prawdziwą nazwą.
#[test]
fn model_input_has_no_names_and_reply_gets_them_back() {
    let state = AiState::default();
    let runner = Fake::answering(json!({
        "taskId": 2, "confidence": 0.9, "reason": "Rozmowa dotyczy Osoba 1."
    }));
    let d = dict();
    let ask = ask_with(&state, &runner, &d);
    let open = vec![
        task(1, "Eksport CSV", "WMS", "WMS-12"),
        task(2, "Rozmowa z Janem Kowalskim", "Zespół", ""),
    ];
    let found = decide(
        &ask,
        "kowalski-api",
        Some("feature/jan-kowalski-csv"),
        Some("zapytaj Kowalskiego o eksport"),
        &open,
    )
    .expect("podpowiedź");
    assert_eq!(found.task_id, 2);
    let sent = runner.stdin();
    for name in ["Kowalsk", "Kowalski", "Janem"] {
        assert!(!sent.contains(name), "{name} w: {sent}");
    }
    assert!(sent.contains("Osoba 1"), "{sent}");
    // Zadania i klucze nadal są w danych.
    assert!(sent.contains("WMS-12"));
    // Odpowiedź przeszła przez `restore`.
    assert_eq!(found.reason, "Rozmowa dotyczy Jan Kowalski.");
}

#[test]
fn ts_input_has_no_names_and_options_without_leaks() {
    let state = AiState::default();
    let runner = Fake::answering(json!({
        "tsTaskId": "1103", "confidence": 0.8, "reason": "Pasuje do Osoba 1."
    }));
    let d = dict();
    let ask = ask_with(&state, &runner, &d);
    let query = TsQuery {
        title: "Spotkanie z Kowalskim".into(),
        project: "Foundation".into(),
        key: "WMS-5".into(),
        note: "Rozmowa o ofercie dla Jana Kowalskiego".into(),
    };
    let options = vec![
        TsOption {
            id: "1103".into(),
            name: "Code review".into(),
            project: "Foundation".into(),
        },
        TsOption {
            id: "1101".into(),
            name: "Rozwój".into(),
            project: "WMS".into(),
        },
    ];
    let pick = ts_pick(&ask, &query, &options).unwrap();
    let sent = runner.stdin();
    assert!(!sent.contains("Kowalsk"), "{sent}");
    assert!(sent.contains("dayNote"));
    assert!(sent.contains("1103") && sent.contains("Rozwój"));
    assert_eq!(pick.ts_task_id.as_deref(), Some("1103"));
    assert_eq!(pick.reason, "Pasuje do Jan Kowalski.");
}

// ---------------------------------------------------------------------------
// Próg pewności

#[test]
fn assign_answer_needs_confidence_of_at_least_06_and_a_listed_task() {
    let open = tasks();
    let ok = |id: Value, c: f64| {
        parse_assign(
            &json!({"taskId": id, "confidence": c, "reason": "r"}),
            &open,
        )
    };
    assert!(ok(json!(1), 0.59).is_none());
    assert_eq!(ok(json!(1), 0.6).map(|p| p.task_id), Some(1));
    assert_eq!(ok(json!(3), 1.0).map(|p| p.task_id), Some(3));
    // Pewność poza zakresem, `null`, zadanie spoza listy i śmieci nie dają podpowiedzi.
    assert!(ok(json!(1), 1.2).is_none());
    assert!(ok(Value::Null, 0.95).is_none());
    assert!(ok(json!(99), 0.95).is_none());
    assert!(ok(json!("1"), 0.95).is_none());
    assert!(parse_assign(&json!({"taskId": 1}), &open).is_none());
    assert!(parse_assign(&json!("tekst"), &open).is_none());
    let p = ok(json!(2), 0.75).unwrap();
    assert_eq!((p.confidence, p.reason.as_str()), (0.75, "r"));
}

#[test]
fn ts_answer_applies_the_same_threshold_and_list() {
    let options = vec![TsOption {
        id: "1103".into(),
        name: "Code review".into(),
        project: String::new(),
    }];
    let pick = |id: Value, c: f64| {
        parse_ts(
            &json!({"tsTaskId": id, "confidence": c, "reason": "bo tak"}),
            &options,
        )
    };
    let good = pick(json!("1103"), 0.6);
    assert_eq!(good.ts_task_id.as_deref(), Some("1103"));
    assert_eq!(good.reason, "bo tak");
    let low = pick(json!("1103"), 0.5);
    assert_eq!(low.ts_task_id, None);
    assert!(low.reason.starts_with(BAD_ANSWER) && low.reason.ends_with("bo tak"));
    assert_eq!(pick(Value::Null, 0.9).ts_task_id, None);
    let outside = pick(json!("9999"), 0.9);
    assert_eq!(outside.ts_task_id, None);
    assert!(outside.reason.contains("spoza listy"));
}

#[test]
fn schemas_accept_good_answers_and_reject_bad_ones() {
    let good = json!({"taskId": null, "confidence": 0.1, "reason": ""});
    assert!(ai::validate(&assign_schema(), &good).is_ok());
    assert!(ai::validate(
        &assign_schema(),
        &json!({"taskId": 3, "confidence": 0.7, "reason": "x"})
    )
    .is_ok());
    assert!(ai::validate(
        &assign_schema(),
        &json!({"taskId": "3", "confidence": 0.7, "reason": "x"})
    )
    .is_err());
    assert!(ai::validate(
        &assign_schema(),
        &json!({"taskId": 3, "confidence": 1.7, "reason": "x"})
    )
    .is_err());
    assert!(ai::validate(&assign_schema(), &json!({"taskId": 3, "confidence": 0.7})).is_err());
    assert!(ai::validate(
        &ts_schema(),
        &json!({"tsTaskId": null, "confidence": 0, "reason": ""})
    )
    .is_ok());
    assert!(ai::validate(
        &ts_schema(),
        &json!({"tsTaskId": 5, "confidence": 0, "reason": ""})
    )
    .is_err());
}

// ---------------------------------------------------------------------------
// Kiedy pytać

#[test]
fn model_is_asked_only_for_a_long_working_unmatched_session() {
    let s = AppSettings::default();
    let a = agent("s1", "C:\\Projekty\\api", NOW - 3 * MIN);
    assert!(wants(&a, &s, NOW));
    // Za krótko (reguła `suggest::MIN_WORK`).
    assert!(!wants(&agent("s1", "C:\\api", NOW - MIN), &s, NOW));
    assert!(wants(
        &agent("s1", "C:\\api", NOW - suggest::MIN_WORK),
        &s,
        NOW
    ));
    // Heurystyka dopasowała: bez wywołania.
    let mut matched = a.clone();
    matched.task = Some(Match {
        task_id: Some(1),
        title: "X".into(),
        key: None,
        source: "folder".into(),
        done: false,
    });
    assert!(!wants(&matched, &s, NOW));
    // Wyłączony przełącznik asystenta albo propozycji agentów.
    let off = AppSettings {
        ai_assign_suggest: false,
        ..AppSettings::default()
    };
    assert!(!wants(&a, &off, NOW));
    let no_agents = AppSettings {
        agent_suggest: false,
        ..AppSettings::default()
    };
    assert!(!wants(&a, &no_agents, NOW));
    // Wyciszony folder (także nadrzędny).
    let muted = AppSettings {
        agent_muted_folders: vec!["c:/projekty".into()],
        ..AppSettings::default()
    };
    assert!(!wants(&a, &muted, NOW));
    // Bez identyfikatora sesji i sygnały z okna albo przeglądarki nie pytają.
    assert!(!wants(&agent("", "C:\\api", NOW - 3 * MIN), &s, NOW));
    let mut window = a.clone();
    window.kind = "window".into();
    assert!(!wants(&window, &s, NOW));
}

#[test]
fn no_call_without_open_tasks() {
    let calls = Cell::new(0);
    let ask = |_: &str, _: &str, _: &Value| {
        calls.set(calls.get() + 1);
        Ok(json!({}))
    };
    assert!(decide(&ask, "api", None, None, &[]).is_none());
    assert_eq!(calls.get(), 0);
}

#[test]
fn first_prompt_is_kept_only_with_the_switch_and_an_engine() {
    let mut s = AppSettings::default();
    s.ai_engine = "claude".into();
    assert!(keep_prompts(&s));
    s.ai_engine = "codex".into();
    assert!(keep_prompts(&s));
    s.ai_engine = "off".into();
    assert!(!keep_prompts(&s));
    s.ai_engine = String::new();
    assert!(!keep_prompts(&s));
    s.ai_engine = "claude".into();
    s.ai_assign_suggest = false;
    assert!(!keep_prompts(&s));
}

// ---------------------------------------------------------------------------
// Pamięć po session_id

#[test]
fn one_call_per_session_even_after_failure_or_rejection() {
    let mut memory = Memory::default();
    let a = agent("s1", "C:\\api", NOW - 3 * MIN);
    let key = Memory::key(&a);
    assert_eq!(memory.begin(&key), Slot::Ask);
    // Wywołanie trwa: kolejne przebiegi pętli nie pytają.
    assert_eq!(memory.begin(&key), Slot::Waiting);
    memory.finish(&key, None);
    for _ in 0..3 {
        assert_eq!(memory.begin(&key), Slot::Answer(None));
    }
    // Inna sesja albo inny agent o tym samym identyfikatorze to nowe pytanie.
    assert_eq!(
        memory.begin(&Memory::key(&agent("s2", "C:\\api", 0))),
        Slot::Ask
    );
    let mut codex = agent("s1", "C:\\api", 0);
    codex.kind = "codex".into();
    assert_eq!(memory.begin(&Memory::key(&codex)), Slot::Ask);
    // Podpowiedź zostaje zapamiętana.
    let proposal = Proposal {
        task_id: 1,
        confidence: 0.8,
        reason: "r".into(),
    };
    let key = Memory::key(&agent("s3", "C:\\api", 0));
    assert_eq!(memory.begin(&key), Slot::Ask);
    memory.finish(&key, Some(proposal.clone()));
    assert_eq!(memory.begin(&key), Slot::Answer(Some(proposal)));
}

#[test]
fn memory_forgets_the_oldest_sessions_beyond_its_limit() {
    let mut memory = Memory::default();
    let first = Memory::key(&agent("first", "C:\\api", 0));
    assert_eq!(memory.begin(&first), Slot::Ask);
    for i in 0..REMEMBERED {
        memory.begin(&Memory::key(&agent(&format!("s{i}"), "C:\\api", 0)));
    }
    assert_eq!(memory.map.len(), REMEMBERED);
    assert_eq!(memory.begin(&first), Slot::Ask);
}

/// Pętla agentów woła sesję co 5 s: model dostaje jedno wywołanie, a wynik
/// (także brak podpowiedzi po błędzie) nie wraca do pytania.
#[test]
fn loop_calls_the_model_once_per_session() {
    let state = AiState::default();
    let runner = Fake::answering(json!({"taskId": 1, "confidence": 0.9, "reason": "pasuje"}));
    let d = dict();
    let ask = ask_with(&state, &runner, &d);
    let mut memory = Memory::default();
    let a = agent("s1", "C:\\Projekty\\api", NOW - 3 * MIN);
    let s = AppSettings::default();
    let mut seen = Vec::new();
    for _ in 0..6 {
        if !wants(&a, &s, NOW) {
            continue;
        }
        let key = Memory::key(&a);
        match memory.begin(&key) {
            Slot::Ask => {
                let answer = decide(&ask, "api", Some("main"), Some("napraw eksport"), &tasks());
                memory.finish(&key, answer);
            }
            Slot::Answer(a) => seen.push(a.map(|p| p.task_id)),
            Slot::Waiting => {}
        }
    }
    assert_eq!(runner.count(), 1);
    assert_eq!(seen, vec![Some(1); 5]);
    // Błąd wywołania: brak podpowiedzi i brak kolejnego wywołania.
    let failing = Fake::new(|_| Err(RunError::Timeout));
    let state = AiState::default();
    let ask = ask_with(&state, &failing, &d);
    let mut memory = Memory::default();
    for _ in 0..4 {
        let key = Memory::key(&a);
        if memory.begin(&key) == Slot::Ask {
            memory.finish(&key, decide(&ask, "api", None, None, &tasks()));
        }
    }
    assert_eq!(failing.count(), 1);
    assert_eq!(memory.begin(&Memory::key(&a)), Slot::Answer(None));
}

#[test]
fn model_error_or_null_answer_means_no_suggestion() {
    let state = AiState::default();
    let d = dict();
    let none = Fake::answering(json!({"taskId": null, "confidence": 0.9, "reason": "nic"}));
    assert!(decide(&ask_with(&state, &none, &d), "api", None, None, &tasks()).is_none());
    let bad = Fake::answering(json!({"taskId": 1, "confidence": 0.3, "reason": "może"}));
    let state = AiState::default();
    assert!(decide(&ask_with(&state, &bad, &d), "api", None, None, &tasks()).is_none());
    // Odpowiedź niezgodna ze schematem to błąd wywołania.
    let wrong = Fake::answering(json!({"foo": 1}));
    let state = AiState::default();
    assert!(decide(&ask_with(&state, &wrong, &d), "api", None, None, &tasks()).is_none());
}

// ---------------------------------------------------------------------------
// Baner w suggest.rs

fn with_assist(mut a: Agent, task: i64, prompt: Option<&str>) -> Agent {
    a.assist = Some(Match {
        task_id: Some(task),
        title: format!("Zadanie {task}"),
        key: None,
        source: SOURCE.into(),
        done: false,
    });
    a.first_prompt = prompt.map(str::to_string);
    a
}

#[test]
fn assistant_suggestion_goes_through_the_same_rule_as_other_proposals() {
    let s = AppSettings::default();
    let mut m = suggest::Memory::default();
    let a = with_assist(
        agent("s1", "C:\\Projekty\\api", NOW - 3 * MIN),
        1,
        Some("napraw   eksport\nCSV"),
    );
    let list = m.pending(&[a.clone()], None, &s, NOW);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task.source, SOURCE);
    assert_eq!(list[0].task.task_id, Some(1));
    // Baner pokazuje początek promptu (białe znaki zwinięte).
    assert_eq!(
        list[0].agents[0].hint.as_deref(),
        Some("napraw eksport CSV")
    );
    // Reguła 2 minut i timer mierzący to zadanie.
    let young = with_assist(agent("s1", "C:\\api", NOW - MIN), 1, None);
    assert!(m.pending(&[young], None, &s, NOW).is_empty());
    assert!(m.pending(&[a.clone()], Some(1), &s, NOW).is_empty());
    // Wyciszony folder.
    let muted = AppSettings {
        agent_muted_folders: vec!["C:\\Projekty".into()],
        ..AppSettings::default()
    };
    assert!(m.pending(&[a.clone()], None, &muted, NOW).is_empty());
    // „Nie” wycisza parę (agent, zadanie).
    assert!(m.dismiss(&[a.clone()], "t:1", NOW));
    assert!(m.pending(&[a.clone()], None, &s, NOW).is_empty());
    // Dopasowanie heurystyką (`task`) ma pierwszeństwo i nie niesie podpowiedzi.
    let mut other = suggest::Memory::default();
    let mut matched = a.clone();
    matched.task = Some(Match {
        task_id: Some(2),
        title: "Z folderu".into(),
        key: None,
        source: "folder".into(),
        done: false,
    });
    let list = other.pending(&[matched], None, &s, NOW);
    assert_eq!(list[0].task.task_id, Some(2));
    assert_eq!(list[0].agents[0].hint, None);
}

#[test]
fn assistant_suggestion_message_asks_which_task_the_session_belongs_to() {
    let s = AppSettings::default();
    let mut m = suggest::Memory::default();
    let a = with_assist(agent("s1", "C:\\api", NOW - 3 * MIN), 1, None);
    let list = m.pending(&[a], None, &s, NOW);
    let message = suggest::message(&list).unwrap();
    assert_eq!(
        message.title,
        "Asystent: do którego zadania należy ta sesja?"
    );
    assert_eq!(message.body, "api · Zadanie 1");
    assert_eq!(message.actions.len(), 1);
}

#[test]
fn assist_never_replaces_the_matched_task_used_for_agent_time() {
    let a = with_assist(agent("s1", "C:\\api", NOW - 3 * MIN), 1, None);
    // Czas agentów (`agent_time`) bierze `task`, a podpowiedź jest osobnym polem.
    assert!(a.task.is_none());
    assert_eq!(a.assist.as_ref().map(|m| m.source.as_str()), Some(SOURCE));
}

#[test]
fn assist_match_reads_title_key_and_done_and_skips_trashed() {
    let conn = Connection::open_in_memory().unwrap();
    db::init(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tasks(title,created_at,done) VALUES('Eksport CSV',0,0),('Gotowe',0,1),('W koszu',0,0),('Z Redmine',0,0);
         INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-12');
         INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(4,'https://r.example.pl',1234);
         INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(3,5);",
    )
    .unwrap();
    let proposal = |id| assist_match(&conn, id).unwrap();
    let m = proposal(1).unwrap();
    assert_eq!(
        (
            m.task_id,
            m.title.as_str(),
            m.key.as_deref(),
            m.source.as_str(),
            m.done
        ),
        (Some(1), "Eksport CSV", Some("WMS-12"), SOURCE, false)
    );
    assert!(proposal(2).unwrap().done);
    assert_eq!(proposal(4).unwrap().key.as_deref(), Some("#1234"));
    assert!(proposal(3).is_none());
    assert!(proposal(99).is_none());
}

// ---------------------------------------------------------------------------
// Pierwszy prompt z hooka (B8)

#[test]
fn working_session_carries_id_and_first_prompt_only_while_kept() {
    use crate::hooks::{HookEvent, Sessions};
    let event = |name: &str, prompt: Option<&str>| HookEvent {
        agent: "claude".into(),
        session_id: "sess-1".into(),
        cwd: "C:\\Projekty\\api".into(),
        event: name.into(),
        prompt: prompt.map(str::to_string),
        ..HookEvent::default()
    };
    let mut s = Sessions::default();
    s.set_keep_prompts(true);
    s.apply(&event("UserPromptSubmit", Some("napraw eksport")), NOW);
    s.apply(&event("UserPromptSubmit", Some("drugi")), NOW + 1);
    let working = s.working();
    assert_eq!(working.len(), 1);
    assert_eq!(working[0].session_id, "sess-1");
    assert_eq!(working[0].first_prompt.as_deref(), Some("napraw eksport"));
    // Wyłączenie przełącznika czyści zapamiętany prompt.
    s.set_keep_prompts(false);
    assert_eq!(s.working()[0].first_prompt, None);
    s.apply(&event("UserPromptSubmit", Some("trzeci")), NOW + 2);
    assert_eq!(s.working()[0].first_prompt, None);
    // Bez przełącznika od początku prompt nie jest w ogóle zapamiętany.
    let mut off = Sessions::default();
    off.apply(&event("UserPromptSubmit", Some("tajne")), NOW);
    assert_eq!(off.working()[0].first_prompt, None);
}

#[test]
fn ts_question_uses_the_day_note_of_the_chosen_day() {
    let conn = Connection::open_in_memory().unwrap();
    db::init(&conn).unwrap();
    conn.execute_batch(
        r#"INSERT INTO tasks(title,created_at) VALUES('Eksport CSV',0);
         INSERT INTO task_details(task_id,json) VALUES(1,'{"project":"WMS","notes":{"2026-10-15":"wczoraj","2026-10-16":"dziś import"}}');
         INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-12');"#,
    )
    .unwrap();
    let tasks = db::snapshot(&conn).unwrap().tasks;
    let q = ts_query(&tasks, 1, "2026-10-16").unwrap();
    assert_eq!(
        (
            q.title.as_str(),
            q.project.as_str(),
            q.key.as_str(),
            q.note.as_str()
        ),
        ("Eksport CSV", "WMS", "WMS-12", "dziś import")
    );
    assert_eq!(ts_query(&tasks, 1, "2026-10-20").unwrap().note, "");
    assert!(ts_query(&tasks, 9, "2026-10-16").is_none());
}

#[test]
fn ts_pick_returns_errors_from_the_model_call() {
    let failing = Fake::new(|_| Err(RunError::Timeout));
    let state = AiState::default();
    let d = dict();
    let ask = ask_with(&state, &failing, &d);
    let err = ts_pick(
        &ask,
        &TsQuery::default(),
        &[TsOption {
            id: "1".into(),
            name: "A".into(),
            project: String::new(),
        }],
    )
    .unwrap_err();
    assert_eq!(err, ai::TIMED_OUT);
}
