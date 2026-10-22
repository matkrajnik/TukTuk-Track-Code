//! Migracja z Todo Track do TukTuk Track (PLAN_BACKLOG.md, N1).
//!
//! Od 2.0 aplikacja ma nowy identyfikator, więc system szuka danych w nowych
//! miejscach. Przy pierwszym starcie (idempotentnie, z logiem na stderr):
//! 1. katalog danych (baza z plikami `-wal` / `-shm`, kopie zapasowe, stan okien):
//!    kopia do nowego katalogu tylko wtedy, gdy nowy nie ma bazy; stary zostaje jako
//!    zapas z plikiem [`MARKER_FILE`]; gdy oba mają bazę, nic nie jest nadpisywane;
//! 2. dane WebView2 w `%LOCALAPPDATA%` (najlepszym wysiłkiem);
//! 3. poświadczenia Jiry, kalendarza, TS i Redmine: odczyt starego wpisu, zapis
//!    pod nowym identyfikatorem, odczyt kontrolny i dopiero wtedy usunięcie starego;
//! 4. rozszerzenie przeglądarki: usunięcie starego hosta (klucze rejestru i manifest);
//! 5. autostart: wpis `Run` ze starą nazwą zamieniony na nowy z tymi samymi argumentami.
//!
//! Wpisy hooków (`hooks_config`) i serwera MCP (`mcp_register`) usuwa ich własna
//! rejestracja. Katalogi są parametrem, a rejestr i magazyn poświadczeń leżą za
//! cechami [`Registry`] i [`CredentialStore`], więc testy pracują w katalogu
//! tymczasowym i nie dotykają HKCU ani Menedżera poświadczeń.
use crate::hooks_config::legacy_names as old;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Plik bazy w katalogu danych (od 2.0).
pub const DB_FILE: &str = "tuktuk-track.sqlite";
/// Plik w starym katalogu: dane zostały skopiowane, katalog jest zapasem.
pub const MARKER_FILE: &str = "PRZENIESIONO-DO-TUKTUK-TRACK.txt";
/// Klucz w tabeli `settings`: czas (Unix ms) zakończenia migracji bez błędów.
pub const MIGRATED_KEY: &str = "legacy_migrated_at";
/// Klucz w tabeli `settings`: wynik ostatniej migracji dla Ustawień (JSON).
pub const STATUS_KEY: &str = "legacy_status";
/// Klucz w tabeli `settings`: czas (Unix ms) sprawdzenia i usunięcia starego serwera
/// MCP we wszystkich wykrytych CLI; potem rejestracja MCP już go nie szuka.
pub const MCP_CHECKED_KEY: &str = "legacy_mcp_checked_at";
/// Klucz `HKCU` z wpisami uruchamiania z Windows.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

// ---------------------------------------------------------------------------
// Katalog danych

/// Wynik migracji katalogu danych.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DirState {
    /// Nie ma starego katalogu z bazą (świeża instalacja albo już wszystko przeniesione).
    None,
    /// Stary katalog skopiowany do nowego.
    Copied,
    /// Nowy katalog ma bazę, a stary ma znacznik z poprzedniej migracji.
    AlreadyMoved,
    /// Oba katalogi mają bazę (bez znacznika): nic nie nadpisano.
    Both,
    /// Kopiowanie bazy się nie udało; w tym uruchomieniu aplikacja używa starego katalogu.
    Failed,
}

/// Wynik migracji katalogu: skąd aplikacja otwiera bazę w tym uruchomieniu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirReport {
    pub state: DirState,
    /// Katalog z bazą w tym uruchomieniu (nowy; stary tylko przy [`DirState::Failed`]).
    pub data_dir: PathBuf,
    /// Nazwa pliku bazy w `data_dir`.
    pub db_file: &'static str,
    pub warnings: Vec<String>,
}

fn log(message: &str) {
    eprintln!("Migracja z poprzedniej nazwy: {message}");
}

