//! Migracja z poprzedniej nazwy aplikacji (N1): katalogi w katalogu tymczasowym,
//! poświadczenia na zastępczym magazynie i rejestr w pamięci. Żaden test nie dotyka
//! prawdziwego HKCU, Menedżera poświadczeń ani danych użytkownika.
use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tuktuk-track-legacy-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

// ---------------------------------------------------------------------------
// Katalog danych

/// Stary katalog z bazą (prawdziwy plik SQLite z jednym zadaniem), WAL, kopią i stanem okien.
fn old_data(root: &Path) -> PathBuf {
    let dir = root.join(old::IDENTIFIER);
    std::fs::create_dir_all(&dir).unwrap();
    let db = Connection::open(dir.join(old::DB_FILE)).unwrap();
    crate::db::init(&db).unwrap();
    db.execute(
        "INSERT INTO tasks(title,created_at) VALUES('Stare zadanie',1)",
        [],
    )
    .unwrap();
    drop(db);
    write(&dir.join(format!("{}-wal", old::DB_FILE)), "wal");
    write(&dir.join(format!("{}-shm", old::DB_FILE)), "shm");
    write(&dir.join("backups").join("kopia.sqlite"), "kopia");
    write(&dir.join(".window-state.json"), "{\"main\":{}}");
    dir
}

fn title_in(db_path: &Path) -> String {
    Connection::open(db_path)
        .unwrap()
        .query_row("SELECT title FROM tasks", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn dir_without_old_directory_does_nothing() {
    let root = temp("brak-starego");
    let new = root.join("nowy");
    let report = migrate_dir(&root.join(old::IDENTIFIER), &new);
    assert_eq!(report.state, DirState::None);
    assert_eq!(report.data_dir, new);
    assert_eq!(report.db_file, DB_FILE);
    assert!(!new.exists());
}

#[test]
fn dir_with_old_directory_without_database_does_nothing() {
    let root = temp("stary-bez-bazy");
    write(&root.join(old::IDENTIFIER).join("inny.txt"), "x");
    let new = root.join("nowy");
    assert_eq!(
        migrate_dir(&root.join(old::IDENTIFIER), &new).state,
        DirState::None
    );
    assert!(!new.exists());
}

#[test]
fn dir_copies_old_data_when_new_has_no_database() {
    let root = temp("kopia");
    let old_dir = old_data(&root);
    let new = root.join("pl.tuktuktrack.desktop");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Copied, "{:?}", report.warnings);
    assert_eq!(report.data_dir, new);
    // Pliki WAL i SHM pod nową nazwą (sprawdzone przed otwarciem bazy, które je sprząta),
    // reszta katalogu bez zmian.
    assert_eq!(read(&new.join(format!("{DB_FILE}-wal"))), "wal");
    assert_eq!(read(&new.join(format!("{DB_FILE}-shm"))), "shm");
    assert_eq!(title_in(&new.join(DB_FILE)), "Stare zadanie");
    assert_eq!(read(&new.join("backups").join("kopia.sqlite")), "kopia");
    assert_eq!(read(&new.join(".window-state.json")), "{\"main\":{}}");
    assert!(!new.join(old::DB_FILE).exists());
    assert!(!new.join(format!("{DB_FILE}.kopiowanie")).exists());
    // Stary katalog zostaje jako zapas ze znacznikiem.
    assert_eq!(title_in(&old_dir.join(old::DB_FILE)), "Stare zadanie");
    assert!(read(&old_dir.join(MARKER_FILE)).contains("TukTuk Track"));
    assert!(!new.join(MARKER_FILE).exists());
}

#[test]
fn dir_keeps_existing_files_in_new_directory_without_database() {
    let root = temp("istniejace");
    let old_dir = old_data(&root);
    let new = root.join("nowy");
    write(&new.join(".window-state.json"), "{\"nowy\":true}");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Copied);
    assert_eq!(read(&new.join(".window-state.json")), "{\"nowy\":true}");
    assert_eq!(read(&new.join("backups").join("kopia.sqlite")), "kopia");
}

#[test]
fn dir_retry_replaces_partial_copies_from_interrupted_attempt() {
    let root = temp("przerwana");
    let old_dir = old_data(&root);
    let new = root.join("nowy");
    // Przerwana wcześniejsza próba: ucięty plik tymczasowy, bez bazy.
    write(&new.join("backups").join("kopia.sqlite.kopiowanie"), "uc");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Copied, "{:?}", report.warnings);
    assert_eq!(read(&new.join("backups").join("kopia.sqlite")), "kopia");
    assert!(!new.join("backups").join("kopia.sqlite.kopiowanie").exists());
}

