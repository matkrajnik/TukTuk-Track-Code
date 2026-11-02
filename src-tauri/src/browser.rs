//! Rozszerzenie przeglądarki Chrome i Edge (PLAN_1.7.md, W4).
//!
//! Rozszerzenie (`browser-extension/`) wysyła adres i tytuł aktywnej karty, ale
//! tylko dla domen z listy w jego opcjach (domyślnie `*.atlassian.net`). Wiadomości
//! idą przez native messaging do programu `tuktuk-track-bridge.exe`
//! (`src/bin/bridge.rs`), a ten przekazuje je tutaj nazwanym potokiem
//! `\\.\pipe\tuktuk-track-bridge-<użytkownik>` (ramka z `native_msg`).
//!
//! Z wiadomości zostaje w pamięci tylko rozpoznany klucz Jiry i chwila odbioru
//! (adres i tytuł nie są przechowywane, nie trafiają do bazy ani logów). Klucz
//! traci ważność po [`foreground::HOLD`] (30 s) bez odświeżenia. Pętla okna
//! (`foreground::observe`) używa go, gdy przeglądarka jest na pierwszym planie:
//! sygnał z rozszerzenia ma wtedy pierwszeństwo przed tytułem okna i trafia do
//! tej samej reguły propozycji co W3, ze źródłem „Przeglądarka”.
//!
//! Przy włączonym ustawieniu `browser_extension` aplikacja rejestruje hosta
//! w `HKCU\Software\Google\Chrome\NativeMessagingHosts` i
//! `HKCU\Software\Microsoft\Edge\NativeMessagingHosts` (manifest hosta obok exe),
//! a przy wyłączeniu usuwa wpisy i manifest. Bez uprawnień administratora.
//!
//! Timer nigdy nie przełącza się sam: to tylko propozycja.
use crate::foreground::HOLD;
use crate::native_msg::{self, HOST_NAME};
use crate::settings::AppSettings;
use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// Stały identyfikator rozszerzenia z pola `key` w `browser-extension/manifest.json`
/// (pierwsze 16 bajtów SHA-256 klucza publicznego, cyfry szesnastkowe jako a–p).
pub const EXTENSION_ID: &str = "cecpcpkmmmkgfdklnkkkigofkcibnhbl";
/// Rejestr hostów Chrome (pod `HKCU`).
pub const CHROME_HOSTS: &str = r"Software\Google\Chrome\NativeMessagingHosts";
/// Rejestr hostów Edge (pod `HKCU`).
pub const EDGE_HOSTS: &str = r"Software\Microsoft\Edge\NativeMessagingHosts";
/// Nazwa programu hosta obok exe aplikacji.
pub const BRIDGE_EXE: &str = "tuktuk-track-bridge.exe";
/// Domyślna domena (jak w opcjach rozszerzenia).
pub const DEFAULT_DOMAIN: &str = "*.atlassian.net";
/// Rozszerzenie uznajemy za połączone, gdy odezwało się w tym czasie (wysyła stan
/// co 10 s).
pub const CONNECTED_FOR: i64 = 45_000;
/// Najdłuższy adres i tytuł przyjmowany z rozszerzenia.
const MAX_URL: usize = 8192;
const MAX_TITLE: usize = 2048;

/// Czy host pasuje do wzorca: `*.domena` = poddomeny, inaczej dokładnie ta nazwa.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let pattern = pattern.trim().to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(domain) if !domain.is_empty() => host.ends_with(&format!(".{domain}")),
        _ => !pattern.is_empty() && host == pattern,
    }
}

/// Domeny przyjmowane przez aplikację: `*.atlassian.net` i witryny Jiry oraz
/// Redmine, które aplikacja zna (połączone konto i zadania ze zgłoszeniem). Strony spoza tej listy
/// są odrzucane, nawet gdy rozszerzenie ma szerszą listę.
pub fn allowed_patterns(sites: &[String]) -> Vec<String> {
    let mut out = vec![DEFAULT_DOMAIN.to_string()];
    for site in sites {
        let host = Url::parse(site)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        if let Some(host) = host {
            if !out.iter().any(|p| host_matches(&host, p)) {
                out.push(host);
            }
        }
    }
    out
}

/// Klucz w całości (`WMS-123`), bez dodatków.
fn exact_key(text: &str) -> Option<String> {
    let text = text.trim();
    crate::jira::valid_key(text)
        .then(|| crate::agents::jira_key(text))
        .flatten()
        .filter(|k| k == text)
}

/// Rozpoznanie zgłoszenia Jiry z karty: `/browse/KEY`, `selectedIssue=KEY`,
/// `/issues/KEY`, a na końcu klucz w tytule. `None` dla adresów spoza
/// dozwolonych domen, innych schematów niż http(s) i stron bez klucza. Strony
/// zgłoszeń Redmine rozpoznaje [`page_key`] z adresem Redmine (aplikacja
/// używa tylko `page_key`; ta postać zostaje dla testów Jiry).
#[cfg(test)]
pub fn issue_key(url: &str, title: &str, patterns: &[String]) -> Option<String> {
    page_key(url, title, patterns, "")
}

/// Jak [`issue_key`], a do tego strona zgłoszenia Redmine pod adresem
/// `redmine_site` (host, port i prefiks ścieżki z konfiguracji) jako `#1234`.
/// Pusty `redmine_site` = bez Redmine.
pub fn page_key(url: &str, title: &str, patterns: &[String], redmine_site: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    if !patterns.iter().any(|p| host_matches(host, p)) {
        return None;
    }
    let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    let after = |name: &str| {
        segments
            .windows(2)
            .find(|w| w[0] == name)
            .and_then(|w| exact_key(w[1]))
    };
    if let Some(key) = after("browse") {
        return Some(key);
    }
    if let Some(key) = url
        .query_pairs()
        .find(|(k, _)| k == "selectedIssue")
        .and_then(|(_, v)| exact_key(&v))
    {
        return Some(key);
    }
    if let Some(key) = after("issues") {
        return Some(key);
    }
    // Redmine (PLAN_1.9.md, R3): tylko `<prefiks>/issues/1234` pod
    // skonfigurowanym adresem Redmine, jako `#1234`. Inne ścieżki tego samego
    // hosta (np. GitLab pod innym prefiksem) i `*.atlassian.net` są pomijane.
    if !redmine_site.is_empty() && !host_matches(host, DEFAULT_DOMAIN) {
        if let Some(id) = crate::redmine::issue_page(&url, redmine_site) {
            return Some(format!("#{id}"));
        }
    }
    crate::agents::jira_key(title)
}

/// Ostatnia strona z rozszerzenia: tylko rozpoznany klucz i chwila odbioru.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub key: Option<String>,
    pub received: i64,
}

