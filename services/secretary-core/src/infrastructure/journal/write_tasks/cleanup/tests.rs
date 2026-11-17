use super::*;
use crate::{
    domain::{
        operation::{Outcome, Preview, ResourceKind},
        write_task::{TaskContext, TaskScope, WriteTarget},
    },
    infrastructure::journal::Journal,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn context() -> TaskContext {
    TaskContext {
        gateway_instance: "gateway".into(),
        agent_id: "agent".into(),
        session_key: "owner-session".into(),
        session_id: "incarnation".into(),
    }
}
fn scope() -> TaskScope {
    TaskScope {
        objective: "Bounded sample update".into(),
        mapping_version: "v1".into(),
        max_operations: 1000,
        undo_of: None,
        targets: vec![WriteTarget::Document {
            document: "notes".into(),
            document_id: "doc".into(),
            tab_id: "tab".into(),
        }],
    }
}
fn capture(id: &str) -> Preview {
    Preview {
        resource_kind: ResourceKind::GoogleDocs,
        operation_id: id.into(),
        actor: "owner".into(),
        spreadsheet: "notes".into(),
        view: "tab".into(),
        action: "replace_text".into(),
        record_id: "doc".into(),
        fields: json!({"find":"old","replacement":"new"}),
        before: json!({"text":"old note\n"}),
        after: json!({"text":"new note\n"}),
        mapping_version: "v1".into(),
        source_revision: "r1".into(),
        expires_at: chrono::Utc::now().timestamp() + 300,
        preview_digest: String::new(),
    }
}
async fn open(path: &Path) -> (Journal, JournalWriteTasks) {
    let journal = Journal::open(path).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment")
        .await
        .unwrap();
    (journal, ledger)
}
async fn original(journal: &Journal, id: &str) -> Preview {
    let p = journal.create(capture(id)).await.unwrap();
    journal
        .approve(id, &p.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply(id).await.unwrap();
    journal.finish(id, Outcome::Applied).await.unwrap();
    p
}
fn selection(ids: &[&str]) -> CleanupSelection {
    CleanupSelection {
        task_ids: vec![],
        operation_ids: ids.iter().map(|id| (*id).into()).collect(),
        completed_before: chrono::Utc::now().timestamp(),
    }
}
async fn plan(ledger: &JournalWriteTasks, selected: CleanupSelection) -> Value {
    ledger
        .cleanup(&CleanupCommand::Preview {
            selection: selected,
        })
        .await
        .unwrap()
}
fn decide(plan: &Value, decision: CleanupDecision) -> CleanupCommand {
    CleanupCommand::Decide {
        cleanup_id: plan["cleanup_id"].as_str().unwrap().into(),
        preview_digest: plan["preview_digest"].as_str().unwrap().into(),
        decision,
    }
}
fn error<T>(result: Result<T, CoreError>, code: &str) {
    assert_eq!(result.err().expect("must deny").1, code);
}
async fn grant(ledger: &JournalWriteTasks) -> String {
    let proposal = ledger.propose(&context(), &scope()).await.unwrap();
    let id = proposal["task_id"].as_str().unwrap();
    ledger
        .decide(
            id,
            &context(),
            proposal["scope_digest"].as_str().unwrap(),
            0,
            super::super::TaskDecision::Approve,
        )
        .await
        .unwrap();
    id.into()
}
async fn completed(ledger: &JournalWriteTasks, id: &str) {
    ledger
        .end(id, &context(), super::super::TaskEnd::Completed)
        .await
        .unwrap();
}

#[tokio::test]
async fn exact_owner_decision_removes_only_payloads_and_replays_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    let original = original(&journal, "original").await;
    let preview = plan(&ledger, selection(&["original"])).await;
    assert_eq!(preview["plan"]["applied_originals_losing_evidence"], 1);
    let identity = &preview["plan"]["members"][0]["identity"];
    assert_eq!(identity["record_id"], "doc");
    assert!(identity.get("before").is_none());
    assert!(identity.get("fields").is_none());
    assert!(
        preview["plan"]["estimated_reusable_payload_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    let request = decide(&preview, CleanupDecision::Approve);
    let (one, two) = tokio::join!(ledger.cleanup(&request), ledger.cleanup(&request));
    let applied = one.unwrap();
    assert_eq!(two.unwrap(), applied);
    assert_eq!(applied["removed_operations"], 1);
    assert_eq!(applied["explicit_physical_shrink"], false);
    assert_eq!(applied["native_memory_deleted"], false);
    assert!(applied["page_size"].as_u64().unwrap() > 0);
    let history = journal.history(Some("original"), 20, 0).await.unwrap();
    assert_eq!(history["operation"]["state"], "applied");
    assert_eq!(
        history["operation"]["request_digest"],
        original.request_digest()
    );
    assert_eq!(
        history["operation"]["evidence"]["cleanup_id"],
        preview["cleanup_id"]
    );
    error(
        journal.create(original).await,
        "operation_evidence_unavailable",
    );
    assert_eq!(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await
            .unwrap(),
        applied
    );
    error(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Reject))
            .await,
        "cleanup_decision_conflict",
    );
    let events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE event LIKE 'evidence_removed:%'")
            .fetch_one(&journal.pool)
            .await
            .unwrap();
    assert_eq!(events, 1);
    drop(ledger);
    drop(journal);
    let (_, ledger) = open(directory.path()).await;
    assert_eq!(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await
            .unwrap(),
        applied
    );
    let observed = ledger
        .cleanup(&CleanupCommand::Inspect {
            cleanup_id: preview["cleanup_id"].as_str().unwrap().into(),
        })
        .await
        .unwrap();
    assert_eq!(observed["result"], applied);
}

#[tokio::test]
async fn pending_preview_requires_fresh_confirmation_after_restart_and_reject_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    original(&journal, "original").await;
    let preview = plan(&ledger, selection(&["original"])).await;
    let mut wrong = decide(&preview, CleanupDecision::Approve);
    if let CleanupCommand::Decide { preview_digest, .. } = &mut wrong {
        *preview_digest = "wrong".into();
    }
    error(ledger.cleanup(&wrong).await, "cleanup_digest_mismatch");
    let stored: String = sqlx::query_scalar("SELECT plan FROM evidence_cleanups WHERE id=?")
        .bind(preview["cleanup_id"].as_str().unwrap())
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE evidence_cleanups SET plan=json_set(plan,'$.estimated_reusable_payload_bytes',0)",
    )
    .execute(&journal.pool)
    .await
    .unwrap();
    error(
        ledger
            .cleanup(&CleanupCommand::Inspect {
                cleanup_id: preview["cleanup_id"].as_str().unwrap().into(),
            })
            .await,
        "journal_unavailable",
    );
    sqlx::query("UPDATE evidence_cleanups SET plan=?")
        .bind(stored)
        .execute(&journal.pool)
        .await
        .unwrap();
    drop(ledger);
    drop(journal);
    let (journal, ledger) = open(directory.path()).await;
    error(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await,
        "cleanup_confirmation_required",
    );
    assert_eq!(
        ledger
            .cleanup(&CleanupCommand::Inspect {
                cleanup_id: preview["cleanup_id"].as_str().unwrap().into()
            })
            .await
            .unwrap()["requires_fresh_preview"],
        true
    );
    let fresh = plan(&ledger, selection(&["original"])).await;
    assert_ne!(fresh["preview_digest"], preview["preview_digest"]);
    let denied = ledger
        .cleanup(&decide(&fresh, CleanupDecision::Reject))
        .await
        .unwrap();
    assert_eq!(denied["removed_operations"], 0);
    assert_eq!(
        ledger
            .cleanup(&decide(&fresh, CleanupDecision::Reject))
            .await
            .unwrap(),
        denied
    );
    error(
        ledger
            .cleanup(&decide(&fresh, CleanupDecision::Approve))
            .await,
        "cleanup_decision_conflict",
    );
    assert!(journal.get("original").await.is_ok());
}