#[test]
fn early_migration_paths_match_tauri_directories() {
    let vars = |name: &str| match name {
        "APPDATA" => Some(r"C:\Users\ala\AppData\Roaming".to_string()),
        "LOCALAPPDATA" => Some(r"C:\Users\ala\AppData\Local".to_string()),
        _ => None,
    };
    let dirs = early_dirs("pl.tuktuktrack.desktop", &vars);
    if cfg!(windows) {
        let (data, local) = dirs.unwrap();
        assert_eq!(
            data,
            PathBuf::from(r"C:\Users\ala\AppData\Roaming").join("pl.tuktuktrack.desktop")
        );
        assert_eq!(
            local,
            Some(PathBuf::from(r"C:\Users\ala\AppData\Local").join("pl.tuktuktrack.desktop"))
        );
        // Bez %APPDATA% migracja zostaje w `setup` aplikacji.
        assert_eq!(early_dirs("x", &|_| None), None);
    } else {
        assert_eq!(dirs, None);
    }
}

#[test]
fn early_migration_copies_window_state_and_webview_data_before_plugins() {
    // `migrate_dirs` działa przed `tauri::Builder`: plugin stanu okien wczyta wtedy
    // już przeniesiony `.window-state.json` z pozycjami okien.
    let root = temp("wczesna");
    let roaming = root.join("Roaming");
    let local = root.join("Local");
    old_data(&roaming);
    write(
        &local
            .join(old::IDENTIFIER)
            .join("EBWebView")
            .join("Local Storage")
            .join("x"),
        "ls",
    );
    let new = roaming.join("pl.tuktuktrack.desktop");
    let new_local = local.join("pl.tuktuktrack.desktop");
    let report = migrate_dirs(&new, Some(&new_local));
    assert_eq!(report.state, DirState::Copied);
    assert_eq!(read(&new.join(".window-state.json")), "{\"main\":{}}");
    assert_eq!(
        read(&new_local.join("EBWebView").join("Local Storage").join("x")),
        "ls"
    );
    // Drugi start: nic do zrobienia.
    assert_eq!(
        migrate_dirs(&new, Some(&new_local)).state,
        DirState::AlreadyMoved
    );
}

#[test]
fn dir_with_both_databases_overwrites_nothing() {
    let root = temp("oba");
    let old_dir = old_data(&root);
    let new = root.join("nowy");
    let db = Connection::open({
        std::fs::create_dir_all(&new).unwrap();
        new.join(DB_FILE)
    })
    .unwrap();
    crate::db::init(&db).unwrap();
    db.execute(
        "INSERT INTO tasks(title,created_at) VALUES('Nowe zadanie',1)",
        [],
    )
    .unwrap();
    drop(db);
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Both);
    assert_eq!(report.data_dir, new);
    assert_eq!(title_in(&new.join(DB_FILE)), "Nowe zadanie");
    assert_eq!(title_in(&old_dir.join(old::DB_FILE)), "Stare zadanie");
    assert!(!new.join("backups").exists());
    assert!(!old_dir.join(MARKER_FILE).exists());
}

#[test]
fn dir_second_run_after_copy_does_nothing() {
    let root = temp("ponowne");
    let old_dir = old_data(&root);
    let new = root.join("nowy");
    assert_eq!(migrate_dir(&old_dir, &new).state, DirState::Copied);
    // Dane w nowym katalogu zmieniają się; ponowne uruchomienie ich nie rusza.
    let db = Connection::open(new.join(DB_FILE)).unwrap();
    db.execute("UPDATE tasks SET title='Zmienione'", [])
        .unwrap();
    drop(db);
    write(&new.join("backups").join("kopia.sqlite"), "zmieniona");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::AlreadyMoved);
    assert_eq!(title_in(&new.join(DB_FILE)), "Zmienione");
    assert_eq!(read(&new.join("backups").join("kopia.sqlite")), "zmieniona");
}