/// Przeglądarka z pola `browser` wiadomości (rozszerzenie bierze ją z
/// `navigator.userAgentData`): tylko znane nazwy, inaczej pusta (nieznana).
pub fn browser_tag(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "chrome" => "chrome",
        "edge" => "edge",
        "brave" => "brave",
        "opera" => "opera",
        _ => "",
    }
}

/// Stan rozszerzenia w pamięci.
#[derive(Debug, Default)]
pub struct Bridge {
    /// Ostatnia dozwolona strona osobno dla każdej przeglądarki (Chrome i Edge
    /// z rozszerzeniem działają jednocześnie). Wpis znika po `clear` z tej
    /// przeglądarki (inna strona) albo przy wyłączonym ustawieniu. „clear” od
    /// przeglądarki w tle (`reason: "background"`) niczego nie zmienia: na
    /// pierwszym planie może być wtedy inna instancja (druga przeglądarka albo
    /// profil), a klucz liczy się tylko dla przeglądarki na pierwszym planie.
    pages: HashMap<&'static str, Page>,
    /// Ostatnia wiadomość od rozszerzenia (dowolna).
    last_message: Option<i64>,
    /// Ostatni rozpoznany klucz i kiedy przyszedł.
    last_key: Option<(String, i64)>,
    /// Skonfigurowany adres Redmine (strony zgłoszeń jako `#1234`); pusty = bez Redmine.
    redmine_site: String,
}

impl Bridge {
    /// Klucz z rozszerzenia bez wskazania przeglądarki (wpis od przeglądarki
    /// o nieznanej nazwie), jeśli świeży.
    pub fn fresh_key(&self, now: i64) -> Option<String> {
        self.fresh_key_for(now, "")
    }

    /// Klucz z rozszerzenia dla przeglądarki `process` (np. `msedge.exe`) na
    /// pierwszym planie, jeśli świeży (najwyżej `HOLD` od odbioru). Wpis tej
    /// przeglądarki, a bez niego wpis od przeglądarki, której rozszerzenie nie
    /// rozpoznało (np. Arc). Wpis innej rozpoznanej przeglądarki się nie liczy.
    pub fn fresh_key_for(&self, now: i64, process: &str) -> Option<String> {
        let tag = crate::foreground::browser_tag(process).unwrap_or("");
        let page = self.pages.get(tag).or_else(|| self.pages.get(""))?;
        if now < page.received || now - page.received > HOLD {
            return None;
        }
        page.key.clone()
    }

    pub fn clear(&mut self) {
        self.pages.clear();
    }

    /// Adres Redmine z konfiguracji dla kolejnych wiadomości (pusty = bez Redmine).
    pub fn set_redmine_site(&mut self, site: &str) {
        site.clone_into(&mut self.redmine_site);
    }

    /// Jedna wiadomość z rozszerzenia (przez host). Zwraca odpowiedź dla
    /// rozszerzenia. `patterns` z [`allowed_patterns`].
    pub fn handle(&mut self, msg: &Value, enabled: bool, patterns: &[String], now: i64) -> Value {
        self.last_message = Some(now);
        if !enabled {
            self.pages.clear();
            return json!({ "ok": false, "connected": true, "enabled": false, "status": "wyłączone" });
        }
        let browser = browser_tag(
            msg.get("browser")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let kind = msg.get("type").and_then(Value::as_str).unwrap_or("page");
        match kind {
            "page" => {
                let url = msg.get("url").and_then(Value::as_str).unwrap_or_default();
                let title = msg.get("title").and_then(Value::as_str).unwrap_or_default();
                if url.len() > MAX_URL || title.len() > MAX_TITLE {
                    self.pages.remove(browser);
                    return json!({ "ok": false, "connected": true, "enabled": true, "error": "Za długi adres albo tytuł." });
                }
                let key = page_key(url, title, patterns, &self.redmine_site);
                if let Some(key) = &key {
                    self.last_key = Some((key.clone(), now));
                }
                self.pages.insert(
                    browser,
                    Page {
                        key: key.clone(),
                        received: now,
                    },
                );
                json!({ "ok": true, "connected": true, "enabled": true, "key": key })
            }
            "clear" => {
                // Przeglądarka w tle nie gasi klucza innej instancji.
                if msg.get("reason").and_then(Value::as_str) != Some("background") {
                    self.pages.remove(browser);
                }
                json!({ "ok": true, "connected": true, "enabled": true, "key": null })
            }
            "ping" | "hello" => json!({ "ok": true, "connected": true, "enabled": true }),
            _ => {
                json!({ "ok": false, "connected": true, "enabled": true, "error": "Nieznany rodzaj wiadomości." })
            }
        }
    }
}

/// Manifest hosta native messaging (`allowed_origins` ze stałym identyfikatorem).
pub fn host_manifest(bridge: &Path) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "TukTuk Track — podpowiedzi z przeglądarki",
        "path": bridge.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    })
}

/// Wpis hosta pod `base` w `HKCU`: domyślna wartość klucza `base\HOST_NAME` to
/// ścieżka manifestu.
#[cfg(windows)]
pub fn register_at(base: &str, manifest: &Path) -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(format!(r"{base}\{HOST_NAME}"))
        .map_err(|e| format!("Nie udało się zapisać rejestru: {e}"))?;
    key.set_value("", &manifest.to_string_lossy().to_string())
        .map_err(|e| format!("Nie udało się zapisać rejestru: {e}"))
}

/// Usuwa wpis hosta spod `base` (brak wpisu to nie błąd).
#[cfg(windows)]
pub fn unregister_at(base: &str) -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!(r"{base}\{HOST_NAME}")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Nie udało się usunąć wpisu z rejestru: {e}")),
    }
}

/// Ścieżka manifestu zapisana pod `base`, jeśli wpis istnieje.
#[cfg(windows)]
pub fn registered_at(base: &str) -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"{base}\{HOST_NAME}"))
        .and_then(|k| k.get_value::<String, _>(""))
        .ok()
}

#[cfg(not(windows))]
pub fn register_at(_base: &str, _manifest: &Path) -> Result<(), String> {
    Err("Rozszerzenie przeglądarki działa tylko w Windows.".into())
}
#[cfg(not(windows))]
pub fn unregister_at(_base: &str) -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
pub fn registered_at(_base: &str) -> Option<String> {
    None
}

/// Rejestruje hosta pod wszystkimi `bases` (manifest musi już istnieć).
pub fn register_all(bases: &[&str], manifest: &Path) -> Result<(), String> {
    for base in bases {
        register_at(base, manifest)?;
    }
    Ok(())
}

