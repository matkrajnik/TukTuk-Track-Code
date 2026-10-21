//! Worktree Orki z kafelka zadania (PLAN_BACKLOG.md, B9).
//!
//! Orca ma CLI `orca.cmd` (poza PATH) z wyjściem `--json`, które rozmawia z lokalnym
//! runtime Orki. Ten moduł buduje polecenia (`worktree create`, `worktree list`,
//! `reference find`, `repo list`, `status`, `open`), parsuje odpowiedzi i mapuje
//! foldery worktree na klucze zgłoszeń (dodatkowe źródło dopasowania agenta do
//! zadania). Uruchamianie procesu idzie przez cechę `Runner`, więc testy
//! podstawiają zapisane odpowiedzi i nigdy nie uruchamiają prawdziwej Orki.
//!
//! Nic nie startuje samo: `worktree create` wykonuje się tylko po kliknięciu
//! w dialogu; Orkę uruchamia aplikacja tylko przy włączonym „Uruchamiaj Orkę, gdy
//! nie działa” i tylko przy tworzeniu worktree albo odczycie listy repozytoriów
//! po kliknięciu. Tło (mapowanie folderów) tylko czyta `worktree list` i nigdy
//! nie uruchamia Orki.
use crate::agents::{jira_key, norm_folder};
use crate::db;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

pub const NOT_RUNNING: &str = "Orca nie działa. Uruchom Orkę albo włącz „Uruchamiaj Orkę, gdy nie działa” w Ustawieniach → Agenci.";
pub const NOT_FOUND: &str =
    "Nie znaleziono orca.cmd. Podaj ścieżkę w Ustawieniach → Agenci albo zainstaluj Orkę.";
const START_FAILED: &str = "Orca nie uruchomiła się w ciągu 20 sekund.";
const UNREADABLE: &str = "Orca zwróciła nieczytelną odpowiedź.";
/// Najwięcej znaków promptu przekazanego agentowi.
const MAX_PROMPT: usize = 4000;
/// Jak często (ms) tło odświeża mapowanie folderów z `worktree list`.
const MAP_TTL: i64 = 120_000;
const SHORT: Duration = Duration::from_secs(20);
const LONG: Duration = Duration::from_secs(90);

// ---------------------------------------------------------------------------
// Uruchamianie procesu

/// Wynik polecenia.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Uruchamia `orca.cmd`; testy podstawiają zapisane odpowiedzi.
pub trait Runner {
    fn run(&self, program: &Path, args: &[String], timeout: Duration) -> Result<Output, String>;
    /// Przerwa między próbami (testy ją pomijają).
    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Prawdziwy proces, bez okna konsoli.
pub struct Process;

impl Runner for Process {
    fn run(&self, program: &Path, args: &[String], timeout: Duration) -> Result<Output, String> {
        use std::process::{Command, Stdio};
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: bez migającego okna konsoli.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Nie udało się uruchomić orca.cmd: {e}"))?;
        let read = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut text = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut text);
                }
                String::from_utf8_lossy(&text).into_owned()
            })
        };
        let out = read(child.stdout.take().map(|p| Box::new(p) as _));
        let err = read(child.stderr.take().map(|p| Box::new(p) as _));
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Orca nie odpowiedziała w oczekiwanym czasie.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(40)),
                Err(e) => return Err(format!("Błąd procesu orca.cmd: {e}")),
            }
        };
        Ok(Output {
            success: status.success(),
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Ścieżka do orca.cmd

/// Domyślna ścieżka `%LOCALAPPDATA%\Programs\orca\resources\bin\orca.cmd`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join("Programs")
            .join("orca")
            .join("resources")
            .join("bin")
            .join("orca.cmd"),
    )
}

/// Ścieżka z ustawień albo wykryta domyślna; tylko istniejący plik.
pub fn resolve_path(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    let path = if configured.is_empty() {
        default_path()?
    } else {
        PathBuf::from(configured)
    };
    path.is_file().then_some(path)
}

// ---------------------------------------------------------------------------
// Budowa poleceń

/// Argumenty `status --json`.
pub fn args_status() -> Vec<String> {
    vec!["status".into(), "--json".into()]
}
/// Argumenty `open --json` (uruchamia Orkę).
pub fn args_open() -> Vec<String> {
    vec!["open".into(), "--json".into()]
}
/// Argumenty `worktree list --json`.
pub fn args_list() -> Vec<String> {
    vec!["worktree".into(), "list".into(), "--json".into()]
}
/// Argumenty `repo list --json`.
pub fn args_repos() -> Vec<String> {
    vec!["repo".into(), "list".into(), "--json".into()]
}
/// Argumenty `reference find --json <klucz>`.
pub fn args_reference(key: &str) -> Result<Vec<String>, String> {
    let key = safe_token(key, "Zgłoszenie", 300)?;
    Ok(vec![
        "reference".into(),
        "find".into(),
        "--json".into(),
        key,
    ])
}