#[test]
fn dir_failed_copy_falls_back_to_old_directory() {
    let root = temp("blad");
    let old_dir = old_data(&root);
    // Nowa „ścieżka” jest plikiem: nie da się w niej utworzyć katalogu.
    let new = root.join("nowy");
    write(&new, "to jest plik");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Failed);
    assert_eq!(report.data_dir, old_dir);
    assert_eq!(report.db_file, old::DB_FILE);
    assert_eq!(
        title_in(&report.data_dir.join(report.db_file)),
        "Stare zadanie"
    );
    assert!(!old_dir.join(MARKER_FILE).exists());
    assert!(!report.warnings.is_empty());
}

#[test]
fn dir_retry_after_interrupted_copy_replaces_orphaned_wal() {
    let root = temp("przerwane");
    let old_dir = old_data(&root);
    let new = root.join("nowy");
    // Poprzednia, przerwana próba zostawiła WAL bez bazy.
    write(&new.join(format!("{DB_FILE}-wal")), "osierocony");
    let report = migrate_dir(&old_dir, &new);
    assert_eq!(report.state, DirState::Copied);
    assert_eq!(read(&new.join(format!("{DB_FILE}-wal"))), "wal");
}

#[test]
fn local_data_is_copied_only_when_new_has_no_webview_data() {
    let root = temp("lokalne");
    let old_local = root.join(old::IDENTIFIER);
    write(
        &old_local.join("EBWebView").join("Default").join("plik"),
        "dane",
    );
    let new_local = root.join("nowy");
    assert!(migrate_local(&old_local, &new_local).is_empty());
    assert_eq!(
        read(&new_local.join("EBWebView").join("Default").join("plik")),
        "dane"
    );
    // Drugi raz: nowe dane WebView2 już są, nic nie jest nadpisywane.
    write(
        &new_local.join("EBWebView").join("Default").join("plik"),
        "nowe",
    );
    migrate_local(&old_local, &new_local);
    assert_eq!(
        read(&new_local.join("EBWebView").join("Default").join("plik")),
        "nowe"
    );
    // Brak starego katalogu: nic się nie dzieje.
    let other = root.join("inny");
    assert!(migrate_local(&root.join("brak"), &other).is_empty());
    assert!(!other.exists());
}

// ---------------------------------------------------------------------------
// Poświadczenia

const OLD_PREFIX: &str = "stary.id";
const NEW_PREFIX: &str = "nowy.id";

/// Zastępczy magazyn z wtrąconymi błędami.
#[derive(Default)]
struct FakeStore {
    items: RefCell<HashMap<(String, String), String>>,
    fail_get: RefCell<Vec<String>>,
    fail_set: RefCell<Vec<String>>,
    fail_delete: RefCell<Vec<String>>,
    /// Zapis daje po odczycie inną wartość (uszkodzony zapis).
    corrupt_set: RefCell<bool>,
    calls: RefCell<usize>,
}
impl FakeStore {
    fn put(&self, service: &str, user: &str, secret: &str) {
        self.items
            .borrow_mut()
            .insert((service.into(), user.into()), secret.into());
    }
    fn has(&self, service: &str, user: &str) -> Option<String> {
        self.items
            .borrow()
            .get(&(service.to_string(), user.to_string()))
            .cloned()
    }
}
impl CredentialStore for FakeStore {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        *self.calls.borrow_mut() += 1;
        if self.fail_get.borrow().iter().any(|s| s == service) {
            return Err("magazyn zablokowany".into());
        }
        Ok(self.has(service, user))
    }
    fn set(&self, service: &str, user: &str, secret: &str) -> Result<(), String> {
        *self.calls.borrow_mut() += 1;
        if self.fail_set.borrow().iter().any(|s| s == service) {
            return Err("odmowa zapisu".into());
        }
        let stored = if *self.corrupt_set.borrow() {
            format!("{secret}?")
        } else {
            secret.to_string()
        };
        self.put(service, user, &stored);
        Ok(())
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        *self.calls.borrow_mut() += 1;
        if self.fail_delete.borrow().iter().any(|s| s == service) {
            return Err("odmowa usunięcia".into());
        }
        self.items
            .borrow_mut()
            .remove(&(service.to_string(), user.to_string()));
        Ok(())
    }
}

fn users(jira: Option<&str>, redmine: Option<&str>) -> Vec<(&'static str, Option<String>)> {
    vec![
        ("jira", jira.map(str::to_string)),
        ("calendar", None),
        ("timesheet", None),
        ("redmine", redmine.map(str::to_string)),
    ]
}

