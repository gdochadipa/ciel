//! Real SQLite task authority. No native owner routing or provider dispatch is claimed.
use secretary_core::{
    domain::{
        operation::{OperationState, Outcome, Preview, ResourceKind},
        write_task::{TaskContext, TaskScope, WriteTarget},
    },
    journal::{
        write_tasks::{JournalWriteTasks, TaskDecision, TaskEnd},
        Journal,
    },
    CoreError,
};
use serde_json::{json, Value};

fn context() -> TaskContext {
    TaskContext {
        gateway_instance: "gateway-1".into(),
        agent_id: "agent".into(),
        session_key: "agent:owner".into(),
        session_id: "conversation-1".into(),
    }
}
fn scope() -> TaskScope {
    TaskScope {
        objective: "Correct the registered sample cells and note".into(),
        mapping_version: "mapping-1".into(),
        max_operations: 10,
        undo_of: None,
        targets: vec![
            WriteTarget::Grid {
                spreadsheet: "sheet".into(),
                spreadsheet_id: "physical_sheet".into(),
                view: "grid".into(),
                sheet_id: 7,
                range: "A1:B2".into(),
            },
            WriteTarget::Document {
                document: "notes".into(),
                document_id: "physical-doc".into(),
                tab_id: "t.0".into(),
            },
        ],
    }
}
fn capture(id: &str, document: bool) -> Preview {
    Preview {
        resource_kind: if document {
            ResourceKind::GoogleDocs
        } else {
            ResourceKind::SheetsGrid
        },
        operation_id: id.into(),
        actor: "owner".into(),
        spreadsheet: if document { "notes" } else { "sheet" }.into(),
        view: if document { "t.0" } else { "grid" }.into(),
        action: if document { "replace_text" } else { "update" }.into(),
        record_id: if document {
            "physical-doc"
        } else {
            "physical_sheet:7"
        }
        .into(),
        fields: if document {
            json!({"find":"old","replacement":"new"})
        } else {
            json!({"range":"A1:B1","values":[["new",false]]})
        },
        before: if document {
            json!({"text":"old content\n"})
        } else {
            json!([[{"stringValue":"private-evidence"},null]])
        },
        after: if document {
            json!({"text":"new content\n"})
        } else {
            json!([[{"stringValue":"new"},{"boolValue":false}]])
        },
        mapping_version: "mapping-1".into(),
        source_revision: "revision-1".into(),
        expires_at: chrono::Utc::now().timestamp() + 300,
        preview_digest: String::new(),
    }
}
fn id(proposal: &Value) -> &str {
    proposal["task_id"].as_str().unwrap()
}
fn hash(proposal: &Value) -> &str {
    proposal["scope_digest"].as_str().unwrap()
}
fn error<T>(result: Result<T, CoreError>, code: &str) {
    assert_eq!(result.err().expect("expected denial").1, code);
}
async fn authorized(ledger: &JournalWriteTasks) -> Value {
    let proposal = ledger.propose(&context(), &scope()).await.unwrap();
    ledger
        .decide(
            id(&proposal),
            &context(),
            hash(&proposal),
            0,
            TaskDecision::Approve,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn one_owner_decision_admits_incremental_grid_and_docs_without_legacy_approval() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let proposal = ledger.propose(&context(), &scope()).await.unwrap();
    error(
        ledger
            .admit(id(&proposal), &context(), capture("before-approval", false))
            .await,
        "write_task_not_active",
    );
    error(
        ledger
            .decide(id(&proposal), &context(), "wrong", 0, TaskDecision::Approve)
            .await,
        "write_task_context",
    );
    let grant = ledger
        .decide(
            id(&proposal),
            &context(),
            hash(&proposal),
            0,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    assert_eq!(grant["grant_revision"], 1);
    assert_eq!(
        ledger
            .decide(
                id(&proposal),
                &context(),
                hash(&proposal),
                0,
                TaskDecision::Approve
            )
            .await
            .unwrap(),
        grant
    );
    error(
        ledger.propose(&context(), &scope()).await,
        "write_task_already_active",
    );
    for (name, document) in [("step-1", false), ("step-2", true)] {
        let operation = ledger
            .admit(id(&proposal), &context(), capture(name, document))
            .await
            .unwrap();
        assert_eq!(operation.state, OperationState::Previewed);
        error(
            journal
                .approve(name, &operation.preview.preview_digest, "approve")
                .await,
            "task_authority_required",
        );
        error(journal.begin_apply(name).await, "task_authority_required");
        assert_eq!(
            ledger
                .claim(id(&proposal), &context(), name)
                .await
                .unwrap()
                .state,
            OperationState::Applying
        );
        error(
            ledger.claim(id(&proposal), &context(), name).await,
            "operation_conflict",
        );
        journal.finish(name, Outcome::Applied).await.unwrap();
    }
    let history = ledger.history(20, 0).await.unwrap();
    assert_eq!(history["tasks"][0]["admitted_operations"], 2);
    assert_eq!(history["tasks"][0]["grant_revision"], 1);
    assert_eq!(
        history["tasks"][0]["operation_states"],
        json!({"applied":2})
    );
    assert!(!history.to_string().contains("private-evidence"));
    assert_eq!(
        ledger
            .end(id(&proposal), &context(), TaskEnd::Completed)
            .await
            .unwrap()["reason"],
        "completed"
    );
    error(
        ledger
            .decide(
                id(&proposal),
                &context(),
                hash(&proposal),
                0,
                TaskDecision::Approve,
            )
            .await,
        "write_task_closed",
    );
}

#[tokio::test]
async fn changed_request_scope_and_context_leave_no_new_operation() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    let p = capture("step", false);
    ledger
        .admit(id(&grant), &context(), p.clone())
        .await
        .unwrap();
    for mutate in ["owner", "mapping", "target", "range"] {
        let mut p = capture("bad", false);
        match mutate {
            "owner" => p.actor = "other".into(),
            "mapping" => p.mapping_version = "changed".into(),
            "target" => p.record_id = "other:7".into(),
            _ => p.fields["range"] = json!("Z1:AA1"),
        }
        error(
            ledger.admit(id(&grant), &context(), p).await,
            "write_task_scope",
        );
        assert!(journal.find("bad").await.unwrap().is_none());
    }
    for mutate in ["agent", "session", "key", "gateway"] {
        let mut ctx = context();
        match mutate {
            "agent" => ctx.agent_id = "other".into(),
            "session" => ctx.session_id = "new".into(),
            "key" => ctx.session_key = "other".into(),
            _ => ctx.gateway_instance = "other".into(),
        }
        error(
            ledger.claim(id(&grant), &ctx, "step").await,
            "write_task_context",
        );
    }
    assert_eq!(
        ledger
            .admit(id(&grant), &context(), p.clone())
            .await
            .unwrap()
            .state,
        OperationState::Previewed
    );
    let mut changed = p;
    changed.source_revision = "different".into();
    error(
        ledger.admit(id(&grant), &context(), changed).await,
        "operation_conflict",
    );
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["admitted_operations"],
        1
    );
}