/// Prośba o utworzenie worktree (dialog na kafelku zadania).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateRequest {
    /// Nazwa worktree.
    pub name: String,
    /// Folder repozytorium (`--repo path:<folder>`).
    pub repo: String,
    /// Klucz Jiry albo adres zgłoszenia (`--reference`).
    pub reference: Option<String>,
    /// `claude` albo `codex`; bez agenta prompt nie jest wysyłany.
    pub agent: Option<String>,
    pub prompt: Option<String>,
    pub base_branch: Option<String>,
    /// `--activate`: Orca przełącza się na nowy worktree.
    pub activate: bool,
}

/// Wartość argumentu bez znaków, które `cmd.exe` interpretuje w pliku `.cmd`
/// (cudzysłów, `%`) i bez znaków sterujących; błąd zamiast cichej zmiany.
fn safe_token(value: &str, what: &str, max: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{what}: pusta wartość."));
    }
    if value.chars().count() > max
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '%'))
    {
        return Err(format!(
            "{what}: niedozwolony znak albo zbyt długa wartość."
        ));
    }
    Ok(value.to_string())
}

/// Prompt w jednej linii (nowe linie to znaki, których `.cmd` nie przyjmie),
/// bez `"` i `%`, do `MAX_PROMPT` znaków.
pub fn clean_prompt(text: &str) -> String {
    let text: String = text
        .chars()
        .filter(|c| *c != '%')
        .map(|c| if c == '"' { '\'' } else { c })
        .collect();
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(MAX_PROMPT).collect()
}

/// Argumenty `worktree create --json …`.
pub fn args_create(req: &CreateRequest) -> Result<Vec<String>, String> {
    let name = safe_token(&req.name, "Nazwa worktree", 100)?;
    let repo = safe_token(&req.repo, "Repozytorium", 1024)?;
    let mut args: Vec<String> = vec![
        "worktree".into(),
        "create".into(),
        "--json".into(),
        "--name".into(),
        name,
        "--repo".into(),
        format!("path:{repo}"),
    ];
    if let Some(base) = req.base_branch.as_deref().filter(|b| !b.trim().is_empty()) {
        args.push("--base-branch".into());
        args.push(safe_token(base, "Gałąź bazowa", 200)?);
    }
    if let Some(reference) = req.reference.as_deref().filter(|r| !r.trim().is_empty()) {
        args.push("--reference".into());
        args.push(safe_token(reference, "Zgłoszenie", 300)?);
    }
    match req.agent.as_deref().map(str::trim) {
        None | Some("") => {}
        Some(agent @ ("claude" | "codex")) => {
            args.push("--agent".into());
            args.push(agent.into());
            let prompt = clean_prompt(req.prompt.as_deref().unwrap_or(""));
            if !prompt.is_empty() {
                args.push("--prompt".into());
                args.push(prompt);
            }
        }
        Some(_) => return Err("Agent musi być claude albo codex.".into()),
    }
    if req.activate {
        args.push("--activate".into());
    }
    Ok(args)
}

