//! Worktree Git na tymczasowych repozytoriach (lokalny git, bez sieci).

use std::sync::atomic::{AtomicU32, Ordering};

use workflow_core::ports::{NewRun, RunRepository};
use workflow_core::template::analysis_implementation_review;

use super::*;
use crate::workflows::runs::SqliteRunRepository;
use crate::workflows::store;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Unikalny katalog testu; nazwa ze spacją i polskimi znakami.
struct Sandbox {
    root: PathBuf,
    _git: crate::workflows::test_git::GitPermit,
}

impl Sandbox {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("tuktuk wf żółć {}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("katalog testu");
        Self {
            root,
            _git: crate::workflows::test_git::GitPermit::acquire(),
        }
    }

    fn app_data(&self) -> PathBuf {
        let dir = self.root.join("app data");
        std::fs::create_dir_all(&dir).expect("app data");
        dir
    }

    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).expect("repo");
        g(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "jeden\n").expect("plik");
        commit(&dir, "start");
        dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Sprzątanie best-effort; worktree zostawia pliki tylko-do-odczytu w .git.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn g(dir: &Path, args: &[&str]) -> String {
    let mut full = vec!["-c", "user.name=t", "-c", "user.email=t@example.com"];
    full.extend_from_slice(args);
    git_text(dir, &full).expect("git")
}

fn commit(dir: &Path, message: &str) {
    g(dir, &["add", "-A"]);
    g(dir, &["commit", "-q", "-m", message]);
}

struct Env {
    db: SharedDb,
    repo: SqliteRunRepository,
    version_id: i64,
}

impl Env {
    fn new() -> Self {
        let conn = Connection::open_in_memory().expect("db");
        crate::db::init(&conn).expect("init");
        let id = store::create(&conn, "P", "", &analysis_implementation_review(), 1).expect("p");
        store::publish(&conn, id, 1, 2).expect("publish");
        let version_id = conn
            .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
            .expect("wersja");
        let db = Arc::new(Mutex::new(conn));
        Self {
            repo: SqliteRunRepository::new(db.clone()),
            db,
            version_id,
        }
    }

    fn run(&self, request: &str) -> RunId {
        self.repo
            .create_run(&NewRun {
                version_id: self.version_id,
                request_id: request.into(),
                now: 10,
                ..NewRun::default()
            })
            .expect("run")
            .run_id
    }
}

fn rejected_text(result: Result<Workspace, WorkspaceError>) -> String {
    match result {
        Err(WorkspaceError::Rejected(m)) => m,
        other => panic!("oczekiwano odrzucenia, jest {other:?}"),
    }
}

#[test]
fn valid_repo_gets_a_worktree_under_app_data_and_a_reservation() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo z spacją");
    let run = env.run("r1");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), run, 5).expect("workspace");
    assert_eq!(ws.base_commit, g(&repo, &["rev-parse", "HEAD"]));
    assert!(ws.worktree_path.join("a.txt").exists());
    assert!(is_under(
        &ws.worktree_path,
        &canonical(&worktrees_dir(&sb.app_data())).expect("kanon")
    ));
    let access = GitWorkspace::new(env.db.clone());
    assert_eq!(
        access.workspace_path(run),
        Some(ws.worktree_path.to_string_lossy().into_owned())
    );
    release(&env.db, run, 6).expect("release");
    assert_eq!(access.workspace_path(run), None);
    assert!(ws.worktree_path.exists());
}

#[test]
fn second_reservation_of_the_same_path_conflicts() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let first = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let other = Workspace {
        run_id: env.run("b"),
        ..first.clone()
    };
    assert_eq!(reserve(&env.db, &other, 2), Err(WorkspaceError::Conflict));
    // Po zwolnieniu ścieżkę można zarezerwować ponownie.
    release(&env.db, first.run_id, 3).expect("release");
    assert_eq!(reserve(&env.db, &other, 4), Ok(()));
}

