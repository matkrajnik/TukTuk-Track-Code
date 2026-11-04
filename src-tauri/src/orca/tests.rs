//! Testy B9: budowa poleceń, parsowanie odpowiedzi z fixture, mapowanie folder → klucz
//! i przebieg tworzenia na podstawionym `Runner` (żaden test nie uruchamia orca.cmd).
use super::*;
use std::cell::RefCell;

const WORKTREES: &str = include_str!("../../tests/fixtures/orca_worktrees.json");
const REFERENCE: &str = include_str!("../../tests/fixtures/orca_reference.json");
const REPOS: &str = include_str!("../../tests/fixtures/orca_repos.json");
const STATUS: &str = include_str!("../../tests/fixtures/orca_status.json");
const STOPPED: &str = include_str!("../../tests/fixtures/orca_status_stopped.json");
const CREATED: &str = include_str!("../../tests/fixtures/orca_create.json");
const FAILED: &str = include_str!("../../tests/fixtures/orca_error.json");

fn ok(stdout: &str) -> Result<Output, String> {
    Ok(Output {
        success: true,
        stdout: stdout.into(),
        stderr: String::new(),
    })
}
fn failed(stdout: &str, stderr: &str) -> Result<Output, String> {
    Ok(Output {
        success: false,
        stdout: stdout.into(),
        stderr: stderr.into(),
    })
}
fn out(success: bool, stdout: &str) -> Output {
    Output {
        success,
        stdout: stdout.into(),
        stderr: String::new(),
    }
}

