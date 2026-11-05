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
    // Wersje 1 (definicje) i 2 (przebiegi, E3).
    assert_eq!(versions, 2);
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