#[tokio::test]
async fn concurrent_admission_and_claim_are_serialized_and_do_not_reset_limits() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let mut bounds = scope();
    bounds.max_operations = 1;
    let grant = ledger.propose(&context(), &bounds).await.unwrap();
    ledger
        .decide(
            id(&grant),
            &context(),
            hash(&grant),
            0,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    let ctx = context();
    let p = capture("step", false);
    let (first, replay) = tokio::join!(
        ledger.admit(id(&grant), &ctx, p.clone()),
        ledger.admit(id(&grant), &ctx, p)
    );
    assert_eq!(
        first.unwrap().preview.preview_digest,
        replay.unwrap().preview.preview_digest
    );
    let (first, second) = tokio::join!(
        ledger.claim(id(&grant), &ctx, "step"),
        ledger.claim(id(&grant), &ctx, "step")
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let failure = if first.is_err() { first } else { second };
    error(failure, "operation_conflict");
    journal.finish("step", Outcome::Applied).await.unwrap();
    error(
        ledger.admit(id(&grant), &ctx, capture("extra", true)).await,
        "write_task_limit",
    );
    assert!(journal.find("extra").await.unwrap().is_none());
}

#[tokio::test]
async fn two_different_previews_cannot_race_past_the_same_predecessor() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    let ctx = context();
    let (a, b) = tokio::join!(
        ledger.admit(id(&grant), &ctx, capture("a", false)),
        ledger.admit(id(&grant), &ctx, capture("b", true))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    error(if a.is_err() { a } else { b }, "write_task_predecessor");
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["admitted_operations"],
        1
    );
}

#[tokio::test]
async fn cancellation_and_reset_revoke_claims_and_old_callbacks_without_erasing_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("pending", false))
        .await
        .unwrap();
    let mut next = context();
    next.session_id = "conversation-2".into();
    error(
        ledger.propose(&next, &scope()).await,
        "write_task_already_active",
    );
    ledger.invalidate_context(&context()).await.unwrap();
    assert_eq!(
        journal.get("pending").await.unwrap().state,
        OperationState::Rejected
    );
    error(
        ledger.claim(id(&grant), &context(), "pending").await,
        "write_task_not_active",
    );
    error(
        ledger
            .decide(
                id(&grant),
                &context(),
                hash(&grant),
                0,
                TaskDecision::Approve,
            )
            .await,
        "write_task_closed",
    );
    let fresh = ledger.propose(&next, &scope()).await.unwrap();
    assert_ne!(id(&fresh), id(&grant));
    ledger
        .decide(id(&fresh), &next, hash(&fresh), 0, TaskDecision::Approve)
        .await
        .unwrap();
    ledger.invalidate_context(&context()).await.unwrap();
    assert_eq!(
        ledger.history(20, 0).await.unwrap()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["task_id"] == id(&fresh))
            .unwrap()["phase"],
        "active"
    );
    ledger
        .end(id(&fresh), &next, TaskEnd::Cancelled)
        .await
        .unwrap();
    assert_eq!(
        journal.get("pending").await.unwrap().preview.before,
        capture("unused", false).before
    );
}