#[test]
fn uncommitted_changes_are_rejected_and_left_untouched() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    std::fs::write(repo.join("a.txt"), "zmiana użytkownika\n").expect("zapis");
    std::fs::write(repo.join("nowy.txt"), "x").expect("zapis");
    let message = rejected_text(prepare_workspace(
        &env.db,
        &repo,
        &sb.app_data(),
        env.run("r"),
        1,
    ));
    assert!(message.contains("niezacommitowane"));
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).expect("odczyt"),
        "zmiana użytkownika\n"
    );
    assert!(repo.join("nowy.txt").exists());
    assert!(!worktrees_dir(&sb.app_data()).join("1").exists());
}

#[test]
fn directory_without_git_and_repo_with_submodules_are_rejected() {
    let sb = Sandbox::new();
    let env = Env::new();
    let plain = sb.root.join("zwykly");
    std::fs::create_dir_all(&plain).expect("katalog");
    let text = rejected_text(prepare_workspace(
        &env.db,
        &plain,
        &sb.app_data(),
        env.run("a"),
        1,
    ));
    assert!(text.contains("nie jest repozytorium Git") || text.contains("katalog główny"));

    let repo = sb.repo("z-modulami");
    std::fs::write(repo.join(".gitmodules"), "[submodule \"x\"]\n").expect("zapis");
    commit(&repo, "moduły");
    let text = rejected_text(prepare_workspace(
        &env.db,
        &repo,
        &sb.app_data(),
        env.run("b"),
        1,
    ));
    assert!(text.contains("submodułami"));
}

/// Podmoduł zapisany tylko w indeksie (bez `.gitmodules`) też jest odrzucany:
/// sprawdzenie idzie przez `ls-files --stage` (tryb 160000), nie przez skrypt
/// `git submodule`, który pod obciążeniem przekraczał limit czasu.
#[test]
fn gitlink_in_the_index_without_gitmodules_is_rejected() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("gitlink");
    let head = g(&repo, &["rev-parse", "HEAD"]);
    g(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{head},modul"),
        ],
    );
    g(&repo, &["commit", "-q", "-m", "gitlink"]);
    assert!(!repo.join(".gitmodules").exists());
    let text = rejected_text(prepare_workspace(
        &env.db,
        &repo,
        &sb.app_data(),
        env.run("gl"),
        1,
    ));
    assert!(text.contains("submodułami"), "{text}");
}

#[test]
fn source_validation_does_not_run_the_submodule_script() {
    let sb = Sandbox::new();
    let repo = sb.repo("bez-modulow");
    assert!(!has_gitlinks(&repo).expect("ls-files"));
}

#[test]
fn subdirectory_of_a_repo_is_not_the_root() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    std::fs::create_dir_all(repo.join("src")).expect("katalog");
    let text = rejected_text(prepare_workspace(
        &env.db,
        &repo.join("src"),
        &sb.app_data(),
        env.run("a"),
        1,
    ));
    assert!(text.contains("katalog główny"));
}

#[test]
fn network_paths_are_rejected() {
    let sb = Sandbox::new();
    let env = Env::new();
    for path in [r"\\server\share\repo", r"\\?\UNC\server\share\repo"] {
        let text = rejected_text(prepare_workspace(
            &env.db,
            Path::new(path),
            &sb.app_data(),
            env.run(path),
            1,
        ));
        assert!(text.contains("sieciowe"), "{path}: {text}");
    }
}

#[test]
fn source_inside_app_data_workflows_is_rejected() {
    let sb = Sandbox::new();
    let env = Env::new();
    let app = sb.app_data();
    let inside = workflows_dir(&app).join("repo");
    std::fs::create_dir_all(&inside).expect("katalog");
    g(&inside, &["init", "-q"]);
    std::fs::write(inside.join("a"), "x").expect("zapis");
    commit(&inside, "start");
    let text = rejected_text(prepare_workspace(&env.db, &inside, &app, env.run("a"), 1));
    assert!(text.contains("katalogu danych"));
}

#[cfg(windows)]
#[test]
fn junction_to_the_repo_resolves_to_the_same_canonical_source() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("prawdziwe repo");
    let link = sb.root.join("skrót junction");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&repo)
        .output()
        .expect("mklink");
    assert!(status.status.success(), "mklink: {status:?}");
    let ws = prepare_workspace(&env.db, &link, &sb.app_data(), env.run("a"), 1).expect("ws");
    assert!(same_path(
        &ws.source_repo,
        &canonical(&repo).expect("kanon")
    ));
}