fn state_of(results: &[CredResult], kind: &str) -> CredState {
    results
        .iter()
        .find(|r| r.kind == kind)
        .unwrap()
        .state
        .clone()
}

#[test]
fn credentials_are_moved_and_old_entry_is_removed() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "https://a.atlassian.net|jan@x.pl", "token");
    let results = migrate_credentials(
        &store,
        OLD_PREFIX,
        NEW_PREFIX,
        &users(Some("https://a.atlassian.net|jan@x.pl"), None),
    );
    assert_eq!(state_of(&results, "jira"), CredState::Moved);
    assert_eq!(state_of(&results, "calendar"), CredState::NotConfigured);
    assert_eq!(
        store.has("nowy.id.jira", "https://a.atlassian.net|jan@x.pl"),
        Some("token".into())
    );
    assert_eq!(
        store.has("stary.id.jira", "https://a.atlassian.net|jan@x.pl"),
        None
    );
}

#[test]
fn credentials_missing_old_entry_is_not_an_error() {
    let store = FakeStore::default();
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    assert_eq!(state_of(&results, "jira"), CredState::NoEntry);
    assert!(store.items.borrow().is_empty());
}

#[test]
fn credentials_write_error_keeps_old_entry_and_does_not_block_others() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "token-jiry");
    store.put("stary.id.redmine", "r", "klucz");
    store.fail_set.borrow_mut().push("nowy.id.jira".into());
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), Some("r")));
    assert!(matches!(state_of(&results, "jira"), CredState::Failed(_)));
    assert_eq!(store.has("stary.id.jira", "u"), Some("token-jiry".into()));
    assert_eq!(store.has("nowy.id.jira", "u"), None);
    // Redmine przeniesione mimo błędu Jiry.
    assert_eq!(state_of(&results, "redmine"), CredState::Moved);
    assert_eq!(store.has("nowy.id.redmine", "r"), Some("klucz".into()));
    assert_eq!(store.has("stary.id.redmine", "r"), None);
}

#[test]
fn credentials_control_read_must_match_before_old_entry_is_removed() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "token");
    *store.corrupt_set.borrow_mut() = true;
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    match state_of(&results, "jira") {
        CredState::Failed(e) => assert!(e.contains("kontrolny"), "{e}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(store.has("stary.id.jira", "u"), Some("token".into()));
}

#[test]
fn credentials_read_error_of_old_entry_is_reported() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "token");
    store.fail_get.borrow_mut().push("stary.id.jira".into());
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    assert!(matches!(state_of(&results, "jira"), CredState::Failed(_)));
    assert_eq!(store.has("nowy.id.jira", "u"), None);
}

#[test]
fn credentials_delete_error_keeps_old_entry_as_backup() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "token");
    store.fail_delete.borrow_mut().push("stary.id.jira".into());
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    assert!(matches!(
        state_of(&results, "jira"),
        CredState::MovedKeptOld(_)
    ));
    assert_eq!(store.has("nowy.id.jira", "u"), Some("token".into()));
}

#[test]
fn credentials_existing_different_new_entry_is_never_overwritten() {
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "stary-token");
    store.put("nowy.id.jira", "u", "nowy-token");
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    assert_eq!(state_of(&results, "jira"), CredState::AlreadyThere);
    assert_eq!(store.has("nowy.id.jira", "u"), Some("nowy-token".into()));
    assert_eq!(store.has("stary.id.jira", "u"), Some("stary-token".into()));
}

#[test]
fn credentials_rerun_after_interrupted_move_removes_old_entry() {
    // Poprzednie uruchomienie zapisało nowy wpis, ale nie zdążyło usunąć starego.
    let store = FakeStore::default();
    store.put("stary.id.jira", "u", "token");
    store.put("nowy.id.jira", "u", "token");
    let results = migrate_credentials(&store, OLD_PREFIX, NEW_PREFIX, &users(Some("u"), None));
    assert_eq!(state_of(&results, "jira"), CredState::Moved);
    assert_eq!(store.has("stary.id.jira", "u"), None);
}

// ---------------------------------------------------------------------------
// Rejestr