/// Usuwa wpisy spod wszystkich `bases`; zwraca pierwszy błąd po próbie każdego.
pub fn unregister_all(bases: &[&str]) -> Result<(), String> {
    let mut first = Ok(());
    for base in bases {
        if let Err(e) = unregister_at(base) {
            if first.is_ok() {
                first = Err(e);
            }
        }
    }
    first
}

const BASES: [&str; 2] = [CHROME_HOSTS, EDGE_HOSTS];
const MANIFEST_FILE: &str = "pl.tuktuktrack.bridge.json";

/// Co ile pętla w tle ponawia nieudaną rejestrację hosta.
pub const RETRY: i64 = 60_000;

/// Stan rejestracji hosta w rejestrze.
#[derive(Debug, Default)]
pub struct Registration {
    /// Ostatnio udanie zastosowany stan (`None` = jeszcze nie albo błąd).
    pub applied: Option<bool>,
    /// Ostatnia próba (do ponowień z pętli w tle).
    pub last_try: i64,
}

impl Registration {
    /// Czy próbować teraz: ustawienie różni się od udanej rejestracji; ponowienie
    /// z pętli w tle (`periodic`) dopiero po [`RETRY`] od ostatniej próby.
    pub fn due(&self, enabled: bool, now: i64, periodic: bool) -> bool {
        self.applied != Some(enabled)
            && (!periodic || now < self.last_try || now - self.last_try >= RETRY)
    }
}

/// Stan aplikacji: rozszerzenie w pamięci i wynik ostatniej rejestracji.
#[derive(Default)]
pub struct State {
    bridge: Mutex<Bridge>,
    registration: Mutex<Registration>,
    error: Mutex<String>,
}

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Katalog exe aplikacji (tam leży `tuktuk-track-bridge.exe`).
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Miejsca manifestu: obok exe, a gdy tam nie da się pisać, w danych aplikacji.
fn manifest_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    exe_dir()
        .into_iter()
        .chain(app.path().app_local_data_dir().ok())
        .collect()
}

fn write_manifest(app: &tauri::AppHandle, bridge: &Path) -> Result<PathBuf, String> {
    let body = serde_json::to_vec_pretty(&host_manifest(bridge)).map_err(|e| e.to_string())?;
    let mut last = String::new();
    for dir in manifest_dirs(app) {
        let path = dir.join(MANIFEST_FILE);
        let _ = std::fs::create_dir_all(&dir);
        match std::fs::write(&path, &body) {
            Ok(()) => return Ok(path),
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("Nie udało się zapisać manifestu hosta: {last}"))
}

/// Rejestracja przy włączonym ustawieniu, usunięcie przy wyłączonym.
pub fn apply(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        let bridge = exe_dir()
            .map(|d| d.join(BRIDGE_EXE))
            .filter(|p| p.is_file())
            .ok_or_else(|| {
                format!(
                    "Brak programu {BRIDGE_EXE} obok aplikacji. Zainstaluj TukTuk Track ponownie."
                )
            })?;
        let manifest = write_manifest(app, &bridge)?;
        register_all(&BASES, &manifest)
    } else {
        let result = unregister_all(&BASES);
        for dir in manifest_dirs(app) {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILE));
        }
        if let Some(state) = app.try_state::<State>() {
            if let Ok(mut b) = state.bridge.lock() {
                b.clear();
            }
        }
        result
    }
}

/// Stosuje ustawienie, gdy różni się od ostatnio udanej rejestracji (start
/// aplikacji, każdy zapis ustawień, przywrócenie kopii). Błąd trafia do stanu
/// (`browser_status`), a nieudana rejestracja zostaje do ponowienia.
pub fn sync(app: &tauri::AppHandle, s: &AppSettings) {
    attempt(app, s.browser_extension, crate::db::now(), false);
}

/// Ponowienie po błędzie z pętli w tle (najwyżej raz na [`RETRY`]).
pub fn retry(app: &tauri::AppHandle, now: i64) {
    let due = app
        .try_state::<State>()
        .and_then(|s| s.registration.lock().ok().map(|r| r.applied.is_none()))
        .unwrap_or(false);
    if due {
        attempt(
            app,
            crate::settings::current(app).browser_extension,
            now,
            true,
        );
    }
}

fn attempt(app: &tauri::AppHandle, enabled: bool, now: i64, periodic: bool) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    {
        let Ok(mut reg) = state.registration.lock() else {
            return;
        };
        if !reg.due(enabled, now, periodic) {
            return;
        }
        reg.last_try = now;
    }
    let result = apply(app, enabled);
    if let Ok(mut reg) = state.registration.lock() {
        reg.applied = result.is_ok().then_some(enabled);
    }
    if let Ok(mut e) = state.error.lock() {
        *e = result.err().unwrap_or_default();
    };
}

/// Świeży klucz z rozszerzenia dla przeglądarki `process` na pierwszym planie
/// (dla `foreground::observe`).
pub fn fresh_key(app: &tauri::AppHandle, now: i64, process: &str) -> Option<String> {
    let state = app.try_state::<State>()?;
    let bridge = state.bridge.lock().ok()?;
    bridge.fresh_key_for(now, process)
}

/// Witryny znane aplikacji: Jira (połączone konto i zadania z kluczem) oraz
/// Redmine (skonfigurowany adres i zadania z numerem, PLAN_1.9.md R3), a obok
/// skonfigurowany adres Redmine (pusty bez konfiguracji) do rozpoznawania stron
/// zgłoszeń: tylko pod nim `/issues/1234` jest zgłoszeniem Redmine.
fn known_sites(app: &tauri::AppHandle) -> (Vec<String>, String) {
    let mut sites: Vec<String> = crate::jira::config(app)
        .map(|c| vec![c.site])
        .unwrap_or_default();
    let redmine_site = crate::redmine::config(app)
        .map(|c| c.site)
        .unwrap_or_default();
    if !redmine_site.is_empty() {
        sites.push(redmine_site.clone());
    }
    if let Ok(db) = app.state::<crate::Database>().0.lock() {
        for query in [
            "SELECT DISTINCT site FROM jira_tasks LIMIT 50",
            "SELECT DISTINCT site FROM redmine_tasks LIMIT 50",
        ] {
            if let Ok(mut q) = db.prepare(query) {
                if let Ok(rows) = q.query_map([], |r| r.get::<_, String>(0)) {
                    sites.extend(rows.flatten());
                }
            }
        }
    }
    (sites, redmine_site)
}

