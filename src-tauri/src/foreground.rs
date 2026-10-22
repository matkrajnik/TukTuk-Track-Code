//! Podpowiedzi z aktywnego okna (PLAN_1.7.md, W3).
//!
//! Gdy ustawienie `window_suggest` jest włączone (domyślnie nie), pętla agentów co
//! 5 s czyta tytuł i nazwę procesu okna na pierwszym planie (`GetForegroundWindow`,
//! `GetWindowTextW`, `QueryFullProcessImageNameW`). Tytuły trzymamy tylko w pamięci:
//! nie trafiają do bazy ani logów, a wyłączenie ustawienia czyści stan.
//!
//! Rozpoznanie (`recognize`, `resolve_clue`):
//! - przeglądarki, Teams i Outlook: klucz Jiry z tytułu → zadanie jak w
//!   `agents::resolve`;
//! - VS Code, Cursor i JetBrains: nazwa folderu z tytułu → folder z `agent_bindings`
//!   albo ostatnio widziany folder agenta → gałąź git → klucz → zadanie.
//!
//! Wynik to [`Signal`] ze źródłem (`window`, w W4 także `browser`). [`Tracker`]
//! zamienia kolejne sygnały na „pseudoagenta” (`agents::Agent` z `kind` równym
//! źródłu), którego `suggest::Memory` traktuje jak agenta AI: ta sama reguła
//! 2 minut, „Nie teraz”, wyciszone foldery, powiadomienie i kolejka trybu skupienia.
//! Rozszerzenie przeglądarki (W4, `browser`) podaje klucz z adresu karty: gdy
//! przeglądarka jest na pierwszym planie, ma on pierwszeństwo przed tytułem okna
//! (`pick`) i trafia do tego samego `Tracker` ze źródłem `browser`.
//!
//! Timer nigdy nie przełącza się sam: to tylko propozycja.
use crate::agents::{self, Agent, Head, Match};
use crate::settings::AppSettings;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

/// Źródło sygnału: aktywne okno (W3).
pub const WINDOW: &str = "window";
/// Źródło sygnału: rozszerzenie przeglądarki (W4).
pub const BROWSER: &str = "browser";
/// Sygnał bez odświeżenia dłuższy niż ten czas wygasa; powrót do tego samego
/// zadania po dłuższej przerwie liczy 2 minuty od nowa.
pub const HOLD: i64 = 30_000;

/// Czy `kind` to sygnał z okna albo przeglądarki (a nie agent AI).
pub fn is_signal(kind: &str) -> bool {
    kind == WINDOW || kind == BROWSER
}

/// Czy propozycje dla danego źródła są włączone w ustawieniach.
pub fn enabled(kind: &str, s: &AppSettings) -> bool {
    match kind {
        WINDOW => s.window_suggest,
        BROWSER => s.browser_extension,
        _ => s.agent_suggest,
    }
}

/// Okno na pierwszym planie: tytuł i nazwa pliku procesu (np. `chrome.exe`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    pub title: String,
    pub process: String,
}

/// Rodzaj aplikacji, z której czytamy tytuł.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Family {
    /// Przeglądarki, Teams, Outlook: klucz Jiry w tytule.
    Keyed,
    /// VS Code i pochodne: „plik - folder - Visual Studio Code”.
    Code,
    /// JetBrains: „projekt – plik” (starsze: „projekt [C:\ścieżka] – plik”).
    JetBrains,
}

/// Aplikacja po nazwie procesu: nazwa do wyświetlenia i rodzaj.
fn app(process: &str) -> Option<(&'static str, Family)> {
    let name = process
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    Some(match name {
        "chrome" => ("Google Chrome", Family::Keyed),
        "msedge" => ("Microsoft Edge", Family::Keyed),
        "firefox" => ("Firefox", Family::Keyed),
        "brave" => ("Brave", Family::Keyed),
        "opera" => ("Opera", Family::Keyed),
        "vivaldi" => ("Vivaldi", Family::Keyed),
        "arc" => ("Arc", Family::Keyed),
        "jira" => ("Jira", Family::Keyed),
        "ms-teams" | "teams" => ("Microsoft Teams", Family::Keyed),
        "outlook" | "olk" => ("Outlook", Family::Keyed),
        "code" => ("Visual Studio Code", Family::Code),
        "code - insiders" => ("VS Code Insiders", Family::Code),
        "cursor" => ("Cursor", Family::Code),
        "windsurf" => ("Windsurf", Family::Code),
        "idea64" | "idea" => ("IntelliJ IDEA", Family::JetBrains),
        "pycharm64" | "pycharm" => ("PyCharm", Family::JetBrains),
        "webstorm64" | "webstorm" => ("WebStorm", Family::JetBrains),
        "rider64" | "rider" => ("Rider", Family::JetBrains),
        "goland64" | "goland" => ("GoLand", Family::JetBrains),
        "clion64" | "clion" => ("CLion", Family::JetBrains),
        "phpstorm64" | "phpstorm" => ("PhpStorm", Family::JetBrains),
        "rubymine64" | "rubymine" => ("RubyMine", Family::JetBrains),
        "datagrip64" | "datagrip" => ("DataGrip", Family::JetBrains),
        "rustrover64" | "rustrover" => ("RustRover", Family::JetBrains),
        "studio64" | "studio" => ("Android Studio", Family::JetBrains),
        _ => return None,
    })
}

