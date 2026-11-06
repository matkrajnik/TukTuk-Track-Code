//! Kontrola architektury dostępu (E7): każda komenda `workflow_*` zarejestrowana w
//! `main.rs` ma jawną decyzję, czy wymaga klucza. Nowa komenda bez wpisu na jedną
//! z list nie przejdzie CI, a komenda z klucza musi przejść przez sprawdzenie
//! odblokowania w źródle.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Komendy działające bez klucza: dostęp, historia, zatrzymanie, odzyskanie i
/// zgłoszone problemy (przy odzyskiwaniu dostępu da się je zobaczyć i zwolnić).
const KEYLESS: [&str; 9] = [
    "workflow_access",
    "workflow_unlock",
    "workflow_lock",
    "workflow_runs",
    "workflow_run_get",
    "workflow_run_events",
    "workflow_cancel",
    "workflow_recover",
    "workflow_faults",
];

/// Komendy wymagające odblokowanego modułu.
const KEYED: [&str; 16] = [
    "workflow_list",
    "workflow_get",
    "workflow_create",
    "workflow_save_draft",
    "workflow_validate",
    "workflow_publish",
    "workflow_archive",
    "workflow_duplicate",
    "workflow_export",
    "workflow_import",
    "workflow_preflight",
    "workflow_version_graph",
    "workflow_start",
    "workflow_decide",
    "workflow_resume",
    "workflow_artifact_read",
];

/// Wywołania sprawdzające klucz: bezpośrednie (`with_unlocked`, `require_unlocked`,
/// `run_commands::unlocked`) albo przez funkcję rdzenia, która je robi.
const GUARDS: [&str; 8] = [
    "with_unlocked",
    "require_unlocked",
    "unlocked",
    "start_run",
    "preflight_graph",
    "decide",
    "resume_run",
    "artifact_read",
];
/// Funkcje rdzenia, do których komendy delegują; same muszą sprawdzać klucz.
const CORE_GUARDED: [&str; 5] = [
    "start_run",
    "decide",
    "resume_run",
    "artifact_read",
    "preflight_graph",
];
/// Bezpośrednie sprawdzenia klucza w funkcjach rdzenia.
const DIRECT_GUARDS: [&str; 2] = ["require_unlocked", "unlocked"];

fn src(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    // Checkout na Windows (core.autocrlf) daje CRLF: źródło porównujemy po LF.
    normalize(&text)
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn registered() -> BTreeSet<String> {
    let main = src("main.rs");
    let mut names = BTreeSet::new();
    for line in main.lines() {
        let line = line.trim().trim_end_matches(',');
        if let Some(rest) = line.strip_prefix("workflows::") {
            let name = rest.rsplit("::").next().unwrap();
            if name.starts_with("workflow_") {
                names.insert(name.to_owned());
            }
        }
    }
    names
}

/// Ciało funkcji najwyższego poziomu bez sygnatury: od pierwszego `{` po linii
/// `fn name(` bez wcięcia do pierwszej linii równej `}` (także z CRLF). Sygnatura
/// (np. `fn workflow_decide(`) nie liczy się jako wywołanie. Brak końca daje `None`,
/// a nie resztę pliku: inaczej sprawdzenie przeszłoby dzięki późniejszym funkcjom.
fn body_of<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let signature = format!("fn {name}(");
    let mut offset = 0;
    let mut start = None;
    for line in source.split_inclusive('\n') {
        let top_level = !line.starts_with(char::is_whitespace);
        if start.is_none() && top_level && line.contains(&signature) {
            start = Some(offset);
        } else if let Some(start) = start {
            if line.trim_end_matches(['\r', '\n']) == "}" {
                let item = &source[start..offset];
                return Some(&item[item.find('{')?..]);
            }
        }
        offset += line.len();
    }
    None
}

