//! Przestrzeń robocza przebiegu na worktree Git (PLAN_WDROZENIA.md, 8.3; etap E4).
//!
//! Źródłowe repozytorium użytkownika nigdy nie jest zmieniane: agent pracuje w
//! odrębnym worktree pod `<app-data>/workflows/worktrees/<runId>`. Git jest
//! wołany z listą argumentów (bez powłoki), z limitem czasu i bez trzymania
//! blokady bazy: połączenie jest blokowane tylko na czas jednej krótkiej
//! transakcji rezerwacji albo odczytu.
#![cfg_attr(not(test), allow(dead_code))]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_cli::Spec;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use workflow_core::ports::{RunId, WorkspaceAccess};

/// Najdłuższe wykonanie jednego polecenia Git.
const GIT_TIMEOUT: Duration = Duration::from_secs(60);
/// Częstość sprawdzania końca polecenia Git.
const GIT_POLL: Duration = Duration::from_millis(10);
/// Najwięcej bajtów wyjścia Git trzymanych w pamięci (diff binarny dużego repozytorium).
const GIT_MAX_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;

const ERR_UNC: &str =
    "Ścieżki sieciowe (UNC) nie są obsługiwane. Użyj repozytorium na dysku lokalnym.";
const ERR_NOT_ROOT: &str = "Wskaż katalog główny repozytorium Git (git rev-parse --show-toplevel).";
const ERR_NOT_GIT: &str = "Wskazany katalog nie jest repozytorium Git.";
const ERR_SUBMODULES: &str = "Repozytoria z submodułami nie są jeszcze obsługiwane.";
const ERR_DIRTY: &str = "Repozytorium ma niezacommitowane zmiany. Zacommituj je albo odłóż (stash); aplikacja nie przenosi zmian do katalogu roboczego agenta.";
const ERR_UNDER_APP_DATA: &str =
    "Repozytorium źródłowe leży w katalogu danych Workflows aplikacji.";
const ERR_ESCAPES: &str = "Katalog roboczy agenta wskazuje poza katalog Workflows aplikacji.";

/// Rezerwacja katalogu roboczego przebiegu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub run_id: RunId,
    pub source_repo: PathBuf,
    pub worktree_path: PathBuf,
    pub base_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Powód po polsku, gotowy dla użytkownika.
    Rejected(String),
    /// Ten katalog roboczy jest już zarezerwowany.
    Conflict,
    Git(String),
    Storage(String),
    /// Katalog roboczy zmieniono tak, że Git mógłby czytać konfigurację spoza
    /// rezerwacji (np. podmieniony plik `.git`); kroku nie wolno zaakceptować.
    PermissionViolation(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::Rejected(m)
            | WorkspaceError::Git(m)
            | WorkspaceError::Storage(m)
            | WorkspaceError::PermissionViolation(m) => f.write_str(m),
            WorkspaceError::Conflict => f.write_str("Katalog roboczy jest już zarezerwowany."),
        }
    }
}

impl std::error::Error for WorkspaceError {}

fn rejected(message: &str) -> WorkspaceError {
    WorkspaceError::Rejected(message.to_owned())
}

fn storage(error: impl std::fmt::Display) -> WorkspaceError {
    WorkspaceError::Storage(format!("Błąd bazy przestrzeni roboczych: {error}"))
}

// ---------------------------------------------------------------------------
// Git

/// Wyłącza mechanizmy Git, które uruchamiają polecenia z konfiguracji albo
/// zawartości repozytorium (fsmonitor, hooki, automatyczne gc). Dokładane przed
/// każdym poleceniem; konfiguracja systemowa i globalna zostają (agent nie ma do
/// nich zapisu), a konfiguracja repozytorium pochodzi z jawnego `GIT_DIR`.
const GIT_HARDENING: [&str; 10] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    if cfg!(windows) {
        "core.hooksPath=NUL"
    } else {
        "core.hooksPath=/dev/null"
    },
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.auto=false",
    "-c",
    "core.untrackedCache=false",
];