/// Argument do pokazania w poleceniu: w cudzysłowie, gdy ma spację lub znak specjalny.
fn display_arg(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_alphanumeric() || "-_.:/\\#@+=,".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

/// Polecenie do podglądu w dialogu: nazwa programu i argumenty.
pub fn command_line(program: &Path, args: &[String]) -> String {
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "orca.cmd".into());
    std::iter::once(name)
        .chain(args.iter().map(|a| display_arg(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Parsowanie odpowiedzi

/// Treść `result` odpowiedzi `--json`; `ok: false` albo `error` daje komunikat Orki.
fn payload(text: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text.trim()).map_err(|_| UNREADABLE.to_string())?;
    let failed = value.get("ok") == Some(&Value::Bool(false)) || value.get("error").is_some();
    if failed {
        let error = value.get("error");
        let message = error
            .and_then(|e| e.get("message").or(Some(e)))
            .and_then(Value::as_str)
            .unwrap_or("nieznany błąd");
        return Err(format!("Orca: {message}"));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

fn text_of<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|k| value.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .find(|s| !s.is_empty())
}

/// Lista pod `key`, gdy wartość to obiekt, albo sama tablica.
fn list_of<'a>(value: &'a Value, keys: &[&str]) -> &'a [Value] {
    if let Some(list) = value.as_array() {
        return list;
    }
    keys.iter()
        .find_map(|k| value.get(*k).and_then(Value::as_array))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Repozytorium znane Orce.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// Worktree z `worktree list` albo `reference find`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub name: String,
    /// Podpięte zgłoszenia: klucze Jiry, adresy albo `#numer`.
    pub references: Vec<String>,
}

/// Odpowiedź `worktree create`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
}

/// `refs/heads/x` → `x`.
fn short_branch(branch: &str) -> String {
    branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .to_string()
}

/// Teksty zgłoszeń podpiętych do worktree (różne kształty: napis, tablica,
/// obiekt z `key` / `id` / `url`).
fn references(item: &Value) -> Vec<String> {
    fn push(out: &mut Vec<String>, value: &Value) {
        match value {
            Value::String(s) if !s.trim().is_empty() => out.push(s.trim().to_string()),
            Value::Number(n) => out.push(format!("#{n}")),
            Value::Array(list) => list.iter().for_each(|v| push(out, v)),
            Value::Object(_) => {
                if let Some(t) = text_of(value, &["key", "id", "identifier", "url", "number"]) {
                    out.push(t.to_string());
                } else if let Some(n) = value.get("id").and_then(Value::as_i64) {
                    out.push(format!("#{n}"));
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for key in [
        "reference",
        "references",
        "linkedIssue",
        "linkedIssues",
        "issue",
        "issues",
    ] {
        if let Some(value) = item.get(key) {
            push(&mut out, value);
        }
    }
    out.dedup();
    out
}

fn worktree_of(item: &Value) -> Option<Worktree> {
    let path = text_of(item, &["path", "worktreePath", "directory"])?.to_string();
    let branch = text_of(item, &["branch", "head"]).map(short_branch);
    let name = text_of(item, &["displayName", "name"])
        .map(str::to_string)
        .or_else(|| branch.clone())
        .unwrap_or_default();
    Some(Worktree {
        path,
        branch,
        name,
        references: references(item),
    })
}

/// Worktree z `worktree list --json` albo `reference find --json`.
pub fn parse_worktrees(text: &str) -> Result<Vec<Worktree>, String> {
    let value = payload(text)?;
    Ok(list_of(&value, &["worktrees", "items", "results"])
        .iter()
        .filter_map(worktree_of)
        .collect())
}

/// Repozytoria z `repo list --json`; bez ścieżki pomijane.
pub fn parse_repos(text: &str) -> Result<Vec<Repo>, String> {
    let value = payload(text)?;
    Ok(list_of(&value, &["repos", "repositories", "items"])
        .iter()
        .filter_map(|item| {
            let path = text_of(item, &["path", "rootPath", "directory"])?.to_string();
            let name = text_of(item, &["displayName", "name"])
                .map(str::to_string)
                .unwrap_or_else(|| {
                    path.trim_end_matches(['\\', '/'])
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .to_string()
                });
            let id = text_of(item, &["id", "repoId"])
                .unwrap_or(&path)
                .to_string();
            Some(Repo { id, name, path })
        })
        .collect())
}

/// Pierwsza wartość logiczna o tej nazwie w całym drzewie JSON.
fn find_bool(value: &Value, key: &str) -> Option<bool> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_bool)
            .or_else(|| map.values().find_map(|v| find_bool(v, key))),
        Value::Array(list) => list.iter().find_map(|v| find_bool(v, key)),
        _ => None,
    }
}

/// Czy Orca działa: nieudane polecenie to „nie działa”; udane z polem `running`
/// bierze to pole, a bez niego uznaje Orkę za działającą.
pub fn parse_status(output: &Output) -> bool {
    if !output.success {
        return false;
    }
    match serde_json::from_str::<Value>(output.stdout.trim()) {
        Ok(value) => {
            if value.get("ok") == Some(&Value::Bool(false)) {
                return false;
            }
            find_bool(&value, "running").unwrap_or(true)
        }
        Err(_) => true,
    }
}

/// Worktree z odpowiedzi `worktree create`.
pub fn parse_created(text: &str) -> Result<Created, String> {
    let value = payload(text)?;
    let item = value.get("worktree").unwrap_or(&value);
    let found = worktree_of(item).ok_or("Orca nie podała ścieżki nowego worktree.")?;
    Ok(Created {
        name: found.name,
        path: found.path,
        branch: found.branch,
    })
}

// ---------------------------------------------------------------------------
// Klucz zgłoszenia i mapowanie folderów

/// Klucz zgłoszenia z tekstu referencji: klucz Jiry (także z adresu `/browse/KEY`),
/// `#1234` albo adres `…/issues/1234` jako `#1234`.
pub fn reference_key(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(key) = jira_key(text) {
        return Some(key);
    }
    let digits = |s: &str| -> Option<String> {
        let n: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        (!n.is_empty() && n.len() <= 10).then_some(n)
    };
    if let Some(rest) = text.strip_prefix('#') {
        return digits(rest).map(|n| format!("#{n}"));
    }
    let at = text.find("/issues/")?;
    digits(&text[at + "/issues/".len()..]).map(|n| format!("#{n}"))
}

/// Folder worktree (znormalizowany) → klucz zgłoszenia, tylko dla worktree
/// z podpiętym zgłoszeniem. Dłuższe foldery pierwsze.
pub fn folder_keys(worktrees: &[Worktree]) -> Vec<(String, String)> {
    let mut map: Vec<(String, String)> = worktrees
        .iter()
        .filter_map(|w| {
            let key = w.references.iter().find_map(|r| reference_key(r))?;
            let folder = norm_folder(&w.path);
            (!folder.is_empty()).then_some((folder, key))
        })
        .collect();
    map.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    map.dedup_by(|a, b| a.0 == b.0);
    map
}

/// Klucz dla folderu roboczego agenta: worktree, w którym (albo w podfolderze
/// którego) pracuje.
pub fn key_for_folder(map: &[(String, String)], cwd: &str) -> Option<String> {
    let cwd = norm_folder(cwd);
    if cwd.is_empty() {
        return None;
    }
    map.iter()
        .find(|(folder, _)| cwd == *folder || cwd.starts_with(&format!("{folder}/")))
        .map(|(_, key)| key.clone())
}

// ---------------------------------------------------------------------------
// Operacje na Orce (przez `Runner`)

fn run_json(
    runner: &dyn Runner,
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let output = runner.run(program, args, timeout)?;
    if !output.success {
        // Orca często zwraca błąd w JSON-ie także na stdout.
        if let Err(message) = payload(&output.stdout) {
            if message != UNREADABLE {
                return Err(message);
            }
        }
        let detail = output.stderr.trim();
        return Err(if detail.is_empty() {
            "Polecenie Orki nie powiodło się.".into()
        } else {
            format!("Orca: {}", detail.lines().next().unwrap_or(detail))
        });
    }
    Ok(output.stdout)
}

/// Czy Orca działa (`status --json`).
pub fn is_running(runner: &dyn Runner, program: &Path) -> bool {
    runner
        .run(program, &args_status(), SHORT)
        .map(|o| parse_status(&o))
        .unwrap_or(false)
}

/// Orca działa albo (przy `autostart`) zostaje uruchomiona i czekamy na nią do
/// `attempts` sekund; inaczej komunikat „Orca nie działa”.
pub fn ensure_running(
    runner: &dyn Runner,
    program: &Path,
    autostart: bool,
    attempts: u32,
) -> Result<(), String> {
    if is_running(runner, program) {
        return Ok(());
    }
    if !autostart {
        return Err(NOT_RUNNING.into());
    }
    run_json(runner, program, &args_open(), LONG)?;
    for _ in 0..attempts {
        runner.pause(Duration::from_secs(1));
        if is_running(runner, program) {
            return Ok(());
        }
    }
    Err(START_FAILED.into())
}

/// Tworzy worktree (po kliknięciu w dialogu).
pub fn create(
    runner: &dyn Runner,
    program: &Path,
    autostart: bool,
    req: &CreateRequest,
) -> Result<Created, String> {
    // Najpierw argumenty: błędna prośba nie uruchamia Orki.
    let args = args_create(req)?;
    ensure_running(runner, program, autostart, 20)?;
    parse_created(&run_json(runner, program, &args, LONG)?)
}

/// Repozytoria Orki do wyboru w dialogu.
pub fn repos(runner: &dyn Runner, program: &Path, autostart: bool) -> Result<Vec<Repo>, String> {
    ensure_running(runner, program, autostart, 20)?;
    parse_repos(&run_json(runner, program, &args_repos(), SHORT)?)
}

/// Worktree podpięte do zgłoszenia (`reference find`).
pub fn find(
    runner: &dyn Runner,
    program: &Path,
    autostart: bool,
    key: &str,
) -> Result<Vec<Worktree>, String> {
    let args = args_reference(key)?;
    ensure_running(runner, program, autostart, 20)?;
    parse_worktrees(&run_json(runner, program, &args, SHORT)?)
}

// ---------------------------------------------------------------------------
// Stan aplikacji i komendy

/// Mapowanie folderów worktree na klucze, odświeżane w tle (tylko odczyt).
#[derive(Default)]
pub struct State(Mutex<Cache>);

#[derive(Default)]
struct Cache {
    at: Option<i64>,
    busy: bool,
    map: Vec<(String, String)>,
}

/// Klucz zgłoszenia worktree, w którym pracuje agent (ostatni odczyt `worktree list`).
pub fn lookup(app: &tauri::AppHandle, cwd: &str) -> Option<String> {
    let state = app.try_state::<State>()?;
    let cache = state.0.lock().ok()?;
    key_for_folder(&cache.map, cwd)
}

/// Odświeża mapowanie, gdy jest starsze niż `MAP_TTL`; w osobnym wątku, bez
/// uruchamiania Orki. Wołać tylko wtedy, gdy jakiś agent nie ma jeszcze klucza.
pub fn refresh_if_due(app: &tauri::AppHandle, now: i64) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    {
        let Ok(mut cache) = state.0.lock() else {
            return;
        };
        if cache.busy || cache.at.is_some_and(|at| now >= at && now - at < MAP_TTL) {
            return;
        }
        cache.busy = true;
        cache.at = Some(now);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let path = resolve_path(&crate::settings::current(&app).orca_path);
        let found = path.and_then(|program| {
            let runner = Process;
            if !is_running(&runner, &program) {
                return None;
            }
            run_json(&runner, &program, &args_list(), SHORT)
                .and_then(|text| parse_worktrees(&text))
                .ok()
        });
        let changed = app
            .try_state::<State>()
            .and_then(|state| {
                let mut cache = state.0.lock().ok()?;
                let map = found.as_deref().map(folder_keys).unwrap_or_default();
                cache.busy = false;
                let changed = cache.map != map;
                cache.map = map;
                Some(changed)
            })
            .unwrap_or(false);
        if changed {
            crate::agents::rematch(&app);
        }
    });
}

