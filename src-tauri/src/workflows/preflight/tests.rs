use rusqlite::Connection;
use workflow_core::template::analysis_implementation_review;

use super::*;
use crate::ai::Cli;
use crate::workflows::{preflight_graph, require_unlocked, store};

fn cli(version: &str) -> Option<Cli> {
    Some(Cli {
        path: "x".into(),
        version: version.into(),
    })
}

fn detected(claude: Option<&str>, codex: Option<&str>) -> Detected {
    Detected {
        claude: claude.and_then(cli),
        codex: codex.and_then(cli),
    }
}

const SANDBOX_OK: &str = "[windows]\nsandbox = \"elevated\"\n";

fn node<'a>(list: &'a [NodeReadiness], id: &str) -> &'a NodeReadiness {
    list.iter().find(|n| n.node_id == id).expect("węzeł")
}

#[test]
fn missing_claude_is_cli_missing() {
    let list = preflight(
        &analysis_implementation_review(),
        &detected(None, Some("0.163.0")),
        Some(SANDBOX_OK),
    );
    let review = node(&list, "review");
    assert!(!review.cli_found);
    assert_eq!(
        review.blocked.as_ref().unwrap().code,
        FailureCode::CliMissing
    );
}

#[test]
fn tested_claude_review_uses_cli_control_without_block() {
    let list = preflight(
        &analysis_implementation_review(),
        &detected(Some("2.1.318 (Claude Code)"), Some("codex-cli 0.163.0")),
        Some(SANDBOX_OK),
    );
    let review = node(&list, "review");
    assert_eq!(review.enforcement, Enforcement::CliControl);
    assert!(review.blocked.is_none() && review.supported && review.warning.is_none());
}

#[test]
fn old_or_foreign_major_is_unsupported_and_newer_is_a_warning() {
    for version in ["1.9.0", "2.0.5", "3.0.0"] {
        let list = preflight(
            &analysis_implementation_review(),
            &detected(Some(version), Some("0.163.0")),
            Some(SANDBOX_OK),
        );
        let review = node(&list, "review");
        assert_eq!(
            review.blocked.as_ref().map(|b| b.code),
            Some(FailureCode::UnsupportedVersion),
            "{version}"
        );
    }
    let list = preflight(
        &analysis_implementation_review(),
        &detected(Some("2.2.0"), Some("0.170.1")),
        Some(SANDBOX_OK),
    );
    let review = node(&list, "review");
    assert!(review.supported && review.blocked.is_none());
    assert_eq!(review.warning.as_deref(), Some("niezweryfikowana wersja"));
}

fn claude_implementation_graph() -> WorkflowGraph {
    let mut graph = analysis_implementation_review();
    for n in &mut graph.nodes {
        if let NodeKind::Agent(c) = &mut n.kind {
            if c.role == workflow_core::model::AgentRole::Implementation {
                c.provider = Provider::Claude;
            }
        }
    }
    graph
}

/// Decyzja użytkownika 2026-11-06: Claude w kroku z zapisem działa od wersji z
/// `--restricted` (2.1.319); egzekwowanie to kontrola CLI w trybie restricted.
#[test]
fn claude_write_is_allowed_from_the_restricted_version() {
    for version in ["2.1.319", "2.1.400 (Claude Code)"] {
        let list = preflight(
            &claude_implementation_graph(),
            &detected(Some(version), Some("0.163.0")),
            Some(SANDBOX_OK),
        );
        let item = node(&list, "implementation");
        assert!(item.blocked.is_none(), "{version}: {:?}", item.blocked);
        assert_eq!(item.enforcement, Enforcement::CliRestricted, "{version}");
        assert!(item.supported);
        // Wersja nowsza niż przetestowana 2.1.318: ostrzeżenie jak dotąd.
        assert_eq!(item.warning.as_deref(), Some("niezweryfikowana wersja"));
    }
}