fn git_spec(cwd: &Path, args: &[&str], extra_env: &[(String, String)]) -> Spec {
    let mut env = vec![
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
        ("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned()),
    ];
    env.extend_from_slice(extra_env);
    Spec {
        program: PathBuf::from("git"),
        args: GIT_HARDENING
            .iter()
            .chain(args)
            .map(|a| (*a).to_owned())
            .collect(),
        stdin: String::new(),
        cwd: Some(cwd.to_path_buf()),
        env,
    }
}

fn read_all(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            // Błąd odczytu kończy zbieranie; kod wyjścia Git i tak rozstrzyga o wyniku.
            let _ = pipe.take(GIT_MAX_OUTPUT_BYTES).read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Uruchamia Git w `cwd` (repozytorium użytkownika albo testu); zwraca stdout
/// albo komunikat z końcówką stderr. Dla worktree przebiegu: [`WorktreeGit`].
pub fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, WorkspaceError> {
    run_git(cwd, args, &[])
}

fn run_git(
    cwd: &Path,
    args: &[&str],
    extra_env: &[(String, String)],
) -> Result<Vec<u8>, WorkspaceError> {
    let mut command = agent_cli::command(&git_spec(cwd, args, extra_env));
    let mut child = command
        .spawn()
        .map_err(|e| WorkspaceError::Git(format!("Nie udało się uruchomić polecenia git: {e}")))?;
    drop(child.stdin.take());
    let out = read_all(child.stdout.take());
    let err = read_all(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > GIT_TIMEOUT => {
                // Po przekroczeniu czasu kończymy Git; wynik kill nie zmienia komunikatu.
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorkspaceError::Git(format!(
                    "Polecenie git {} przekroczyło limit czasu.",
                    args.first().copied().unwrap_or("")
                )));
            }
            Ok(None) => std::thread::sleep(GIT_POLL),
            Err(e) => return Err(WorkspaceError::Git(format!("Błąd czekania na git: {e}"))),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        let text = String::from_utf8_lossy(&stderr);
        let tail: String = text.trim().chars().take(300).collect();
        Err(WorkspaceError::Git(format!(
            "git {} zakończył się błędem: {tail}",
            args.first().copied().unwrap_or("")
        )))
    }
}

fn git_text(cwd: &Path, args: &[&str]) -> Result<String, WorkspaceError> {
    git(cwd, args).map(|b| String::from_utf8_lossy(&b).trim().to_owned())
}

// ---------------------------------------------------------------------------
// Ścieżki

/// `\\server\share` i `\\?\UNC\...`; `\\?\C:\...` to zwykły dysk lokalny.
fn is_network_path(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with(r"\\?\unc\") || (text.starts_with(r"\\") && !text.starts_with(r"\\?\"))
}

/// Ścieżka kanoniczna bez prefiksu `\\?\` (Git go nie lubi).
fn canonical(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let canon = std::fs::canonicalize(path).map_err(|e| {
        WorkspaceError::Rejected(format!(
            "Nie można odczytać ścieżki {}: {e}",
            path.display()
        ))
    })?;
    let text = canon.to_string_lossy().into_owned();
    if is_network_path(&text) {
        return Err(rejected(ERR_UNC));
    }
    Ok(match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => canon,
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn is_under(path: &Path, parent: &Path) -> bool {
    let (p, q) = (
        path.to_string_lossy().to_lowercase(),
        parent.to_string_lossy().to_lowercase(),
    );
    path.starts_with(parent) || p.starts_with(&format!("{}\\", q.trim_end_matches('\\')))
}

pub fn workflows_dir(app_data: &Path) -> PathBuf {
    app_data.join("workflows")
}

pub fn worktrees_dir(app_data: &Path) -> PathBuf {
    workflows_dir(app_data).join("worktrees")
}

// ---------------------------------------------------------------------------
// Rezerwacje

pub type SharedDb = Arc<Mutex<Connection>>;

fn conn(db: &SharedDb) -> Result<std::sync::MutexGuard<'_, Connection>, WorkspaceError> {
    db.lock()
        .map_err(|_| WorkspaceError::Storage("Blokada bazy jest zatruta.".into()))
}

fn reserve(db: &SharedDb, ws: &Workspace, now: i64) -> Result<(), WorkspaceError> {
    let db = conn(db)?;
    db.execute(
        "INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at)
         VALUES(?1, ?2, ?3, ?4, 'reserved', ?5)",
        params![
            ws.run_id,
            ws.source_repo.to_string_lossy(),
            ws.worktree_path.to_string_lossy(),
            ws.base_commit,
            now
        ],
    )
    .map(|_| ())
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            WorkspaceError::Conflict
        }
        other => storage(other),
    })
}

