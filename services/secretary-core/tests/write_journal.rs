use secretary_core::{
    domain::operation::OperationState,
    journal::{Journal, Preview},
};
use serde_json::json;
async fn store() -> Journal {
    let dir = std::env::temp_dir().join(format!("secretary-journal-{}", uuid::Uuid::new_v4()));
    Journal::open(&dir).await.unwrap()
}
fn preview() -> Preview {
    Preview {
        operation_id: "op-demo-one".into(),
        actor: "configured-owner".into(),
        spreadsheet: "wedding-demo".into(),
        view: "tasks".into(),
        action: "append".into(),
        record_id: "record-generated".into(),
        fields: json!({"title":"Sample task"}),
        before: json!(null),
        after: json!({"title":"Sample task"}),
        mapping_version: "demo-v1".into(),
        source_revision: "snapshot-one".into(),
        expires_at: chrono::Utc::now().timestamp() + 300,
        preview_digest: String::new(),
    }
}
#[tokio::test]
async fn immutable_previews_and_approvals_survive_restart_without_replaying_intents() {
    let journal = store().await;
    let p = journal.create(preview()).await.unwrap();
    let again = journal.create(preview()).await.unwrap();
    assert_eq!(p.preview_digest, again.preview_digest);
    let mut altered = preview();
    altered.fields = json!({"title":"Altered task"});
    assert!(journal.create(altered).await.is_err());
    assert!(journal
        .approve("op-demo-one", "wrong-digest", "approve")
        .await
        .is_err());
    journal
        .approve("op-demo-one", &p.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply("op-demo-one").await.unwrap();
    let dir = journal.directory().to_path_buf();
    drop(journal);
    let restarted = Journal::open(&dir).await.unwrap();
    assert_eq!(
        restarted.get("op-demo-one").await.unwrap().state,
        OperationState::Reconciliation
    );
    assert!(restarted.begin_apply("op-demo-one").await.is_err());
}
#[tokio::test]
async fn rejected_or_expired_previews_cannot_enter_applying() {
    let journal = store().await;
    let p = journal.create(preview()).await.unwrap();
    journal
        .approve("op-demo-one", &p.preview_digest, "reject")
        .await
        .unwrap();
    assert!(journal.begin_apply("op-demo-one").await.is_err());
    let mut expired = preview();
    expired.operation_id = "expired-op".into();
    expired.expires_at = 0;
    let p = journal.create(expired).await.unwrap();
    assert!(journal
        .approve("expired-op", &p.preview_digest, "approve")
        .await
        .is_err());
    assert_eq!(
        journal.get("expired-op").await.unwrap().state,
        OperationState::Expired
    );
}
#[tokio::test]
async fn a_second_service_cannot_claim_the_same_journal_and_recover_a_live_write() {
    let journal = store().await;
    assert!(Journal::open(journal.directory()).await.is_err());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(journal.directory().join("secretary.sqlite3"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[tokio::test]
async fn uncertainty_requires_evidence_and_terminal_operations_cannot_be_replayed() {
    use secretary_core::domain::operation::Outcome;
    let journal = store().await;
    let p = journal.create(preview()).await.unwrap();
    journal
        .approve(&p.operation_id, &p.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply(&p.operation_id).await.unwrap();
    journal
        .finish(&p.operation_id, Outcome::Uncertain)
        .await
        .unwrap();
    assert!(journal
        .finish(&p.operation_id, Outcome::KnownFailure)
        .await
        .is_err());
    assert!(journal.begin_apply(&p.operation_id).await.is_err());
    assert_eq!(
        journal.get(&p.operation_id).await.unwrap().state,
        OperationState::Uncertain
    );
    journal
        .finish(&p.operation_id, Outcome::Applied)
        .await
        .unwrap();
    assert!(journal
        .approve(&p.operation_id, &p.preview_digest, "approve")
        .await
        .is_err());
    assert!(journal
        .finish(&p.operation_id, Outcome::Conflict)
        .await
        .is_err());
    assert_eq!(
        journal.history(Some(&p.operation_id), 20, 0).await.unwrap()["operation"]["state"],
        "applied"
    );
}

#[tokio::test]
async fn live_journal_backup_restores_interrupted_write_without_authorizing_replay() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let journal = store().await;
    let preview = journal.create(preview()).await.unwrap();
    journal
        .approve(&preview.operation_id, &preview.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply(&preview.operation_id).await.unwrap();
    // Keep the actual Core connection open: committed WAL state must be included.
    assert!(
        std::fs::metadata(journal.directory().join("secretary.sqlite3-wal"))
            .unwrap()
            .len()
            > 0
    );
    let backup_dir =
        std::env::temp_dir().join(format!("secretary-backup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&backup_dir).unwrap();
    std::fs::set_permissions(&backup_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let snapshot = backup_dir.join("snapshot.sqlite3");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/backup.py");
    let run_backup = || {
        Command::new("python3")
            .arg(&script)
            .arg(journal.directory().join("secretary.sqlite3"))
            .arg(&snapshot)
            .output()
            .unwrap()
    };
    assert!(run_backup().status.success());
    assert_eq!(
        std::fs::metadata(&snapshot).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let original_snapshot = std::fs::read(&snapshot).unwrap();
    assert!(!run_backup().status.success());
    assert_eq!(std::fs::read(&snapshot).unwrap(), original_snapshot);
    assert_eq!(
        journal.get(&preview.operation_id).await.unwrap().state,
        OperationState::Applying
    );
    // Restoring uses a separate protected directory and does not disturb the live source.
    let restored_dir = backup_dir.join("restored");
    std::fs::create_dir(&restored_dir).unwrap();
    std::fs::copy(&snapshot, restored_dir.join("secretary.sqlite3")).unwrap();
    let restored = Journal::open(&restored_dir).await.unwrap();
    let recovered = restored.get(&preview.operation_id).await.unwrap();
    assert_eq!(recovered.preview.preview_digest, preview.preview_digest);
    assert_eq!(recovered.state, OperationState::Reconciliation);
    assert!(restored.begin_apply(&preview.operation_id).await.is_err());
    let history = restored
        .history(Some(&preview.operation_id), 20, 0)
        .await
        .unwrap();
    assert!(history["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event"] == "approved"));
    assert_eq!(
        journal.get(&preview.operation_id).await.unwrap().state,
        OperationState::Applying
    );
    assert!(std::fs::read_dir(&backup_dir).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".secretary-backup-")));
    drop(restored);
    drop(journal);
    std::fs::remove_dir_all(&restored_dir).unwrap();
    std::fs::remove_dir_all(&backup_dir).unwrap();
}