/// Stan Orki dla dialogu i Ustawień.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Znaleziono orca.cmd.
    pub found: bool,
    /// Używana ścieżka (pusta, gdy nie znaleziono).
    pub path: String,
    /// Domyślna ścieżka do wykrywania.
    pub default_path: String,
    pub running: bool,
    pub autostart: bool,
}

fn program(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_path(&crate::settings::current(app).orca_path).ok_or_else(|| NOT_FOUND.to_string())
}

#[tauri::command]
pub async fn orca_status(app: tauri::AppHandle) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::current(&app);
        let path = resolve_path(&settings.orca_path);
        let running = path.as_deref().is_some_and(|p| is_running(&Process, p));
        Status {
            found: path.is_some(),
            path: path
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            default_path: default_path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            running,
            autostart: settings.orca_autostart,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Polecenie z podglądu dialogu; nic nie uruchamia.
#[tauri::command]
pub fn orca_command(app: tauri::AppHandle, req: CreateRequest) -> Result<String, String> {
    let args = args_create(&req)?;
    let path = program(&app).unwrap_or_else(|_| PathBuf::from("orca.cmd"));
    Ok(command_line(&path, &args))
}

#[tauri::command]
pub async fn orca_repos(app: tauri::AppHandle) -> Result<Vec<Repo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let program = program(&app)?;
        repos(
            &Process,
            &program,
            crate::settings::current(&app).orca_autostart,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn orca_find(app: tauri::AppHandle, key: String) -> Result<Vec<Worktree>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let program = program(&app)?;
        find(
            &Process,
            &program,
            crate::settings::current(&app).orca_autostart,
            &key,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Tworzy worktree; jedyne miejsce, które uruchamia `worktree create`, wołane
/// wyłącznie z przycisku w dialogu.
#[tauri::command]
pub async fn orca_create(app: tauri::AppHandle, req: CreateRequest) -> Result<Created, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let program = program(&app)?;
        let created = create(
            &Process,
            &program,
            crate::settings::current(&app).orca_autostart,
            &req,
        )?;
        // Nowy worktree z `--reference` ma trafić do mapowania folderów.
        if let Some(state) = app.try_state::<State>() {
            if let Ok(mut cache) = state.0.lock() {
                cache.at = Some(db::now() - MAP_TTL);
            }
        }
        Ok(created)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