#[test]
fn artifact_hash_changes_with_edits_and_new_files_and_is_stable_otherwise() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let run = env.run("a");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), run, 1).expect("ws");
    let access = GitWorkspace::new(env.db.clone());
    let clean = access.artifact_hash(run).expect("hash");
    assert_eq!(access.artifact_hash(run).expect("hash"), clean);
    std::fs::write(ws.worktree_path.join("a.txt"), "dwa\n").expect("zapis");
    let edited = access.artifact_hash(run).expect("hash");
    assert_ne!(edited, clean);
    assert_eq!(access.artifact_hash(run).expect("hash"), edited);
    std::fs::write(ws.worktree_path.join("nowy.txt"), "x\n").expect("zapis");
    let added = access.artifact_hash(run).expect("hash");
    assert_ne!(added, edited);
}

#[test]
fn snapshot_detects_write_delete_and_new_untracked_files() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let wt = &ws.worktree_path;
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    let snapshot = |_: &PathBuf| git.snapshot();
    let base = snapshot(wt).expect("snapshot");
    assert_eq!(snapshot(wt).expect("snapshot"), base);

    std::fs::write(wt.join("a.txt"), "inna treść\n").expect("zapis");
    let written = snapshot(wt).expect("snapshot");
    assert_ne!(written, base);

    // Przywrócenie przez git (zapis ręczny zależy od autocrlf na tej maszynie).
    g(wt, &["checkout", "--", "a.txt"]);
    assert_eq!(snapshot(wt).expect("snapshot"), base);

    std::fs::remove_file(wt.join("a.txt")).expect("usunięcie");
    let deleted = snapshot(wt).expect("snapshot");
    assert_ne!(deleted, base);

    g(wt, &["checkout", "--", "a.txt"]);
    std::fs::write(wt.join("notatka.txt"), "a").expect("zapis");
    let untracked = snapshot(wt).expect("snapshot");
    assert_ne!(untracked, base);
    std::fs::write(wt.join("notatka.txt"), "b").expect("zapis");
    assert_ne!(snapshot(wt).expect("snapshot"), untracked);
}

#[test]
fn snapshot_detects_writes_to_gitignored_paths() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    std::fs::write(repo.join(".gitignore"), "target/\n.env\n").expect("zapis");
    commit(&repo, "ignore");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    let base = git.snapshot().expect("snapshot");
    std::fs::write(ws.worktree_path.join(".env"), "SEKRET=1").expect("zapis");
    let env_file = git.snapshot().expect("snapshot");
    assert_ne!(env_file, base);
    std::fs::create_dir_all(ws.worktree_path.join("target/debug")).expect("katalog");
    std::fs::write(ws.worktree_path.join("target/debug/out.txt"), "x").expect("zapis");
    assert_ne!(git.snapshot().expect("snapshot"), env_file);
}

#[test]
fn snapshot_handles_a_nested_repository_and_sees_writes_inside_it() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    let nested = ws.worktree_path.join("vendor");
    std::fs::create_dir_all(&nested).expect("katalog");
    g(&nested, &["init", "-q"]);
    std::fs::write(nested.join("x.txt"), "1").expect("zapis");
    let first = git.snapshot().expect("migawka z zagnieżdżonym repo");
    assert_eq!(git.snapshot().expect("snapshot"), first);
    std::fs::write(nested.join("x.txt"), "2").expect("zapis");
    assert_ne!(git.snapshot().expect("snapshot"), first);
}

#[cfg(windows)]
#[test]
fn snapshot_does_not_follow_or_fail_on_a_dangling_symlink() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    let link = ws.worktree_path.join("wiszacy");
    if std::os::windows::fs::symlink_file(sb.root.join("nie-ma-takiego"), &link).is_err() {
        // Bez uprawnień do dowiązań (brak trybu dewelopera) nie ma czego sprawdzać.
        return;
    }
    let first = git.snapshot().expect("migawka z wiszącym dowiązaniem");
    std::fs::remove_file(&link).expect("usunięcie");
    std::os::windows::fs::symlink_file(sb.root.join("inny-cel"), &link).expect("dowiązanie");
    assert_ne!(git.snapshot().expect("snapshot"), first);
}