/// Stary katalog obok nowego (ten sam katalog nadrzędny, stary identyfikator).
pub fn old_dir_for(new_dir: &Path) -> Option<PathBuf> {
    new_dir.parent().map(|p| p.join(old::IDENTIFIER))
}

fn marker_text(new_dir: &Path) -> String {
    format!(
        "Dane z {} zostały skopiowane do TukTuk Track (katalog {}).\r\n\
         Ten katalog jest zapasem: TukTuk Track go nie zmienia i nie używa. \
         Po sprawdzeniu, że wszystko jest w nowej aplikacji, można go usunąć.\r\n",
        old::NAME,
        new_dir.display()
    )
}

/// Kopiuje drzewo katalogów; istniejące pliki docelowe zostają. `skip` pomija
/// pliki z katalogu głównego (pliki bazy kopiowane osobno). Błędy trafiają do
/// `warnings`, kopiowanie idzie dalej.
fn copy_tree(src: &Path, dst: &Path, skip: &dyn Fn(&str) -> bool, warnings: &mut Vec<String>) {
    if let Err(e) = std::fs::create_dir_all(dst) {
        warnings.push(format!("{}: {e}", dst.display()));
        return;
    }
    let entries = match std::fs::read_dir(src) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(format!("{}: {e}", src.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if skip(&name_text) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to, &|_| false, warnings);
        } else if !to.exists() {
            if let Err(e) = copy_file_atomic(&from, &to) {
                warnings.push(format!("{}: {e}", from.display()));
            }
        }
    }
}

/// Przyrostek pliku tymczasowego przy kopiowaniu.
const TEMP_SUFFIX: &str = ".kopiowanie";

/// Kopiuje plik przez plik tymczasowy obok celu i zmianę nazwy: pod docelową nazwą
/// leży albo pełna kopia, albo nic (przerwane kopiowanie nie zostawia uciętych kopii
/// zapasowych, które ponowna próba by pominęła).
fn copy_file_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut temp_name = to.file_name().unwrap_or_default().to_os_string();
    temp_name.push(TEMP_SUFFIX);
    let temp = to.with_file_name(temp_name);
    let result = std::fs::copy(from, &temp).and_then(|_| std::fs::rename(&temp, to));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Pliki bazy starego katalogu: główny, `-wal`, `-shm`.
fn old_db_files() -> [String; 3] {
    [
        old::DB_FILE.to_string(),
        format!("{}-wal", old::DB_FILE),
        format!("{}-shm", old::DB_FILE),
    ]
}

/// Migracja katalogu danych (przed otwarciem bazy). Baza kopiowana jako ostatnia i
/// przez plik tymczasowy: nowy katalog „ma bazę” dopiero po pełnej kopii, więc
/// przerwane kopiowanie powtarza się przy następnym starcie.
pub fn migrate_dir(old_dir: &Path, new_dir: &Path) -> DirReport {
    let mut report = DirReport {
        state: DirState::None,
        data_dir: new_dir.to_path_buf(),
        db_file: DB_FILE,
        warnings: Vec::new(),
    };
    if !old_dir.join(old::DB_FILE).is_file() {
        return report;
    }
    if new_dir.join(DB_FILE).is_file() {
        report.state = if old_dir.join(MARKER_FILE).is_file() {
            DirState::AlreadyMoved
        } else {
            DirState::Both
        };
        return report;
    }
    log(&format!(
        "kopiuję dane z {} do {}",
        old_dir.display(),
        new_dir.display()
    ));
    match copy_db_dir(old_dir, new_dir, &mut report.warnings) {
        Ok(()) => {
            report.state = DirState::Copied;
            if let Err(e) = std::fs::write(old_dir.join(MARKER_FILE), marker_text(new_dir)) {
                report
                    .warnings
                    .push(format!("znacznik w starym katalogu: {e}"));
            }
        }
        Err(e) => {
            log(&format!("kopiowanie bazy nie powiodło się: {e}"));
            // Nowy katalog bez pełnej bazy: osierocone pliki WAL by jej zaszkodziły.
            for suffix in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(new_dir.join(format!("{DB_FILE}{suffix}")));
            }
            let _ = std::fs::remove_file(new_dir.join(format!("{DB_FILE}{TEMP_SUFFIX}")));
            report.state = DirState::Failed;
            report.data_dir = old_dir.to_path_buf();
            report.db_file = old_db_name();
            report.warnings.push(e);
        }
    }
    report
}

