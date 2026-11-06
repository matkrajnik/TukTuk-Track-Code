//! Warstwa „Baza” z PLAN_WDROZENIA.md, 11.

use super::*;
use workflow_core::template::{analysis_implementation_review, blank};
use workflow_core::IssueCode;

fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::db::init(&db).unwrap();
    db
}

fn template(db: &Connection) -> i64 {
    create(
        db,
        "Proces",
        "Opis",
        &analysis_implementation_review(),
        1_000,
    )
    .unwrap()
}

#[test]
fn init_is_idempotent_and_records_module_schema() {
    let db = db();
    crate::db::init(&db).unwrap();
    let versions: i64 = db
        .query_row("SELECT COUNT(*) FROM workflow_schema_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    // Wersje 1 (definicje), 2 (przebiegi, E3), 3 (katalogi robocze, E4), 4 (cel,
    // artefakty, E5) i 5 (rejestr procesów, retencja, powód zwolnienia, E6).
    assert_eq!(versions, 5);
    assert_eq!(module_schema_of(&db), 5);
}

#[test]
fn migration_keeps_existing_tasks() {
    let mut db = db();
    crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
    db.execute_batch("DROP TABLE workflow_versions; DROP TABLE workflows;")
        .unwrap();
    crate::db::init(&db).unwrap();
    let tasks: i64 = db
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 1);
    assert!(list(&db, true).unwrap().is_empty());
}

#[test]
fn unlock_flag_persists() {
    let db = db();
    assert!(!is_unlocked(&db).unwrap());
    set_unlocked(&db).unwrap();
    set_unlocked(&db).unwrap();
    assert!(is_unlocked(&db).unwrap());
}

#[test]
fn draft_saves_and_reads_back() {
    let db = db();
    let id = template(&db);
    let detail = get(&db, id).unwrap();
    assert_eq!(detail.draft, analysis_implementation_review());
    assert_eq!(detail.summary.draft_revision, 1);
    assert!(detail.issues.is_empty());
    assert!(detail.summary.unpublished_changes);
    assert_eq!(detail.summary.published_version, None);
}

#[test]
fn stale_revision_is_a_conflict() {
    let db = db();
    let id = template(&db);
    let revision = save_draft(&db, id, 1, "Nowa nazwa", "", &blank(), 2_000).unwrap();
    assert_eq!(revision, 2);
    let stale = save_draft(&db, id, 1, "Inne okno", "", &blank(), 3_000);
    assert!(matches!(stale, Err(StoreError::Conflict)));
    assert_eq!(get(&db, id).unwrap().summary.name, "Nowa nazwa");
}

#[test]
fn draft_with_issues_saves_but_does_not_publish() {
    let db = db();
    let id = template(&db);
    let mut graph = analysis_implementation_review();
    graph.edges.retain(|e| e.id != "approval-rejected");
    save_draft(&db, id, 1, "Proces", "", &graph, 2_000).unwrap();
    let detail = get(&db, id).unwrap();
    assert!(detail
        .issues
        .iter()
        .any(|i| i.code == IssueCode::MissingConnection));
    assert!(matches!(
        publish(&db, id, 2, 3_000),
        Err(StoreError::Invalid(_))
    ));
    assert!(get(&db, id).unwrap().versions.is_empty());
}

#[test]
fn publish_creates_immutable_numbered_versions() {
    let db = db();
    let id = template(&db);
    let first = publish(&db, id, 1, 2_000).unwrap();
    assert_eq!(first.version, 1);
    // Podwójne kliknięcie: ta sama wersja, bez nowego wpisu.
    assert_eq!(publish(&db, id, 1, 2_500).unwrap(), first);
    let summary = get(&db, id).unwrap().summary;
    assert_eq!(summary.published_version, Some(1));
    assert!(!summary.unpublished_changes);

    let mut graph = analysis_implementation_review();
    graph.nodes[1].label = "Analiza 2".into();
    save_draft(&db, id, 1, "Proces", "", &graph, 3_000).unwrap();
    assert!(get(&db, id).unwrap().summary.unpublished_changes);
    let second = publish(&db, id, 2, 4_000).unwrap();
    assert_eq!(second.version, 2);
    assert_ne!(second.hash, first.hash);

    let update = db.execute("UPDATE workflow_versions SET definition_json='{}'", []);
    assert!(update.is_err());
    let delete = db.execute("DELETE FROM workflow_versions", []);
    assert!(delete.is_err());
    assert_eq!(get(&db, id).unwrap().versions.len(), 2);
}