#[tokio::test]
async fn uncertain_outcome_requires_observation_and_fresh_confirmation_not_old_callback_replay() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("lost", true))
        .await
        .unwrap();
    ledger.claim(id(&grant), &context(), "lost").await.unwrap();
    error(
        ledger.end(id(&grant), &context(), TaskEnd::Completed).await,
        "write_task_recovery_required",
    );
    journal.finish("lost", Outcome::Uncertain).await.unwrap();
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["phase"],
        "suspended"
    );
    error(
        ledger
            .decide(
                id(&grant),
                &context(),
                hash(&grant),
                0,
                TaskDecision::Approve,
            )
            .await,
        "write_task_stale_decision",
    );
    error(
        ledger
            .decide(
                id(&grant),
                &context(),
                hash(&grant),
                2,
                TaskDecision::Approve,
            )
            .await,
        "write_task_recovery_required",
    );
    journal.finish("lost", Outcome::Applied).await.unwrap();
    let resumed = ledger
        .decide(
            id(&grant),
            &context(),
            hash(&grant),
            2,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    assert_eq!(resumed["grant_revision"], 3);
    error(
        ledger.claim(id(&grant), &context(), "lost").await,
        "write_task_stale_member",
    );
    ledger
        .admit(id(&grant), &context(), capture("next", false))
        .await
        .unwrap();
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["admitted_operations"],
        2
    );
}

#[tokio::test]
async fn a_new_task_id_cannot_bypass_an_uncertain_cancelled_write() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("lost", false))
        .await
        .unwrap();
    ledger.claim(id(&grant), &context(), "lost").await.unwrap();
    ledger
        .end(id(&grant), &context(), TaskEnd::Cancelled)
        .await
        .unwrap();
    journal.finish("lost", Outcome::Uncertain).await.unwrap();
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["phase"],
        "closed"
    );
    let fresh = authorized(&ledger).await;
    error(
        ledger
            .admit(id(&fresh), &context(), capture("bypass", false))
            .await,
        "write_task_source_unresolved",
    );
    assert!(journal.find("bypass").await.unwrap().is_none());
    journal.finish("lost", Outcome::Applied).await.unwrap();
    ledger
        .admit(id(&fresh), &context(), capture("after-observation", false))
        .await
        .unwrap();
}

#[tokio::test]
async fn restart_suspends_authority_and_recovers_applying_without_changing_previews() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    let before = ledger
        .admit(id(&grant), &context(), capture("interrupted", false))
        .await
        .unwrap()
        .preview;
    ledger
        .claim(id(&grant), &context(), "interrupted")
        .await
        .unwrap();
    drop(ledger);
    drop(journal);
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    assert_eq!(
        journal.get("interrupted").await.unwrap().state,
        OperationState::Reconciliation
    );
    assert_eq!(
        serde_json::to_value(journal.get("interrupted").await.unwrap().preview).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    let mut restarted = context();
    restarted.gateway_instance = "gateway-2".into();
    error(
        ledger.claim(id(&grant), &restarted, "interrupted").await,
        "write_task_not_active",
    );
    error(
        ledger
            .decide(
                id(&grant),
                &restarted,
                hash(&grant),
                2,
                TaskDecision::Approve,
            )
            .await,
        "write_task_recovery_required",
    );
    journal
        .finish("interrupted", Outcome::Applied)
        .await
        .unwrap();
    ledger
        .decide(
            id(&grant),
            &restarted,
            hash(&grant),
            2,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    error(
        ledger
            .admit(id(&grant), &context(), capture("old-host", false))
            .await,
        "write_task_context",
    );
    ledger
        .admit(id(&grant), &restarted, capture("new-host", false))
        .await
        .unwrap();
}

#[tokio::test]
async fn restart_discards_unsent_old_previews_before_accepting_fresh_capture() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("old-preview", false))
        .await
        .unwrap();
    drop(ledger);
    drop(journal);
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    ledger
        .decide(
            id(&grant),
            &context(),
            hash(&grant),
            2,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    assert_eq!(
        journal.get("old-preview").await.unwrap().state,
        OperationState::Rejected
    );
    ledger
        .admit(id(&grant), &context(), capture("fresh-preview", false))
        .await
        .unwrap();
    error(
        ledger.claim(id(&grant), &context(), "old-preview").await,
        "write_task_stale_member",
    );
}