/// Rejestr w pamięci: (klucz, nazwa wartości) → wartość.
#[derive(Default)]
struct FakeRegistry {
    values: RefCell<HashMap<(String, String), String>>,
    binary: RefCell<HashMap<(String, String), Vec<u8>>>,
    fail_set: RefCell<bool>,
    fail_delete_key: RefCell<Vec<String>>,
}
impl FakeRegistry {
    fn put(&self, key: &str, name: &str, value: &str) {
        self.values
            .borrow_mut()
            .insert((key.into(), name.into()), value.into());
    }
    fn value(&self, key: &str, name: &str) -> Option<String> {
        self.values
            .borrow()
            .get(&(key.to_string(), name.to_string()))
            .cloned()
    }
    fn put_binary(&self, key: &str, name: &str, value: &[u8]) {
        self.binary
            .borrow_mut()
            .insert((key.into(), name.into()), value.to_vec());
    }
    fn binary_value(&self, key: &str, name: &str) -> Option<Vec<u8>> {
        self.binary
            .borrow()
            .get(&(key.to_string(), name.to_string()))
            .cloned()
    }
}
impl Registry for FakeRegistry {
    fn get(&self, key: &str, name: &str) -> Result<Option<String>, String> {
        Ok(self.value(key, name))
    }
    fn get_binary(&self, key: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.binary_value(key, name))
    }
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        if *self.fail_set.borrow() {
            return Err("odmowa zapisu".into());
        }
        self.put(key, name, value);
        Ok(())
    }
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        self.binary
            .borrow_mut()
            .remove(&(key.to_string(), name.to_string()));
        self.values
            .borrow_mut()
            .remove(&(key.to_string(), name.to_string()));
        Ok(())
    }
    fn delete_key(&self, key: &str) -> Result<(), String> {
        if self.fail_delete_key.borrow().iter().any(|k| k == key) {
            return Err("odmowa usunięcia".into());
        }
        let prefix = format!("{key}\\");
        self.values
            .borrow_mut()
            .retain(|(k, _), _| k != key && !k.starts_with(&prefix));
        Ok(())
    }
}

#[test]
fn old_host_registry_keys_are_built_under_both_browsers() {
    assert_eq!(
        old_host_key(crate::browser::CHROME_HOSTS),
        r"Software\Google\Chrome\NativeMessagingHosts\pl.todotrack.bridge"
    );
    assert_eq!(
        old_host_key(crate::browser::EDGE_HOSTS),
        r"Software\Microsoft\Edge\NativeMessagingHosts\pl.todotrack.bridge"
    );
    assert_eq!(old_manifest_file(), "pl.todotrack.bridge.json");
    // Nowy host ma inną nazwę, więc czyszczenie starego nie dotyka nowego.
    assert_eq!(crate::native_msg::HOST_NAME, "pl.tuktuktrack.bridge");
}

#[test]
fn extension_migration_removes_old_host_keys_and_manifests_only() {
    let reg = FakeRegistry::default();
    let bases = [crate::browser::CHROME_HOSTS, crate::browser::EDGE_HOSTS];
    for base in bases {
        reg.put(
            &old_host_key(base),
            "",
            r"C:\Stara\pl.todotrack.bridge.json",
        );
        reg.put(
            &format!(r"{base}\{}", crate::native_msg::HOST_NAME),
            "",
            r"C:\Nowa\pl.tuktuktrack.bridge.json",
        );
    }
    let dir = temp("manifest");
    write(&dir.join(old_manifest_file()), "{}");
    write(&dir.join("pl.tuktuktrack.bridge.json"), "{}");
    let warnings = migrate_extension(&reg, &bases, &[dir.clone(), dir.join("brak")]);
    assert!(warnings.is_empty(), "{warnings:?}");
    for base in bases {
        assert_eq!(reg.value(&old_host_key(base), ""), None);
        assert_eq!(
            reg.value(&format!(r"{base}\{}", crate::native_msg::HOST_NAME), ""),
            Some(r"C:\Nowa\pl.tuktuktrack.bridge.json".into())
        );
    }
    assert!(!dir.join(old_manifest_file()).exists());
    assert!(dir.join("pl.tuktuktrack.bridge.json").exists());
}