#[test]
fn claude_write_on_an_older_version_is_blocked_with_the_upgrade_message() {
    let list = preflight(
        &claude_implementation_graph(),
        &detected(Some("2.1.318"), Some("0.163.0")),
        Some(SANDBOX_OK),
    );
    let item = node(&list, "implementation");
    let blocked = item.blocked.clone().expect("blokada");
    assert_eq!(blocked.code, FailureCode::UnsupportedVersion);
    // Tekst z progu `CLAUDE_RESTRICTED` i wykrytej wersji (ten sam w fixture Playwright).
    assert_eq!(blocked.message, msg_claude_write("2.1.318"));
    assert_eq!(
        blocked.message,
        "Krok z zapisem dla Claude wymaga Claude Code 2.1.319 lub nowszego (wykryto 2.1.318). Zaktualizuj Claude Code albo użyj Codexa."
    );
    assert!(
        !blocked.message.contains("--restricted"),
        "bez żargonu o fladze"
    );
    assert_eq!(item.enforcement, Enforcement::None);
    // Ten sam Claude 2.1.318 w kroku tylko do odczytu jest gotowy.
    let review = node(&list, "review");
    assert!(review.blocked.is_none());
    assert_eq!(review.enforcement, Enforcement::CliControl);
}

/// Przypadki brzegowe kroku z zapisem: nowsza seria 2.x przechodzi z ostrzeżeniem,
/// inna seria główna i nierozpoznana wersja padają na `check_version` (ogólny
/// komunikat o wersji), nie na komunikacie o kroku z zapisem.
#[test]
fn claude_write_edge_versions() {
    let item_for = |version: &str| {
        let list = preflight(
            &claude_implementation_graph(),
            &detected(Some(version), Some("0.163.0")),
            Some(SANDBOX_OK),
        );
        node(&list, "implementation").clone()
    };
    let newer = item_for("2.2.0");
    assert!(newer.blocked.is_none(), "{:?}", newer.blocked);
    assert_eq!(newer.enforcement, Enforcement::CliRestricted);
    assert_eq!(newer.warning.as_deref(), Some("niezweryfikowana wersja"));

    let foreign = item_for("3.0.0");
    let blocked = foreign.blocked.expect("blokada");
    assert_eq!(blocked.code, FailureCode::UnsupportedVersion);
    assert!(
        blocked
            .message
            .starts_with("Wersja 3.0.0 nie jest obsługiwana"),
        "{}",
        blocked.message
    );
    assert!(!blocked.message.contains("Krok z zapisem"));
    assert!(!foreign.supported);

    let garbage = item_for("nightly");
    let blocked = garbage.blocked.expect("blokada");
    assert_eq!(blocked.code, FailureCode::UnsupportedVersion);
    assert_eq!(
        blocked.message,
        "Nie rozpoznano wersji programu agenta: nightly."
    );
    assert!(!garbage.supported);
}

#[test]
fn enforcement_serializes_for_the_interface() {
    assert_eq!(
        serde_json::to_string(&Enforcement::CliRestricted).unwrap(),
        "\"cliRestricted\""
    );
}

#[test]
fn codex_needs_a_configured_sandbox() {
    let d = detected(Some("2.1.318"), Some("0.163.0"));
    let graph = analysis_implementation_review();
    for config in [
        None,
        Some("[windows]\n"),
        Some("[windows]\nsandbox = \"off\"\n"),
    ] {
        let list = preflight(&graph, &d, config);
        let implementation = node(&list, "implementation");
        assert_eq!(
            implementation.blocked.as_ref().map(|b| b.code),
            Some(FailureCode::PermissionViolation),
            "{config:?}"
        );
        assert_eq!(implementation.enforcement, Enforcement::None);
    }
    let list = preflight(&graph, &d, Some(SANDBOX_OK));
    let implementation = node(&list, "implementation");
    assert!(implementation.blocked.is_none());
    assert_eq!(implementation.enforcement, Enforcement::CodexSandbox);
}

#[test]
fn broken_toml_blocks_without_panicking() {
    let list = preflight(
        &analysis_implementation_review(),
        &detected(Some("2.1.318"), Some("0.163.0")),
        Some("[windows\nsandbox ==="),
    );
    let blocked = node(&list, "implementation")
        .blocked
        .clone()
        .expect("blokada");
    assert!(blocked.message.contains("uszkodzony"));
}