/// Odpowiedź na jedną ramkę od hosta.
fn on_message(app: &tauri::AppHandle, body: &[u8]) -> Value {
    let Ok(msg) = serde_json::from_slice::<Value>(body) else {
        return json!({ "ok": false, "error": "Niepoprawna wiadomość." });
    };
    let settings = crate::settings::current(app);
    let (sites, redmine_site) = known_sites(app);
    let patterns = allowed_patterns(&sites);
    let Some(state) = app.try_state::<State>() else {
        return json!({ "ok": false, "error": "Aplikacja jeszcze startuje." });
    };
    let reply = match state.bridge.lock() {
        Ok(mut b) => {
            b.set_redmine_site(&redmine_site);
            b.handle(
                &msg,
                settings.browser_extension,
                &patterns,
                crate::db::now(),
            )
        }
        Err(_) => json!({ "ok": false, "error": "Błąd stanu aplikacji." }),
    };
    reply
}

/// Stan dla Ustawień → Czas pracy.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// Wpisy hosta są w rejestrze Chrome i Edge.
    pub registered: bool,
    /// Rozszerzenie odezwało się w ostatnich 45 s.
    pub connected: bool,
    pub last_message: Option<i64>,
    pub last_key: Option<String>,
    pub last_key_at: Option<i64>,
    pub error: String,
    pub extension_id: String,
    /// Folder rozszerzenia do „Załaduj rozpakowane” (instalacja albo repozytorium).
    pub extension_folder: Option<String>,
}

/// Folder `browser-extension` z instalacji (zasoby NSIS) albo z repozytorium (dev).
fn extension_folder(app: &tauri::AppHandle) -> Option<PathBuf> {
    let installed = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join("browser-extension"));
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("browser-extension"));
    installed
        .into_iter()
        .chain(dev)
        .find(|p| p.join("manifest.json").is_file())
}

#[tauri::command]
pub fn browser_status(app: tauri::AppHandle) -> Status {
    let s = crate::settings::current(&app);
    let now = crate::db::now();
    let mut status = Status {
        enabled: s.browser_extension,
        registered: BASES.iter().all(|b| registered_at(b).is_some()),
        extension_id: EXTENSION_ID.into(),
        extension_folder: extension_folder(&app).map(|p| p.to_string_lossy().into_owned()),
        ..Status::default()
    };
    if let Some(state) = app.try_state::<State>() {
        if let Ok(b) = state.bridge.lock() {
            status.last_message = b.last_message;
            status.connected = s.browser_extension
                && b.last_message
                    .is_some_and(|t| now >= t && now - t <= CONNECTED_FOR);
            if let Some((key, at)) = &b.last_key {
                status.last_key = Some(key.clone());
                status.last_key_at = Some(*at);
            }
        }
        if let Ok(e) = state.error.lock() {
            status.error = e.clone();
        }
    }
    status
}

/// Otwiera folder rozszerzenia w Eksploratorze.
#[tauri::command]
pub fn browser_open_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let folder = extension_folder(&app).ok_or("Nie znaleziono folderu rozszerzenia.")?;
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Nie udało się otworzyć folderu: {e}"))
}

/// Serwer nazwanego potoku: każde połączenie hosta to jedna ramka i odpowiedź.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = pipe::serve(&native_msg::pipe_name(), move |body| on_message(&app, body)) {
            eprintln!("Rozszerzenie przeglądarki: potok niedostępny ({e}).");
        }
    });
}

#[cfg(windows)]
pub(crate) mod pipe {
    use crate::native_msg::win::{SecurityAttributes, TimedReader, UserOnly};
    use crate::native_msg::{read_frame, write_frame};
    use serde_json::Value;
    use std::ffi::c_void;
    use std::fs::File;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    type Handle = *mut c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_size: u32,
            in_size: u32,
            timeout: u32,
            security: *mut c_void,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn FlushFileBuffers(file: Handle) -> i32;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    const PIPE_ACCESS_DUPLEX: u32 = 0x3;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const ERROR_PIPE_CONNECTED: u32 = 535;

