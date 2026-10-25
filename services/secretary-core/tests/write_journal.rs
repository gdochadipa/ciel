use secretary_core::journal::{Journal, Preview};
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
        "reconciliation"
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
    assert_eq!(journal.get("expired-op").await.unwrap().state, "expired");
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