#[tokio::test]
async fn read_only_state_and_owner_or_deployment_changes_cannot_acquire_task_authority() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open_read_only_bound(directory.path(), "owner", "deployment-1")
        .await
        .unwrap();
    error(
        JournalWriteTasks::open(journal, "owner", "deployment-1").await,
        "readonly_profile",
    );
    for (owner, generation) in [("other", "deployment-1"), ("owner", "deployment-2")] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal, "owner", "deployment-1")
            .await
            .unwrap();
        authorized(&ledger).await;
        drop(ledger);
        let journal = Journal::open(directory.path()).await.unwrap();
        error(
            JournalWriteTasks::open(journal, owner, generation).await,
            "write_task_binding",
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal, "owner", "deployment-1")
        .await
        .unwrap();
    drop(ledger);
    error(
        Journal::open_read_only_bound(directory.path(), "owner", "deployment-1").await,
        "readonly_legacy_state_unbound",
    );
}

#[tokio::test]
async fn legacy_rows_remain_compatible_but_cannot_be_adopted_into_a_task() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let legacy = journal.create(capture("legacy", false)).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    error(
        ledger.admit(id(&grant), &context(), legacy.clone()).await,
        "operation_conflict",
    );
    error(
        ledger.claim(id(&grant), &context(), "legacy").await,
        "write_task_membership",
    );
    journal
        .approve("legacy", &legacy.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply("legacy").await.unwrap();
    error(
        ledger
            .admit(id(&grant), &context(), capture("new", false))
            .await,
        "write_task_source_unresolved",
    );
    journal
        .finish("legacy", Outcome::KnownFailure)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(journal.get("legacy").await.unwrap().preview).unwrap(),
        serde_json::to_value(legacy).unwrap()
    );
    ledger
        .admit(id(&grant), &context(), capture("new", false))
        .await
        .unwrap();
}

#[tokio::test]
async fn malformed_scope_or_inverse_evidence_does_not_create_authority_or_intent() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let mut invalid = scope();
    invalid.max_operations = 0;
    error(
        ledger.propose(&context(), &invalid).await,
        "write_task_scope",
    );
    invalid = scope();
    invalid.targets.push(invalid.targets[0].clone());
    error(
        ledger.propose(&context(), &invalid).await,
        "write_task_scope",
    );
    let grant = authorized(&ledger).await;
    let mut p = capture("bad", false);
    p.before[0][0] = json!({"formulaValue":"=1"});
    error(
        ledger.admit(id(&grant), &context(), p).await,
        "undo_unavailable",
    );
    assert!(journal.find("bad").await.unwrap().is_none());
    let mut expired = capture("expired", false);
    expired.expires_at = 1;
    error(
        ledger.admit(id(&grant), &context(), expired).await,
        "write_task_capture_expired",
    );
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["admitted_operations"],
        0
    );
    error(ledger.history(21, 0).await, "invalid_input");
}

#[tokio::test]
async fn expiry_discards_only_the_capture_and_does_not_require_another_task_approval() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    let mut p = capture("short-lived-capture", false);
    p.expires_at = chrono::Utc::now().timestamp() + 1;
    ledger.admit(id(&grant), &context(), p).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    error(
        ledger
            .claim(id(&grant), &context(), "short-lived-capture")
            .await,
        "write_task_capture_expired",
    );
    assert_eq!(
        journal.get("short-lived-capture").await.unwrap().state,
        OperationState::Expired
    );
    ledger
        .admit(id(&grant), &context(), capture("fresh", false))
        .await
        .unwrap();
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["grant_revision"],
        1
    );
}