#[test]
fn extension_migration_error_of_one_key_does_not_block_the_rest() {
    let reg = FakeRegistry::default();
    let chrome = old_host_key(crate::browser::CHROME_HOSTS);
    let edge = old_host_key(crate::browser::EDGE_HOSTS);
    reg.put(&chrome, "", "x");
    reg.put(&edge, "", "x");
    reg.fail_delete_key.borrow_mut().push(chrome.clone());
    let warnings = migrate_extension(
        &reg,
        &[crate::browser::CHROME_HOSTS, crate::browser::EDGE_HOSTS],
        &[],
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(reg.value(&chrome, ""), Some("x".into()));
    assert_eq!(reg.value(&edge, ""), None);
}

#[test]
fn autostart_value_keeps_arguments_and_uses_new_exe() {
    let new_exe = r"C:\Users\ala\AppData\Local\TukTuk Track\tuktuk-track.exe";
    assert_eq!(
        autostart_value(
            r#""C:\Users\ala\AppData\Local\Todo Track\todo-track.exe" --hidden"#,
            new_exe
        ),
        format!("\"{new_exe}\" --hidden")
    );
    assert_eq!(
        autostart_value(r#""C:\Programy\Todo Track\todo-track.exe""#, new_exe),
        format!("\"{new_exe}\"")
    );
    // Wpis bez cudzysłowów (ścieżka bez spacji).
    assert_eq!(
        autostart_value(r"C:\Apps\todo-track.exe --hidden", new_exe),
        format!("\"{new_exe}\" --hidden")
    );
    assert_eq!(
        autostart_value(r"C:\Apps\todo-track.exe", new_exe),
        format!("\"{new_exe}\"")
    );
}

const NEW_EXE: &str = r"C:\L\TukTuk Track\tuktuk-track.exe";

#[test]
fn autostart_is_renamed_with_the_same_arguments() {
    let reg = FakeRegistry::default();
    reg.put(
        RUN_KEY,
        old::NAME,
        r#""C:\L\Todo Track\todo-track.exe" --hidden"#,
    );
    let state = migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE);
    assert_eq!(state, AutostartState::Moved);
    assert_eq!(reg.value(RUN_KEY, old::NAME), None);
    assert_eq!(
        reg.value(RUN_KEY, "TukTuk Track"),
        Some(format!("\"{NEW_EXE}\" --hidden"))
    );
}

#[test]
fn autostart_without_old_entry_is_left_alone() {
    let reg = FakeRegistry::default();
    assert_eq!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::None
    );
    assert_eq!(reg.value(RUN_KEY, "TukTuk Track"), None);
}