fn violation_text(result: Result<String, WorkspaceError>) -> String {
    match result {
        Err(WorkspaceError::PermissionViolation(m)) => m,
        other => panic!("oczekiwano naruszenia, jest {other:?}"),
    }
}

#[test]
fn replaced_dotgit_with_fsmonitor_config_runs_nothing_and_is_a_violation() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    let wt = &ws.worktree_path;

    // Agent podmienia plik .git na katalog z core.fsmonitor wskazującym jego skrypt.
    let marker = sb.root.join("fsmonitor-uruchomiony.txt");
    let hook = sb.root.join("hook.sh");
    let marker_unix = marker.to_string_lossy().replace('\\', "/");
    std::fs::write(&hook, format!("#!/bin/sh\necho x > \"{marker_unix}\"\n")).expect("hook");
    std::fs::remove_file(wt.join(".git")).expect("usunięcie gitfile");
    g(wt, &["init", "-q"]);
    let hook_unix = hook.to_string_lossy().replace('\\', "/");
    // Ścieżka testu ma spacje; Git uruchamia fsmonitor przez powłokę, stąd cudzysłowy.
    g(wt, &["config", "core.fsmonitor", &format!("'{hook_unix}'")]);

    // Kontrola: zwykły Git z odkrywaniem repozytorium uruchomiłby skrypt agenta.
    let plain = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt)
        .output()
        .expect("git status");
    assert!(plain.status.success());
    let attack_works = marker.exists();
    let _ = std::fs::remove_file(&marker);

    assert!(violation_text(git.snapshot()).contains(".git"));
    assert!(violation_text(git.artifact_hash(&ws.base_commit)).contains(".git"));
    let access = GitWorkspace::new(env.db.clone());
    assert!(matches!(
        access.artifact_hash_checked(ws.run_id),
        Err(WorkspaceError::PermissionViolation(_))
    ));
    assert!(
        !marker.exists(),
        "Git uruchomił polecenie z konfiguracji agenta"
    );
    assert!(
        attack_works,
        "kontrola: fsmonitor nie zadziałał w zwykłym Gicie"
    );
}

#[test]
fn gitfile_pointing_at_another_git_dir_is_a_violation() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let other = sb.repo("obce repo");
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), env.run("a"), 1).expect("ws");
    let git = WorktreeGit::for_workspace(&ws).expect("git");
    assert_eq!(git.verify(), Ok(()));
    std::fs::write(
        ws.worktree_path.join(".git"),
        format!("gitdir: {}\n", other.join(".git").display()),
    )
    .expect("zapis");
    violation_text(git.snapshot());
}

#[test]
fn failed_reservation_after_worktree_add_leaves_no_orphaned_worktree() {
    let sb = Sandbox::new();
    let env = Env::new();
    let repo = sb.repo("repo");
    let app = sb.app_data();
    let run = env.run("a");
    let blocker = env.run("b");
    let worktrees = worktrees_dir(&app);
    std::fs::create_dir_all(&worktrees).expect("katalog");
    let target = canonical(&worktrees).expect("kanon").join(run.to_string());
    // Inny przebieg trzyma rezerwację tej ścieżki: reserve() zwróci Conflict po `worktree add`.
    let taken = Workspace {
        run_id: blocker,
        source_repo: repo.clone(),
        worktree_path: target.clone(),
        base_commit: "x".into(),
    };
    reserve(&env.db, &taken, 1).expect("rezerwacja blokująca");
    assert_eq!(
        prepare_workspace(&env.db, &repo, &app, run, 2),
        Err(WorkspaceError::Conflict)
    );
    assert!(!target.exists(), "worktree został na dysku");
    let list = g(&repo, &["worktree", "list", "--porcelain"]);
    assert_eq!(list.matches("worktree ").count(), 1, "{list}");
    // Po zwolnieniu blokady ponowna próba się udaje (bez Conflict na target.exists()).
    release(&env.db, blocker, 3).expect("release");
    assert!(prepare_workspace(&env.db, &repo, &app, run, 4).is_ok());
}