#[tokio::test]
async fn a_definite_failure_stops_execution_until_explicit_fresh_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("failed", false))
        .await
        .unwrap();
    ledger
        .claim(id(&grant), &context(), "failed")
        .await
        .unwrap();
    journal
        .finish("failed", Outcome::KnownFailure)
        .await
        .unwrap();
    error(
        ledger
            .admit(
                id(&grant),
                &context(),
                capture("before-confirmation", false),
            )
            .await,
        "write_task_not_active",
    );
    error(
        ledger
            .decide(
                id(&grant),
                &context(),
                hash(&grant),
                0,
                TaskDecision::Approve,
            )
            .await,
        "write_task_stale_decision",
    );
    ledger
        .decide(
            id(&grant),
            &context(),
            hash(&grant),
            2,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    error(
        ledger.claim(id(&grant), &context(), "failed").await,
        "write_task_stale_member",
    );
    ledger
        .admit(id(&grant), &context(), capture("new-attempt", false))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_legacy_uncertain_write_started_after_admission_fences_claim() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    ledger
        .admit(id(&grant), &context(), capture("pending", false))
        .await
        .unwrap();
    let legacy = journal.create(capture("legacy", false)).await.unwrap();
    journal
        .approve("legacy", &legacy.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply("legacy").await.unwrap();
    error(
        ledger.claim(id(&grant), &context(), "pending").await,
        "write_task_source_unresolved",
    );
    assert_eq!(
        journal.get("pending").await.unwrap().state,
        OperationState::Previewed
    );
    journal
        .finish("legacy", Outcome::KnownFailure)
        .await
        .unwrap();
    ledger
        .claim(id(&grant), &context(), "pending")
        .await
        .unwrap();
}

#[tokio::test]
async fn same_scope_new_task_has_a_distinct_approval_digest_and_history_is_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal, "owner", "deployment-1")
        .await
        .unwrap();
    let first = authorized(&ledger).await;
    ledger
        .end(id(&first), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let second = ledger.propose(&context(), &scope()).await.unwrap();
    assert_ne!(hash(&first), hash(&second));
    error(
        ledger
            .decide(
                id(&second),
                &context(),
                hash(&first),
                0,
                TaskDecision::Approve,
            )
            .await,
        "write_task_context",
    );
    let page = ledger.history(1, 0).await.unwrap();
    assert_eq!(page["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(page["next_offset"], 1);
    assert!(ledger.history(1, 1).await.unwrap()["next_offset"].is_null());
}

#[tokio::test]
async fn revising_an_unsent_approach_keeps_task_authority_but_never_reuses_spent_intent() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let grant = authorized(&ledger).await;
    let before = ledger
        .admit(id(&grant), &context(), capture("discarded", false))
        .await
        .unwrap()
        .preview;
    for _ in 0..2 {
        assert_eq!(
            ledger
                .discard(id(&grant), &context(), "discarded")
                .await
                .unwrap()
                .state,
            OperationState::Rejected
        );
    }
    assert_eq!(
        serde_json::to_value(journal.get("discarded").await.unwrap().preview).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    error(
        ledger.claim(id(&grant), &context(), "discarded").await,
        "operation_conflict",
    );
    let mut replacement = capture("discarded", false);
    replacement.source_revision = "fresh-source".into();
    error(
        ledger.admit(id(&grant), &context(), replacement).await,
        "operation_conflict",
    );
    ledger
        .admit(id(&grant), &context(), capture("new-approach", true))
        .await
        .unwrap();
    ledger
        .claim(id(&grant), &context(), "new-approach")
        .await
        .unwrap();
    error(
        ledger.discard(id(&grant), &context(), "new-approach").await,
        "operation_conflict",
    );
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["grant_revision"],
        1
    );
    assert_eq!(
        ledger.history(1, 0).await.unwrap()["tasks"][0]["admitted_operations"],
        2
    );
}

#[tokio::test]
async fn duplicate_attachment_and_foreign_legacy_owner_cannot_create_authority() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    error(
        JournalWriteTasks::open(journal.clone(), "owner", "deployment-1").await,
        "write_tasks_already_attached",
    );
    assert!(ledger.history(1, 0).await.unwrap()["tasks"]
        .as_array()
        .unwrap()
        .is_empty());
    let other = tempfile::tempdir().unwrap();
    let journal = Journal::open(other.path()).await.unwrap();
    let mut p = capture("legacy-other-owner", false);
    p.actor = "previous-owner".into();
    let p = journal.create(p).await.unwrap();
    error(
        JournalWriteTasks::open(journal.clone(), "owner", "deployment-1").await,
        "write_task_legacy_owner",
    );
    assert_eq!(
        journal
            .get("legacy-other-owner")
            .await
            .unwrap()
            .preview
            .preview_digest,
        p.preview_digest
    );
}

fn task_record<'a>(page: &'a Value, task: &str) -> &'a Value {
    page["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["task_id"] == task)
        .unwrap()
}

fn inverse_capture(original: &secretary_core::domain::operation::Operation, id: &str) -> Preview {
    use secretary_core::domain::compensation::{Inverse, InverseChange};
    let inverse = Inverse::from_applied(original, id, "owner").unwrap();
    let mut p = original.preview.clone();
    p.operation_id = id.into();
    p.fields = match inverse.change() {
        InverseChange::Grid(c) => c.fields.clone(),
        InverseChange::Document(c) => c.fields.clone(),
    };
    p.before = original.preview.after.clone();
    p.after = original.preview.before.clone();
    p.source_revision = "fresh-inverse-source".into();
    p.expires_at = chrono::Utc::now().timestamp() + 300;
    p.preview_digest.clear();
    p
}
async fn applied_member(
    ledger: &JournalWriteTasks,
    journal: &Journal,
    task: &str,
    name: &str,
    document: bool,
) -> secretary_core::domain::operation::Operation {
    ledger
        .admit(task, &context(), capture(name, document))
        .await
        .unwrap();
    ledger.claim(task, &context(), name).await.unwrap();
    journal.finish(name, Outcome::Applied).await.unwrap()
}