#[tokio::test]
async fn cleanup_protects_grants_dialogs_recovery_age_and_whole_completed_tasks() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    original(&journal, "original").await;
    let preview = plan(&ledger, selection(&["original"])).await;
    let pending = ledger.propose(&context(), &scope()).await.unwrap();
    let pending_id = pending["task_id"].as_str().unwrap();
    error(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await,
        "cleanup_active_task",
    );
    ledger
        .end(pending_id, &context(), super::super::TaskEnd::Cancelled)
        .await
        .unwrap();
    journal.create(capture("pending")).await.unwrap();
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selection(&["original"]),
            })
            .await,
        "cleanup_recovery_required",
    );
    let p = journal.get("pending").await.unwrap().preview;
    journal
        .approve("pending", &p.preview_digest, "reject")
        .await
        .unwrap();
    let mut old = selection(&["original"]);
    old.completed_before = 1;
    error(
        ledger
            .cleanup(&CleanupCommand::Preview { selection: old })
            .await,
        "cleanup_age_mismatch",
    );
    let task = grant(&ledger).await;
    let op = ledger
        .admit(&task, &context(), capture("member"))
        .await
        .unwrap();
    ledger
        .discard(&task, &context(), &op.preview.operation_id)
        .await
        .unwrap();
    completed(&ledger, &task).await;
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selection(&["member"]),
            })
            .await,
        "cleanup_whole_task_required",
    );
    let mut selected = selection(&[]);
    selected.task_ids.push(pending_id.into());
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selected,
            })
            .await,
        "cleanup_empty_selection",
    );
    let cancelled = grant(&ledger).await;
    let member = ledger
        .admit(&cancelled, &context(), capture("cancelled-member"))
        .await
        .unwrap();
    ledger
        .discard(&cancelled, &context(), &member.preview.operation_id)
        .await
        .unwrap();
    ledger
        .end(&cancelled, &context(), super::super::TaskEnd::Cancelled)
        .await
        .unwrap();
    let mut selected = selection(&[]);
    selected.task_ids.push(cancelled);
    let cancelled_preview = plan(&ledger, selected).await;
    assert_eq!(cancelled_preview["plan"]["tasks"][0]["reason"], "cancelled");
    ledger
        .cleanup(&decide(&cancelled_preview, CleanupDecision::Approve))
        .await
        .unwrap();
    ledger
        .cleanup(&decide(&preview, CleanupDecision::Approve))
        .await
        .unwrap();
    assert!(journal.get("pending").await.is_ok());
}