#[test]
fn locked_module_returns_the_locked_error() {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    let error = require_unlocked(&db).unwrap_err().to_string();
    assert!(error.contains("nie jest odblokowany"));
    store::set_unlocked(&db).unwrap();
    assert!(require_unlocked(&db).is_ok());
}

#[test]
fn version_graph_reads_the_published_definition() {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    let id = store::create(&db, "P", "", &analysis_implementation_review(), 1).unwrap();
    store::publish(&db, id, 1, 2).unwrap();
    let version: i64 = db
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    let graph = store::version_graph(&db, version).unwrap();
    assert_eq!(graph, analysis_implementation_review());
    assert!(store::version_graph(&db, 999).is_err());
}

/// Szablon z review przez Codexa (read-only).
fn codex_review_graph() -> WorkflowGraph {
    let mut graph = analysis_implementation_review();
    for n in &mut graph.nodes {
        if let NodeKind::Agent(c) = &mut n.kind {
            if c.role == workflow_core::model::AgentRole::Review {
                c.provider = Provider::Codex;
                assert_eq!(c.access, AccessProfile::ReadOnly);
            }
        }
    }
    graph
}

#[test]
fn codex_read_only_review_also_needs_a_configured_sandbox() {
    let d = detected(Some("2.1.318"), Some("0.163.0"));
    let graph = codex_review_graph();
    for config in [
        None,
        Some(
            "[windows]
",
        ),
        Some(
            "[windows]
mode = \"x\"
",
        ),
    ] {
        let list = preflight(&graph, &d, config);
        let review = node(&list, "review");
        assert_eq!(review.access, AccessProfile::ReadOnly);
        let blocked = review.blocked.as_ref().expect("blokada");
        assert_eq!(blocked.code, FailureCode::PermissionViolation, "{config:?}");
        assert!(blocked.message.contains("sandboxa Windows"));
        assert_eq!(review.enforcement, Enforcement::None);
    }
    let list = preflight(&graph, &d, Some(SANDBOX_OK));
    let review = node(&list, "review");
    assert!(review.blocked.is_none());
    assert_eq!(review.enforcement, Enforcement::CodexSandbox);
}

/// Graf wyłącznie z węzłami Claude (analiza i review).
fn claude_only_graph() -> WorkflowGraph {
    let mut graph = analysis_implementation_review();
    graph
        .nodes
        .retain(|n| !matches!(&n.kind, NodeKind::Agent(c) if c.provider == Provider::Codex));
    graph
}

#[test]
fn unreadable_codex_config_is_not_read_for_a_claude_only_graph() {
    let graph = claude_only_graph();
    assert!(!uses_codex(&graph));
    let list =
        preflight_reading_config(&graph, &detected(Some("2.1.318"), Some("0.163.0")), || {
            panic!("config.toml nie powinien być czytany")
        });
    assert!(!list.is_empty());
    assert!(list.iter().all(|n| n.blocked.is_none()));
}

#[test]
fn unreadable_codex_config_blocks_only_codex_nodes() {
    let list = preflight_reading_config(
        &analysis_implementation_review(),
        &detected(Some("2.1.318"), Some("0.163.0")),
        || Err("Nie można odczytać config.toml Codexa: odmowa dostępu".into()),
    );
    let implementation = node(&list, "implementation")
        .blocked
        .clone()
        .expect("blokada");
    assert_eq!(implementation.code, FailureCode::PermissionViolation);
    assert!(implementation.message.contains("odmowa dostępu"));
    assert!(node(&list, "review").blocked.is_none());
    assert!(node(&list, "analysis").blocked.is_none());
}

#[test]
fn preflight_command_path_is_locked_before_unlocking() {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    let id = store::create(&db, "P", "", &analysis_implementation_review(), 1).unwrap();
    store::publish(&db, id, 1, 2).unwrap();
    let version: i64 = db
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    let error = preflight_graph(&db, version).unwrap_err().to_string();
    assert!(error.contains("nie jest odblokowany"), "{error}");
    store::set_unlocked(&db).unwrap();
    assert_eq!(
        preflight_graph(&db, version).unwrap(),
        analysis_implementation_review()
    );
}