#[tokio::test]
async fn inverse_links_are_atomic_replay_bound_and_reverse_the_closed_task_order() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let first = applied_member(&ledger, &journal, id(&forward), "grid-original", false).await;
    let second = applied_member(&ledger, &journal, id(&forward), "doc-original", true).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    error(
        ledger
            .admit_inverse(
                id(&undo),
                &context(),
                "grid-original",
                inverse_capture(&first, "too-early"),
            )
            .await,
        "undo_reverse_order",
    );
    assert!(journal.find("too-early").await.unwrap().is_none());
    let p = inverse_capture(&second, "doc-inverse");
    let stored = ledger
        .admit_inverse(id(&undo), &context(), "doc-original", p.clone())
        .await
        .unwrap();
    assert_eq!(stored.state, OperationState::Previewed);
    assert_eq!(
        ledger
            .admit_inverse(id(&undo), &context(), "doc-original", p.clone())
            .await
            .unwrap()
            .preview
            .preview_digest,
        stored.preview.preview_digest
    );
    error(
        ledger.admit(id(&undo), &context(), p).await,
        "operation_conflict",
    );
    ledger
        .claim(id(&undo), &context(), "doc-inverse")
        .await
        .unwrap();
    journal
        .finish("doc-inverse", Outcome::Applied)
        .await
        .unwrap();
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "grid-original",
            inverse_capture(&first, "grid-inverse"),
        )
        .await
        .unwrap();
    ledger
        .claim(id(&undo), &context(), "grid-inverse")
        .await
        .unwrap();
    journal
        .finish("grid-inverse", Outcome::Applied)
        .await
        .unwrap();
    let history = ledger
        .inverse_history("grid-original", 20, 0)
        .await
        .unwrap();
    assert_eq!(history["original_state"], "applied");
    assert_eq!(history["attempts"][0]["state"], "applied");
    assert_eq!(
        history["attempts"][0]["original_digest"],
        first.preview.preview_digest
    );
    assert!(!history.to_string().contains("private-evidence"));
    assert_eq!(
        journal
            .get("grid-original")
            .await
            .unwrap()
            .preview
            .preview_digest,
        first.preview.preview_digest
    );
    assert_eq!(
        task_record(&ledger.history(20, 0).await.unwrap(), id(&undo))["admitted_operations"],
        2
    );
}

#[tokio::test]
async fn inverse_requires_applied_original_closed_task_and_exact_retained_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    ledger
        .admit(id(&forward), &context(), capture("original", false))
        .await
        .unwrap();
    let mut projected = journal.get("original").await.unwrap();
    projected.state = OperationState::Applied;
    let candidate = inverse_capture(&projected, "inverse");
    error(
        ledger
            .admit_inverse(id(&forward), &context(), "original", candidate.clone())
            .await,
        "undo_unavailable",
    );
    ledger
        .claim(id(&forward), &context(), "original")
        .await
        .unwrap();
    journal.finish("original", Outcome::Applied).await.unwrap();
    error(
        ledger
            .admit_inverse(id(&forward), &context(), "original", candidate.clone())
            .await,
        "undo_original_task_active",
    );
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    let mut altered = candidate.clone();
    altered.before = json!([[{"stringValue":"different"},{"boolValue":false}]]);
    error(
        ledger
            .admit_inverse(id(&undo), &context(), "original", altered)
            .await,
        "undo_conflict",
    );
    error(
        ledger
            .admit_inverse(id(&undo), &context(), "missing", candidate.clone())
            .await,
        "operation_not_found",
    );
    error(
        ledger
            .admit_inverse(id(&undo), &context(), "../invalid", candidate)
            .await,
        "invalid_input",
    );
    assert!(journal.find("inverse").await.unwrap().is_none());
    assert_eq!(
        ledger.inverse_history("original", 20, 0).await.unwrap()["attempts"],
        json!([])
    );
    for (limit, offset) in [(0, 0), (21, 0), (1, 100001)] {
        error(
            ledger.inverse_history("original", limit, offset).await,
            "invalid_input",
        );
    }
    error(
        ledger.inverse_history("missing", 20, 0).await,
        "operation_not_found",
    );
}