#[tokio::test]
async fn original_inverse_edges_require_the_complete_selected_graph() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    let original = original(&journal, "original").await;
    let task = grant(&ledger).await;
    let mut inverse = capture("inverse");
    inverse.fields = json!({"find":"new","replacement":"old"});
    inverse.before = original.after.clone();
    inverse.after = original.before.clone();
    ledger
        .admit_inverse(&task, &context(), "original", inverse)
        .await
        .unwrap();
    ledger.claim(&task, &context(), "inverse").await.unwrap();
    journal.finish("inverse", Outcome::Applied).await.unwrap();
    completed(&ledger, &task).await;
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selection(&["original"]),
            })
            .await,
        "cleanup_dependency_reference",
    );
    let mut selected = selection(&[]);
    selected.task_ids.push(task.clone());
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selected.clone(),
            })
            .await,
        "cleanup_dependency_reference",
    );
    selected.operation_ids.push("original".into());
    let preview = plan(&ledger, selected).await;
    assert_eq!(preview["plan"]["inverse_payloads"], 1);
    ledger
        .cleanup(&decide(&preview, CleanupDecision::Approve))
        .await
        .unwrap();
    let history = ledger.inverse_history("original", 20, 0).await.unwrap();
    assert_eq!(history["attempts"][0]["operation_id"], "inverse");
    assert_eq!(history["attempts"][0]["state"], "applied");
    assert_eq!(history["original_evidence"]["status"], "unavailable");
    assert_eq!(
        ledger.history(20, 0).await.unwrap()["tasks"][0]["unavailable_evidence"],
        1
    );
}