#[test]
fn publish_needs_current_revision() {
    let db = db();
    let id = template(&db);
    save_draft(
        &db,
        id,
        1,
        "Proces",
        "",
        &analysis_implementation_review(),
        2_000,
    )
    .unwrap();
    assert!(matches!(
        publish(&db, id, 1, 3_000),
        Err(StoreError::Conflict)
    ));
}

#[test]
fn archive_hides_keeps_history_and_blocks_edits() {
    let db = db();
    let id = template(&db);
    publish(&db, id, 1, 2_000).unwrap();
    set_archived(&db, id, true, 3_000).unwrap();
    assert!(list(&db, false).unwrap().is_empty());
    assert_eq!(list(&db, true).unwrap().len(), 1);
    assert_eq!(get(&db, id).unwrap().versions.len(), 1);
    assert!(matches!(
        save_draft(&db, id, 1, "X", "", &blank(), 4_000),
        Err(StoreError::BadInput(_))
    ));
    set_archived(&db, id, false, 5_000).unwrap();
    assert_eq!(list(&db, false).unwrap().len(), 1);
}

#[test]
fn rejects_bad_names_and_missing_ids() {
    let db = db();
    assert!(matches!(
        create(&db, "  ", "", &blank(), 1),
        Err(StoreError::BadInput(_))
    ));
    assert!(matches!(
        create(&db, &"x".repeat(MAX_NAME_CHARS + 1), "", &blank(), 1),
        Err(StoreError::BadInput(_))
    ));
    assert!(matches!(get(&db, 999), Err(StoreError::NotFound)));
    assert!(matches!(
        set_archived(&db, 999, true, 1),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn export_import_round_trip_creates_unpublished_draft() {
    let db = db();
    let id = template(&db);
    publish(&db, id, 1, 2_000).unwrap();
    let text = export(&db, id).unwrap();
    assert!(text.contains("\"format\": \"tuktuk-track-workflow\""));
    let copy = import(&db, &text, 3_000).unwrap();
    let detail = get(&db, copy).unwrap();
    assert_ne!(copy, id);
    assert_eq!(detail.draft, analysis_implementation_review());
    assert!(detail.versions.is_empty());
}

#[test]
fn import_rejects_foreign_or_oversized_files() {
    let db = db();
    assert!(import(&db, "{\"name\":\"x\"}", 1).is_err());
    assert!(import(
        &db,
        "{\"format\":\"inny\",\"formatVersion\":1,\"name\":\"x\",\"graph\":{\"schemaVersion\":1,\"nodes\":[],\"edges\":[]}}",
        1
    )
    .is_err());
    assert!(import(&db, &" ".repeat(MAX_IMPORT_BYTES + 1), 1).is_err());
    assert!(list(&db, true).unwrap().is_empty());
}

#[test]
fn duplicate_copies_draft_without_versions() {
    let db = db();
    let id = template(&db);
    publish(&db, id, 1, 2_000).unwrap();
    let copy = duplicate(&db, id, 3_000).unwrap();
    let detail = get(&db, copy).unwrap();
    assert_eq!(detail.summary.name, "Proces (kopia)");
    assert!(detail.versions.is_empty());
}

#[test]
fn hash_is_stable_for_equal_graphs() {
    let template = || graph_hash(&analysis_implementation_review()).unwrap();
    assert_eq!(template(), template());
    assert_ne!(template(), graph_hash(&blank()).unwrap());
}

#[test]
fn published_version_keeps_exact_draft_text_and_hash() {
    let db = db();
    let id = template(&db);
    let version = publish(&db, id, 1, 2_000).unwrap();
    let (draft_json, draft_hash): (String, String) = db
        .query_row(
            "SELECT draft_json, draft_hash FROM workflows WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let definition: String = db
        .query_row(
            "SELECT definition_json FROM workflow_versions WHERE workflow_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(definition, draft_json);
    assert_eq!(version.hash, draft_hash);
    assert_eq!(
        version.hash,
        graph_hash(&analysis_implementation_review()).unwrap()
    );
}

#[test]
fn oversized_draft_is_rejected() {
    let db = db();
    let id = template(&db);
    let mut graph = analysis_implementation_review();
    if let workflow_core::NodeKind::Agent(agent) = &mut graph.nodes[1].kind {
        agent.instruction = "x".repeat(MAX_DRAFT_BYTES);
    }
    assert!(matches!(
        save_draft(&db, id, 1, "Proces", "", &graph, 2_000),
        Err(StoreError::BadInput(_))
    ));
    assert_eq!(get(&db, id).unwrap().summary.draft_revision, 1);
}

#[test]
fn copy_of_long_name_keeps_suffix() {
    let db = db();
    let id = create(&db, &"N".repeat(MAX_NAME_CHARS), "", &blank(), 1).unwrap();
    let copy = duplicate(&db, id, 2).unwrap();
    let name = get(&db, copy).unwrap().summary.name;
    assert!(name.ends_with(" (kopia)"));
    assert_eq!(name.chars().count(), MAX_NAME_CHARS);
}

fn has_column(db: &Connection, table: &str, column: &str) -> bool {
    db.prepare(&format!(
        "SELECT 1 FROM pragma_table_info('{table}') WHERE name = '{column}'"
    ))
    .unwrap()
    .exists([])
    .unwrap()
}

#[test]
fn migration_4_adds_goal_and_artifacts_and_is_idempotent() {
    let db = db();
    init_tables(&db).unwrap();
    init_tables(&db).unwrap();
    assert!(has_column(&db, "workflow_runs", "goal"));
    assert!(has_column(&db, "workflow_artifacts", "rel_path"));
    let bad = db.execute(
        "INSERT INTO workflow_artifacts(run_id, kind, hash, rel_path, size, created_at) VALUES(1, 'zip', 'h', 'p', 1, 1)",
        [],
    );
    assert!(bad.is_err(), "kind poza 'diff' jest odrzucany");
}

/// Wszystkie wiersze tabeli w podanych kolumnach jako tekst (porównanie treści).
fn dump(db: &Connection, table: &str, columns: &[String]) -> Vec<Vec<String>> {
    let sql = format!("SELECT {} FROM {table} ORDER BY rowid", columns.join(", "));
    let mut statement = db.prepare(&sql).unwrap();
    let rows = statement
        .query_map([], |row| {
            (0..columns.len())
                .map(|i| {
                    row.get::<_, rusqlite::types::Value>(i)
                        .map(|v| format!("{v:?}"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap();
    rows.collect::<Result<Vec<_>, _>>().unwrap()
}

fn columns(db: &Connection, table: &str) -> Vec<String> {
    let mut statement = db
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap();
    let names = statement.query_map([], |r| r.get(0)).unwrap();
    names.collect::<Result<Vec<String>, _>>().unwrap()
}

fn has_migration(db: &Connection, version: i64) -> bool {
    db.prepare("SELECT 1 FROM workflow_schema_migrations WHERE version = ?1")
        .unwrap()
        .exists([version])
        .unwrap()
}

/// Baza w stanie E4 (bez `goal`, bez `workflow_artifacts`, bez wpisu migracji 4)
/// z zadaniem, wpisem czasu, przebiegiem i jego zdarzeniami: migracja 4 (także
/// powtórzona) zachowuje treść tych tabel i zapisuje wpis wersji 4.
#[test]
fn migration_4_keeps_existing_tasks_sessions_and_runs() {
    let mut db = db();
    crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
    let task: i64 = db
        .query_row("SELECT id FROM tasks", [], |r| r.get(0))
        .unwrap();
    db.execute(
        "INSERT INTO sessions(task_id, started_at, ended_at) VALUES(?1, 1000, 61000)",
        [task],
    )
    .unwrap();
    let id = template(&db);
    publish(&db, id, 1, 2).unwrap();
    let version: i64 = db
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    db.execute(
        "INSERT INTO workflow_runs(version_id, request_id, definition_json, definition_hash, task_id, task_title, repo, status, created_at, updated_at) VALUES(?1, 'r', '{}', 'h', ?2, 'Zadanie', 'C:/repo', 'running', 1, 2)",
        rusqlite::params![version, task],
    )
    .unwrap();
    let run: i64 = db
        .query_row("SELECT id FROM workflow_runs", [], |r| r.get(0))
        .unwrap();
    for (sequence, kind) in [(1, "runCreated"), (2, "runStateChanged")] {
        db.execute(
            "INSERT INTO workflow_events(run_id, sequence, type, payload_json, created_at) VALUES(?1, ?2, ?3, '{}', ?2)",
            rusqlite::params![run, sequence, kind],
        )
        .unwrap();
    }
    // Stan E4: bez kolumny `goal`, bez tabeli artefaktów i bez wpisu migracji 4.
    db.execute("DROP TABLE workflow_artifacts", []).unwrap();
    db.execute("ALTER TABLE workflow_runs DROP COLUMN goal", [])
        .unwrap();
    db.execute(
        "DELETE FROM workflow_schema_migrations WHERE version = 4",
        [],
    )
    .unwrap();
    assert!(!has_column(&db, "workflow_runs", "goal"));
    assert!(!has_migration(&db, 4));
    let tables = ["tasks", "sessions", "workflow_runs", "workflow_events"];
    let before: Vec<(Vec<String>, Vec<Vec<String>>)> = tables
        .iter()
        .map(|t| {
            let cols = columns(&db, t);
            let rows = dump(&db, t, &cols);
            (cols, rows)
        })
        .collect();
    assert!(before.iter().all(|(_, rows)| !rows.is_empty()));

    init_tables(&db).unwrap();
    init_tables(&db).unwrap();

    assert!(has_column(&db, "workflow_runs", "goal"));
    assert!(has_column(&db, "workflow_artifacts", "rel_path"));
    assert!(has_migration(&db, 4));
    for (table, (cols, rows)) in tables.iter().zip(&before) {
        assert_eq!(&dump(&db, table, cols), rows, "{table}");
    }
    let goal: Option<String> = db
        .query_row("SELECT goal FROM workflow_runs WHERE id = ?1", [run], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(goal, None);
}

#[test]
fn migration_5_is_idempotent() {
    let db = db();
    init_tables(&db).unwrap();
    init_tables(&db).unwrap();
    assert!(has_column(&db, "workflow_processes", "created_ft"));
    assert!(has_column(&db, "workflow_artifacts", "purged_at"));
    assert!(has_column(&db, "workflow_workspaces", "release_reason"));
    assert!(has_migration(&db, 5));
}

#[test]
fn process_state_check_rejects_unknown_value() {
    let db = db();
    let id = template(&db);
    publish(&db, id, 1, 2).unwrap();
    db.execute(
        "INSERT INTO workflow_runs(version_id, request_id, definition_json, definition_hash, status, created_at, updated_at) VALUES((SELECT id FROM workflow_versions), 'r', '{}', 'h', 'running', 1, 2)",
        [],
    )
    .unwrap();
    let insert = |state: &str| {
        db.execute(
            "INSERT INTO workflow_processes(execution_id, run_id, pid, created_ft, job_attached, state, recorded_at) VALUES(?1, 1, 10, 20, 1, ?2, 3)",
            rusqlite::params![format!("e-{state}"), state],
        )
    };
    assert!(insert("running").is_ok());
    assert!(insert("zombie").is_err());
}

/// Baza w stanie E5 (bez migracji 5) z zadaniem, wpisem czasu, przebiegiem, zdarzeniami
/// i artefaktem: migracja 5 (także powtórzona) zachowuje treść i zapisuje wersję 5.
#[test]
fn migration_5_from_e5_database_keeps_tasks_sessions_runs_and_events() {
    let mut db = db();
    crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
    let task: i64 = db
        .query_row("SELECT id FROM tasks", [], |r| r.get(0))
        .unwrap();
    db.execute(
        "INSERT INTO sessions(task_id, started_at, ended_at) VALUES(?1, 1000, 61000)",
        [task],
    )
    .unwrap();
    let id = template(&db);
    publish(&db, id, 1, 2).unwrap();
    let version: i64 = db
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    db.execute(
        "INSERT INTO workflow_runs(version_id, request_id, definition_json, definition_hash, task_id, task_title, repo, status, created_at, updated_at) VALUES(?1, 'r', '{}', 'h', ?2, 'Zadanie', 'C:/repo', 'running', 1, 2)",
        rusqlite::params![version, task],
    )
    .unwrap();
    let run: i64 = db
        .query_row("SELECT id FROM workflow_runs", [], |r| r.get(0))
        .unwrap();
    for (sequence, kind) in [(1, "runCreated"), (2, "runStateChanged")] {
        db.execute(
            "INSERT INTO workflow_events(run_id, sequence, type, payload_json, created_at) VALUES(?1, ?2, ?3, '{}', ?2)",
            rusqlite::params![run, sequence, kind],
        )
        .unwrap();
    }
    db.execute(
        "INSERT INTO workflow_artifacts(run_id, kind, hash, rel_path, size, created_at) VALUES(?1, 'diff', 'h', 'workflows/runs/1/diff-h.patch', 5, 1)",
        [run],
    )
    .unwrap();
    db.execute(
        "INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at) VALUES(?1, 'C:/repo', 'C:/wt', 'abc', 'reserved', 1)",
        [run],
    )
    .unwrap();
    // Stan E5: bez tabeli procesów, kolumn E6 i wpisu migracji 5.
    db.execute("DROP TABLE workflow_processes", []).unwrap();
    db.execute("ALTER TABLE workflow_artifacts DROP COLUMN purged_at", [])
        .unwrap();
    db.execute(
        "ALTER TABLE workflow_workspaces DROP COLUMN release_reason",
        [],
    )
    .unwrap();
    db.execute(
        "DELETE FROM workflow_schema_migrations WHERE version = 5",
        [],
    )
    .unwrap();
    assert!(!has_migration(&db, 5));
    let tables = [
        "tasks",
        "sessions",
        "workflow_runs",
        "workflow_events",
        "workflow_artifacts",
        "workflow_workspaces",
    ];
    let before: Vec<(Vec<String>, Vec<Vec<String>>)> = tables
        .iter()
        .map(|t| {
            let cols = columns(&db, t);
            let rows = dump(&db, t, &cols);
            (cols, rows)
        })
        .collect();
    assert!(before.iter().all(|(_, rows)| !rows.is_empty()));

    init_tables(&db).unwrap();
    init_tables(&db).unwrap();

    assert!(has_migration(&db, 5));
    assert!(has_column(&db, "workflow_processes", "pid"));
    for (table, (cols, rows)) in tables.iter().zip(&before) {
        assert_eq!(&dump(&db, table, cols), rows, "{table}");
    }
    let purged: Option<i64> = db
        .query_row("SELECT purged_at FROM workflow_artifacts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(purged, None);
}

#[test]
fn set_locked_removes_only_the_flag() {
    let db = db();
    let id = template(&db);
    publish(&db, id, 1, 2_000).unwrap();
    db.execute("INSERT INTO settings(key,value) VALUES('theme','dark')", [])
        .unwrap();
    set_unlocked(&db).unwrap();
    set_locked(&db).unwrap();
    set_locked(&db).unwrap();
    assert!(!is_unlocked(&db).unwrap());
    assert_eq!(list(&db, true).unwrap().len(), 1);
    let theme: String = db
        .query_row("SELECT value FROM settings WHERE key='theme'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(theme, "dark");
}

/// Baza w pełnym schemacie z danymi w każdej tabeli modułu oraz w `tasks`,
/// `sessions` i `settings`.
fn seeded_full_db() -> Connection {
    let mut db = db();
    crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
    let task: i64 = db
        .query_row("SELECT id FROM tasks", [], |r| r.get(0))
        .unwrap();
    db.execute(
        "INSERT INTO sessions(task_id, started_at, ended_at) VALUES(?1, 1000, 61000), (?1, 70000, NULL)",
        [task],
    )
    .unwrap();
    db.execute(
        "INSERT INTO settings(key, value) VALUES('test-ustawienie', 'wartość')",
        [],
    )
    .unwrap();
    let id = template(&db);
    publish(&db, id, 1, 2).unwrap();
    db.execute_batch(
        "INSERT INTO workflow_runs(version_id, request_id, definition_json, definition_hash, task_id, task_title, repo, status, created_at, updated_at, goal)
           VALUES((SELECT id FROM workflow_versions), 'r', '{}', 'h', 1, 'Zadanie', 'C:/repo', 'waitingForApproval', 1, 2, 'cel');
         INSERT INTO workflow_node_executions(id, run_id, node_id, iteration, attempt, status) VALUES('e-1', 1, 'analysis', 1, 1, 'completed');
         INSERT INTO workflow_events(run_id, sequence, type, payload_json, created_at) VALUES(1, 1, 'runCreated', '{}', 1), (1, 2, 'runStateChanged', '{}', 2);
         INSERT INTO workflow_approvals(id, run_id, execution_id, artifact_hash, status) VALUES('a-1', 1, 'e-1', 'h', 'pending');
         INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at) VALUES(1, 'C:/repo', 'C:/wt', 'abc', 'reserved', 1);
         INSERT INTO workflow_artifacts(run_id, kind, hash, rel_path, size, created_at) VALUES(1, 'diff', 'h', 'workflows/runs/1/diff-h.patch', 5, 1);
         INSERT INTO workflow_processes(execution_id, run_id, pid, created_ft, job_attached, state, recorded_at) VALUES('e-1', 1, 10, 20, 1, 'exited', 3);",
    )
    .unwrap();
    db
}

/// Cofa bazę do stanu schematu modułu `state` (0 = wydanie 2.4.0 bez modułu),
/// tak jak wyglądała po wydaniu danego etapu.
fn downgrade_to(db: &Connection, state: i64) {
    if state < 5 {
        db.execute_batch(
            "DROP TABLE workflow_processes;
             ALTER TABLE workflow_artifacts DROP COLUMN purged_at;
             ALTER TABLE workflow_workspaces DROP COLUMN release_reason;
             DELETE FROM workflow_schema_migrations WHERE version = 5;",
        )
        .unwrap();
    }
    if state < 4 {
        db.execute_batch(
            "DROP TABLE workflow_artifacts;
             ALTER TABLE workflow_runs DROP COLUMN goal;
             DELETE FROM workflow_schema_migrations WHERE version = 4;",
        )
        .unwrap();
    }
    if state < 3 {
        db.execute_batch(
            "DROP TABLE workflow_workspaces;
             DELETE FROM workflow_schema_migrations WHERE version = 3;",
        )
        .unwrap();
    }
    if state < 2 {
        db.execute_batch(
            "DROP TABLE workflow_approvals; DROP TABLE workflow_node_executions;
             DROP TABLE workflow_events; DROP TABLE workflow_runs;
             DELETE FROM workflow_schema_migrations WHERE version = 2;",
        )
        .unwrap();
    }
    if state < 1 {
        db.execute_batch(
            "DROP TABLE workflow_versions; DROP TABLE workflows;
             DROP TABLE workflow_schema_migrations;",
        )
        .unwrap();
    }
}

fn existing_tables(db: &Connection) -> Vec<String> {
    let mut statement = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    let names = statement.query_map([], |r| r.get(0)).unwrap();
    names.collect::<Result<Vec<String>, _>>().unwrap()
}

/// `db::init` (z `init_tables`) jest idempotentne z każdego stanu schematu 0–5,
/// w tym z bazy wydania 2.4.0 bez żadnej tabeli modułu: dwa uruchomienia
/// nie zmieniają treści istniejących tabel i kończą na pełnym schemacie.
#[test]
fn init_is_idempotent_from_each_schema_state_and_keeps_data() {
    for state in 0..=MODULE_SCHEMA {
        let db = seeded_full_db();
        downgrade_to(&db, state);
        assert_eq!(module_schema_of(&db), state, "stan {state}");
        if state == 0 {
            assert!(
                existing_tables(&db)
                    .iter()
                    .all(|t| !t.starts_with("workflow")),
                "2.4.0: bez tabel modułu"
            );
        }
        let tables = existing_tables(&db);
        let before: Vec<(Vec<String>, Vec<Vec<String>>)> = tables
            .iter()
            .map(|t| {
                let cols = columns(&db, t);
                let rows = dump(&db, t, &cols);
                (cols, rows)
            })
            .collect();
        for table in ["tasks", "sessions", "settings"] {
            let i = tables.iter().position(|t| t == table).unwrap();
            assert!(!before[i].1.is_empty(), "stan {state}: {table} ma dane");
        }

        crate::db::init(&db).unwrap();
        crate::db::init(&db).unwrap();

        assert_eq!(module_schema_of(&db), MODULE_SCHEMA, "stan {state}");
        assert!(has_column(&db, "workflow_processes", "created_ft"));
        assert!(has_column(&db, "workflow_runs", "goal"));
        for (table, (cols, rows)) in tables.iter().zip(&before) {
            let after = dump(&db, table, cols);
            if table == "workflow_schema_migrations" {
                // Dochodzą wpisy brakujących wersji; dotychczasowe zostają bez zmian.
                assert_eq!(&after[..rows.len()], &rows[..], "stan {state}: {table}");
                assert_eq!(after.len() as i64, MODULE_SCHEMA, "stan {state}: {table}");
            } else {
                assert_eq!(&after, rows, "stan {state}: {table}");
            }
        }
        assert!(!is_unlocked(&db).unwrap(), "stan {state}: bez odblokowania");
    }
}
