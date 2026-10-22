//! Kontrola kompletności zmiany nazwy Todo Track → TukTuk Track (PLAN_BACKLOG.md, N1):
//! stare nazwy (`Todo Track`, `todo-track`, `todotrack`, `TODO_TRACK`) nie mogą
//! zostać w kodzie, instalatorze, rozszerzeniu, skryptach ani konfiguracji.
//!
//! Dozwolone wyjątki:
//! - pliki migracji: `legacy.rs`, `legacy_names.rs`, `legacy_tests.rs` i `src/legacy.ts`;
//! - bloki `LEGACY-BEGIN` … `LEGACY-END` w szablonie instalatora (obsługa starej instalacji);
//! - stałe, które zostają bez zmian z decyzji użytkownika: repozytoria GitHub
//!   (`matkrajnik/todo-track`, `matkrajnik/todo-track-private`), plik klucza podpisu
//!   (`todotrack.key`) i katalog klucza rozszerzenia poza repozytorium.
//!
//! Historia wersji w README i pliki planów nie są sprawdzane (poza zakresem).
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 4] = ["todo track", "todo-track", "todotrack", "todo_track"];
/// Fragmenty, które usuwamy przed szukaniem (wyjątki ze stałych niezmienianych).
const ALLOWED_TOKENS: [&str; 3] = [
    "matkrajnik/todo-track",
    "todotrack.key",
    r".todo-track\browser-extension-key.pem",
];
const ALLOWED_FILES: [&str; 4] = [
    "src-tauri/src/legacy.rs",
    "src-tauri/src/legacy_names.rs",
    "src-tauri/src/legacy_tests.rs",
    "src/legacy.ts",
];
const SKIPPED_DIRS: [&str; 4] = ["node_modules", "target", "gen", ".git"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("katalog repozytorium")
        .to_path_buf()
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if p.is_dir() && SKIPPED_DIRS.contains(&name.as_str()) {
            continue;
        }
        collect(&p, out);
    }
}

fn scanned_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for item in [
        "src",
        "src-tauri/src",
        "src-tauri/installer",
        "browser-extension",
        "scripts",
        "src-tauri/tauri.conf.json",
        "src-tauri/Cargo.toml",
        "package.json",
        "index.html",
    ] {
        collect(&root.join(item), &mut files);
    }
    files
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Linie z zakazaną nazwą (po usunięciu dozwolonych fragmentów); linie wewnątrz
/// bloków `LEGACY-BEGIN` … `LEGACY-END` są pomijane.
fn offending_lines(text: &str) -> Vec<(usize, String)> {
    let mut inside = false;
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.contains("LEGACY-BEGIN") {
            inside = true;
        }
        if !inside {
            let mut lower = line.to_lowercase();
            for token in ALLOWED_TOKENS {
                lower = lower.replace(token, "");
            }
            if FORBIDDEN.iter().any(|word| lower.contains(word)) {
                found.push((number + 1, line.trim().to_string()));
            }
        }
        if line.contains("LEGACY-END") {
            inside = false;
        }
    }
    found
}

#[test]
fn no_old_application_name_is_left_outside_the_migration_code() {
    let mut problems = Vec::new();
    for path in scanned_files() {
        let name = relative(&path);
        if ALLOWED_FILES.contains(&name.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // plik binarny (grafiki instalatora)
        };
        for (line, content) in offending_lines(&text) {
            problems.push(format!("{name}:{line}: {content}"));
        }
    }
    assert!(
        problems.is_empty(),
        "Stare nazwy poza kodem migracji:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_scan_sees_the_files_it_is_supposed_to_check() {
    let names: Vec<String> = scanned_files().iter().map(|p| relative(p)).collect();
    for expected in [
        "src/App.vue",
        "src-tauri/src/main.rs",
        "src-tauri/src/legacy.rs",
        "src-tauri/installer/template.nsi",
        "browser-extension/manifest.json",
        "scripts/ship.mjs",
        "src-tauri/tauri.conf.json",
    ] {
        assert!(names.iter().any(|n| n == expected), "{expected}");
    }
}

#[test]
fn the_check_flags_old_names_and_honours_the_exceptions() {
    // Wykrywa każdą z czterech pisowni.
    for bad in [
        "let x = \"Todo Track\";",
        "const ID = 'pl.todotrack.desktop';",
        "process.env.TODO_TRACK_AI",
        "serwer todo-track-mcp",
    ] {
        assert_eq!(offending_lines(bad).len(), 1, "{bad}");
    }
    // Stałe niezmieniane z decyzji użytkownika.
    for ok in [
        "https://github.com/matkrajnik/todo-track/releases/latest/download/latest.json",
        "github.com/matkrajnik/todo-track-private",
        r#"join(".tauri", "todotrack.key")"#,
        r"%USERPROFILE%\.todo-track\browser-extension-key.pem",
        "nowa nazwa tuktuk-track i pl.tuktuktrack.desktop",
    ] {
        assert!(offending_lines(ok).is_empty(), "{ok}");
    }
    // Blok obsługi starej instalacji w szablonie instalatora.
    let nsi = "a\n; LEGACY-BEGIN\n!define X \"Todo Track\"\n; LEGACY-END\nb \"Todo Track\"\n";
    assert_eq!(
        offending_lines(nsi),
        vec![(5, "b \"Todo Track\"".to_string())]
    );
}