#[tokio::test]
async fn audit_failure_rolls_back_all_payloads_receipts_decision_and_events() {
    for event in ["evidence_removed:%", "evidence_cleanup_approved:%"] {
        let directory = tempfile::tempdir().unwrap();
        let (journal, ledger) = open(directory.path()).await;
        original(&journal, "one").await;
        original(&journal, "two").await;
        let preview = plan(&ledger, selection(&["one", "two"])).await;
        sqlx::query(&format!("CREATE TRIGGER fail_cleanup BEFORE INSERT ON audit WHEN NEW.event LIKE '{event}' BEGIN SELECT RAISE(ABORT,'injected audit failure'); END")).execute(&journal.pool).await.unwrap();
        error(
            ledger
                .cleanup(&decide(&preview, CleanupDecision::Approve))
                .await,
            "journal_unavailable",
        );
        for id in ["one", "two"] {
            assert!(journal.get(id).await.is_ok());
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM operations WHERE evidence_cleanup IS NOT NULL",
        )
        .fetch_one(&journal.pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE event LIKE 'evidence_removed:%'")
                .fetch_one(&journal.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TRIGGER fail_cleanup")
            .execute(&journal.pool)
            .await
            .unwrap();
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sqlite_full_rolls_back_cleanup_and_blocks_new_intent_until_storage_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    original(&journal, "original").await;
    let preview = plan(&ledger, selection(&["original"])).await;
    sqlx::query("CREATE TABLE exhaustion_probe(bytes BLOB)")
        .execute(&journal.pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fill_cleanup BEFORE INSERT ON audit WHEN NEW.event LIKE 'evidence_cleanup_approved:%' BEGIN INSERT INTO exhaustion_probe VALUES(zeroblob(4194304)); END").execute(&journal.pool).await.unwrap();
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    sqlx::query(&format!("PRAGMA max_page_count={pages}"))
        .execute(&journal.pool)
        .await
        .unwrap();
    let full = sqlx::query("INSERT INTO exhaustion_probe VALUES(zeroblob(4194304))")
        .execute(&journal.pool)
        .await
        .unwrap_err();
    assert_eq!(
        full.as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("13"),
        "fixture must actually produce SQLITE_FULL"
    );
    error(
        ledger
            .cleanup(&decide(&preview, CleanupDecision::Approve))
            .await,
        "journal_unavailable",
    );
    assert!(journal.get("original").await.is_ok());
    let mut large = capture("not-retained");
    large.after = json!({"text":"x".repeat(131072)});
    error(journal.create(large).await, "journal_unavailable");
    assert!(journal.find("not-retained").await.unwrap().is_none());
    sqlx::query("PRAGMA max_page_count=1073741823")
        .execute(&journal.pool)
        .await
        .unwrap();
    sqlx::query("DROP TRIGGER fill_cleanup")
        .execute(&journal.pool)
        .await
        .unwrap();
    assert_eq!(
        ledger
            .cleanup(&CleanupCommand::Inspect {
                cleanup_id: preview["cleanup_id"].as_str().unwrap().into()
            })
            .await
            .unwrap()["decision"],
        "pending"
    );
    ledger
        .cleanup(&decide(&preview, CleanupDecision::Approve))
        .await
        .unwrap();
}

#[tokio::test]
async fn cleanup_and_new_undo_selection_linearize_without_resurrecting_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    original(&journal, "original").await;
    let preview = plan(&ledger, selection(&["original"])).await;
    let decision = decide(&preview, CleanupDecision::Approve);
    let current = context();
    let mut scoped = scope();
    scoped.undo_of = Some(crate::domain::write_task::UndoSource::Operation {
        operation_id: "original".into(),
    });
    let (cleaned, undo) =
        tokio::join!(ledger.cleanup(&decision), ledger.propose(&current, &scoped));
    let undo = undo.unwrap();
    let undo_id = undo["task_id"].as_str().unwrap();
    ledger
        .decide(
            undo_id,
            &current,
            undo["scope_digest"].as_str().unwrap(),
            0,
            super::super::TaskDecision::Approve,
        )
        .await
        .unwrap();
    use crate::application::task_authority::TaskLedger;
    let original = ledger
        .inverse_original(undo_id, &current, "original", "inverse-after-race")
        .await;
    if cleaned.is_ok() {
        error(original, "operation_evidence_unavailable");
        error(
            journal.get("original").await,
            "operation_evidence_unavailable",
        );
    } else {
        error(cleaned, "cleanup_active_task");
        assert_eq!(original.unwrap().state, OperationState::Applied);
        assert!(journal.get("original").await.is_ok());
    }
}

#[tokio::test]
async fn selectors_and_whole_task_member_limits_never_silently_truncate() {
    let directory = tempfile::tempdir().unwrap();
    let (journal, ledger) = open(directory.path()).await;
    for mut selected in [
        selection(&[]),
        selection(&["same", "same"]),
        selection(&["bad/id"]),
    ] {
        selected.completed_before = chrono::Utc::now().timestamp();
        error(
            ledger
                .cleanup(&CleanupCommand::Preview {
                    selection: selected,
                })
                .await,
            "invalid_input",
        );
    }
    let mut too_many = selection(&[]);
    too_many.operation_ids = (0..21).map(|i| format!("id-{i}")).collect();
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: too_many,
            })
            .await,
        "invalid_input",
    );
    let task = grant(&ledger).await;
    for n in 0..201 {
        let id = format!("member-{n}");
        ledger.admit(&task, &context(), capture(&id)).await.unwrap();
        ledger.claim(&task, &context(), &id).await.unwrap();
        journal.finish(&id, Outcome::Applied).await.unwrap();
    }
    completed(&ledger, &task).await;
    let mut selected = selection(&[]);
    selected.task_ids.push(task);
    error(
        ledger
            .cleanup(&CleanupCommand::Preview {
                selection: selected,
            })
            .await,
        "cleanup_selection_limit",
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_evidence")
        .fetch_one(&journal.pool)
        .await
        .unwrap();
    assert_eq!(count, 201);
}

// Only compiled into unit tests. The child process parks after a precise commit
// boundary so its parent can kill it, without shipping a production failpoint.
pub(super) fn crash_boundary(ledger: &JournalWriteTasks, boundary: &str, result: &Value) {
    if std::env::var("CIEL_CLEANUP_TEST_BOUNDARY").as_deref() != Ok(boundary) {
        return;
    }
    let Ok(path) = std::env::var("CIEL_CLEANUP_TEST_DIRECTORY") else {
        return;
    };
    if ledger.journal.directory() != Path::new(&path) {
        return;
    }
    std::fs::write(
        Path::new(&path).join("boundary.tmp"),
        serde_json::to_vec(result).unwrap(),
    )
    .unwrap();
    std::fs::rename(
        Path::new(&path).join("boundary.tmp"),
        Path::new(&path).join("boundary.json"),
    )
    .unwrap();
    loop {
        std::thread::park();
    }
}
#[tokio::test]
async fn crash_worker() {
    let Ok(path) = std::env::var("CIEL_CLEANUP_TEST_DIRECTORY") else {
        return;
    };
    let (_, ledger) = open(Path::new(&path)).await;
    let preview = plan(&ledger, selection(&["original"])).await;
    ledger
        .cleanup(&decide(&preview, CleanupDecision::Approve))
        .await
        .unwrap();
}
#[test]
fn killed_cleanup_before_or_after_commit_is_atomic_and_receipt_observable() {
    for boundary in ["before_commit", "after_commit"] {
        let directory = tempfile::tempdir().unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (journal, _ledger) = open(directory.path()).await;
            original(&journal, "original").await;
        });
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "infrastructure::journal::write_tasks::cleanup::tests::crash_worker",
                "--nocapture",
            ])
            .env("CIEL_CLEANUP_TEST_DIRECTORY", directory.path())
            .env("CIEL_CLEANUP_TEST_BOUNDARY", boundary)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let marker = directory.path().join("boundary.json");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.is_file() && Instant::now() < deadline && child.try_wait().unwrap().is_none()
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ready = marker.is_file();
        child.kill().ok();
        child.wait().unwrap();
        assert!(ready, "child did not reach {boundary}");
        let observed: Value = serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (journal, ledger) = open(directory.path()).await;
            let review = ledger
                .cleanup(&CleanupCommand::Inspect {
                    cleanup_id: observed["cleanup_id"].as_str().unwrap().into(),
                })
                .await
                .unwrap();
            if boundary == "before_commit" {
                assert!(journal.get("original").await.is_ok());
                assert_eq!(review["decision"], "pending");
                assert_eq!(review["requires_fresh_preview"], true);
            } else {
                error(
                    journal.get("original").await,
                    "operation_evidence_unavailable",
                );
                assert_eq!(review["result"], observed);
                assert_eq!(
                    ledger
                        .cleanup(&decide(&observed, CleanupDecision::Approve))
                        .await
                        .unwrap(),
                    observed
                );
            }
        });
    }
}