/// Ślad pracy w tytule okna.
#[derive(Clone, Debug, PartialEq)]
pub enum Clue {
    /// Klucz Jiry wprost z tytułu.
    Key(String),
    /// Folder projektu z tytułu edytora: sama nazwa albo pełna ścieżka.
    Folder(String),
}

/// Usuwa końcowy dopisek w nawiasach kwadratowych, np. „[Administrator]”,
/// „[WSL: Ubuntu]”.
fn strip_bracket(text: &str) -> &str {
    let text = text.trim();
    match text.strip_suffix(']').and_then(|t| t.rfind(" [")) {
        Some(at) => text[..at].trim(),
        None => text,
    }
}

/// Folder z tytułu VS Code: przedostatni człon „ - ”.
fn code_folder(title: &str) -> Option<String> {
    let title = strip_bracket(title.trim_start_matches(['●', '•', '*', ' ']));
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let root = strip_bracket(parts[parts.len() - 2]);
    let root = root.strip_suffix("(Workspace)").unwrap_or(root).trim();
    (!root.is_empty()).then(|| root.to_string())
}

/// Folder z tytułu JetBrains: pierwszy człon przed „ – ” (albo „ - ”); ścieżka
/// z nawiasu kwadratowego, gdy tytuł ją ma.
fn jetbrains_folder(title: &str) -> Option<String> {
    let first = title
        .split(" \u{2013} ")
        .next()
        .unwrap_or_default()
        .split(" - ")
        .next()
        .unwrap_or_default()
        .trim();
    if let (Some(open), true) = (first.find(" ["), first.ends_with(']')) {
        let path = first[open + 2..first.len() - 1].trim();
        if absolute(path) {
            return Some(path.to_string());
        }
        let name = first[..open].trim();
        return (!name.is_empty()).then(|| name.to_string());
    }
    (!first.is_empty()).then(|| first.to_string())
}

fn absolute(path: &str) -> bool {
    let b = path.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || path.starts_with("\\\\")
        || path.starts_with('/')
}

/// Rozpoznanie okna: nazwa aplikacji i ślad pracy. `None` dla innych aplikacji
/// i tytułów bez śladu.
pub fn recognize(window: &Window) -> Option<(&'static str, Clue)> {
    let (name, family) = app(&window.process)?;
    let clue = match family {
        Family::Keyed => Clue::Key(agents::jira_key(&window.title)?),
        Family::Code => Clue::Folder(code_folder(&window.title)?),
        Family::JetBrains => Clue::Folder(jetbrains_folder(&window.title)?),
    };
    Some((name, clue))
}

/// Przeglądarki, w których działa rozszerzenie (Chrome, Edge i pochodne Chromium).
pub fn is_browser(process: &str) -> bool {
    matches!(
        app(process),
        Some((
            "Google Chrome" | "Microsoft Edge" | "Brave" | "Opera" | "Vivaldi" | "Arc",
            _
        ))
    )
}

/// Przeglądarka z rozszerzeniem po nazwie procesu, tak jak rozszerzenie podaje ją
/// w polu `browser` (`browser::browser_tag`): `Some("")` dla przeglądarki, której
/// nazwy rozszerzenie nie rozróżnia (np. Arc), `None` dla innych programów.
pub fn browser_tag(process: &str) -> Option<&'static str> {
    if !is_browser(process) {
        return None;
    }
    Some(match app(process)?.0 {
        "Google Chrome" => "chrome",
        "Microsoft Edge" => "edge",
        "Brave" => "brave",
        "Opera" => "opera",
        _ => "",
    })
}

/// Wybór śladu: przeglądarka na pierwszym planie ze świeżym kluczem z rozszerzenia
/// (W4, gdy włączone) ma pierwszeństwo; inaczej tytuł okna (W3, gdy włączone).
/// Wynik: źródło, nazwa aplikacji i ślad.
pub fn pick(
    window: &Window,
    browser_key: Option<&str>,
    s: &AppSettings,
) -> Option<(&'static str, &'static str, Clue)> {
    if s.browser_extension && is_browser(&window.process) {
        if let (Some(key), Some((name, _))) = (browser_key, app(&window.process)) {
            return Some((BROWSER, name, Clue::Key(key.to_string())));
        }
    }
    if !s.window_suggest {
        return None;
    }
    let (name, clue) = recognize(window)?;
    Some((WINDOW, name, clue))
}