#[tokio::test]
async fn concurrent_inverse_selection_cannot_attach_two_attempts_to_one_original() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let first = authorized(&ledger).await;
    let mut other = context();
    other.session_key = "agent:other-conversation".into();
    let second = ledger.propose(&other, &scope()).await.unwrap();
    ledger
        .decide(id(&second), &other, hash(&second), 0, TaskDecision::Approve)
        .await
        .unwrap();
    let current = context();
    let (a, b) = tokio::join!(
        ledger.admit_inverse(
            id(&first),
            &current,
            "original",
            inverse_capture(&original, "inverse-a")
        ),
        ledger.admit_inverse(
            id(&second),
            &other,
            "original",
            inverse_capture(&original, "inverse-b")
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
    error(if a.is_err() { a } else { b }, "undo_attempt_blocked");
    let history = ledger.inverse_history("original", 20, 0).await.unwrap();
    assert_eq!(history["attempts"].as_array().unwrap().len(), 1);
    assert_ne!(
        journal.find("inverse-a").await.unwrap().is_some(),
        journal.find("inverse-b").await.unwrap().is_some()
    );
}

#[tokio::test]
async fn inverse_link_failure_rolls_back_capture_quota_and_audit_together() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(directory.path().join("secretary.sqlite3")),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER fail_inverse BEFORE INSERT ON operation_compensations BEGIN SELECT RAISE(ABORT,'injected link failure'); END").execute(&pool).await.unwrap();
    let candidate = inverse_capture(&original, "inverse");
    assert!(ledger
        .admit_inverse(id(&undo), &context(), "original", candidate.clone())
        .await
        .is_err());
    assert!(journal.find("inverse").await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE operation_id='inverse'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        task_record(&ledger.history(20, 0).await.unwrap(), id(&undo))["admitted_operations"],
        0
    );
    sqlx::query("DROP TRIGGER fail_inverse")
        .execute(&pool)
        .await
        .unwrap();
    ledger
        .admit_inverse(id(&undo), &context(), "original", candidate)
        .await
        .unwrap();
    assert_eq!(
        ledger.inverse_history("original", 20, 0).await.unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn failed_inverse_retains_link_and_allows_fresh_authority_but_uncertainty_blocks_new_ids() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "original",
            inverse_capture(&original, "failed-inverse"),
        )
        .await
        .unwrap();
    ledger
        .claim(id(&undo), &context(), "failed-inverse")
        .await
        .unwrap();
    journal
        .finish("failed-inverse", Outcome::KnownFailure)
        .await
        .unwrap();
    ledger
        .end(id(&undo), &context(), TaskEnd::Cancelled)
        .await
        .unwrap();
    let retry = authorized(&ledger).await;
    ledger
        .admit_inverse(
            id(&retry),
            &context(),
            "original",
            inverse_capture(&original, "uncertain-inverse"),
        )
        .await
        .unwrap();
    ledger
        .claim(id(&retry), &context(), "uncertain-inverse")
        .await
        .unwrap();
    journal
        .finish("uncertain-inverse", Outcome::Uncertain)
        .await
        .unwrap();
    ledger
        .end(id(&retry), &context(), TaskEnd::Cancelled)
        .await
        .unwrap();
    let third = authorized(&ledger).await;
    error(
        ledger
            .admit_inverse(
                id(&third),
                &context(),
                "original",
                inverse_capture(&original, "bypass"),
            )
            .await,
        "undo_attempt_blocked",
    );
    let history = ledger.inverse_history("original", 1, 0).await.unwrap();
    assert_eq!(history["next_offset"], 1);
    assert_eq!(
        ledger.inverse_history("original", 1, 1).await.unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(journal.find("bypass").await.unwrap().is_none());
}

#[tokio::test]
async fn applied_inverse_is_not_repeated_and_redo_is_not_an_implicit_new_undo() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "original",
            inverse_capture(&original, "inverse"),
        )
        .await
        .unwrap();
    ledger
        .claim(id(&undo), &context(), "inverse")
        .await
        .unwrap();
    let reversed = journal.finish("inverse", Outcome::Applied).await.unwrap();
    ledger
        .end(id(&undo), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let again = authorized(&ledger).await;
    error(
        ledger
            .admit_inverse(
                id(&again),
                &context(),
                "original",
                inverse_capture(&original, "duplicate"),
            )
            .await,
        "undo_attempt_blocked",
    );
    error(
        ledger
            .admit_inverse(
                id(&again),
                &context(),
                "inverse",
                inverse_capture(&reversed, "redo"),
            )
            .await,
        "undo_inverse_not_supported",
    );
    assert!(journal.find("duplicate").await.unwrap().is_none());
    assert!(journal.find("redo").await.unwrap().is_none());
}

#[tokio::test]
async fn inverse_reference_survives_restart_without_restoring_old_execution_authority() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "original",
            inverse_capture(&original, "inverse"),
        )
        .await
        .unwrap();
    drop(ledger);
    drop(journal);
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let history = ledger.inverse_history("original", 20, 0).await.unwrap();
    assert_eq!(history["attempts"][0]["state"], "previewed");
    error(
        ledger.claim(id(&undo), &context(), "inverse").await,
        "write_task_not_active",
    );
    let page = ledger.history(20, 0).await.unwrap();
    let current = task_record(&page, id(&undo));
    ledger
        .decide(
            id(&undo),
            &context(),
            hash(&undo),
            current["grant_revision"].as_u64().unwrap(),
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    assert_eq!(
        ledger.inverse_history("original", 20, 0).await.unwrap()["attempts"][0]["state"],
        "rejected"
    );
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "original",
            inverse_capture(&original, "fresh-inverse"),
        )
        .await
        .unwrap();
    assert_eq!(
        ledger.inverse_history("original", 20, 0).await.unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        journal.get("original").await.unwrap().state,
        OperationState::Applied
    );
}