type Replies = Vec<(&'static str, Vec<Result<Output, String>>)>;

/// Zapisane odpowiedzi według pierwszych argumentów; zapamiętuje wywołania.
struct Fake {
    replies: RefCell<Replies>,
    calls: RefCell<Vec<Vec<String>>>,
}
impl Fake {
    fn new(replies: Replies) -> Self {
        Self {
            replies: RefCell::new(replies),
            calls: RefCell::new(Vec::new()),
        }
    }
    fn called(&self, first: &str) -> usize {
        self.calls.borrow().iter().filter(|c| c[0] == first).count()
    }
}
impl Runner for Fake {
    fn run(&self, _: &Path, args: &[String], _: Duration) -> Result<Output, String> {
        self.calls.borrow_mut().push(args.to_vec());
        let mut replies = self.replies.borrow_mut();
        let queue = replies
            .iter_mut()
            .find(|(first, _)| *first == args[0])
            .unwrap_or_else(|| panic!("nieoczekiwane polecenie {args:?}"));
        if queue.1.len() > 1 {
            queue.1.remove(0)
        } else {
            queue.1[0].clone()
        }
    }
    fn pause(&self, _: Duration) {}
}

fn prog() -> PathBuf {
    PathBuf::from(r"C:\Orca\bin\orca.cmd")
}

fn request() -> CreateRequest {
    CreateRequest {
        name: "wms-200-nowy".into(),
        repo: r"C:\Projekty\wms".into(),
        reference: Some("WMS-200".into()),
        agent: Some("claude".into()),
        prompt: Some("WMS-200: Nowy eksport\n\nNotatka z \"dnia\" 50%".into()),
        base_branch: None,
        activate: false,
    }
}

#[test]
fn builds_create_command_with_reference_agent_and_prompt() {
    let args = args_create(&request()).unwrap();
    assert_eq!(
        args,
        [
            "worktree",
            "create",
            "--json",
            "--name",
            "wms-200-nowy",
            "--repo",
            r"path:C:\Projekty\wms",
            "--reference",
            "WMS-200",
            "--agent",
            "claude",
            "--prompt",
            "WMS-200: Nowy eksport Notatka z 'dnia' 50"
        ]
    );
    assert_eq!(
        command_line(&prog(), &args),
        "orca.cmd worktree create --json --name wms-200-nowy --repo path:C:\\Projekty\\wms --reference WMS-200 --agent claude --prompt \"WMS-200: Nowy eksport Notatka z 'dnia' 50\""
    );
}

#[test]
fn create_command_optional_parts_and_validation() {
    // Bez agenta prompt nie jest wysyłany; gałąź bazowa i --activate dochodzą na końcu.
    let mut req = request();
    req.agent = None;
    req.reference = None;
    req.base_branch = Some("develop".into());
    req.activate = true;
    assert_eq!(
        args_create(&req).unwrap(),
        [
            "worktree",
            "create",
            "--json",
            "--name",
            "wms-200-nowy",
            "--repo",
            r"path:C:\Projekty\wms",
            "--base-branch",
            "develop",
            "--activate"
        ]
    );
    // Agent bez promptu: tylko --agent.
    req.agent = Some("codex".into());
    req.prompt = Some("   ".into());
    let args = args_create(&req).unwrap();
    assert!(args.windows(2).any(|w| w == ["--agent", "codex"]));
    assert!(!args.contains(&"--prompt".to_string()));
    // Błędne wejścia.
    let bad = |change: fn(&mut CreateRequest)| {
        let mut r = request();
        change(&mut r);
        args_create(&r).is_err()
    };
    assert!(bad(|r| r.name = "".into()));
    assert!(bad(|r| r.name = "a\"b".into()));
    assert!(bad(|r| r.name = "a%PATH%".into()));
    assert!(bad(|r| r.repo = "".into()));
    assert!(bad(|r| r.repo = "C:\\x\ny".into()));
    assert!(bad(|r| r.agent = Some("gpt".into())));
    assert!(bad(|r| r.reference = Some("A\"B".into())));
    assert_eq!(
        args_reference("WMS-1").unwrap(),
        ["reference", "find", "--json", "WMS-1"]
    );
    assert!(args_reference("W\"1").is_err());
    assert_eq!(args_status(), ["status", "--json"]);
    assert_eq!(args_list(), ["worktree", "list", "--json"]);
    assert_eq!(args_open(), ["open", "--json"]);
    assert_eq!(args_repos(), ["repo", "list", "--json"]);
}

#[test]
fn prompt_is_one_line_without_quotes_and_percent_and_capped() {
    assert_eq!(clean_prompt("a\r\n\tb  \"c\" %d%"), "a b 'c' d");
    assert_eq!(
        clean_prompt(&"x".repeat(MAX_PROMPT + 50)).chars().count(),
        MAX_PROMPT
    );
}

#[test]
fn parses_worktree_list_fixture() {
    let list = parse_worktrees(WORKTREES).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[0].path, r"C:\Orca\wms\WMS-123-popraw-eksport");
    assert_eq!(list[0].branch.as_deref(), Some("WMS-123-popraw-eksport"));
    assert_eq!(list[0].name, "WMS-123 popraw eksport");
    assert_eq!(list[0].references, ["WMS-123"]);
    assert_eq!(list[1].references, ["https://redmine.firma.pl/issues/4521"]);
    assert!(list[2].references.is_empty());
    assert_eq!(list[3].references, ["WMS-9"]);
}

#[test]
fn parses_reference_find_status_repos_and_created() {
    let found = parse_worktrees(REFERENCE).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].references, ["WMS-123"]);
    let repos = parse_repos(REPOS).unwrap();
    assert_eq!(
        repos,
        [
            Repo {
                id: "repo-wms".into(),
                name: "wms".into(),
                path: r"C:\Projekty\wms".into()
            },
            Repo {
                id: "repo-api".into(),
                name: "api".into(),
                path: r"C:\Projekty\api".into()
            },
        ]
    );
    assert!(parse_status(&out(true, STATUS)));
    assert!(!parse_status(&out(true, STOPPED)));
    assert!(!parse_status(&out(false, STATUS)));
    assert!(!parse_status(&out(true, FAILED)));
    let created = parse_created(CREATED).unwrap();
    assert_eq!(created.path, r"C:\Orca\wms\WMS-200-nowy");
    assert_eq!(created.branch.as_deref(), Some("wms-200-nowy"));
    assert_eq!(created.name, "wms-200-nowy");
}