/// Nazwa pliku bazy w starym katalogu (do otwarcia w trybie awaryjnym).
fn old_db_name() -> &'static str {
    old::DB_FILE
}

fn copy_db_dir(old_dir: &Path, new_dir: &Path, warnings: &mut Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(new_dir).map_err(|e| format!("{}: {e}", new_dir.display()))?;
    let skip_names = old_db_files();
    let marker = MARKER_FILE.to_string();
    copy_tree(
        old_dir,
        new_dir,
        &|name| name == marker || skip_names.iter().any(|n| n == name),
        warnings,
    );
    // Osierocone pliki WAL poprzedniej, przerwanej próby.
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(new_dir.join(format!("{DB_FILE}{suffix}")));
    }
    for suffix in ["-wal", "-shm"] {
        let from = old_dir.join(format!("{}{suffix}", old::DB_FILE));
        if from.is_file() {
            std::fs::copy(&from, new_dir.join(format!("{DB_FILE}{suffix}")))
                .map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    let from = old_dir.join(old::DB_FILE);
    copy_file_atomic(&from, &new_dir.join(DB_FILE)).map_err(|e| format!("{}: {e}", from.display()))
}

/// Dane WebView2 (w tym `localStorage`) z `%LOCALAPPDATA%`: kopia tylko wtedy, gdy
/// nowy katalog jeszcze ich nie ma. Najlepszym wysiłkiem: błędy tylko w ostrzeżeniach.
pub fn migrate_local(old_local: &Path, new_local: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if !old_local.is_dir() || new_local.join("EBWebView").exists() {
        return warnings;
    }
    copy_tree(old_local, new_local, &|_| false, &mut warnings);
    warnings
}

// ---------------------------------------------------------------------------
// Poświadczenia

/// Magazyn poświadczeń (Menedżer poświadczeń Windows); w testach zastępczy.
pub trait CredentialStore {
    /// `Ok(None)`: wpisu nie ma.
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String>;
    fn set(&self, service: &str, user: &str, secret: &str) -> Result<(), String>;
    /// Brak wpisu to nie błąd.
    fn delete(&self, service: &str, user: &str) -> Result<(), String>;
}

/// Menedżer poświadczeń Windows przez `keyring`.
pub struct SystemCredentials;

impl SystemCredentials {
    fn entry(service: &str, user: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(service, user).map_err(|e| e.to_string())
    }
}

impl CredentialStore for SystemCredentials {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        match Self::entry(service, user)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, service: &str, user: &str, secret: &str) -> Result<(), String> {
        Self::entry(service, user)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        match Self::entry(service, user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Wynik przeniesienia jednego poświadczenia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredState {
    /// Moduł nie jest połączony (brak użytkownika w konfiguracji).
    NotConfigured,
    /// Starego wpisu nie ma (np. logowanie wygasło albo już przeniesione).
    NoEntry,
    /// Zapisane pod nowym identyfikatorem, sprawdzone, stary usunięty.
    Moved,
    /// Zapisane i sprawdzone, ale starego nie udało się usunąć (zostaje zapasem).
    MovedKeptOld(String),
    /// Nowy wpis już istnieje z inną wartością: nic nie zmieniono.
    AlreadyThere,
    /// Odczyt, zapis albo odczyt kontrolny nie powiódł się; stary wpis zostaje.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredResult {
    pub kind: &'static str,
    pub state: CredState,
}

/// Przenosi poświadczenia: `users` to (rodzaj, użytkownik z przeniesionej
/// konfiguracji). Stary wpis znika dopiero po odczycie kontrolnym nowego, a błąd
/// jednego rodzaju nie blokuje pozostałych.
pub fn migrate_credentials(
    store: &dyn CredentialStore,
    old_prefix: &str,
    new_prefix: &str,
    users: &[(&'static str, Option<String>)],
) -> Vec<CredResult> {
    users
        .iter()
        .map(|(kind, user)| CredResult {
            kind,
            state: match user {
                None => CredState::NotConfigured,
                Some(user) => move_one(
                    store,
                    &format!("{old_prefix}.{kind}"),
                    &format!("{new_prefix}.{kind}"),
                    user,
                ),
            },
        })
        .collect()
}

fn move_one(store: &dyn CredentialStore, old: &str, new: &str, user: &str) -> CredState {
    let secret = match store.get(old, user) {
        Ok(Some(secret)) => secret,
        Ok(None) => return CredState::NoEntry,
        Err(e) => return CredState::Failed(format!("odczyt: {e}")),
    };
    match store.get(new, user) {
        Err(e) => return CredState::Failed(format!("odczyt nowego wpisu: {e}")),
        Ok(Some(existing)) if existing != secret => return CredState::AlreadyThere,
        Ok(Some(_)) => {}
        Ok(None) => {
            if let Err(e) = store.set(new, user, &secret) {
                return CredState::Failed(format!("zapis: {e}"));
            }
            match store.get(new, user) {
                Ok(Some(back)) if back == secret => {}
                Ok(_) => return CredState::Failed("odczyt kontrolny: inna wartość".into()),
                Err(e) => return CredState::Failed(format!("odczyt kontrolny: {e}")),
            }
        }
    }
    match store.delete(old, user) {
        Ok(()) => CredState::Moved,
        Err(e) => CredState::MovedKeptOld(e),
    }
}

/// Użytkownicy wpisów z przeniesionej konfiguracji (w kolejności [`CREDENTIAL_KINDS`]).
fn credential_users(db: &Connection) -> Vec<(&'static str, Option<String>)> {
    vec![
        ("jira", crate::jira::credential_user(db)),
        ("calendar", crate::calendar::credential_user(db)),
        ("timesheet", crate::timesheet::credential_user(db)),
        ("redmine", crate::redmine::credential_user(db)),
    ]
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "jira" => "Jira",
        "calendar" => "kalendarz Microsoft",
        "timesheet" => "Ideo Timesheet",
        "redmine" => "Redmine",
        _ => "inne",
    }
}

// ---------------------------------------------------------------------------
// Rejestr: rozszerzenie przeglądarki i autostart

/// Dostęp do `HKCU`; w testach zastępczy.
pub trait Registry {
    fn get(&self, key: &str, name: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String>;
    /// Wartość binarna (`REG_BINARY`, np. `StartupApproved`); `Ok(None)`: jej nie ma.
    fn get_binary(&self, key: &str, name: &str) -> Result<Option<Vec<u8>>, String>;
    /// Brak wartości to nie błąd.
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String>;
    /// Usuwa klucz z podkluczami; brak klucza to nie błąd.
    fn delete_key(&self, key: &str) -> Result<(), String>;
}

/// `HKEY_CURRENT_USER`.
#[cfg(windows)]
pub struct Hkcu;

#[cfg(windows)]
impl Registry for Hkcu {
    fn get(&self, key: &str, name: &str) -> Result<Option<String>, String> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(key) else {
            return Ok(None);
        };
        match key.get_value::<String, _>(name) {
            Ok(value) => Ok(Some(value)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn get_binary(&self, key: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(key) else {
            return Ok(None);
        };
        match key.get_raw_value(name) {
            Ok(value) => Ok(Some(value.bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(key)
            .map_err(|e| e.to_string())?;
        key.set_value(name, &value.to_string())
            .map_err(|e| e.to_string())
    }
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        use winreg::{
            enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
            RegKey,
        };
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, KEY_SET_VALUE)
        else {
            return Ok(());
        };
        match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn delete_key(&self, key: &str) -> Result<(), String> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(key) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Poza Windows nie ma rejestru: wszystko jest puste.
#[cfg(not(windows))]
pub struct Hkcu;

#[cfg(not(windows))]
impl Registry for Hkcu {
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn get_binary(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn delete_value(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn delete_key(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Klucz starego hosta native messaging pod `base` (`...\NativeMessagingHosts`).
pub fn old_host_key(base: &str) -> String {
    format!(r"{base}\{}", old::HOST_NAME)
}

/// Plik manifestu starego hosta.
pub fn old_manifest_file() -> String {
    format!("{}.json", old::HOST_NAME)
}

/// Usuwa stary host rozszerzenia: klucze `HKCU` pod każdym `base` i manifest w
/// `manifest_dirs`. Nowego hosta rejestruje `browser::sync` zgodnie z ustawieniem.
/// Zwraca ostrzeżenia (błąd jednego elementu nie przerywa reszty).
pub fn migrate_extension(
    reg: &dyn Registry,
    bases: &[&str],
    manifest_dirs: &[PathBuf],
) -> Vec<String> {
    let mut warnings = Vec::new();
    for base in bases {
        if let Err(e) = reg.delete_key(&old_host_key(base)) {
            warnings.push(format!("rejestr {base}: {e}"));
        }
    }
    for dir in manifest_dirs {
        let path = dir.join(old_manifest_file());
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    warnings
}

/// Wartość nowego wpisu `Run`: nowa ścieżka exe i te same argumenty co w starym wpisie
/// (np. `--hidden`).
pub fn autostart_value(old_value: &str, new_exe: &str) -> String {
    let value = old_value.trim();
    let args = if let Some(rest) = value.strip_prefix('"') {
        rest.split_once('"').map_or("", |(_, args)| args)
    } else {
        value
            .to_ascii_lowercase()
            .find(".exe")
            .map_or("", |i| &value[i + ".exe".len()..])
    }
    .trim();
    let exe = crate::autostart::quoted_path(new_exe);
    if args.is_empty() {
        exe
    } else {
        format!("{exe} {args}")
    }
}

/// Wynik przeniesienia autostartu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutostartState {
    /// Starego wpisu nie było (autostart wyłączony).
    None,
    Moved,
    /// Nowy wpis już istnieje: stary tylko usunięto.
    AlreadyThere,
    /// Stary wpis był wyłączony w Menedżerze zadań: nowego nie utworzono, stary usunięto.
    Disabled,
    Failed(String),
}

/// Wyłączenia wpisów `Run` z Menedżera zadań (`HKCU`).
pub const STARTUP_APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Czy wartość `StartupApproved\Run` oznacza wpis wyłączony w Menedżerze zadań. Ta sama
/// reguła co w `auto-launch` (`is_enabled`): wpis jest włączony, gdy ostatnie osiem
/// bajtów (czas wyłączenia) to zera; krótsza wartość niczego nie wyłącza.
pub fn startup_disabled(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes.iter().rev().take(8).any(|b| *b != 0)
}

/// Zamienia wpis `Run\<stara nazwa>` na `Run\<nowa nazwa>`; stary znika dopiero po
/// odczycie kontrolnym nowego. Wpis wyłączony w Menedżerze zadań nie jest przenoszony
/// (autostart zostaje wyłączony, jak wybrał użytkownik), a stara wartość
/// `StartupApproved\Run` jest usuwana razem ze starym wpisem.
pub fn migrate_autostart(
    reg: &dyn Registry,
    old_name: &str,
    new_name: &str,
    new_exe: &str,
) -> AutostartState {
    let old_value = match reg.get(RUN_KEY, old_name) {
        Ok(Some(value)) => value,
        Ok(None) => {
            // Osierocone wyłączenie bez wpisu `Run` niczemu nie służy.
            return match reg.delete_value(STARTUP_APPROVED_KEY, old_name) {
                Ok(()) => AutostartState::None,
                Err(e) => AutostartState::Failed(e),
            };
        }
        Err(e) => return AutostartState::Failed(e),
    };
    let disabled = match reg.get_binary(STARTUP_APPROVED_KEY, old_name) {
        Ok(bytes) => bytes.is_some_and(|b| startup_disabled(&b)),
        Err(e) => return AutostartState::Failed(e),
    };
    if disabled {
        if let Err(e) = reg.delete_value(RUN_KEY, old_name) {
            return AutostartState::Failed(e);
        }
        return match reg.delete_value(STARTUP_APPROVED_KEY, old_name) {
            Ok(()) => AutostartState::Disabled,
            Err(e) => AutostartState::Failed(e),
        };
    }
    let existing = match reg.get(RUN_KEY, new_name) {
        Ok(existing) => existing,
        Err(e) => return AutostartState::Failed(e),
    };
    let state = if existing.is_some() {
        AutostartState::AlreadyThere
    } else {
        let value = autostart_value(&old_value, new_exe);
        if let Err(e) = reg.set(RUN_KEY, new_name, &value) {
            return AutostartState::Failed(e);
        }
        match reg.get(RUN_KEY, new_name) {
            Ok(Some(back)) if back == value => {}
            Ok(_) => return AutostartState::Failed("odczyt kontrolny: inna wartość".into()),
            Err(e) => return AutostartState::Failed(e),
        }
        AutostartState::Moved
    };
    if let Err(e) = reg.delete_value(RUN_KEY, old_name) {
        return AutostartState::Failed(e);
    }
    match reg.delete_value(STARTUP_APPROVED_KEY, old_name) {
        Ok(()) => state,
        Err(e) => AutostartState::Failed(e),
    }
}

// ---------------------------------------------------------------------------
// Całość i stan dla Ustawień

/// Wynik migracji do pokazania w Ustawieniach.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    /// Czas (Unix ms) zakończenia migracji bez błędów; `None`: jeszcze nie albo z błędami.
    pub migrated_at: Option<i64>,
    /// `none`, `copied`, `alreadyMoved`, `both`, `failed`.
    pub dir: String,
    /// Rodzaje poświadczeń (`jira`, `calendar`, `timesheet`, `redmine`), których logowanie
    /// trzeba połączyć ponownie; sekcja danego modułu w Ustawieniach pokazuje „Połącz ponownie”.
    pub reconnect: Vec<String>,
    /// Zdanie do pokazania w Ustawieniach (pusty tekst: nic do pokazania).
    pub message: String,
}

/// Dane środowiska migracji (po otwarciu bazy).
pub struct Ctx<'a> {
    /// Nowy identyfikator (`pl.tuktuktrack.desktop`).
    pub identifier: &'a str,
    /// Nowa nazwa wpisu `Run` (nazwa produktu).
    pub product_name: &'a str,
    pub exe: &'a Path,
    /// Katalogi, w których mógł leżeć manifest starego hosta.
    pub manifest_dirs: Vec<PathBuf>,
    pub dir: &'a DirReport,
    pub now: i64,
}

fn message(dir: DirState, reconnect: &[String]) -> String {
    let mut parts = Vec::new();
    match dir {
        DirState::Both => parts.push(format!(
            "Znaleziono dane {} i dane TukTuk Track. Używane są dane TukTuk Track, \
             stary katalog danych nie został zmieniony.",
            old::NAME
        )),
        DirState::Failed => parts.push(format!(
            "Nie udało się przenieść danych z {}. W tym uruchomieniu aplikacja używa \
             starego katalogu danych; przeniesienie zostanie ponowione przy następnym starcie.",
            old::NAME
        )),
        _ => {}
    }
    if !reconnect.is_empty() {
        parts.push(format!(
            "Nie udało się przenieść logowania: {}. Połącz te konta ponownie w Ustawieniach.",
            reconnect
                .iter()
                .map(|k| kind_label(k))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    parts.join(" ")
}

fn write_setting(db: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    )
    .map(|_| ())
}

fn read_setting(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
}

/// Czy stary serwer MCP został już sprawdzony i usunięty we wszystkich wykrytych CLI.
pub fn mcp_checked(db: &Connection) -> bool {
    read_setting(db, MCP_CHECKED_KEY).is_some()
}

/// Zapisuje trwały znacznik sprawdzenia starego serwera MCP (czas Unix ms).
pub fn mark_mcp_checked(db: &Connection, now: i64) -> rusqlite::Result<()> {
    write_setting(db, MCP_CHECKED_KEY, &now.to_string())
}

/// Zapisany wynik ostatniej migracji (domyślny, gdy jej jeszcze nie było).
pub fn load_status(db: &Connection) -> Status {
    db.query_row(
        "SELECT value FROM settings WHERE key=?1",
        [STATUS_KEY],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|raw| serde_json::from_str(&raw).ok())
    .unwrap_or_default()
}

fn dir_name(state: DirState) -> &'static str {
    match state {
        DirState::None => "none",
        DirState::Copied => "copied",
        DirState::AlreadyMoved => "alreadyMoved",
        DirState::Both => "both",
        DirState::Failed => "failed",
    }
}

/// Reszta migracji po otwarciu bazy: poświadczenia, rozszerzenie, autostart.
/// Gdy `legacy_migrated_at` jest w bazie, nic się nie dzieje. Znacznik powstaje po
/// przebiegu bez błędów (błąd poświadczeń, autostartu albo kopiowania bazy powtarza
/// próbę przy następnym starcie; stare wpisy zostają do tego czasu).
pub fn run(db: &Connection, ctx: &Ctx, store: &dyn CredentialStore, reg: &dyn Registry) -> Status {
    let done = db
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [MIGRATED_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    if done.is_some() {
        return load_status(db);
    }
    let mut failed = ctx.dir.state == DirState::Failed;
    for warning in &ctx.dir.warnings {
        log(warning);
    }

    let results = migrate_credentials(
        store,
        old::IDENTIFIER,
        ctx.identifier,
        &credential_users(db),
    );
    let mut reconnect = Vec::new();
    for result in &results {
        match &result.state {
            CredState::Failed(e) => {
                log(&format!("poświadczenie {}: {e}", result.kind));
                reconnect.push(result.kind.to_string());
                failed = true;
            }
            CredState::MovedKeptOld(e) => {
                log(&format!(
                    "poświadczenie {}: nie usunięto starego: {e}",
                    result.kind
                ));
            }
            _ => {}
        }
    }

    for warning in migrate_extension(
        reg,
        &[crate::browser::CHROME_HOSTS, crate::browser::EDGE_HOSTS],
        &ctx.manifest_dirs,
    ) {
        log(&format!("rozszerzenie: {warning}"));
    }

    let autostart = migrate_autostart(reg, old::NAME, ctx.product_name, &ctx.exe.to_string_lossy());
    if let AutostartState::Failed(e) = &autostart {
        log(&format!("autostart: {e}"));
        failed = true;
    }

    let mut status = Status {
        migrated_at: (!failed).then_some(ctx.now),
        dir: dir_name(ctx.dir.state).to_string(),
        message: message(ctx.dir.state, &reconnect),
        reconnect,
    };
    if let Ok(raw) = serde_json::to_string(&status) {
        if let Err(e) = write_setting(db, STATUS_KEY, &raw) {
            log(&format!("zapis stanu: {e}"));
        }
    }
    if let Some(at) = status.migrated_at {
        if let Err(e) = write_setting(db, MIGRATED_KEY, &at.to_string()) {
            log(&format!("zapis znacznika: {e}"));
            status.migrated_at = None;
        }
    }
    status
}

/// Migracja katalogu danych `new_dir` (i najlepszym wysiłkiem danych WebView2 w
/// `new_local`) ze starych katalogów obok nich.
pub fn migrate_dirs(new_dir: &Path, new_local: Option<&Path>) -> DirReport {
    let report = match old_dir_for(new_dir) {
        Some(old_dir) => migrate_dir(&old_dir, new_dir),
        None => DirReport {
            state: DirState::None,
            data_dir: new_dir.to_path_buf(),
            db_file: DB_FILE,
            warnings: Vec::new(),
        },
    };
    if report.state == DirState::Copied {
        if let Some(new_local) = new_local {
            if let Some(old_local) = old_dir_for(new_local) {
                for warning in migrate_local(&old_local, new_local) {
                    log(&format!("dane WebView2: {warning}"));
                }
            }
        }
    }
    report
}

/// Migracja wykonana przed `tauri::Builder` (zanim pluginy wczytają swoje pliki).
#[derive(Clone, Debug)]
pub struct Early {
    /// Katalog danych, dla którego liczono migrację.
    pub new_dir: PathBuf,
    pub report: DirReport,
}

/// Katalogi danych aplikacji wyliczone bez `AppHandle`: `%APPDATA%\<identyfikator>`
/// i `%LOCALAPPDATA%\<identyfikator>` (tak jak `app_data_dir`, `app_config_dir` i
/// `app_local_data_dir` Tauri na Windows). `None` poza Windows albo bez zmiennych.
pub fn early_dirs(
    identifier: &str,
    var: &dyn Fn(&str) -> Option<String>,
) -> Option<(PathBuf, Option<PathBuf>)> {
    if !cfg!(windows) {
        return None;
    }
    let roaming = var("APPDATA").filter(|v| !v.is_empty())?;
    let local = var("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(|v| PathBuf::from(v).join(identifier));
    Some((PathBuf::from(roaming).join(identifier), local))
}

/// Migracja katalogu danych przed rejestracją pluginów: `tauri-plugin-window-state`
/// wczytuje `.window-state.json` w swoim `setup`, przed `setup` aplikacji, więc stan
/// okien z poprzedniej nazwy musi już leżeć w nowym katalogu. Bez wyliczonych
/// katalogów migracja odbywa się w `setup` ([`migrate_dirs_native`]).
pub fn migrate_dirs_early(identifier: &str) -> Option<Early> {
    let (new_dir, new_local) = early_dirs(identifier, &|name| std::env::var(name).ok())?;
    let report = migrate_dirs(&new_dir, new_local.as_deref());
    Some(Early { new_dir, report })
}

/// Migracja katalogu danych na prawdziwych ścieżkach aplikacji (przed otwarciem
/// bazy); dane WebView2 najlepszym wysiłkiem. Wynik migracji sprzed `tauri::Builder`
/// jest używany, gdy dotyczył tego samego katalogu.
pub fn migrate_dirs_native(
    app: &tauri::AppHandle,
    early: Option<Early>,
) -> Result<DirReport, String> {
    use tauri::Manager;
    let new_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if let Some(early) = early.filter(|e| e.new_dir == new_dir) {
        return Ok(early.report);
    }
    let new_local = app.path().app_local_data_dir().ok();
    Ok(migrate_dirs(&new_dir, new_local.as_deref()))
}

/// Reszta migracji na prawdziwym rejestrze i Menedżerze poświadczeń.
pub fn run_native(app: &tauri::AppHandle, db: &Connection, dir: &DirReport) -> Status {
    use tauri::Manager;
    let Ok(exe) = std::env::current_exe() else {
        return Status::default();
    };
    let ctx = Ctx {
        identifier: &app.config().identifier,
        product_name: &app.package_info().name,
        exe: &exe,
        manifest_dirs: exe
            .parent()
            .map(Path::to_path_buf)
            .into_iter()
            .chain(app.path().app_local_data_dir().ok())
            .collect(),
        dir,
        now: crate::db::now(),
    };
    run(db, &ctx, &SystemCredentials, &Hkcu)
}

/// Wynik migracji do Ustawień (informacja o dwóch katalogach z danymi, o logowaniach
/// do połączenia ponownie).
#[tauri::command]
pub fn legacy_status(app: tauri::AppHandle) -> Result<Status, String> {
    use tauri::Manager;
    let db = app.state::<crate::Database>();
    let db = db.0.lock().map_err(|e| e.to_string())?;
    Ok(load_status(&db))
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