/// Ostatni człon ścieżki (bez względu na separator i wielkość liter).
fn leaf(folder: &str) -> String {
    agents::norm_folder(folder)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Folder dla nazwy z tytułu edytora: najpierw przypisania folderów
/// (`agent_bindings`), potem foldery agentów widziane teraz (`seen`), potem
/// ostatnie okresy pracy agentów (`agent_sessions`). Pełna ścieżka z tytułu
/// wygrywa od razu.
pub fn find_folder(db: &Connection, name: &str, seen: &[String]) -> Option<String> {
    let name = name.trim();
    if absolute(name) {
        return Some(name.to_string());
    }
    let wanted = name.to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let bound: Vec<String> = agents::bindings(db)
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.folder)
        .collect();
    let recent: Vec<String> = db
        .prepare(
            "SELECT folder FROM agent_sessions WHERE folder<>'' GROUP BY folder ORDER BY MAX(\"end\") DESC LIMIT 200",
        )
        .and_then(|mut q| {
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    bound
        .into_iter()
        .chain(seen.iter().cloned())
        .chain(recent)
        .find(|f| leaf(f) == wanted)
}

/// Sygnał pracy z okna albo przeglądarki, gotowy do propozycji.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    /// `WINDOW` albo `BROWSER`.
    pub source: &'static str,
    /// Nazwa aplikacji, np. „Google Chrome”.
    pub app: String,
    /// Folder projektu (edytory); pusty dla klucza z tytułu.
    pub folder: String,
    pub branch: Option<String>,
    pub key: Option<String>,
    pub task: Match,
}

/// Dopasowanie śladu do zadania. `read` czyta gałąź folderu (w aplikacji
/// `agents::git_branch`). Bez dopasowanego zadania nie ma sygnału.
pub fn resolve_clue(
    db: &Connection,
    source: &'static str,
    app: &str,
    clue: &Clue,
    seen: &[String],
    read: impl Fn(&Path) -> Option<Head>,
) -> Option<Signal> {
    let (folder, branch, key) = match clue {
        Clue::Key(key) => (String::new(), None, Some(key.clone())),
        Clue::Folder(name) => {
            let folder = find_folder(db, name, seen)?;
            let branch = match read(Path::new(&folder)) {
                Some(Head::Branch(b)) => Some(b),
                _ => None,
            };
            let key = branch.as_deref().and_then(agents::jira_key);
            (folder, branch, key)
        }
    };
    let task = agents::resolve(db, &folder, key.as_deref()).ok()??;
    Some(Signal {
        source,
        app: app.to_string(),
        folder,
        branch,
        key,
        task,
    })
}

fn goal(signal: &Signal) -> String {
    crate::suggest::target(signal.task.task_id, signal.task.key.as_deref())
}

/// Ciągłość sygnału: od kiedy ten sam ślad (folder, zadanie) jest na
/// pierwszym planie i kiedy widziano go ostatnio.
#[derive(Debug, Default)]
pub struct Tracker {
    current: Option<(Signal, i64, i64)>,
}

impl Tracker {
    /// Nowa obserwacja okna. `None` = okno bez śladu: sygnał gaśnie po `HOLD`.
    pub fn observe(&mut self, signal: Option<Signal>, now: i64) {
        let Some(signal) = signal else {
            self.expire(now);
            return;
        };
        let since = match &self.current {
            // To samo zadanie z tytułu okna i z rozszerzenia (W4) liczy się dalej:
            // źródło może się zmienić, gdy sygnał rozszerzenia chwilę nie dotrze.
            Some((old, since, seen))
                if agents::norm_folder(&old.folder) == agents::norm_folder(&signal.folder)
                    && goal(old) == goal(&signal)
                    && now - seen <= HOLD
                    && now >= *seen =>
            {
                *since
            }
            _ => now,
        };
        self.current = Some((signal, since, now));
    }

    /// Okno TukTuk Track na pierwszym planie (np. klik w baner): ostatni sygnał trwa,
    /// ale czas w TukTuk Track nie liczy się do reguły 2 minut: `since` przesuwa się
    /// o tyle samo co `seen`, więc policzony czas pracy w oknie stoi w miejscu.
    pub fn keep(&mut self, now: i64) {
        if let Some((_, since, seen)) = &mut self.current {
            if now - *seen <= HOLD && now >= *seen {
                *since += now - *seen;
                *seen = now;
            }
        }
        self.expire(now);
    }

    fn expire(&mut self, now: i64) {
        if matches!(&self.current, Some((_, _, seen)) if now - seen > HOLD || now < *seen) {
            self.current = None;
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    /// Sygnał jako „pseudoagent” dla `suggest::Memory` (reguła 2 minut liczy się od
    /// `since`).
    pub fn agent(&self, now: i64) -> Option<Agent> {
        let (signal, since, seen) = self.current.as_ref()?;
        if now - seen > HOLD {
            return None;
        }
        let name = match signal.folder.as_str() {
            "" => signal.app.clone(),
            folder => format!(
                "{} · {}",
                signal.app,
                folder
                    .trim_end_matches(['\\', '/'])
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(folder)
            ),
        };
        Some(Agent {
            kind: signal.source.to_string(),
            name,
            cwd: signal.folder.clone(),
            since: *since,
            session: None,
            branch: signal.branch.clone(),
            detached: false,
            key: signal.key.clone(),
            task: Some(signal.task.clone()),
            ..Agent::default()
        })
    }
}

/// Stan w pamięci (bez zapisu tytułów okien).
#[derive(Default)]
pub struct State(Mutex<Tracker>);

/// Rejestruje stan; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Sygnały do propozycji (najwyżej jeden: okno na pierwszym planie).
pub fn current(app: &tauri::AppHandle, now: i64) -> Vec<Agent> {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().and_then(|t| t.agent(now)))
        .into_iter()
        .collect()
}

/// Jeden przebieg (z pętli agentów co 5 s): odczyt okna, rozpoznanie, zapis w
/// pamięci. Przy wyłączonym ustawieniu nic nie czyta. `branch` czyta gałąź folderu
/// (w pętli: przez cache `Branches` z TTL 30 s, jak u agentów).
pub fn observe(
    app: &tauri::AppHandle,
    now: i64,
    mut branch: impl FnMut(&Path) -> Option<agents::Head>,
) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let settings = crate::settings::current(app);
    if !settings.window_suggest && !settings.browser_extension {
        if let Ok(mut t) = state.0.lock() {
            t.clear();
        }
        return;
    }
    // Tytuł okna czytamy tylko przy włączonych podpowiedziach z okien (W3); dla
    // samego rozszerzenia (W4) wystarczy nazwa programu na pierwszym planie.
    let Some((window, own)) = foreground(settings.window_suggest) else {
        if let Ok(mut t) = state.0.lock() {
            t.observe(None, now);
        }
        return;
    };
    if own {
        if let Ok(mut t) = state.0.lock() {
            t.keep(now);
        }
        return;
    }
    // Klucz z rozszerzenia (W4) tylko dla przeglądarki na pierwszym planie.
    let browser_key = (settings.browser_extension && is_browser(&window.process))
        .then(|| crate::browser::fresh_key(app, now, &window.process))
        .flatten();
    let signal =
        pick(&window, browser_key.as_deref(), &settings).and_then(|(source, name, clue)| {
            let seen: Vec<String> = agents::current(app).into_iter().map(|a| a.cwd).collect();
            // Folder szukany pod blokadą bazy, gałąź czytana poza nią (dysk sieciowy).
            let folder = match &clue {
                Clue::Folder(n) => {
                    let db = app.state::<crate::Database>();
                    let db = db.0.lock().ok()?;
                    Some(find_folder(&db, n, &seen)?)
                }
                Clue::Key(_) => None,
            };
            let head = folder.as_deref().and_then(|f| branch(Path::new(f)));
            let clue = folder.map(Clue::Folder).unwrap_or(clue);
            let mut signal = {
                let db = app.state::<crate::Database>();
                let db = db.0.lock().ok()?;
                resolve_clue(&db, source, name, &clue, &[], |_| head.clone())?
            };
            if signal.task.title.is_empty() {
                // `#1234` (Redmine z rozszerzenia, R3): temat z listy Redmine,
                // pozostałe klucze: podsumowanie z listy Jiry, jak dotąd.
                if let Some(key) = signal.task.key.clone() {
                    signal.task.title = crate::suggest::cached_title(app, &key).unwrap_or_default();
                }
            }
            Some(signal)
        });
    let lock = state.0.lock();
    if let Ok(mut t) = lock {
        t.observe(signal, now);
    }
}

/// Okno na pierwszym planie i czy należy do tego procesu (TukTuk Track).
#[cfg(windows)]
fn foreground(read_title: bool) -> Option<(Window, bool)> {
    type Handle = *mut std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> Handle;
        fn GetWindowTextW(hwnd: Handle, text: *mut u16, max: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Handle, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() {
            return Some((Window::default(), true));
        }
        let title = if read_title {
            let mut text = [0u16; 512];
            let len = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
            String::from_utf16_lossy(&text[..len.clamp(0, 512) as usize])
        } else {
            String::new()
        };
        let mut process = String::new();
        let handle = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
        if !handle.is_null() {
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0 {
                let path = String::from_utf16_lossy(&buf[..(size as usize).min(buf.len())]);
                process = path.rsplit('\\').next().unwrap_or_default().to_string();
            }
            CloseHandle(handle);
        }
        Some((Window { title, process }, false))
    }
}
#[cfg(not(windows))]
fn foreground(_read_title: bool) -> Option<(Window, bool)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::suggest::{Memory, MIN_WORK};

    const MIN: i64 = 60_000;
    const NOW: i64 = 100 * MIN;

    fn win(title: &str, process: &str) -> Window {
        Window {
            title: title.into(),
            process: process.into(),
        }
    }

    #[test]
    fn recognizes_sample_titles() {
        // Chrome i Edge z Jirą.
        assert_eq!(
            recognize(&win(
                "[WMS-123] Eksport faktur - Jira - Google Chrome",
                "chrome.exe"
            )),
            Some(("Google Chrome", Clue::Key("WMS-123".into())))
        );
        assert_eq!(
            recognize(&win(
                "WMS-77 Poprawka importu - Jira i 3 inne karty — Profil 1 — Microsoft\u{200b} Edge",
                "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"
            )),
            Some(("Microsoft Edge", Clue::Key("WMS-77".into())))
        );
        // Teams i Outlook.
        assert_eq!(
            recognize(&win(
                "Czat | WMS-5 przegląd | Microsoft Teams",
                "ms-teams.exe"
            )),
            Some(("Microsoft Teams", Clue::Key("WMS-5".into())))
        );
        assert_eq!(
            recognize(&win("RE: ABC-9 termin - Outlook", "OUTLOOK.EXE")),
            Some(("Outlook", Clue::Key("ABC-9".into())))
        );
        // VS Code: plik, folder, nazwa; brudny plik, Workspace, WSL, Administrator.
        assert_eq!(
            recognize(&win("main.rs - wms - Visual Studio Code", "Code.exe")),
            Some(("Visual Studio Code", Clue::Folder("wms".into())))
        );
        assert_eq!(
            recognize(&win(
                "● App.vue - api (Workspace) - Visual Studio Code [Administrator]",
                "Code.exe"
            )),
            Some(("Visual Studio Code", Clue::Folder("api".into())))
        );
        assert_eq!(
            recognize(&win("lib.rs - wms [WSL: Ubuntu] - Cursor", "Cursor.exe")),
            Some(("Cursor", Clue::Folder("wms".into())))
        );
        assert_eq!(
            recognize(&win("wms - Visual Studio Code", "code.exe")),
            Some(("Visual Studio Code", Clue::Folder("wms".into())))
        );
        // IntelliJ: nowy tytuł i starszy z pełną ścieżką.
        assert_eq!(
            recognize(&win("wms \u{2013} Main.java", "idea64.exe")),
            Some(("IntelliJ IDEA", Clue::Folder("wms".into())))
        );
        assert_eq!(
            recognize(&win(
                "wms [C:\\Projekty\\wms] \u{2013} ...\\Main.java [wms-core]",
                "idea64.exe"
            )),
            Some(("IntelliJ IDEA", Clue::Folder("C:\\Projekty\\wms".into())))
        );
        // Brak dopasowania: inna aplikacja, tytuł bez klucza, sam edytor.
        assert_eq!(
            recognize(&win("WMS-1 notatki - Notatnik", "notepad.exe")),
            None
        );
        assert_eq!(
            recognize(&win("Nowa karta - Google Chrome", "chrome.exe")),
            None
        );
        assert_eq!(recognize(&win("Visual Studio Code", "Code.exe")), None);
        assert_eq!(recognize(&win("", "")), None);
    }

    fn open() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tasks(title,created_at) VALUES('Eksport',0),('Import',0),('Przypisane',0);
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-123'),(2,'https://x.atlassian.net','WMS-12');
             INSERT INTO agent_bindings(folder,task_id) VALUES('C:\\Projekty\\api',3);
             INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES('claude','C:\\Projekty\\wms',NULL,0,1000);",
        )
        .unwrap();
        conn
    }
    fn branch(name: &'static str) -> impl Fn(&Path) -> Option<Head> {
        move |_| Some(Head::Branch(name.into()))
    }

    #[test]
    fn resolves_key_folder_binding_and_branch() {
        let db = open();
        // Klucz z tytułu przeglądarki → lokalne zadanie.
        let s = resolve_clue(
            &db,
            WINDOW,
            "Google Chrome",
            &Clue::Key("WMS-123".into()),
            &[],
            |_| None,
        )
        .unwrap();
        assert_eq!((s.task.task_id, s.folder.as_str()), (Some(1), ""));
        // Klucz nieznany: brak sygnału.
        assert!(
            resolve_clue(&db, WINDOW, "Edge", &Clue::Key("XYZ-1".into()), &[], |_| {
                None
            })
            .is_none()
        );
        // VS Code: folder z przypisania wygrywa bez gałęzi.
        let s = resolve_clue(
            &db,
            WINDOW,
            "Visual Studio Code",
            &Clue::Folder("API".into()),
            &[],
            branch("main"),
        )
        .unwrap();
        assert_eq!(
            (s.task.task_id, s.task.source.as_str()),
            (Some(3), "folder")
        );
        assert_eq!(s.folder, "C:\\Projekty\\api");
        // Folder ostatnio widziany u agenta → gałąź → klucz → zadanie.
        let s = resolve_clue(
            &db,
            WINDOW,
            "Cursor",
            &Clue::Folder("wms".into()),
            &[],
            branch("feature/WMS-12-import"),
        )
        .unwrap();
        assert_eq!(s.task.task_id, Some(2));
        assert_eq!(s.key.as_deref(), Some("WMS-12"));
        assert_eq!(s.branch.as_deref(), Some("feature/WMS-12-import"));
        // Folder agenta pracującego teraz (`seen`) i pełna ścieżka z IntelliJ.
        let s = resolve_clue(
            &db,
            WINDOW,
            "IntelliJ IDEA",
            &Clue::Folder("shop".into()),
            &["D:\\repo\\shop".into()],
            branch("WMS-123-x"),
        )
        .unwrap();
        assert_eq!(
            (s.task.task_id, s.folder.as_str()),
            (Some(1), "D:\\repo\\shop")
        );
        let s = resolve_clue(
            &db,
            WINDOW,
            "IntelliJ IDEA",
            &Clue::Folder("C:\\inne\\x".into()),
            &[],
            branch("bugfix/WMS-12"),
        )
        .unwrap();
        assert_eq!(s.task.task_id, Some(2));
        // Nieznany folder, gałąź bez klucza: brak sygnału.
        assert!(resolve_clue(
            &db,
            WINDOW,
            "Code",
            &Clue::Folder("nic".into()),
            &[],
            branch("WMS-1")
        )
        .is_none());
        assert!(resolve_clue(
            &db,
            WINDOW,
            "Code",
            &Clue::Folder("wms".into()),
            &[],
            branch("main")
        )
        .is_none());
    }

    fn signal(task: i64, folder: &str) -> Signal {
        Signal {
            source: WINDOW,
            app: "Google Chrome".into(),
            folder: folder.into(),
            branch: None,
            key: None,
            task: Match {
                task_id: Some(task),
                title: format!("Zadanie {task}"),
                key: None,
                source: "task".into(),
                done: false,
            },
        }
    }
    fn on() -> AppSettings {
        AppSettings {
            window_suggest: true,
            ..AppSettings::default()
        }
    }
    fn pending(t: &Tracker, m: &mut Memory, s: &AppSettings, at: i64) -> Vec<Option<i64>> {
        m.pending(&t.agent(at).into_iter().collect::<Vec<_>>(), None, s, at)
            .into_iter()
            .map(|x| x.task.task_id)
            .collect()
    }

    #[test]
    fn window_signal_follows_the_shared_two_minute_rule() {
        let s = on();
        let mut t = Tracker::default();
        let mut m = Memory::default();
        // Co 5 s to samo okno: propozycja dopiero po MIN_WORK, jak u agentów.
        let mut at = NOW;
        while at < NOW + MIN_WORK {
            t.observe(Some(signal(1, "")), at);
            assert!(pending(&t, &mut m, &s, at).is_empty(), "za wcześnie: {at}");
            at += 5_000;
        }
        t.observe(Some(signal(1, "")), at);
        assert_eq!(pending(&t, &mut m, &s, at), [Some(1)]);
        let agent = t.agent(at).unwrap();
        assert_eq!(
            (agent.kind.as_str(), agent.name.as_str()),
            (WINDOW, "Google Chrome")
        );
        // Przejście na okno TukTuk Track nie gasi propozycji.
        t.keep(at + 20_000);
        assert_eq!(pending(&t, &mut m, &s, at + 40_000), [Some(1)]);
        // Wyłączone ustawienie: brak propozycji z okna (agenci osobno).
        assert!(pending(&t, &mut m, &AppSettings::default(), at + 40_000).is_empty());
        let agents_off = AppSettings {
            agent_suggest: false,
            ..on()
        };
        assert_eq!(pending(&t, &mut m, &agents_off, at + 40_000), [Some(1)]);
    }

    #[test]
    fn time_in_tuktuk_track_does_not_count_toward_the_two_minute_rule() {
        let s = on();
        let mut t = Tracker::default();
        let mut m = Memory::default();
        // 5 s w Chrome z zadaniem, potem ponad 2 min w TukTuk Track: bez propozycji.
        feed(&mut t, signal(1, ""), NOW, NOW + 5_000);
        let mut at = NOW + 10_000;
        while at <= NOW + 5_000 + MIN_WORK + 10_000 {
            t.keep(at);
            assert!(pending(&t, &mut m, &s, at).is_empty(), "za wcześnie: {at}");
            at += 5_000;
        }
        // Powrót do tego samego okna liczy dalej od 5 s (plus jeden przebieg 5 s
        // od ostatniego `keep`), nie od czasu w TukTuk Track.
        let back = at;
        t.observe(Some(signal(1, "")), back);
        assert!(pending(&t, &mut m, &s, back).is_empty());
        assert_eq!(back - t.agent(back).unwrap().since, 10_000);
        feed(
            &mut t,
            signal(1, ""),
            back + 5_000,
            back + MIN_WORK - 15_000,
        );
        assert!(pending(&t, &mut m, &s, back + MIN_WORK - 15_000).is_empty());
        t.observe(Some(signal(1, "")), back + MIN_WORK - 10_000);
        assert_eq!(pending(&t, &mut m, &s, back + MIN_WORK - 10_000), [Some(1)]);
        // Gotowa propozycja nie gaśnie i nie cofa się podczas pobytu w TukTuk Track.
        t.keep(back + MIN_WORK);
        assert_eq!(pending(&t, &mut m, &s, back + MIN_WORK + 5_000), [Some(1)]);
    }

    /// Ten sam sygnał co 5 s od `from` do `to` włącznie.
    fn feed(t: &mut Tracker, s: Signal, from: i64, to: i64) {
        let mut at = from;
        while at <= to {
            t.observe(Some(s.clone()), at);
            at += 5_000;
        }
    }

    #[test]
    fn switching_window_or_long_gap_restarts_the_count() {
        let s = on();
        let mut t = Tracker::default();
        let mut m = Memory::default();
        feed(&mut t, signal(1, ""), NOW, NOW + MIN);
        // Inne zadanie: liczenie od nowa.
        let start = NOW + MIN + 5_000;
        feed(&mut t, signal(2, ""), start, start + MIN_WORK - 5_000);
        assert!(pending(&t, &mut m, &s, start + MIN_WORK - 5_000).is_empty());
        // Krótko okno bez śladu: sygnał trwa, czas się nie zeruje.
        t.observe(None, start + MIN_WORK);
        t.observe(Some(signal(2, "")), start + MIN_WORK + 5_000);
        assert_eq!(pending(&t, &mut m, &s, start + MIN_WORK + 5_000), [Some(2)]);
        // Ponad HOLD bez śladu: sygnał wygasa, a powrót liczy od nowa.
        let later = NOW + 10 * MIN;
        t.observe(None, later);
        assert!(t.agent(later).is_none());
        t.observe(Some(signal(2, "")), later + 5_000);
        assert!(pending(&t, &mut m, &s, later + 5_000).is_empty());
    }

    #[test]
    fn not_now_and_muted_folder_apply_to_window_signals() {
        let s = on();
        let mut t = Tracker::default();
        let mut m = Memory::default();
        feed(&mut t, signal(1, "C:\\Projekty\\wms"), NOW - 5 * MIN, NOW);
        assert_eq!(pending(&t, &mut m, &s, NOW), [Some(1)]);
        let live: Vec<Agent> = t.agent(NOW).into_iter().collect();
        assert!(m.dismiss(&live, "t:1", NOW));
        assert!(pending(&t, &mut m, &s, NOW).is_empty());
        // Wyciszony folder (z podfolderami) dotyczy też sygnału z edytora.
        let muted = AppSettings {
            agent_muted_folders: vec!["C:\\Projekty".into()],
            ..on()
        };
        assert!(pending(&t, &mut Memory::default(), &muted, NOW).is_empty());
        // Powiadomienie ze źródłem „Okno”.
        let list = Memory::default().pending(&live, None, &s, NOW);
        let msg = crate::suggest::message(&list).unwrap();
        assert_eq!(msg.title, "Pracujesz teraz przy innym zadaniu?");
        assert_eq!(msg.body, "Okno: Google Chrome · wms · Zadanie 1");
        assert_eq!(msg.actions[0].argument(), "agent-switch:1");
    }

    #[test]
    fn browser_extension_key_wins_when_browser_is_in_front() {
        let both = AppSettings {
            window_suggest: true,
            browser_extension: true,
            ..AppSettings::default()
        };
        let ext = AppSettings {
            browser_extension: true,
            ..AppSettings::default()
        };
        let chrome = win("[WMS-1] Tablica WMS - Jira - Google Chrome", "chrome.exe");
        // Świeży klucz z rozszerzenia (tablica z selectedIssue) przed tytułem okna.
        assert_eq!(
            pick(&chrome, Some("WMS-45"), &both),
            Some((BROWSER, "Google Chrome", Clue::Key("WMS-45".into())))
        );
        // Bez świeżego klucza (wygasł albo strona bez zgłoszenia): tytuł okna.
        assert_eq!(
            pick(&chrome, None, &both),
            Some((WINDOW, "Google Chrome", Clue::Key("WMS-1".into())))
        );
        // Tylko rozszerzenie: tytuł okna nie jest używany.
        assert_eq!(pick(&chrome, None, &ext), None);
        assert_eq!(
            pick(
                &win(
                    "x",
                    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"
                ),
                Some("WMS-45"),
                &ext
            ),
            Some((BROWSER, "Microsoft Edge", Clue::Key("WMS-45".into())))
        );
        // Rozszerzenie wyłączone: klucz z rozszerzenia pomijany.
        assert_eq!(
            pick(&chrome, Some("WMS-45"), &on()).map(|p| p.2),
            Some(Clue::Key("WMS-1".into()))
        );
        // Na pierwszym planie inny program (VS Code, Firefox bez rozszerzenia).
        let code = win("main.rs - wms - Visual Studio Code", "Code.exe");
        assert_eq!(
            pick(&code, Some("WMS-45"), &both).map(|p| p.0),
            Some(WINDOW)
        );
        assert_eq!(pick(&code, Some("WMS-45"), &ext), None);
        assert_eq!(
            pick(
                &win("Jira - Mozilla Firefox", "firefox.exe"),
                Some("WMS-45"),
                &ext
            ),
            None
        );
    }

    fn browser(task: i64) -> Signal {
        Signal {
            source: BROWSER,
            task: Match {
                key: Some(format!("WMS-{task}")),
                ..signal(task, "").task
            },
            ..signal(task, "")
        }
    }

    #[test]
    fn browser_signal_uses_its_own_switch_and_the_shared_rule() {
        let ext = AppSettings {
            browser_extension: true,
            ..AppSettings::default()
        };
        let mut t = Tracker::default();
        let mut m = Memory::default();
        // 1 min z tytułu okna, potem to samo zadanie z rozszerzenia: liczenie trwa.
        feed(&mut t, signal(7, ""), NOW, NOW + MIN);
        feed(&mut t, browser(7), NOW + MIN + 5_000, NOW + MIN_WORK);
        assert_eq!(t.agent(NOW + MIN_WORK).unwrap().since, NOW);
        assert_eq!(pending(&t, &mut m, &ext, NOW + MIN_WORK), [Some(7)]);
        // Wyłącznik okna nie dotyczy przeglądarki i odwrotnie.
        assert!(pending(&t, &mut m, &on(), NOW + MIN_WORK).is_empty());
        let agent = t.agent(NOW + MIN_WORK).unwrap();
        assert_eq!(agent.kind, BROWSER);
        // Powiadomienie: „Pracujesz teraz przy WMS-7?” ze źródłem „Przeglądarka”.
        let list = Memory::default().pending(&[agent], None, &ext, NOW + MIN_WORK);
        let msg = crate::suggest::message(&list).unwrap();
        assert_eq!(msg.title, "Pracujesz teraz przy WMS-7?");
        assert_eq!(msg.body, "Przeglądarka: Google Chrome · Zadanie 7");
        assert_eq!(msg.actions[0].argument(), "agent-switch:7");
        // Bez odświeżenia przez ponad 30 s sygnał gaśnie.
        t.observe(None, NOW + MIN_WORK + HOLD + 1);
        assert!(t.agent(NOW + MIN_WORK + HOLD + 1).is_none());
    }

    #[test]
    fn not_now_and_notification_survive_a_source_change() {
        let both = AppSettings {
            window_suggest: true,
            browser_extension: true,
            ..AppSettings::default()
        };
        // Propozycja z rozszerzenia: powiadomienie raz, potem „Nie teraz”.
        let mut t = Tracker::default();
        let mut m = Memory::default();
        feed(&mut t, browser(7), NOW, NOW + MIN_WORK);
        let at = NOW + MIN_WORK;
        let live: Vec<Agent> = t.agent(at).into_iter().collect();
        let list = m.pending(&live, None, &both, at);
        assert_eq!(m.unnotified(&list, at).len(), 1);
        assert!(m.dismiss(&live, "t:7", at));
        // Jedno odświeżenie rozszerzenia nie dotarło: to samo zadanie z tytułu okna.
        let window = Signal {
            task: browser(7).task,
            ..signal(7, "")
        };
        t.observe(Some(window.clone()), at + 5_000);
        assert_eq!(t.agent(at + 5_000).unwrap().kind, WINDOW);
        assert!(pending(&t, &mut m, &both, at + 5_000).is_empty());
        // I odwrotnie: powrót do rozszerzenia też nie pokazuje propozycji.
        t.observe(Some(browser(7)), at + 10_000);
        assert!(pending(&t, &mut m, &both, at + 10_000).is_empty());

        // Bez „Nie teraz”: zmiana źródła nie wysyła drugiego powiadomienia.
        let mut t = Tracker::default();
        let mut m = Memory::default();
        feed(&mut t, browser(7), NOW, NOW + MIN_WORK);
        let live: Vec<Agent> = t.agent(at).into_iter().collect();
        let list = m.pending(&live, None, &both, at);
        assert_eq!(m.unnotified(&list, at).len(), 1);
        t.observe(Some(window), at + 5_000);
        let live: Vec<Agent> = t.agent(at + 5_000).into_iter().collect();
        let list = m.pending(&live, None, &both, at + 5_000);
        assert_eq!(list.len(), 1);
        assert!(m.unnotified(&list, at + 5_000).is_empty());
    }
}