/// Czy `body` wywołuje wolną funkcję `name` (całe słowo, nie metodę `.name(`),
/// więc `is_unlocked(` ani `handle.start_run(` się nie liczą.
fn calls(body: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    body.match_indices(&needle).any(|(i, _)| {
        body[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
    })
}

#[test]
fn guard_matcher_ignores_signatures_methods_and_lookalikes() {
    let source = "pub async fn workflow_decide(app: AppHandle) -> R {\n    is_unlocked(&db);\n    handle.decide(x);\n}\n";
    let body = body_of(source, "workflow_decide").unwrap();
    assert!(GUARDS.iter().all(|g| !calls(body, g)), "{body}");
    assert!(calls("{ run_commands::decide(&db) }", "decide"));
    assert!(calls("{ unlocked(db)?; }", "unlocked"));
    assert!(!calls("{ store::is_unlocked(db)?; }", "unlocked"));
}

#[test]
fn body_ends_at_its_own_brace_with_crlf_even_if_a_later_function_has_a_guard() {
    let crlf = "pub fn workflow_list(db: D) -> R {\r\n    store::list(db)\r\n}\r\n\r\n\
                pub fn workflow_get(db: D) -> R {\r\n    with_unlocked(db)\r\n}\r\n";
    for source in [crlf.to_owned(), normalize(crlf)] {
        let body = body_of(&source, "workflow_list").unwrap();
        assert!(!body.contains("workflow_get"), "{body:?}");
        assert!(GUARDS.iter().all(|g| !calls(body, g)), "{body:?}");
        let later = body_of(&source, "workflow_get").unwrap();
        assert!(calls(later, "with_unlocked"), "{later:?}");
    }
    // Funkcja bez zamknięcia nie ma ciała (zamiast „reszty pliku”).
    assert!(body_of(
        "fn workflow_list() {\n    with_unlocked(db)\n",
        "workflow_list"
    )
    .is_none());
    // Funkcja zagnieżdżona (z wcięciem) to nie komenda najwyższego poziomu.
    assert!(body_of(
        "mod t {\n    fn workflow_list() {\n    }\n}\n",
        "workflow_list"
    )
    .is_none());
}

#[test]
fn real_bodies_end_before_the_next_function() {
    for file in ["workflows/mod.rs", "workflows/run_commands.rs"] {
        let source = src(file);
        assert!(!source.contains('\r'), "{file}: CRLF po normalizacji");
        for name in KEYED.iter().chain(CORE_GUARDED.iter()) {
            if let Some(body) = body_of(&source, name) {
                assert!(
                    body.len() < source.len() / 2,
                    "{file}: {name} sięga końca pliku"
                );
                assert!(
                    !body.contains("\nfn "),
                    "{file}: {name} obejmuje następną funkcję"
                );
                assert!(
                    !body.contains("\npub "),
                    "{file}: {name} obejmuje następną funkcję"
                );
            }
        }
    }
}

#[test]
fn every_registered_command_is_classified_exactly_once() {
    let registered = registered();
    assert!(registered.len() >= 20, "mało komend: {registered:?}");
    let keyless: BTreeSet<String> = KEYLESS.iter().map(|s| (*s).to_owned()).collect();
    let keyed: BTreeSet<String> = KEYED.iter().map(|s| (*s).to_owned()).collect();
    assert!(keyless.is_disjoint(&keyed), "komenda na obu listach");
    let classified: BTreeSet<String> = keyless.union(&keyed).cloned().collect();
    let missing: Vec<_> = registered.difference(&classified).collect();
    assert!(
        missing.is_empty(),
        "komendy bez decyzji o kluczu (dopisz do KEYLESS albo KEYED): {missing:?}"
    );
    let stale: Vec<_> = classified.difference(&registered).collect();
    assert!(
        stale.is_empty(),
        "komendy z list, których nie ma w main.rs: {stale:?}"
    );
}

#[test]
fn keyed_commands_check_the_key() {
    let sources = [src("workflows/mod.rs"), src("workflows/run_commands.rs")];
    for name in KEYED {
        let body = sources
            .iter()
            .find_map(|s| body_of(s, name))
            .unwrap_or_else(|| panic!("brak źródła komendy {name}"));
        assert!(
            GUARDS.iter().any(|g| calls(body, g)),
            "{name} nie sprawdza klucza"
        );
    }
}

#[test]
fn core_functions_behind_keyed_commands_check_the_key() {
    let sources = [src("workflows/run_commands.rs"), src("workflows/mod.rs")];
    for name in CORE_GUARDED {
        let body = sources
            .iter()
            .find_map(|s| body_of(s, name))
            .unwrap_or_else(|| panic!("brak funkcji {name}"));
        assert!(
            DIRECT_GUARDS.iter().any(|g| calls(body, g)),
            "{name} nie sprawdza klucza"
        );
    }
}