#[test]
fn autostart_existing_new_entry_is_not_overwritten() {
    let reg = FakeRegistry::default();
    reg.put(RUN_KEY, old::NAME, r#""C:\Stara\todo-track.exe" --hidden"#);
    reg.put(RUN_KEY, "TukTuk Track", r#""C:\Nowa\tuktuk-track.exe""#);
    assert_eq!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::AlreadyThere
    );
    assert_eq!(reg.value(RUN_KEY, old::NAME), None);
    assert_eq!(
        reg.value(RUN_KEY, "TukTuk Track"),
        Some(r#""C:\Nowa\tuktuk-track.exe""#.into())
    );
}

#[test]
fn autostart_write_error_keeps_old_entry() {
    let reg = FakeRegistry::default();
    reg.put(RUN_KEY, old::NAME, r#""C:\Stara\todo-track.exe" --hidden"#);
    *reg.fail_set.borrow_mut() = true;
    assert!(matches!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::Failed(_)
    ));
    assert!(reg.value(RUN_KEY, old::NAME).is_some());
}

/// Wartość `StartupApproved\Run` po wyłączeniu w Menedżerze zadań (bajt 0x03 i czas).
const TASK_MANAGER_DISABLED: [u8; 12] =
    [3, 0, 0, 0, 0x8c, 0x1f, 0x2a, 0x55, 0x10, 0x3b, 0xdb, 0x01];
/// Wartość włączonego wpisu (zapisuje ją `auto-launch` przy włączeniu).
const TASK_MANAGER_ENABLED: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

#[test]
fn startup_approved_values_follow_auto_launch_rule() {
    assert!(startup_disabled(&TASK_MANAGER_DISABLED));
    assert!(!startup_disabled(&TASK_MANAGER_ENABLED));
    assert!(!startup_disabled(&[3, 0]));
    assert!(!startup_disabled(&[]));
}

#[test]
fn autostart_disabled_in_task_manager_is_not_turned_on() {
    let reg = FakeRegistry::default();
    reg.put(
        RUN_KEY,
        old::NAME,
        r#""C:\L\Todo Track\todo-track.exe" --hidden"#,
    );
    reg.put_binary(STARTUP_APPROVED_KEY, old::NAME, &TASK_MANAGER_DISABLED);
    assert_eq!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::Disabled
    );
    // Nowego wpisu nie ma (autostart zostaje wyłączony), stare wpisy usunięte.
    assert_eq!(reg.value(RUN_KEY, "TukTuk Track"), None);
    assert_eq!(reg.value(RUN_KEY, old::NAME), None);
    assert_eq!(reg.binary_value(STARTUP_APPROVED_KEY, old::NAME), None);
    assert_eq!(reg.binary_value(STARTUP_APPROVED_KEY, "TukTuk Track"), None);
}

#[test]
fn autostart_enabled_in_task_manager_is_moved_and_old_override_removed() {
    let reg = FakeRegistry::default();
    reg.put(RUN_KEY, old::NAME, r#""C:\L\Todo Track\todo-track.exe""#);
    reg.put_binary(STARTUP_APPROVED_KEY, old::NAME, &TASK_MANAGER_ENABLED);
    assert_eq!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::Moved
    );
    assert_eq!(
        reg.value(RUN_KEY, "TukTuk Track"),
        Some(format!("\"{NEW_EXE}\""))
    );
    assert_eq!(reg.binary_value(STARTUP_APPROVED_KEY, old::NAME), None);
}

#[test]
fn autostart_orphaned_old_override_is_removed() {
    let reg = FakeRegistry::default();
    reg.put_binary(STARTUP_APPROVED_KEY, old::NAME, &TASK_MANAGER_DISABLED);
    assert_eq!(
        migrate_autostart(&reg, old::NAME, "TukTuk Track", NEW_EXE),
        AutostartState::None
    );
    assert_eq!(reg.binary_value(STARTUP_APPROVED_KEY, old::NAME), None);
}

// ---------------------------------------------------------------------------
// Całość

fn db_with_accounts() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    for (key, value) in [
        (
            "jira",
            r#"{"site":"https://a.atlassian.net","email":"jan@x.pl","displayName":"","boards":[]}"#,
        ),
        ("timesheet", r#"{"apiUrl":"https://ts.x.pl","login":"jan"}"#),
        ("redmine", r#"{"site":"https://r.x.pl","login":"jan"}"#),
        (
            "calendar",
            r#"{"clientId":"ABCD-1234","tenant":"","email":"jan@x.pl"}"#,
        ),
    ] {
        write_setting(&db, key, value).unwrap();
    }
    db
}

fn dir_report(state: DirState) -> DirReport {
    DirReport {
        state,
        data_dir: PathBuf::from("nowy"),
        db_file: DB_FILE,
        warnings: Vec::new(),
    }
}

fn ctx<'a>(dir: &'a DirReport, exe: &'a Path) -> Ctx<'a> {
    Ctx {
        identifier: NEW_PREFIX,
        product_name: "TukTuk Track",
        exe,
        manifest_dirs: Vec::new(),
        dir,
        now: 1234,
    }
}

fn old_accounts(store: &FakeStore) {
    store.put(
        "pl.todotrack.desktop.jira",
        "https://a.atlassian.net|jan@x.pl",
        "j",
    );
    store.put("pl.todotrack.desktop.timesheet", "https://ts.x.pl|jan", "t");
    store.put("pl.todotrack.desktop.redmine", "https://r.x.pl|jan", "r");
    store.put(
        "pl.todotrack.desktop.calendar",
        "organizations|abcd-1234",
        "c",
    );
}

#[test]
fn credential_users_follow_the_modules_entry_names() {
    let db = db_with_accounts();
    let users = credential_users(&db);
    assert_eq!(
        users,
        vec![
            ("jira", Some("https://a.atlassian.net|jan@x.pl".to_string())),
            ("calendar", Some("organizations|abcd-1234".to_string())),
            ("timesheet", Some("https://ts.x.pl|jan".to_string())),
            ("redmine", Some("https://r.x.pl|jan".to_string())),
        ]
    );
    // Bez konfiguracji nie ma użytkowników (nic do przenoszenia).
    let empty = Connection::open_in_memory().unwrap();
    crate::db::init(&empty).unwrap();
    assert!(credential_users(&empty).iter().all(|(_, u)| u.is_none()));
}

#[test]
fn run_moves_everything_and_marks_migration_done_once() {
    let db = db_with_accounts();
    let store = FakeStore::default();
    old_accounts(&store);
    let reg = FakeRegistry::default();
    reg.put(RUN_KEY, old::NAME, r#""C:\Stara\todo-track.exe" --hidden"#);
    let dir = dir_report(DirState::Copied);
    let exe = PathBuf::from(NEW_EXE);
    let status = run(&db, &ctx(&dir, &exe), &store, &reg);
    assert_eq!(status.migrated_at, Some(1234));
    assert_eq!(status.dir, "copied");
    assert!(status.reconnect.is_empty());
    assert!(status.message.is_empty());
    for (kind, user) in [
        ("jira", "https://a.atlassian.net|jan@x.pl"),
        ("timesheet", "https://ts.x.pl|jan"),
        ("redmine", "https://r.x.pl|jan"),
        ("calendar", "organizations|abcd-1234"),
    ] {
        assert!(
            store.has(&format!("nowy.id.{kind}"), user).is_some(),
            "{kind}"
        );
        assert!(
            store
                .has(&format!("pl.todotrack.desktop.{kind}"), user)
                .is_none(),
            "{kind}"
        );
    }
    assert_eq!(
        reg.value(RUN_KEY, "TukTuk Track"),
        Some(format!("\"{NEW_EXE}\" --hidden"))
    );
    let marker: String = db
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [MIGRATED_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(marker, "1234");
    // Drugi start: nic się nie dzieje, zapisany wynik wraca bez dotykania magazynu.
    let calls = *store.calls.borrow();
    let again = run(&db, &ctx(&dir_report(DirState::None), &exe), &store, &reg);
    assert_eq!(*store.calls.borrow(), calls);
    assert_eq!(again.migrated_at, Some(1234));
    assert_eq!(again.dir, "copied");
}

#[test]
fn run_with_credential_error_retries_next_time_and_asks_to_reconnect() {
    let db = db_with_accounts();
    let store = FakeStore::default();
    old_accounts(&store);
    store.fail_set.borrow_mut().push("nowy.id.jira".into());
    let reg = FakeRegistry::default();
    let dir = dir_report(DirState::Copied);
    let exe = PathBuf::from(NEW_EXE);
    let status = run(&db, &ctx(&dir, &exe), &store, &reg);
    assert_eq!(status.migrated_at, None);
    assert_eq!(status.reconnect, vec!["jira".to_string()]);
    assert!(status.message.contains("Jira"));
    assert!(status.message.contains("Połącz"));
    assert!(db
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [MIGRATED_KEY],
            |r| r.get::<_, String>(0)
        )
        .is_err());
    assert_eq!(load_status(&db), status);
    // Stary wpis Jiry zostaje; reszta przeniesiona.
    assert!(store
        .has(
            "pl.todotrack.desktop.jira",
            "https://a.atlassian.net|jan@x.pl"
        )
        .is_some());
    assert!(store.has("nowy.id.redmine", "https://r.x.pl|jan").is_some());
    // Następny start po naprawie magazynu kończy migrację.
    store.fail_set.borrow_mut().clear();
    let retry = run(
        &db,
        &ctx(&dir_report(DirState::AlreadyMoved), &exe),
        &store,
        &reg,
    );
    assert_eq!(retry.migrated_at, Some(1234));
    assert!(retry.reconnect.is_empty());
    assert!(store
        .has("nowy.id.jira", "https://a.atlassian.net|jan@x.pl")
        .is_some());
    assert!(store
        .has(
            "pl.todotrack.desktop.jira",
            "https://a.atlassian.net|jan@x.pl"
        )
        .is_none());
}

#[test]
fn run_reports_two_databases_without_changing_anything() {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    let store = FakeStore::default();
    let reg = FakeRegistry::default();
    let dir = dir_report(DirState::Both);
    let exe = PathBuf::from(NEW_EXE);
    let status = run(&db, &ctx(&dir, &exe), &store, &reg);
    assert_eq!(status.dir, "both");
    assert!(status.message.contains("Używane są dane TukTuk Track"));
    assert_eq!(status.migrated_at, Some(1234));
    assert_eq!(load_status(&db), status);
}

#[test]
fn run_after_failed_directory_copy_is_not_marked_done() {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    let dir = dir_report(DirState::Failed);
    let exe = PathBuf::from(NEW_EXE);
    let status = run(
        &db,
        &ctx(&dir, &exe),
        &FakeStore::default(),
        &FakeRegistry::default(),
    );
    assert_eq!(status.migrated_at, None);
    assert_eq!(status.dir, "failed");
    assert!(status.message.contains("następnym starcie"));
}
