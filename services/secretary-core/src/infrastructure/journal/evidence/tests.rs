use super::*;
use crate::domain::operation::Outcome;
use crate::infrastructure::journal::Journal;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

fn preview(id: &str) -> Preview {
    Preview {
        resource_kind: ResourceKind::GoogleDocs,
        operation_id: id.into(),
        actor: "owner".into(),
        spreadsheet: "notes".into(),
        view: "tab".into(),
        action: "replace_text".into(),
        record_id: "physical-doc".into(),
        fields: json!({"find":"old","replacement":"new"}),
        before: json!({"text":"old\n"}),
        after: json!({"text":"new\n"}),
        mapping_version: "v1".into(),
        source_revision: "rev1".into(),
        expires_at: chrono::Utc::now().timestamp() + 300,
        preview_digest: String::new(),
    }
}

async fn legacy(path: &std::path::Path) -> (SqliteConnection, Preview, String) {
    std::fs::create_dir_all(path).unwrap();
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path.join("secretary.sqlite3"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TABLE operations(operation_id TEXT PRIMARY KEY,request_digest TEXT NOT NULL,preview TEXT NOT NULL,state TEXT NOT NULL,updated_at INTEGER NOT NULL)")
        .execute(&mut conn).await.unwrap();
    sqlx::query("CREATE TABLE audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT,operation_id TEXT,event TEXT NOT NULL,occurred_at INTEGER NOT NULL)")
        .execute(&mut conn).await.unwrap();
    let mut p = preview("legacy");
    p.preview_digest = digest(&serde_json::to_value(&p).unwrap());
    // Noncanonical whitespace must survive the migration byte for byte.
    let raw = serde_json::to_string_pretty(&p).unwrap();
    sqlx::query("INSERT INTO operations VALUES(?,?,?,'applied',7)")
        .bind(&p.operation_id)
        .bind(p.request_digest())
        .bind(&raw)
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO audit VALUES(42,'legacy','applied',7)")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE legacy_links(id TEXT PRIMARY KEY,original_id TEXT REFERENCES operations(operation_id))").execute(&mut conn).await.unwrap();
    sqlx::query("INSERT INTO legacy_links VALUES('inverse','legacy')")
        .execute(&mut conn)
        .await
        .unwrap();
    (conn, p, raw)
}

#[tokio::test]
async fn legacy_upgrade_preserves_raw_payload_digests_audit_and_foreign_keys() {
    let directory = tempfile::tempdir().unwrap();
    let (conn, p, raw) = legacy(directory.path()).await;
    drop(conn);
    for _ in 0..2 {
        let journal = Journal::open(directory.path()).await.unwrap();
        assert_eq!(
            journal.get("legacy").await.unwrap().preview.preview_digest,
            p.preview_digest
        );
        let stored: String = sqlx::query_scalar(
            "SELECT preview FROM operation_evidence WHERE operation_id='legacy'",
        )
        .fetch_one(&journal.pool)
        .await
        .unwrap();
        assert_eq!(stored, raw);
        let request: String =
            sqlx::query_scalar("SELECT request_digest FROM operations WHERE operation_id='legacy'")
                .fetch_one(&journal.pool)
                .await
                .unwrap();
        assert_eq!(request, p.request_digest());
        let audit: i64 =
            sqlx::query_scalar("SELECT sequence FROM audit WHERE operation_id='legacy'")
                .fetch_one(&journal.pool)
                .await
                .unwrap();
        assert_eq!(audit, 42);
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&journal.pool)
            .await
            .unwrap()
            .is_empty());
        assert!(
            sqlx::query("DELETE FROM operations WHERE operation_id='legacy'")
                .execute(&journal.pool)
                .await
                .is_err()
        );
        drop(journal);
    }
}

#[tokio::test]
async fn interrupted_upgrade_rolls_back_ddl_and_exact_legacy_payload() {
    let directory = tempfile::tempdir().unwrap();
    let (mut conn, _, raw) = legacy(directory.path()).await;
    sqlx::query("CREATE TRIGGER fail_upgrade BEFORE UPDATE ON operations BEGIN SELECT RAISE(ABORT,'injected migration interruption'); END")
        .execute(&mut conn).await.unwrap();
    drop(conn);
    assert_eq!(
        Journal::open(directory.path()).await.err().unwrap().1,
        "journal_unavailable"
    );
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(directory.path().join("secretary.sqlite3")),
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT preview FROM operations WHERE operation_id='legacy'"
        )
        .fetch_one(&mut conn)
        .await
        .unwrap(),
        raw
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name='operation_evidence'")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_upgrade")
        .execute(&mut conn)
        .await
        .unwrap();
    drop(conn);
    assert!(Journal::open(directory.path()).await.is_ok());
}