/// Zwalnia rezerwację; katalog zostaje na dysku do przeglądu (bez reset, clean i merge).
pub fn release(db: &SharedDb, run_id: RunId, now: i64) -> Result<(), WorkspaceError> {
    let db = conn(db)?;
    db.execute(
        "UPDATE workflow_workspaces SET state = 'released', released_at = ?2
         WHERE run_id = ?1 AND state = 'reserved'",
        params![run_id, now],
    )
    .map(|_| ())
    .map_err(storage)
}

fn reserved(db: &SharedDb, run_id: RunId) -> Result<Option<Workspace>, WorkspaceError> {
    let db = conn(db)?;
    db.query_row(
        "SELECT source_repo, worktree_path, base_commit FROM workflow_workspaces
         WHERE run_id = ?1 AND state = 'reserved'",
        params![run_id],
        |row| {
            Ok(Workspace {
                run_id,
                source_repo: PathBuf::from(row.get::<_, String>(0)?),
                worktree_path: PathBuf::from(row.get::<_, String>(1)?),
                base_commit: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(storage)
}

// ---------------------------------------------------------------------------
// Przygotowanie

/// Sprawdza źródło i zwraca ścieżkę kanoniczną korzenia repozytorium oraz `HEAD`.
fn validate_source(source: &Path, app_data: &Path) -> Result<(PathBuf, String), WorkspaceError> {
    if is_network_path(&source.to_string_lossy()) {
        return Err(rejected(ERR_UNC));
    }
    let root = canonical(source)?;
    if !root.is_dir() {
        return Err(rejected(ERR_NOT_GIT));
    }
    let top = match git_text(&root, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => top,
        Err(_) => return Err(rejected(ERR_NOT_GIT)),
    };
    if !same_path(&canonical(Path::new(&top))?, &root) {
        return Err(rejected(ERR_NOT_ROOT));
    }
    if root.join(".gitmodules").exists() {
        return Err(rejected(ERR_SUBMODULES));
    }
    if !git_text(&root, &["submodule", "status"])?.is_empty() {
        return Err(rejected(ERR_SUBMODULES));
    }
    if !git_text(&root, &["status", "--porcelain"])?.is_empty() {
        return Err(rejected(ERR_DIRTY));
    }
    let workflows = workflows_dir(app_data);
    if workflows.exists() && is_under(&root, &canonical(&workflows)?) {
        return Err(rejected(ERR_UNDER_APP_DATA));
    }
    let base = git_text(&root, &["rev-parse", "HEAD"])?;
    Ok((root, base))
}

/// Tworzy worktree przebiegu i rezerwuje go krótką transakcją.
pub fn prepare_workspace(
    db: &SharedDb,
    source: &Path,
    app_data: &Path,
    run_id: RunId,
    now: i64,
) -> Result<Workspace, WorkspaceError> {
    let (root, base_commit) = validate_source(source, app_data)?;
    let worktrees = worktrees_dir(app_data);
    std::fs::create_dir_all(&worktrees).map_err(|e| {
        WorkspaceError::Storage(format!("Nie można utworzyć katalogu worktree: {e}"))
    })?;
    let worktrees_canonical = canonical(&worktrees)?;
    let target = worktrees_canonical.join(run_id.to_string());
    if target.exists() {
        return Err(WorkspaceError::Conflict);
    }
    let target_text = target.to_string_lossy().into_owned();
    git(
        &root,
        &["worktree", "add", "--detach", &target_text, &base_commit],
    )?;
    let registered = || -> Result<Workspace, WorkspaceError> {
        let worktree_path = canonical(&target)?;
        if !is_under(&worktree_path, &worktrees_canonical) {
            return Err(rejected(ERR_ESCAPES));
        }
        let workspace = Workspace {
            run_id,
            source_repo: root.clone(),
            worktree_path,
            base_commit: base_commit.clone(),
        };
        // Katalog administracyjny worktree musi dać się odtworzyć z rezerwacji.
        WorktreeGit::for_workspace(&workspace)?.verify()?;
        reserve(db, &workspace, now)?;
        Ok(workspace)
    };
    registered().inspect_err(|_| remove_worktree(&root, &target_text))
}

/// Sprzątanie po nieudanym przygotowaniu: worktree nie może zostać osierocony w
/// repozytorium źródłowym, bo ponowna próba trafiłaby na `Conflict`.
fn remove_worktree(root: &Path, target: &str) {
    // Best effort: zwracamy pierwotny błąd przygotowania, nie błąd sprzątania.
    let _ = git(root, &["worktree", "remove", "--force", target]);
    let _ = git(root, &["worktree", "prune"]);
}

// ---------------------------------------------------------------------------
// Git w worktree przebiegu

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn violation(message: &str) -> WorkspaceError {
    WorkspaceError::PermissionViolation(message.to_owned())
}

const ERR_GITFILE: &str = "Plik .git w katalogu roboczym przebiegu został podmieniony albo usunięty; Git nie zostanie w nim uruchomiony.";
const ERR_TOO_MANY: &str =
    "Katalog roboczy ma za dużo plików nieśledzonych, by sprawdzić krok tylko do odczytu.";

/// Najwięcej wpisów nieśledzonych w migawce; przekroczenie to błąd, nie cicha luka.
const SNAPSHOT_MAX_ENTRIES: usize = 500_000;
/// Do tylu bajtów plik nieśledzony jest haszowany w całości (strumieniowo).
const SNAPSHOT_CONTENT_LIMIT: u64 = 8 * 1024 * 1024;

/// Git w worktree przebiegu z jawnym `GIT_DIR` i `GIT_WORK_TREE` wyliczonymi z
/// rezerwacji (repozytorium źródłowe jest zaufane, worktree nie). Podmieniony
/// przez agenta plik `<wt>/.git` (np. katalog z własnym `core.fsmonitor`) nie
/// wskazuje więc Gitowi konfiguracji, a [`Self::verify`] przed każdą operacją
/// sprawdza, że `.git` nadal wskazuje oczekiwany katalog.
#[derive(Debug, Clone)]
pub struct WorktreeGit {
    work_tree: PathBuf,
    git_dir: PathBuf,
}

impl WorktreeGit {
    pub fn for_workspace(ws: &Workspace) -> Result<Self, WorkspaceError> {
        let common = git_text(
            &ws.source_repo,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let name = ws
            .worktree_path
            .file_name()
            .ok_or_else(|| rejected(ERR_ESCAPES))?;
        let git_dir = canonical(&PathBuf::from(common).join("worktrees").join(name))
            .map_err(|_| violation(ERR_GITFILE))?;
        Ok(Self {
            work_tree: ws.worktree_path.clone(),
            git_dir,
        })
    }

    /// `<wt>/.git` musi być zwykłym plikiem `gitdir: <oczekiwany katalog>`.
    pub fn verify(&self) -> Result<(), WorkspaceError> {
        let dotgit = self.work_tree.join(".git");
        let meta = std::fs::symlink_metadata(&dotgit).map_err(|_| violation(ERR_GITFILE))?;
        if !meta.is_file() || meta.len() > 4096 {
            return Err(violation(ERR_GITFILE));
        }
        let text = std::fs::read_to_string(&dotgit).map_err(|_| violation(ERR_GITFILE))?;
        let target = text
            .trim()
            .strip_prefix("gitdir:")
            .map(str::trim)
            .ok_or_else(|| violation(ERR_GITFILE))?;
        let target = Path::new(target);
        let target = if target.is_absolute() {
            target.to_path_buf()
        } else {
            self.work_tree.join(target)
        };
        match canonical(&target) {
            Ok(path) if same_path(&path, &self.git_dir) => Ok(()),
            _ => Err(violation(ERR_GITFILE)),
        }
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>, WorkspaceError> {
        self.verify()?;
        let env = [
            (
                "GIT_DIR".to_owned(),
                self.git_dir.to_string_lossy().into_owned(),
            ),
            (
                "GIT_WORK_TREE".to_owned(),
                self.work_tree.to_string_lossy().into_owned(),
            ),
        ];
        run_git(&self.work_tree, args, &env)
    }

    /// SHA-256 tego, co użytkownik akceptuje: diff worktree względem `base_commit`
    /// (z nowymi plikami przez intent-to-add). Bez zewnętrznych diffów i textconv.
    pub fn artifact_hash(&self, base_commit: &str) -> Result<String, WorkspaceError> {
        self.run(&["add", "-N", "."])?;
        let diff = self.run(&[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            base_commit,
        ])?;
        Ok(hex(Sha256::digest(&diff)))
    }

    /// Skrót stanu worktree: status, diff względem HEAD oraz pliki nieśledzone,
    /// także ignorowane przez `.gitignore`. Służy do wykrywania zmian w krokach
    /// tylko do odczytu. Plik nieśledzony: rozmiar, czas zmiany i treść czytana
    /// strumieniowo (do [`SNAPSHOT_CONTENT_LIMIT`]); dowiązanie: tekst celu, bez
    /// podążania; katalog (np. zagnieżdżone repozytorium): rekurencyjnie.
    pub fn snapshot(&self) -> Result<String, WorkspaceError> {
        let mut hasher = Sha256::new();
        hasher.update(self.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?);
        hasher.update(self.run(&["diff", "--binary", "--no-ext-diff", "--no-textconv", "HEAD"])?);
        // Bez --exclude-standard: zapis do ścieżki z .gitignore też jest zmianą.
        let untracked = self.run(&["ls-files", "--others", "-z"])?;
        let mut budget = SNAPSHOT_MAX_ENTRIES;
        for name in untracked.split(|b| *b == 0).filter(|n| !n.is_empty()) {
            let relative = String::from_utf8_lossy(name).into_owned();
            let path = self.work_tree.join(relative.trim_end_matches('/'));
            hash_entry(&mut hasher, &path, &relative, &mut budget)?;
        }
        Ok(hex(hasher.finalize()))
    }
}

fn read_error(relative: &str, e: std::io::Error) -> WorkspaceError {
    WorkspaceError::Git(format!("Nie można odczytać {relative}: {e}"))
}

fn modified_nanos(meta: &std::fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

fn hash_entry(
    hasher: &mut Sha256,
    path: &Path,
    relative: &str,
    budget: &mut usize,
) -> Result<(), WorkspaceError> {
    *budget = budget
        .checked_sub(1)
        .ok_or_else(|| WorkspaceError::Storage(ERR_TOO_MANY.into()))?;
    hasher.update(relative.as_bytes());
    hasher.update([0]);
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        // Wpis zniknął między ls-files a odczytem: to też stan, który haszujemy.
        hasher.update(b"missing");
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path).map_err(|e| read_error(relative, e))?;
        hasher.update(b"link:");
        hasher.update(target.to_string_lossy().as_bytes());
        return Ok(());
    }
    if meta.is_dir() {
        hasher.update(b"dir");
        let mut children = std::fs::read_dir(path)
            .map_err(|e| read_error(relative, e))?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| read_error(relative, e))?;
        children.sort();
        for child in children {
            let child_relative = format!("{relative}/{}", child.to_string_lossy());
            hash_entry(hasher, &path.join(&child), &child_relative, budget)?;
        }
        return Ok(());
    }
    hasher.update(meta.len().to_le_bytes());
    hasher.update(modified_nanos(&meta).to_le_bytes());
    if meta.len() <= SNAPSHOT_CONTENT_LIMIT {
        let mut file = std::fs::File::open(path).map_err(|e| read_error(relative, e))?;
        let mut content = Sha256::new();
        std::io::copy(&mut file, &mut content).map_err(|e| read_error(relative, e))?;
        hasher.update(content.finalize());
    }
    Ok(())
}

/// Dostęp silnika do rezerwacji z bazy.
pub struct GitWorkspace {
    db: SharedDb,
}

impl GitWorkspace {
    pub fn new(db: SharedDb) -> Self {
        Self { db }
    }

    fn worktree(&self, run_id: RunId) -> Result<(WorktreeGit, Workspace), WorkspaceError> {
        let ws = reserved(&self.db, run_id)?.ok_or_else(|| {
            WorkspaceError::Storage("Przebieg nie ma zarezerwowanego katalogu roboczego.".into())
        })?;
        Ok((WorktreeGit::for_workspace(&ws)?, ws))
    }

    /// Skrót artefaktu z typowanym błędem (naruszenie odróżnione od błędu zapisu).
    pub fn artifact_hash_checked(&self, run_id: RunId) -> Result<String, WorkspaceError> {
        let (git, ws) = self.worktree(run_id)?;
        git.artifact_hash(&ws.base_commit)
    }

    /// Migawka worktree przebiegu (strażnik kroków tylko do odczytu).
    pub fn snapshot(&self, run_id: RunId) -> Result<String, WorkspaceError> {
        self.worktree(run_id)?.0.snapshot()
    }
}

impl WorkspaceAccess for GitWorkspace {
    fn artifact_hash(&self, run_id: RunId) -> Result<String, String> {
        self.artifact_hash_checked(run_id)
            .map_err(|e| e.to_string())
    }

    fn workspace_path(&self, run_id: RunId) -> Option<String> {
        reserved(&self.db, run_id)
            .ok()
            .flatten()
            .map(|ws| ws.worktree_path.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests;