// --- E5: artefakt diffu adresowany skrótem -------------------------------------

use crate::workflows::artifacts::{read_diff, ArtifactError, MAX_DIFF_VIEW_BYTES};

/// Worktree z jedną zmianą i `GitWorkspace` zapisujący artefakty.
fn edited_workspace(sb: &Sandbox, env: &Env, request: &str) -> (RunId, Workspace, GitWorkspace) {
    let repo = sb.repo(&format!("repo-{request}"));
    let run = env.run(request);
    let ws = prepare_workspace(&env.db, &repo, &sb.app_data(), run, 1).expect("ws");
    std::fs::write(ws.worktree_path.join("a.txt"), "dwa\n").expect("zapis");
    let access = GitWorkspace::new(env.db.clone()).with_app_data(sb.app_data());
    (run, ws, access)
}

fn count_artifacts(env: &Env) -> i64 {
    env.db
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM workflow_artifacts", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn artifact_hash_equals_hash_of_stored_diff() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, _, access) = edited_workspace(&sb, &env, "h");
    let hash = access.artifact_hash(run).expect("hash");
    let view = read_diff(&env.db, &sb.app_data(), run, &hash, MAX_DIFF_VIEW_BYTES).expect("diff");
    assert_eq!(view.hash, hash);
    assert!(!view.truncated);
    assert!(view.text.contains("+dwa"));
    let bytes = std::fs::read(
        sb.app_data()
            .join(format!("workflows/runs/{run}/diff-{hash}.patch")),
    )
    .expect("plik");
    assert_eq!(hex(Sha256::digest(&bytes)), hash);
}

#[test]
fn stored_diff_is_written_once_per_hash() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, _, access) = edited_workspace(&sb, &env, "once");
    let first = access.artifact_hash(run).expect("hash");
    access.artifact_hash(run).expect("hash");
    assert_eq!(count_artifacts(&env), 1);
    let dir = sb.app_data().join(format!("workflows/runs/{run}"));
    let files: Vec<_> = std::fs::read_dir(dir).unwrap().collect();
    assert_eq!(files.len(), 1, "bez pozostałych plików tymczasowych");
    assert!(first.len() == 64);
}

#[test]
fn read_diff_rejects_unknown_hash_and_other_run() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, _, access) = edited_workspace(&sb, &env, "unk");
    let hash = access.artifact_hash(run).expect("hash");
    let other = env.run("inny");
    let app = sb.app_data();
    assert_eq!(
        read_diff(&env.db, &app, other, &hash, 10).unwrap_err(),
        ArtifactError::Unknown
    );
    assert_eq!(
        read_diff(&env.db, &app, run, &"0".repeat(64), 10).unwrap_err(),
        ArtifactError::Unknown
    );
    // Ścieżka zamiast skrótu nie jest akceptowana.
    assert_eq!(
        read_diff(&env.db, &app, run, "../../x", 10).unwrap_err(),
        ArtifactError::Unknown
    );
}

#[test]
fn read_diff_reports_tampered_file() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, _, access) = edited_workspace(&sb, &env, "tamper");
    let hash = access.artifact_hash(run).expect("hash");
    let file = sb
        .app_data()
        .join(format!("workflows/runs/{run}/diff-{hash}.patch"));
    std::fs::write(&file, "podmieniony").expect("zapis");
    let err = read_diff(&env.db, &sb.app_data(), run, &hash, 100).unwrap_err();
    assert_eq!(err, ArtifactError::Unavailable);
    assert!(err.to_string().contains("niedostępny lub uszkodzony"));
    std::fs::remove_file(&file).expect("usunięcie");
    assert_eq!(
        read_diff(&env.db, &sb.app_data(), run, &hash, 100).unwrap_err(),
        ArtifactError::Unavailable
    );
}

#[test]
fn read_diff_truncates_with_flag() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, _, access) = edited_workspace(&sb, &env, "trunc");
    let hash = access.artifact_hash(run).expect("hash");
    let view = read_diff(&env.db, &sb.app_data(), run, &hash, 20).expect("diff");
    assert!(view.truncated);
    assert!(view.text.len() <= 20);
    assert!(view.size > 20);
}