// Synthetic future cleanup receipt in a disposable test journal. The production
// library deliberately exposes no removal/receipt-writing entry point yet.
async fn remove(journal: &Journal, id: &str) {
    let mut tx = journal.pool.begin().await.unwrap();
    sqlx::query("DELETE FROM operation_evidence WHERE operation_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE operations SET evidence_cleanup=? WHERE operation_id=?")
        .bind(json!({"version":1,"cleanup_id":"cleanup-test","removed_at":100}).to_string())
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,'evidence_removed:cleanup-test',100)")
        .bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn unavailable_evidence_retains_history_and_spends_ids_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let p = journal.create(preview("spent")).await.unwrap();
    journal
        .approve("spent", &p.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply("spent").await.unwrap();
    journal.finish("spent", Outcome::Applied).await.unwrap();
    remove(&journal, "spent").await;
    drop(journal);
    let journal = Journal::open(directory.path()).await.unwrap();
    for error in [
        journal.find("spent").await.err().unwrap(),
        journal.get("spent").await.err().unwrap(),
        journal.create(preview("spent")).await.err().unwrap(),
        journal
            .approve("spent", &p.preview_digest, "approve")
            .await
            .err()
            .unwrap(),
        journal.begin_apply("spent").await.err().unwrap(),
        journal
            .finish("spent", Outcome::Applied)
            .await
            .err()
            .unwrap(),
    ] {
        assert_eq!(error.1, "operation_evidence_unavailable");
    }
    let mut changed = preview("spent");
    changed.fields = json!({"find":"other","replacement":"intent"});
    assert_eq!(
        journal.create(changed).await.err().unwrap().1,
        "operation_conflict"
    );
    assert_eq!(
        journal.get("nonexistent").await.err().unwrap().1,
        "operation_not_found"
    );
    let history = journal.history(Some("spent"), 20, 0).await.unwrap();
    let operation = &history["operation"];
    assert_eq!(operation["state"], "applied");
    assert_eq!(operation["preview_digest"], p.preview_digest);
    assert_eq!(operation["request_digest"], p.request_digest());
    assert_eq!(operation["actor"], "owner");
    assert_eq!(operation["document"], "notes");
    assert_eq!(operation["tab_id"], "tab");
    assert_eq!(operation["record_id"], "physical-doc");
    assert_eq!(
        operation["evidence"],
        json!({"status":"unavailable","cleanup_id":"cleanup-test","removed_at":100})
    );
    for key in ["fields", "before", "after", "source_revision", "expires_at"] {
        assert!(operation.get(key).is_none());
    }
    assert_eq!(
        history["events"][0]["event"],
        "evidence_removed:cleanup-test"
    );
    assert_eq!(
        journal.history(None, 1, 0).await.unwrap()["operations"][0],
        *operation
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_evidence")
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn missing_or_contradictory_evidence_never_becomes_a_tombstone() {
    for receipt in [
        None,
        Some("{}"),
        Some(r#"{"version":1,"cleanup_id":"cleanup-test","removed_at":100}"#),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        journal.create(preview("broken")).await.unwrap();
        if receipt.is_none() {
            sqlx::query("DELETE FROM operation_evidence")
                .execute(&journal.pool)
                .await
                .unwrap();
        } else {
            sqlx::query("UPDATE operations SET evidence_cleanup=?")
                .bind(receipt)
                .execute(&journal.pool)
                .await
                .unwrap();
        }
        assert_eq!(
            journal.get("broken").await.err().unwrap().1,
            "journal_unavailable"
        );
        drop(journal);
        assert_eq!(
            Journal::open(directory.path()).await.err().unwrap().1,
            "journal_unavailable"
        );
    }
}

#[tokio::test]
async fn unresolved_outcomes_cannot_lose_evidence_or_be_recovered_as_success() {
    for state in [
        "previewed",
        "approved",
        "applying",
        "uncertain",
        "reconciliation",
        "conflict",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        journal.create(preview("pending")).await.unwrap();
        sqlx::query("UPDATE operations SET state=?")
            .bind(state)
            .execute(&journal.pool)
            .await
            .unwrap();
        remove(&journal, "pending").await;
        assert_eq!(
            journal.get("pending").await.err().unwrap().1,
            "journal_unavailable"
        );
        drop(journal);
        assert_eq!(
            Journal::open(directory.path()).await.err().unwrap().1,
            "journal_unavailable"
        );
    }
}

#[tokio::test]
async fn legacy_upgrade_copies_multiple_bounded_batches_and_preserves_each_seal() {
    let directory = tempfile::tempdir().unwrap();
    let (mut conn, _, _) = legacy(directory.path()).await;
    for index in 0..130 {
        let mut p = preview(&format!("original-{index:03}"));
        p.preview_digest = digest(&serde_json::to_value(&p).unwrap());
        sqlx::query("INSERT INTO operations VALUES(?,?,?,'rejected',7)")
            .bind(&p.operation_id)
            .bind(p.request_digest())
            .bind(serde_json::to_string(&p).unwrap())
            .execute(&mut conn)
            .await
            .unwrap();
    }
    drop(conn);
    let journal = Journal::open(directory.path()).await.unwrap();
    for index in [0, 63, 64, 127, 128, 129] {
        assert_eq!(
            journal
                .get(&format!("original-{index:03}"))
                .await
                .unwrap()
                .state,
            OperationState::Rejected
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_evidence")
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    assert_eq!(count, 131);
    assert_eq!(
        journal.history(None, 100, 0).await.unwrap()["next_cursor"],
        "100"
    );
    assert_eq!(
        journal.history(None, 100, 100).await.unwrap()["operations"]
            .as_array()
            .unwrap()
            .len(),
        31
    );
}

#[tokio::test]
async fn failed_payload_persistence_does_not_publish_partial_identity_or_audit() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    sqlx::query("CREATE TRIGGER no_payload BEFORE INSERT ON operation_evidence BEGIN SELECT RAISE(ABORT,'injected storage exhaustion'); END")
        .execute(&journal.pool).await.unwrap();
    assert_eq!(
        journal.create(preview("unsaved")).await.err().unwrap().1,
        "journal_unavailable"
    );
    assert!(journal.find("unsaved").await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE operation_id='unsaved'")
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER no_payload")
        .execute(&journal.pool)
        .await
        .unwrap();
    assert!(journal.create(preview("unsaved")).await.is_ok());
}