#[test]
fn parse_errors_carry_orca_message_or_unreadable() {
    assert_eq!(
        parse_worktrees(FAILED).unwrap_err(),
        "Orca: Orca runtime is not running"
    );
    assert_eq!(parse_worktrees("nie json").unwrap_err(), UNREADABLE);
    assert_eq!(parse_worktrees("[]").unwrap(), vec![]);
    assert!(parse_created(r#"{"ok":true,"result":{}}"#).is_err());
}

#[test]
fn reference_key_recognizes_jira_redmine_and_urls() {
    assert_eq!(reference_key("WMS-123").as_deref(), Some("WMS-123"));
    assert_eq!(
        reference_key("https://x.atlassian.net/browse/WMS-12").as_deref(),
        Some("WMS-12")
    );
    assert_eq!(reference_key("#4521").as_deref(), Some("#4521"));
    assert_eq!(
        reference_key("https://redmine.firma.pl/redmine/issues/4521?x=1").as_deref(),
        Some("#4521")
    );
    assert_eq!(reference_key("tekst"), None);
    assert_eq!(reference_key("#abc"), None);
}

#[test]
fn folder_keys_map_worktree_folders_with_longest_match() {
    let mut list = parse_worktrees(WORKTREES).unwrap();
    // Zagnieżdżony worktree z własnym kluczem wygrywa z folderem nadrzędnym.
    list.push(Worktree {
        path: r"C:\Orca\wms\WMS-123-popraw-eksport\inner".into(),
        references: vec!["WMS-124".into()],
        ..Default::default()
    });
    let map = folder_keys(&list);
    // Worktree bez zgłoszenia (główny folder repozytorium) nie trafia do mapy.
    assert_eq!(map.len(), 4);
    assert!(!map.iter().any(|(f, _)| f == "c:/projekty/wms"));
    let key = |cwd: &str| key_for_folder(&map, cwd);
    assert_eq!(
        key(r"C:\Orca\wms\WMS-123-popraw-eksport").as_deref(),
        Some("WMS-123")
    );
    assert_eq!(
        key(r"c:/orca/wms/wms-123-popraw-eksport/src/app").as_deref(),
        Some("WMS-123")
    );
    assert_eq!(
        key(r"C:\Orca\wms\WMS-123-popraw-eksport\inner\x").as_deref(),
        Some("WMS-124")
    );
    assert_eq!(
        key(r"C:\Orca\api\redmine-4521-raport").as_deref(),
        Some("#4521")
    );
    // Końcowy ukośnik w ścieżce z Orki nie psuje dopasowania.
    assert_eq!(key(r"C:\Orca\wms\spike").as_deref(), Some("WMS-9"));
    // Folder o podobnym prefiksie to inny folder.
    assert_eq!(key(r"C:\Orca\wms\WMS-123-popraw-eksport-2"), None);
    assert_eq!(key(r"C:\Projekty\wms"), None);
    assert_eq!(key(""), None);
}

#[test]
fn status_decides_running_and_open_is_called_only_with_autostart() {
    let stopped = Fake::new(vec![("status", vec![ok(STOPPED)])]);
    assert_eq!(
        ensure_running(&stopped, &prog(), false, 3),
        Err(NOT_RUNNING.into())
    );
    assert_eq!(stopped.called("open"), 0);

    // Z autostartem: open, potem status do skutku.
    let starting = Fake::new(vec![
        ("status", vec![ok(STOPPED), ok(STOPPED), ok(STATUS)]),
        ("open", vec![ok(r#"{"ok":true}"#)]),
    ]);
    assert_eq!(ensure_running(&starting, &prog(), true, 5), Ok(()));
    assert_eq!(starting.called("open"), 1);

    // Orca nie wstaje: komunikat po skończeniu prób.
    let never = Fake::new(vec![
        ("status", vec![failed("", "not running")]),
        ("open", vec![ok(r#"{"ok":true}"#)]),
    ]);
    assert_eq!(
        ensure_running(&never, &prog(), true, 2),
        Err(START_FAILED.into())
    );

    // Działająca Orka nie jest otwierana ponownie.
    let running = Fake::new(vec![("status", vec![ok(STATUS)])]);
    assert_eq!(ensure_running(&running, &prog(), true, 2), Ok(()));
    assert_eq!(running.called("open"), 0);
}

#[test]
fn create_runs_worktree_create_only_when_orca_runs() {
    let fake = Fake::new(vec![
        ("status", vec![ok(STATUS)]),
        ("worktree", vec![ok(CREATED)]),
    ]);
    let created = create(&fake, &prog(), false, &request()).unwrap();
    assert_eq!(created.path, r"C:\Orca\wms\WMS-200-nowy");
    {
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1][..3], ["worktree", "create", "--json"]);
    }

    // Orca nie działa, autostart wyłączony: worktree nie jest tworzone.
    let down = Fake::new(vec![("status", vec![ok(STOPPED)])]);
    assert_eq!(
        create(&down, &prog(), false, &request()),
        Err(NOT_RUNNING.into())
    );
    assert_eq!(down.called("worktree"), 0);

    // Błędna prośba nie dotyka Orki w ogóle.
    let never = Fake::new(vec![]);
    let bad = CreateRequest {
        name: "".into(),
        ..request()
    };
    assert!(create(&never, &prog(), true, &bad).is_err());
    assert!(never.calls.borrow().is_empty());

    // Błąd Orki z JSON-a na stdout trafia do użytkownika.
    let refused = Fake::new(vec![
        ("status", vec![ok(STATUS)]),
        ("worktree", vec![failed(FAILED, "")]),
    ]);
    assert_eq!(
        create(&refused, &prog(), false, &request()),
        Err("Orca: Orca runtime is not running".into())
    );
    // Błąd bez JSON-a: pierwsza linia stderr.
    let crashed = Fake::new(vec![
        ("status", vec![ok(STATUS)]),
        ("worktree", vec![failed("", "boom\nwięcej")]),
    ]);
    assert_eq!(
        create(&crashed, &prog(), false, &request()),
        Err("Orca: boom".into())
    );
}

#[test]
fn repos_and_reference_find_go_through_runner() {
    let fake = Fake::new(vec![
        ("status", vec![ok(STATUS)]),
        ("repo", vec![ok(REPOS)]),
        ("reference", vec![ok(REFERENCE)]),
    ]);
    assert_eq!(repos(&fake, &prog(), false).unwrap().len(), 2);
    assert_eq!(find(&fake, &prog(), false, "WMS-123").unwrap().len(), 1);
    assert_eq!(
        fake.calls.borrow().last().unwrap(),
        &["reference", "find", "--json", "WMS-123"]
    );
}

#[test]
fn resolve_path_needs_an_existing_file() {
    assert_eq!(resolve_path(r"C:\nie\ma\takiego\orca.cmd"), None);
    let dir = std::env::temp_dir().join(format!("tt-orca-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("orca.cmd");
    std::fs::write(&file, "@echo off").unwrap();
    assert_eq!(resolve_path(&format!("  {}  ", file.display())), Some(file));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worktree_folders_match_with_subfolders_and_ignore_case() {
    let folders = vec![norm_folder(r"C:\orca\wms-200"), String::new()];
    assert!(in_worktrees(&folders, r"c:\ORCA\wms-200\"));
    assert!(in_worktrees(&folders, r"C:\orca\wms-200\src"));
    assert!(!in_worktrees(&folders, r"C:\orca\wms-2000"));
    assert!(!in_worktrees(&folders, ""));
    // Pusty wpis nie obejmuje wszystkiego.
    assert!(!in_worktrees(&[String::new()], r"C:\x"));
}