#[test]
fn artifacts_live_under_app_data_not_source_repo() {
    let sb = Sandbox::new();
    let env = Env::new();
    let (run, ws, access) = edited_workspace(&sb, &env, "clean");
    access.artifact_hash(run).expect("hash");
    assert_eq!(g(&ws.source_repo, &["status", "--porcelain"]), "");
    assert!(!ws.source_repo.join("workflows").exists());
}

#[test]
fn git_calls_disable_fsmonitor_and_auto_gc() {
    let spec = git_spec(Path::new("."), &["status"], &[]);
    for needed in [
        "core.fsmonitor=false",
        "gc.auto=0",
        "maintenance.auto=false",
    ] {
        assert!(spec.args.iter().any(|a| a == needed), "brak {needed}");
    }
    assert!(spec
        .env
        .iter()
        .any(|(k, v)| k == "GIT_TERMINAL_PROMPT" && v == "0"));
}

#[test]
fn run_git_returns_within_limit_when_grandchild_keeps_pipes_open() {
    // Proces zastępczy: powłoka kończy się od razu, a wnuk (ping) trzyma potoki.
    let spec = Spec {
        program: PathBuf::from("cmd"),
        args: vec![
            "/c".into(),
            "start".into(),
            "/b".into(),
            "ping".into(),
            "-n".into(),
            "30".into(),
            "127.0.0.1".into(),
        ],
        stdin: String::new(),
        cwd: None,
        env: vec![],
    };
    let started = Instant::now();
    // Wynik zależy od tego, czy wnuk zdążył do jobu (wtedy ginie i potoki się
    // zamykają); w obu przypadkach wywołanie nie czeka na koniec wnuka.
    let _ = run_with_limit(&spec, "fake", Duration::from_secs(2));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "wisi bez limitu"
    );
}

/// Po przekroczeniu limitu ginie całe drzewo, nie tylko bezpośredni potomek:
/// wnuk, który po 3 s zapisałby znacznik, nie zdąży tego zrobić.
#[cfg(windows)]
#[test]
fn timeout_kills_the_whole_process_tree() {
    // Katalog ASCII: `cmd` czyta skrypt w stronie kodowej OEM, nie w UTF-8.
    let dir = std::env::temp_dir().join(format!("tuktuk-wf-tree-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("katalog");
    let marker = dir.join("wnuk-zyje.txt");
    let _ = std::fs::remove_file(&marker);
    let script = dir.join("drzewo.cmd");
    std::fs::write(
        &script,
        format!(
            "@echo off
start \"\" /b cmd /d /c \"ping -n 4 127.0.0.1 >nul & echo x> \"{}\"\"
ping -n 30 127.0.0.1 >nul
",
            marker.display()
        ),
    )
    .expect("skrypt");
    let spec = Spec {
        program: PathBuf::from("cmd"),
        args: vec!["/d".into(), "/c".into(), script.display().to_string()],
        stdin: String::new(),
        cwd: Some(dir.clone()),
        env: vec![],
    };
    let result = run_with_limit(&spec, "fake", Duration::from_millis(1500));
    assert!(result.is_err(), "wynik: {result:?}");
    std::thread::sleep(Duration::from_secs(5));
    let survived = marker.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!survived, "wnuk przeżył zakończenie po limicie");
}

/// Anulowanie kroku przerywa trwające polecenie (np. migawkę dużego repozytorium)
/// od razu, a nie dopiero po limicie `GIT_TIMEOUT`.
#[test]
fn cancel_interrupts_a_running_command_quickly() {
    let spec = Spec {
        program: PathBuf::from("ping"),
        args: vec!["-n".into(), "30".into(), "127.0.0.1".into()],
        stdin: String::new(),
        cwd: None,
        env: vec![],
    };
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            cancel.store(true, Ordering::SeqCst);
        });
        run_with_cancel(&spec, "fake", Duration::from_secs(60), Some(&cancel))
    });
    assert_eq!(result, Err(WorkspaceError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "anulowanie czekało na koniec"
    );
}