    /// Instancja potoku dostępna tylko dla bieżącego użytkownika i SYSTEM.
    fn create(name: &[u16], first: bool, security: &UserOnly) -> Result<Handle, String> {
        let mut attributes = security.attributes();
        let flags = PIPE_ACCESS_DUPLEX
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        // Tryb bajtowy, blokujący (PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT = 0).
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                (&mut attributes as *mut SecurityAttributes).cast(),
            )
        };
        if h.is_null() || h as isize == -1 {
            return Err(format!("CreateNamedPipeW: {}", unsafe { GetLastError() }));
        }
        Ok(h)
    }

    /// Najdłuższy czas na odebranie ramki od klienta. Połączenie, które nic nie
    /// wysyła (albo urywa ramkę), jest zamykane, zamiast trzymać wątek.
    pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
    /// Najwięcej obsługiwanych naraz połączeń; nadmiarowe są od razu zamykane.
    const MAX_CLIENTS: usize = 16;
    /// Odstęp ponowień, gdy nie da się utworzyć kolejnej instancji potoku.
    const RETRY_DELAY: Duration = Duration::from_secs(1);

    /// Jedna ramka (jak `native_msg::read_frame`) z limitem czasu.
    fn read_frame_by(file: &mut File, deadline: Instant) -> Option<Vec<u8>> {
        read_frame(&mut TimedReader::new(file, deadline))
            .ok()
            .flatten()
    }

    /// Kolejna instancja potoku; przy błędzie (np. brak zasobów) próbuje dalej co
    /// [`RETRY_DELAY`] zamiast kończyć serwer.
    fn create_next(name: &[u16], security: &UserOnly) -> Handle {
        let mut reported = false;
        loop {
            match create(name, false, security) {
                Ok(h) => return h,
                Err(e) => {
                    if !reported {
                        eprintln!("Rozszerzenie przeglądarki: {e}, ponawiam.");
                        reported = true;
                    }
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }

    /// Obsługa połączonego klienta w osobnym wątku (najwyżej [`MAX_CLIENTS`] naraz).
    fn dispatch(
        pipe: Handle,
        timeout: Duration,
        active: &Arc<AtomicUsize>,
        handle: &Arc<dyn Fn(&[u8]) -> Value + Send + Sync>,
    ) {
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENTS {
            active.fetch_sub(1, Ordering::SeqCst);
            unsafe {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
            }
            return;
        }
        let addr = pipe as usize;
        let handle = handle.clone();
        let active = active.clone();
        std::thread::spawn(move || {
            let mut file = unsafe { File::from_raw_handle(addr as Handle) };
            if let Some(body) = read_frame_by(&mut file, Instant::now() + timeout) {
                let reply = serde_json::to_vec(&handle(&body)).unwrap_or_default();
                if write_frame(&mut file, &reply).is_ok() {
                    unsafe { FlushFileBuffers(file.as_raw_handle() as Handle) };
                }
            }
            unsafe { DisconnectNamedPipe(file.as_raw_handle() as Handle) };
            drop(file);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// Pętla serwera. Zawsze czeka co najmniej jedna instancja potoku, więc inny
    /// proces nie przejmie nazwy. Błąd tylko wtedy, gdy nazwa jest już zajęta.
    pub fn serve(
        name: &str,
        handle: impl Fn(&[u8]) -> Value + Send + Sync + 'static,
    ) -> Result<(), String> {
        serve_with(name, READ_TIMEOUT, handle)
    }

    pub fn serve_with(
        name: &str,
        timeout: Duration,
        handle: impl Fn(&[u8]) -> Value + Send + Sync + 'static,
    ) -> Result<(), String> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let handle: Arc<dyn Fn(&[u8]) -> Value + Send + Sync> = Arc::new(handle);
        let active = Arc::new(AtomicUsize::new(0));
        let security = UserOnly::new().map_err(|e| format!("zabezpieczenia potoku: {e}"))?;
        let mut pipe = create(&wide, true, &security)?;
        loop {
            let ok = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            // Zwykle kolejna instancja powstaje przed oddaniem bieżącej; gdy się nie
            // da, bieżący klient jest obsłużony, a tworzenie ponawiane.
            let next = create(&wide, false, &security).ok();
            if ok {
                dispatch(pipe, timeout, &active, &handle);
            } else {
                unsafe { CloseHandle(pipe) };
            }
            pipe = next.unwrap_or_else(|| create_next(&wide, &security));
        }
    }
}

#[cfg(not(windows))]
pub(crate) mod pipe {
    use serde_json::Value;
    pub fn serve(_name: &str, _handle: impl Fn(&[u8]) -> Value) -> Result<(), String> {
        Err("tylko Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_msg::{encode, read_frame, write_frame, FrameError, MAX};
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    fn jira() -> Vec<String> {
        allowed_patterns(&["https://jira.firma.pl".into()])
    }

    #[test]
    fn recognizes_jira_addresses() {
        let p = jira();
        // Jira Cloud: zgłoszenie.
        assert_eq!(
            issue_key("https://asistonwms.atlassian.net/browse/WMS-123", "", &p).as_deref(),
            Some("WMS-123")
        );
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/browse/WMS-7?focusedCommentId=1#c",
                "",
                &p
            )
            .as_deref(),
            Some("WMS-7")
        );
        // Tablica z otwartym zgłoszeniem.
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/jira/software/c/projects/WMS/boards/12?selectedIssue=WMS-45&quickFilter=3",
                "Tablica WMS - Jira",
                &p
            )
            .as_deref(),
            Some("WMS-45")
        );
        // Nowa nawigacja zgłoszeń i lista z filtrem.
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/jira/software/c/projects/WMS/issues/WMS-9?jql=project%3DWMS",
                "",
                &p
            )
            .as_deref(),
            Some("WMS-9")
        );
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/issues/ABC2-10?filter=10001",
                "",
                &p
            )
            .as_deref(),
            Some("ABC2-10")
        );
        // Filtr bez otwartego zgłoszenia: klucza nie ma ani w adresie, ani w tytule.
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/issues/?filter=10001",
                "Filtry - Jira",
                &p
            ),
            None
        );
        // Klucz w tytule, gdy adres go nie ma.
        assert_eq!(
            issue_key(
                "https://x.atlassian.net/jira/your-work",
                "[WMS-77] Eksport faktur - Jira",
                &p
            )
            .as_deref(),
            Some("WMS-77")
        );
        // Adres ma pierwszeństwo przed tytułem.
        assert_eq!(
            issue_key("https://x.atlassian.net/browse/WMS-1", "WMS-2 coś", &p).as_deref(),
            Some("WMS-1")
        );
        // Witryna Jiry znana aplikacji (serwer firmowy).
        assert_eq!(
            issue_key("https://jira.firma.pl/browse/OPS-3", "", &p).as_deref(),
            Some("OPS-3")
        );
        // Inne domeny odrzucone, także z kluczem w adresie i w tytule.
        for url in [
            "https://github.com/firma/wms/pull/12",
            "https://evil.example/browse/WMS-1",
            "https://atlassian.net.evil.example/browse/WMS-1",
            "https://atlassian.net/browse/WMS-1",
            "file:///C:/x.atlassian.net/browse/WMS-1",
            "chrome://settings",
            "nie adres",
        ] {
            assert_eq!(issue_key(url, "WMS-1 tytuł", &p), None, "{url}");
        }
        // Śmieci w miejscu klucza.
        assert_eq!(
            issue_key("https://x.atlassian.net/browse/wms-1", "", &p),
            None
        );
        assert_eq!(
            issue_key("https://x.atlassian.net/browse/WMS-1x", "", &p),
            None
        );
    }

    #[test]
    fn recognizes_redmine_issue_pages_as_hash_number() {
        let site = "https://redmine.example.pl";
        let p = allowed_patterns(&[site.into()]);
        let key = |url: &str, title: &str| page_key(url, title, &p, site);
        assert_eq!(
            key("https://redmine.example.pl/issues/1234", "").as_deref(),
            Some("#1234")
        );
        // Parametry, fragment, dalsza ścieżka i ten sam Redmine przez http.
        assert_eq!(
            key(
                "https://redmine.example.pl/issues/1234?x=1#note-3",
                "Funkcja #1234: Import - Redmine"
            )
            .as_deref(),
            Some("#1234")
        );
        assert_eq!(
            key("https://redmine.example.pl/issues/77/edit", "").as_deref(),
            Some("#77")
        );
        assert_eq!(
            key("http://Redmine.Example.pl/issues/5", "").as_deref(),
            Some("#5")
        );
        // Nie numer albo nie strona zgłoszenia: brak klucza (tytuł bez klucza Jiry).
        for url in [
            "https://redmine.example.pl/issues/abc",
            "https://redmine.example.pl/issues/",
            "https://redmine.example.pl/issues/0",
            "https://redmine.example.pl/issues/12345678901",
            "https://redmine.example.pl/projects/wms/issues",
            "https://redmine.example.pl/issues/new",
            "https://redmine.example.pl/projects/wms/issues/12",
            "https://redmine.example.pl:8443/issues/12",
        ] {
            assert_eq!(key(url, "Zgłoszenia - Redmine"), None, "{url}");
        }
        // Host spoza listy.
        assert_eq!(key("https://inny.example.pl/issues/1234", ""), None);
        assert_eq!(
            key("https://redmine.example.pl.evil.example/issues/1234", ""),
            None
        );
        // Jira bez zmian: klucz Jiry pod `/issues/` wygrywa, a numer na
        // `*.atlassian.net` nie jest zgłoszeniem Redmine.
        assert_eq!(
            key("https://x.atlassian.net/issues/WMS-9", "").as_deref(),
            Some("WMS-9")
        );
        assert_eq!(key("https://x.atlassian.net/issues/1234", ""), None);
        // Bez znanego Redmine numer nie jest rozpoznawany.
        assert_eq!(
            issue_key("https://redmine.example.pl/issues/1234", "", &jira()),
            None
        );
        assert_eq!(
            issue_key("https://redmine.example.pl/issues/1234", "", &p),
            None
        );
    }

    #[test]
    fn redmine_under_a_path_prefix_on_a_shared_host() {
        // Redmine pod `/redmine` na hoście dzielonym z innymi aplikacjami: host
        // jest dozwolony w całości, ale zgłoszeniem jest tylko `/redmine/issues/N`.
        let site = "https://firma.example.pl/redmine";
        let p = allowed_patterns(&[site.into()]);
        let key = |url: &str| page_key(url, "", &p, site);
        assert_eq!(
            key("https://firma.example.pl/redmine/issues/1234?x=1").as_deref(),
            Some("#1234")
        );
        assert_eq!(
            key("https://firma.example.pl/redmine/issues/42/time_entries").as_deref(),
            Some("#42")
        );
        for url in [
            // GitLab na tym samym hoście.
            "https://firma.example.pl/gitlab/grp/proj/-/issues/42",
            // Ścieżka bez prefiksu i z innym prefiksem.
            "https://firma.example.pl/issues/42",
            "https://firma.example.pl/redmine2/issues/42",
            "https://firma.example.pl/inny/redmine/issues/42",
            "https://firma.example.pl/redmine/projects/wms/issues/42",
        ] {
            assert_eq!(key(url), None, "{url}");
        }
        // Stary host z `redmine_tasks` (po zmianie adresu Redmine) jest wśród
        // dozwolonych domen, ale jego strony nie dają numeru.
        let p = allowed_patterns(&[site.into(), "https://stary.example.pl".into()]);
        assert_eq!(
            page_key("https://stary.example.pl/issues/42", "", &p, site),
            None
        );
        // Wiadomość z rozszerzenia: adres Redmine z konfiguracji w stanie mostu.
        let mut b = Bridge::default();
        let page =
            |url: &str| json!({ "type": "page", "url": url, "title": "", "browser": "chrome" });
        let reply = b.handle(
            &page("https://firma.example.pl/redmine/issues/7"),
            true,
            &p,
            1_000,
        );
        assert_eq!(reply["key"], Value::Null);
        b.set_redmine_site(site);
        let reply = b.handle(
            &page("https://firma.example.pl/redmine/issues/7"),
            true,
            &p,
            2_000,
        );
        assert_eq!(reply["key"], "#7");
        let reply = b.handle(
            &page("https://firma.example.pl/gitlab/grp/proj/-/issues/7"),
            true,
            &p,
            3_000,
        );
        assert_eq!(reply["key"], Value::Null);
    }

    #[test]
    fn domain_patterns() {
        assert!(host_matches("asistonwms.atlassian.net", "*.atlassian.net"));
        assert!(host_matches("A.B.Atlassian.NET.", "*.atlassian.net"));
        assert!(!host_matches("atlassian.net", "*.atlassian.net"));
        assert!(!host_matches("xatlassian.net", "*.atlassian.net"));
        assert!(host_matches("jira.firma.pl", "jira.firma.pl"));
        assert!(!host_matches("x.jira.firma.pl", "jira.firma.pl"));
        assert!(!host_matches("x", "*."));
        // Witryny z Jiry: bez powtórzeń z *.atlassian.net, z nazwą hosta.
        assert_eq!(
            allowed_patterns(&[
                "https://x.atlassian.net".into(),
                "https://Jira.Firma.pl/".into(),
                "zły adres".into(),
            ]),
            vec!["*.atlassian.net".to_string(), "jira.firma.pl".into()]
        );
    }

    #[test]
    fn native_messaging_frame() {
        let body = br#"{"url":"https://x.atlassian.net/browse/WMS-1","title":"t","at":1}"#;
        let frame = encode(body).unwrap();
        // 4 bajty długości little-endian + JSON.
        assert_eq!(&frame[..4], &(body.len() as u32).to_le_bytes());
        assert_eq!(&frame[4..], body);
        assert_eq!(frame[0] as usize, body.len());
        // Odczyt kilku ramek po kolei i koniec strumienia.
        let mut stream = Vec::new();
        write_frame(&mut stream, body).unwrap();
        write_frame(&mut stream, b"{}").unwrap();
        let mut r = Cursor::new(stream);
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), body);
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"{}");
        assert!(read_frame(&mut r).unwrap().is_none());
        // Limit 1 MB: dokładnie MAX przechodzi, więcej nie (zapis i odczyt).
        let big = vec![b' '; MAX];
        assert_eq!(encode(&big).unwrap().len(), MAX + 4);
        assert!(matches!(
            encode(&vec![b' '; MAX + 1]),
            Err(FrameError::TooLarge(_))
        ));
        // Za duża ramka jest pomijana, a następna czyta się poprawnie.
        let mut stream = ((MAX + 1) as u32).to_le_bytes().to_vec();
        stream.extend(vec![b'x'; MAX + 1]);
        stream.extend(encode(b"{\"a\":1}").unwrap());
        let mut r = Cursor::new(stream);
        assert!(matches!(read_frame(&mut r), Err(FrameError::TooLarge(n)) if n == MAX + 1));
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"{\"a\":1}");
        // Urwana długość i urwana treść.
        assert!(matches!(
            read_frame(&mut Cursor::new(vec![5u8, 0])),
            Err(FrameError::Truncated)
        ));
        let mut cut = 10u32.to_le_bytes().to_vec();
        cut.extend(b"{}");
        assert!(matches!(
            read_frame(&mut Cursor::new(cut)),
            Err(FrameError::Truncated)
        ));
        // Nazwa potoku z użytkownikiem.
        assert_eq!(
            native_msg::pipe_name_for("Jan Kowalski"),
            r"\\.\pipe\tuktuk-track-bridge-Jan_Kowalski"
        );
        assert_eq!(
            native_msg::pipe_name_for(""),
            r"\\.\pipe\tuktuk-track-bridge-user"
        );
    }

    #[test]
    fn signal_expires_after_30_seconds_without_refresh() {
        let p = jira();
        let mut b = Bridge::default();
        let page = json!({ "type": "page", "url": "https://x.atlassian.net/browse/WMS-5", "title": "", "at": 0 });
        let reply = b.handle(&page, true, &p, 1_000);
        assert_eq!(reply["key"], "WMS-5");
        assert_eq!(b.fresh_key(1_000).as_deref(), Some("WMS-5"));
        assert_eq!(b.fresh_key(1_000 + HOLD).as_deref(), Some("WMS-5"));
        assert_eq!(b.fresh_key(1_000 + HOLD + 1), None);
        // Odświeżenie przedłuża ważność.
        b.handle(&page, true, &p, 20_000);
        assert_eq!(b.fresh_key(1_000 + HOLD + 1).as_deref(), Some("WMS-5"));
        // Zegar cofnięty: nieważne.
        assert_eq!(b.fresh_key(19_999), None);
        // „clear” (inna strona, przeglądarka w tle) gasi od razu.
        b.handle(&json!({ "type": "clear" }), true, &p, 21_000);
        assert_eq!(b.fresh_key(21_000), None);
        // Strona spoza domen: bez klucza, ale ostatni klucz zostaje w stanie.
        b.handle(
            &json!({ "type": "page", "url": "https://github.com/WMS-5", "title": "WMS-5" }),
            true,
            &p,
            22_000,
        );
        assert_eq!(b.fresh_key(22_000), None);
        assert_eq!(b.last_key, Some(("WMS-5".into(), 20_000)));
        // Wyłączone ustawienie: odpowiedź „wyłączone”, nic nie zostaje.
        let off = b.handle(&page, false, &p, 23_000);
        assert_eq!(off["enabled"], false);
        assert_eq!(b.fresh_key(23_000), None);
        assert_eq!(b.last_message, Some(23_000));
        // Za długi adres odrzucony.
        let long = json!({ "type": "page", "url": format!("https://x.atlassian.net/browse/WMS-5?{}", "a".repeat(9000)) });
        assert_eq!(b.handle(&long, true, &p, 24_000)["ok"], false);
        assert_eq!(b.fresh_key(24_000), None);
    }

    #[test]
    fn background_clear_from_another_browser_keeps_the_key() {
        let p = jira();
        let mut b = Bridge::default();
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        let edge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";
        let page = |browser: &str| json!({ "type": "page", "browser": browser, "url": "https://x.atlassian.net/jira/software/c/projects/WMS/boards/1?selectedIssue=WMS-45", "title": "Tablica" });
        let clear = |browser: &str, reason: &str| json!({ "type": "clear", "browser": browser, "reason": reason });
        // Chrome na pierwszym planie z tablicą, Edge (Startup Boost) w tle co 10 s
        // wysyła „clear” z powodem „background”: klucz Chrome zostaje.
        b.handle(&page("chrome"), true, &p, 1_000);
        b.handle(&clear("edge", "background"), true, &p, 2_000);
        assert_eq!(b.fresh_key_for(2_000, chrome).as_deref(), Some("WMS-45"));
        b.handle(&page("chrome"), true, &p, 11_000);
        b.handle(&clear("edge", "background"), true, &p, 12_000);
        assert_eq!(b.fresh_key_for(12_000, chrome).as_deref(), Some("WMS-45"));
        // Drugi profil Chrome w tle (ta sama przeglądarka) też nie gasi klucza.
        b.handle(&clear("chrome", "background"), true, &p, 13_000);
        assert_eq!(b.fresh_key_for(13_000, chrome).as_deref(), Some("WMS-45"));
        // Edge na pierwszym planie nie dostaje klucza z Chrome.
        assert_eq!(b.fresh_key_for(13_000, edge), None);
        b.handle(&page("edge"), true, &p, 14_000);
        assert_eq!(b.fresh_key_for(14_000, edge).as_deref(), Some("WMS-45"));
        // „clear” z przeglądarki na pierwszym planie (inna strona) gasi tylko ją.
        b.handle(&clear("chrome", "page"), true, &p, 15_000);
        assert_eq!(b.fresh_key_for(15_000, chrome), None);
        assert_eq!(b.fresh_key_for(15_000, edge).as_deref(), Some("WMS-45"));
        // Przeglądarka bez rozpoznanej nazwy (np. Arc): wpis bez nazwy.
        b.handle(&page(""), true, &p, 16_000);
        assert_eq!(
            b.fresh_key_for(16_000, "Arc.exe").as_deref(),
            Some("WMS-45")
        );
        assert_eq!(b.fresh_key_for(16_000, chrome).as_deref(), Some("WMS-45"));
        assert_eq!(crate::foreground::browser_tag("brave.exe"), Some("brave"));
        assert_eq!(crate::foreground::browser_tag("Code.exe"), None);
        assert_eq!(browser_tag("Firefox"), "");
        // Wyłączenie czyści wszystkie przeglądarki.
        b.handle(&page("edge"), false, &p, 17_000);
        assert_eq!(b.fresh_key_for(17_000, edge), None);
        assert_eq!(b.fresh_key_for(17_000, "Arc.exe"), None);
    }

    #[test]
    fn failed_registration_is_retried() {
        let mut r = Registration::default();
        // Start: jeszcze nic nie zastosowano.
        assert!(r.due(true, 1_000, false));
        assert!(r.due(true, RETRY, true));
        // Błąd (np. brak tuktuk-track-bridge.exe): `applied` zostaje puste. Zapis
        // ustawień próbuje od razu, pętla w tle dopiero po RETRY.
        r.last_try = 1_000;
        r.applied = None;
        assert!(r.due(true, 2_000, false));
        assert!(!r.due(true, 2_000, true));
        assert!(r.due(true, 1_000 + RETRY, true));
        // Udana rejestracja: bez ponowień, dopóki ustawienie się nie zmieni.
        r.applied = Some(true);
        assert!(!r.due(true, 1_000 + 10 * RETRY, true));
        assert!(!r.due(true, 1_000 + 10 * RETRY, false));
        assert!(r.due(false, 1_000 + 10 * RETRY, false));
    }

    #[test]
    fn extension_id_matches_manifest_key() {
        let manifest: Value =
            serde_json::from_str(include_str!("../../browser-extension/manifest.json")).unwrap();
        let key = base64::engine::general_purpose::STANDARD
            .decode(manifest["key"].as_str().unwrap())
            .unwrap();
        let hash = Sha256::digest(&key);
        let id: String = hash[..16]
            .iter()
            .flat_map(|b| [b >> 4, b & 0xf])
            .map(|n| (b'a' + n) as char)
            .collect();
        assert_eq!(id, EXTENSION_ID);
        assert_eq!(manifest["manifest_version"], 3);
        let host = host_manifest(Path::new(
            r"C:\Programy\TukTuk Track\tuktuk-track-bridge.exe",
        ));
        assert_eq!(host["name"], HOST_NAME);
        assert_eq!(host["type"], "stdio");
        assert_eq!(
            host["allowed_origins"],
            json!([format!("chrome-extension://{EXTENSION_ID}/")])
        );
        assert_eq!(
            host["path"],
            r"C:\Programy\TukTuk Track\tuktuk-track-bridge.exe"
        );
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_round_trip() {
        let name =
            native_msg::pipe_name_for(&format!("test-{}-{}", std::process::id(), crate::db::now()));
        let server = name.clone();
        std::thread::spawn(move || {
            let _ = pipe::serve(&server, |body| {
                let msg: Value = serde_json::from_slice(body).unwrap_or_default();
                json!({ "ok": true, "echo": msg["type"] })
            });
        });
        // Klient jak w `tuktuk-track-bridge`: otwarcie potoku, ramka, odpowiedź.
        let open = || {
            for _ in 0..100 {
                if let Ok(f) = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&name)
                {
                    return f;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("potok niedostępny");
        };
        for kind in ["page", "clear"] {
            let mut f = open();
            write_frame(&mut f, format!("{{\"type\":\"{kind}\"}}").as_bytes()).unwrap();
            let reply: Value =
                serde_json::from_slice(&read_frame(&mut f).unwrap().unwrap()).unwrap();
            assert_eq!(reply, json!({ "ok": true, "echo": kind }));
        }
        // Druga instancja serwera pod tą samą nazwą nie przejmie potoku.
        assert!(pipe::serve(&name, |_| json!({})).is_err());
    }

    /// Serwer testowy pod unikalną nazwą; zwraca nazwę, gdy potok już przyjmuje klientów.
    #[cfg(windows)]
    fn spawn_server(
        prefix: &str,
        handle: impl Fn(&[u8]) -> Value + Send + Sync + 'static,
    ) -> String {
        let name = native_msg::pipe_name_for(&format!(
            "{prefix}-{}-{}",
            std::process::id(),
            crate::db::now()
        ));
        let server = name.clone();
        std::thread::spawn(move || {
            let _ = pipe::serve(&server, handle);
        });
        // Otwarte i od razu zamknięte połączenie serwer po prostu pomija.
        drop(open_pipe(&name));
        name
    }

    /// Połączenie z potokiem testowym; ponawia, póki serwer nie wystawi instancji.
    #[cfg(windows)]
    fn open_pipe(name: &str) -> std::fs::File {
        for _ in 0..100 {
            if let Ok(f) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(name)
            {
                return f;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("potok niedostępny");
    }

    #[cfg(windows)]
    #[test]
    fn pipe_dacl_allows_only_current_user_and_system() {
        let name = spawn_server("dacl", |_| json!({ "ok": true }));
        let f = open_pipe(&name);
        let sid = native_msg::win::current_user_string().unwrap();
        // GA dla obiektu pliku Windows zapisuje jako FA (FILE_ALL_ACCESS).
        assert_eq!(
            native_msg::win::dacl_sddl(&f).unwrap(),
            format!("D:P(A;;FA;;;{sid})(A;;FA;;;SY)")
        );
        // Właściciel to użytkownik: na tym opiera się sprawdzenie klienta, gdy
        // aplikacja działa z podwyższonymi uprawnieniami.
        assert!(native_msg::win::pipe_owner_is_current_user(&f).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn exchange_verifies_server_and_returns_reply() {
        let name = spawn_server("exchange", |body| {
            let msg: Value = serde_json::from_slice(body).unwrap_or_default();
            json!({ "echo": msg["type"] })
        });
        let reply = native_msg::exchange(&name, br#"{"type":"page"}"#).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&reply).unwrap(),
            json!({ "echo": "page" })
        );
    }

    #[cfg(windows)]
    #[test]
    fn exchange_gives_up_when_app_does_not_answer() {
        use std::time::{Duration, Instant};
        let name = spawn_server("stuck", |_| {
            std::thread::sleep(Duration::from_secs(3));
            json!({ "ok": true })
        });
        let started = Instant::now();
        let err = native_msg::exchange_with(&name, b"{}", Duration::from_millis(300)).unwrap_err();
        assert!(
            matches!(&err, FrameError::Io(e) if e.kind() == std::io::ErrorKind::TimedOut),
            "{err}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(windows)]
    #[test]
    fn idle_pipe_client_is_dropped_after_timeout() {
        use std::time::{Duration, Instant};
        let name =
            native_msg::pipe_name_for(&format!("idle-{}-{}", std::process::id(), crate::db::now()));
        let server = name.clone();
        std::thread::spawn(move || {
            let _ = pipe::serve_with(
                &server,
                Duration::from_millis(300),
                |_| json!({ "ok": true }),
            );
        });
        let open = || {
            for _ in 0..100 {
                if let Ok(f) = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&name)
                {
                    return f;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            panic!("potok niedostępny");
        };
        // Klienci, którzy łączą się i nic nie wysyłają (albo urywają ramkę), nie
        // blokują innych i zostają rozłączeni po limicie czasu.
        let mut idle: Vec<std::fs::File> = (0..3).map(|_| open()).collect();
        let mut cut = open();
        std::io::Write::write_all(&mut cut, &10u32.to_le_bytes()).unwrap();
        idle.push(cut);
        let mut f = open();
        write_frame(&mut f, b"{}").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&read_frame(&mut f).unwrap().unwrap()).unwrap(),
            json!({ "ok": true })
        );
        let started = Instant::now();
        for mut c in idle {
            assert!(!matches!(read_frame(&mut c), Ok(Some(_))));
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(windows)]
    #[test]
    fn registers_and_removes_hosts_on_test_branch() {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let root = format!(
            r"Software\TukTukTrackTest\browser-{}-{}",
            std::process::id(),
            crate::db::now()
        );
        let chrome = format!(r"{root}\Google\Chrome\NativeMessagingHosts");
        let edge = format!(r"{root}\Microsoft\Edge\NativeMessagingHosts");
        let bases = [chrome.as_str(), edge.as_str()];
        let manifest = Path::new(r"C:\Programy\TukTuk Track\pl.tuktuktrack.bridge.json");
        assert_eq!(registered_at(&chrome), None);
        register_all(&bases, manifest).unwrap();
        for base in bases {
            assert_eq!(
                registered_at(base).as_deref(),
                Some(r"C:\Programy\TukTuk Track\pl.tuktuktrack.bridge.json")
            );
        }
        // Ponowna rejestracja nadpisuje ścieżkę (np. po aktualizacji).
        register_all(&bases, Path::new(r"D:\inne\m.json")).unwrap();
        assert_eq!(registered_at(&edge).as_deref(), Some(r"D:\inne\m.json"));
        unregister_all(&bases).unwrap();
        for base in bases {
            assert_eq!(registered_at(base), None);
        }
        // Usunięcie nieistniejących wpisów to nie błąd; klucz bazowy zostaje.
        unregister_all(&bases).unwrap();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        assert!(hkcu.open_subkey(&chrome).is_ok());
        hkcu.delete_subkey_all(&root).unwrap();
        let _ = hkcu.delete_subkey(r"Software\TukTukTrackTest");
    }
}