#[tokio::test]
async fn inverse_claim_rechecks_retained_digest_link_and_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    let original = applied_member(&ledger, &journal, id(&forward), "original", false).await;
    ledger
        .end(id(&forward), &context(), TaskEnd::Completed)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    ledger
        .admit_inverse(
            id(&undo),
            &context(),
            "original",
            inverse_capture(&original, "inverse"),
        )
        .await
        .unwrap();
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(directory.path().join("secretary.sqlite3")),
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE operation_compensations SET original_digest='altered' WHERE inverse_id='inverse'",
    )
    .execute(&pool)
    .await
    .unwrap();
    error(
        ledger.claim(id(&undo), &context(), "inverse").await,
        "journal_unavailable",
    );
    assert_eq!(
        journal.get("inverse").await.unwrap().state,
        OperationState::Previewed
    );
    sqlx::query("UPDATE operation_compensations SET original_digest=? WHERE inverse_id='inverse'")
        .bind(&original.preview.preview_digest)
        .execute(&pool)
        .await
        .unwrap();
    ledger
        .end(id(&undo), &context(), TaskEnd::Cancelled)
        .await
        .unwrap();
    error(
        ledger.claim(id(&undo), &context(), "inverse").await,
        "write_task_not_active",
    );
    assert_eq!(
        ledger.inverse_history("original", 20, 0).await.unwrap()["attempts"][0]["state"],
        "rejected"
    );
}

#[tokio::test]
async fn supported_legacy_original_keeps_its_identity_and_requires_fresh_task_authority_for_inverse(
) {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let legacy = journal
        .create(capture("legacy-original", false))
        .await
        .unwrap();
    journal
        .approve("legacy-original", &legacy.preview_digest, "approve")
        .await
        .unwrap();
    journal.begin_apply("legacy-original").await.unwrap();
    let original = journal
        .finish("legacy-original", Outcome::Applied)
        .await
        .unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let proposal = ledger.propose(&context(), &scope()).await.unwrap();
    error(
        ledger
            .admit_inverse(
                id(&proposal),
                &context(),
                "legacy-original",
                inverse_capture(&original, "inverse"),
            )
            .await,
        "write_task_not_active",
    );
    let grant = ledger
        .decide(
            id(&proposal),
            &context(),
            hash(&proposal),
            0,
            TaskDecision::Approve,
        )
        .await
        .unwrap();
    ledger
        .admit_inverse(
            id(&grant),
            &context(),
            "legacy-original",
            inverse_capture(&original, "inverse"),
        )
        .await
        .unwrap();
    ledger
        .claim(id(&grant), &context(), "inverse")
        .await
        .unwrap();
    assert_eq!(
        journal
            .get("legacy-original")
            .await
            .unwrap()
            .preview
            .preview_digest,
        legacy.preview_digest
    );
    assert_eq!(
        ledger
            .inverse_history("legacy-original", 20, 0)
            .await
            .unwrap()["attempts"][0]["state"],
        "applying"
    );
}

#[test]
fn omitted_undo_selection_preserves_scope_serialization_and_invalid_selectors_fail_closed() {
    let legacy = serde_json::to_value(scope()).unwrap();
    assert!(legacy.get("undo_of").is_none());
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<TaskScope>(legacy.clone()).unwrap()).unwrap(),
        legacy
    );
    for selection in [
        json!({"kind":"task","task_id":"bad"}),
        json!({"kind":"operation","operation_id":"../bad"}),
    ] {
        let mut selected = legacy.clone();
        selected["undo_of"] = selection;
        error(
            serde_json::from_value::<TaskScope>(selected)
                .unwrap()
                .validate(),
            "write_task_scope",
        );
    }
    let mut extra = legacy;
    extra["undo_of"] = json!({"kind":"operation","operation_id":"original","approved":true});
    assert!(serde_json::from_value::<TaskScope>(extra).is_err());
}

#[tokio::test]
async fn unresolved_later_original_on_another_source_blocks_earlier_undo() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path()).await.unwrap();
    let ledger = JournalWriteTasks::open(journal.clone(), "owner", "deployment-1")
        .await
        .unwrap();
    let forward = authorized(&ledger).await;
    ledger
        .admit(id(&forward), &context(), capture("first", false))
        .await
        .unwrap();
    ledger
        .claim(id(&forward), &context(), "first")
        .await
        .unwrap();
    journal.finish("first", Outcome::Applied).await.unwrap();
    let first = journal.get("first").await.unwrap();
    ledger
        .admit(id(&forward), &context(), capture("later", true))
        .await
        .unwrap();
    ledger
        .claim(id(&forward), &context(), "later")
        .await
        .unwrap();
    journal.finish("later", Outcome::Uncertain).await.unwrap();
    ledger
        .end(id(&forward), &context(), TaskEnd::Cancelled)
        .await
        .unwrap();
    let undo = authorized(&ledger).await;
    error(
        ledger
            .admit_inverse(
                id(&undo),
                &context(),
                "first",
                inverse_capture(&first, "inverse-first"),
            )
            .await,
        "undo_reverse_order",
    );
    assert!(journal.find("inverse-first").await.unwrap().is_none());
}
