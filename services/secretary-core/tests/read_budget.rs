use secretary_core::{
    application::read_budget::ReadTaskLedger,
    domain::read_budget::{ReadBudgetPolicy, ReadLimits, ReadUsage},
    journal::{read_budget::JournalReadTasks, Journal},
};

fn policy() -> ReadBudgetPolicy {
    serde_json::from_str(include_str!(
        "../../../config/read-budget-policy.example.json"
    ))
    .unwrap()
}
fn limits() -> ReadLimits {
    ReadLimits {
        calls: 4,
        rows: 100,
        document_characters: 20000,
        output_bytes: 32768,
        in_flight: 2,
    }
}

#[tokio::test]
async fn reservations_are_cumulative_atomic_and_bound_to_one_exact_tool() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = JournalReadTasks::open(
        Journal::open(directory.path()).await.unwrap(),
        "owner",
        "generation",
        policy(),
    )
    .await
    .unwrap();
    assert!(ledger.reserve("sheets_list").await.unwrap().grant.is_none());
    ledger.create(limits()).await.unwrap();
    assert!(ledger.create(limits()).await.is_err());
    let grants =
        futures_util::future::join_all((0..8).map(|_| ledger.reserve("sheets_list"))).await;
    let grants: Vec<_> = grants
        .into_iter()
        .map(Result::unwrap)
        .filter_map(|decision| decision.grant)
        .collect();
    // Reservations compete for the same cumulative bytes, not independent copies.
    assert_eq!(grants.len(), 1);
    assert!(ledger.close().await.is_err());
    assert!(ledger.claim(&grants[0].id, "docs_list").await.is_err());
    let grant = ledger.claim(&grants[0].id, "sheets_list").await.unwrap();
    assert!(ledger.claim(&grant.id, "sheets_list").await.is_err());
    ledger
        .finish(
            &grant.id,
            ReadUsage {
                output_bytes: 120,
                ..ReadUsage::default()
            },
        )
        .await
        .unwrap();
    assert!(ledger
        .finish(&grant.id, ReadUsage::default())
        .await
        .is_err());
    let status = ledger.status().await.unwrap();
    assert_eq!(status["charged"]["calls"], 1);
    assert_eq!(status["charged"]["output_bytes"], 512);
    assert_eq!(status["in_flight"], 0);
    ledger.close().await.unwrap();
    ledger.create(limits()).await.unwrap(); // Only the operator lifecycle opens a new task.
}

#[tokio::test]
async fn lost_reservations_survive_restart_and_policy_changes_cannot_reset_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let ledger = JournalReadTasks::open(
        Journal::open(&path).await.unwrap(),
        "owner",
        "generation",
        policy(),
    )
    .await
    .unwrap();
    ledger.create(limits()).await.unwrap();
    let reservation = ledger.reserve("docs_read").await.unwrap().grant.unwrap();
    ledger.claim(&reservation.id, "docs_read").await.unwrap();
    drop(ledger);
    let ledger = JournalReadTasks::open(
        Journal::open(&path).await.unwrap(),
        "owner",
        "generation",
        policy(),
    )
    .await
    .unwrap();
    let status = ledger.status().await.unwrap();
    assert_eq!(status["charged"]["calls"], 1);
    assert_eq!(status["charged"]["document_characters"], 20000);
    assert_eq!(
        status["charged"]["output_bytes"],
        256 + reservation.output_bytes
    );
    assert_eq!(status["in_flight"], 0);
    let stop = ledger.reserve("docs_read").await.unwrap();
    assert!(stop.grant.is_none());
    assert!(!stop.model_text.is_empty());
    assert!(stop.model_text.len() <= 256);
    assert!(ledger
        .reserve("docs_read")
        .await
        .unwrap()
        .model_text
        .is_empty());
    assert!(ledger.claim(&reservation.id, "docs_read").await.is_err());
    let mut increased = limits();
    increased.output_bytes = 65536;
    increased.document_characters = 40000;
    ledger.increase(increased).await.unwrap();
    assert_eq!(ledger.status().await.unwrap()["charged"]["calls"], 1);
    assert!(ledger.reserve("docs_read").await.unwrap().grant.is_some());
    drop(ledger);
    let mut changed = policy();
    changed.revision = "unannounced-change".into();
    assert_eq!(
        JournalReadTasks::open(
            Journal::open(&path).await.unwrap(),
            "owner",
            "generation",
            changed
        )
        .await
        .err()
        .unwrap()
        .1,
        "read_budget_policy_changed"
    );
}

#[tokio::test]
async fn concurrent_calls_reserve_finite_capacity_and_cannot_settle_overruns() {
    let directory = tempfile::tempdir().unwrap();
    let mut policy = policy();
    policy.request_output_bytes = 4096;
    let ledger = JournalReadTasks::open(
        Journal::open(directory.path()).await.unwrap(),
        "owner",
        "generation",
        policy,
    )
    .await
    .unwrap();
    ledger.create(limits()).await.unwrap();
    let replies =
        futures_util::future::join_all((0..8).map(|_| ledger.reserve("sheets_list"))).await;
    let grants: Vec<_> = replies
        .into_iter()
        .map(Result::unwrap)
        .filter_map(|reply| reply.grant)
        .collect();
    assert_eq!(grants.len(), 2);
    assert_eq!(ledger.status().await.unwrap()["charged"]["calls"], 2);
    let grant = ledger.claim(&grants[0].id, "sheets_list").await.unwrap();
    assert!(ledger
        .finish(
            &grant.id,
            ReadUsage {
                output_bytes: grant.output_bytes + 1,
                ..ReadUsage::default()
            }
        )
        .await
        .is_err());
    assert_eq!(ledger.status().await.unwrap()["in_flight"], 2);
    let mut reduced = limits();
    reduced.calls = 3;
    assert!(ledger.increase(reduced).await.is_err());
}

#[tokio::test]
async fn repeated_operator_increases_preserve_usage_and_cannot_rearm_free_notices() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = JournalReadTasks::open(
        Journal::open(directory.path()).await.unwrap(),
        "owner",
        "generation",
        policy(),
    )
    .await
    .unwrap();
    let mut task = limits();
    task.calls = 1;
    task.in_flight = 1;
    ledger.create(task.clone()).await.unwrap();
    let grant = ledger.reserve("sheets_list").await.unwrap().grant.unwrap();
    ledger.claim(&grant.id, "sheets_list").await.unwrap();
    ledger
        .finish(&grant.id, ReadUsage::default())
        .await
        .unwrap();
    assert!(!ledger
        .reserve("sheets_list")
        .await
        .unwrap()
        .model_text
        .is_empty());
    ledger.increase(task.clone()).await.unwrap();
    assert!(ledger
        .reserve("sheets_list")
        .await
        .unwrap()
        .model_text
        .is_empty());
    task.calls = 2;
    assert_eq!(
        ledger.increase(task.clone()).await.unwrap()["charged"]["output_bytes"],
        768
    );
    assert_eq!(
        ledger.increase(task).await.unwrap()["charged"]["output_bytes"],
        768
    );
    let grant = ledger.reserve("sheets_list").await.unwrap().grant.unwrap();
    ledger.claim(&grant.id, "sheets_list").await.unwrap();
    ledger
        .finish(&grant.id, ReadUsage::default())
        .await
        .unwrap();
    assert!(!ledger
        .reserve("sheets_list")
        .await
        .unwrap()
        .model_text
        .is_empty());
    assert!(ledger
        .reserve("sheets_list")
        .await
        .unwrap()
        .model_text
        .is_empty());
    assert_eq!(ledger.status().await.unwrap()["charged"]["calls"], 2);
}

#[tokio::test]
async fn readonly_journal_binding_denies_legacy_recovery_and_owner_changes() {
    use sqlx::{Connection, Row};
    let directory = tempfile::tempdir().unwrap();
    let fresh = directory.path().join("fresh");
    let journal = Journal::open_read_only_bound(&fresh, "owner", "generation")
        .await
        .unwrap();
    drop(journal);
    let journal = Journal::open_read_only_bound(&fresh, "owner", "generation")
        .await
        .unwrap();
    drop(journal);
    assert_eq!(
        Journal::open_read_only_bound(&fresh, "other", "generation")
            .await
            .err()
            .unwrap()
            .1,
        "readonly_state_binding"
    );
    let old = directory.path().join("legacy");
    std::fs::create_dir(&old).unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(old.join("secretary.sqlite3"))
        .create_if_missing(true);
    let mut conn = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE operations(operation_id TEXT PRIMARY KEY,request_digest TEXT NOT NULL,preview TEXT NOT NULL,state TEXT NOT NULL,updated_at INTEGER NOT NULL)").execute(&mut conn).await.unwrap();
    sqlx::query("INSERT INTO operations(operation_id,request_digest,preview,state,updated_at) VALUES('synthetic-op','digest','{}','applying',0)").execute(&mut conn).await.unwrap();
    drop(conn);
    assert_eq!(
        Journal::open_read_only_bound(&old, "owner", "generation")
            .await
            .err()
            .unwrap()
            .1,
        "readonly_legacy_state_unbound"
    );
    let mut conn = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let row = sqlx::query("SELECT state FROM operations WHERE operation_id='synthetic-op'")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("state"), "applying");
}

mod http {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        Router,
    };
    use secretary_core::{
        application::ports::{PortFuture, WorkspacePort, WriteOutcome},
        domain::{
            error::CoreError,
            operation::Preview,
            registry::{Document, Spreadsheet, View},
        },
        AppState, Registry,
    };
    use serde_json::{json, Value};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tower::ServiceExt;
    const CORE: &str = "SYNTHETIC-CORE-0123456789-abcdefgh";
    const OPERATOR: &str = "SYNTHETIC-OPERATOR-9876543210-zyxw";
    struct Source {
        reads: Arc<AtomicUsize>,
        text: String,
    }
    impl WorkspacePort for Source {
        fn read_document<'a>(
            &'a self,
            _: &'a Document,
        ) -> PortFuture<'a, Result<Value, CoreError>> {
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                Ok(
                    json!({"documentId":"demo-document","title":self.text,"revisionId":"r1","tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"é🙂 sample\n"}}]}}]}}}]}),
                )
            })
        }
        fn read<'a>(&'a self, _: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>> {
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                Ok(
                    json!({"spreadsheetId":"generic-spreadsheet","sheets":[{"properties":{"sheetId":7,"title":"Operations"},"data":[{"startRow":1,"startColumn":1,"rowData":[{"values":[{"effectiveValue":{"stringValue":self.text}},{"effectiveValue":{"numberValue":12}},{"effectiveValue":{"boolValue":true}}]}]}]}]}),
                )
            })
        }
        fn write_values<'a>(
            &'a self,
            _: &'a Spreadsheet,
            _: &'a View,
            _: &'a Preview,
            _: Option<usize>,
        ) -> PortFuture<'a, WriteOutcome> {
            Box::pin(async { panic!("read-budget path must never mutate source") })
        }
    }
    struct Fixture {
        app: Router,
        reads: Arc<AtomicUsize>,
        _directory: tempfile::TempDir,
    }
    impl Fixture {
        async fn new(max_bytes: u64, text: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let reads = Arc::new(AtomicUsize::new(0));
            let mut policy = policy();
            policy.request_output_bytes = max_bytes;
            let journal = Journal::open(directory.path()).await.unwrap();
            let ledger = JournalReadTasks::open(journal.clone(), "owner", "generation", policy)
                .await
                .unwrap();
            let registry: Registry =
                serde_json::from_str(include_str!("../../../config/workspace.example.json"))
                    .unwrap();
            let state = AppState::new_read_only(
                registry,
                CORE.into(),
                "owner".into(),
                Source {
                    reads: reads.clone(),
                    text: text.into(),
                },
            )
            .unwrap()
            .with_writes(
                secretary_core::writes::Writes::new(journal, false),
                OPERATOR.into(),
            )
            .unwrap()
            .with_read_tasks(ledger, OPERATOR.into())
            .unwrap();
            Self {
                app: secretary_core::router(state),
                reads,
                _directory: directory,
            }
        }
        async fn post(
            &self,
            path: &str,
            token: &str,
            value: Value,
            id: Option<&str>,
        ) -> (u16, Value) {
            let mut request = Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json");
            if let Some(id) = id {
                request = request.header("x-ciel-read-reservation", id);
            }
            let response = self
                .app
                .clone()
                .oneshot(request.body(Body::from(value.to_string())).unwrap())
                .await
                .unwrap();
            let status = response.status().as_u16();
            let bytes = to_bytes(response.into_body(), 2_097_152).await.unwrap();
            (status, serde_json::from_slice(&bytes).unwrap())
        }
        async fn create(&self, calls: u64) {
            let mut limits = limits();
            limits.calls = calls;
            limits.in_flight = 1;
            assert_eq!(
                self.post(
                    "/v1/read-budget/operator/create",
                    OPERATOR,
                    serde_json::to_value(limits).unwrap(),
                    None
                )
                .await
                .0,
                200
            );
        }
        async fn reserve(&self, tool: &str) -> Value {
            self.post("/v1/read-budget/reserve", CORE, json!({"tool":tool}), None)
                .await
                .1
        }
        async fn status(&self) -> Value {
            self.post("/v1/read-budget/operator/status", OPERATOR, json!({}), None)
                .await
                .1
        }
    }

    #[tokio::test]
    async fn http_lifecycle_requires_operator_and_tool_input_cannot_reset_task_or_quota() {
        let fixture = Fixture::new(4096, "é🙂").await;
        assert!(fixture.reserve("sheets_list").await["model_text"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(
            fixture
                .post(
                    "/v1/read-budget/operator/create",
                    CORE,
                    json!({"actor":"forged"}),
                    None
                )
                .await
                .0,
            401
        );
        fixture.create(2).await;
        assert_eq!(
            fixture
                .post(
                    "/v1/tools/sheets_read",
                    CORE,
                    json!({"spreadsheet":"operations","view":"overview"}),
                    None
                )
                .await
                .0,
            403
        );
        let reservation = fixture.reserve("sheets_read").await;
        let id = reservation["grant"]["id"].as_str().unwrap();
        let (status, wire) = fixture
            .post(
                "/v1/tools/sheets_read",
                CORE,
                json!({"spreadsheet":"operations","view":"overview","limit":1}),
                Some(id),
            )
            .await;
        assert_eq!(status, 200);
        assert_eq!(wire["result"]["ok"], true);
        let text = wire["model_text"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), wire["result"]);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.status().await["charged"]["rows"], 1);
        assert_eq!(
            fixture.status().await["charged"]["output_bytes"],
            256 + (text.len() as u64).max(256)
        );
        assert_eq!(
            fixture
                .post(
                    "/v1/tools/sheets_read",
                    CORE,
                    json!({"spreadsheet":"operations","view":"overview"}),
                    Some(id)
                )
                .await
                .0,
            409
        );
        let reservation = fixture.reserve("sheets_list").await;
        let id = reservation["grant"]["id"].as_str().unwrap();
        let invalid = fixture
            .post(
                "/v1/tools/sheets_list",
                CORE,
                json!({"task_id":"reset"}),
                Some(id),
            )
            .await
            .1;
        assert_eq!(invalid["result"]["error"]["code"], "invalid_input");
        assert_eq!(fixture.status().await["charged"]["calls"], 2);
        assert!(!fixture.reserve("sheets_list").await["model_text"]
            .as_str()
            .unwrap()
            .is_empty());
        assert!(fixture.reserve("sheets_list").await["model_text"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
        // Operator recovery remains independently authenticated and unbudgeted.
        assert_eq!(
            fixture
                .post(
                    "/v1/operations/synthetic-op/reconcile",
                    CORE,
                    json!({}),
                    None
                )
                .await
                .0,
            401
        );
        assert_eq!(
            fixture
                .post(
                    "/v1/operations/synthetic-op/reconcile",
                    OPERATOR,
                    json!({}),
                    None
                )
                .await
                .0,
            404
        );
        assert_eq!(fixture.status().await["charged"]["calls"], 2);
        assert_eq!(
            fixture
                .post(
                    "/v1/read-budget/reserve",
                    CORE,
                    json!({"tool":"sheets_list","task_id":"reset"}),
                    None
                )
                .await
                .0,
            400
        );
    }

    #[tokio::test]
    async fn huge_single_row_is_withheld_without_truncation_and_failed_local_calls_are_charged() {
        let fixture = Fixture::new(512, &"é🙂".repeat(500)).await;
        fixture.create(3).await;
        let reservation = fixture.reserve("sheets_read").await;
        let id = reservation["grant"]["id"].as_str().unwrap();
        let wire = fixture
            .post(
                "/v1/tools/sheets_read",
                CORE,
                json!({"spreadsheet":"operations","view":"overview","limit":1}),
                Some(id),
            )
            .await
            .1;
        assert_eq!(
            wire["result"]["error"]["code"],
            "read_budget_result_too_large"
        );
        assert!(wire["model_text"].as_str().unwrap().len() <= 512);
        assert!(!wire["model_text"].as_str().unwrap().contains('é'));
        assert_eq!(fixture.status().await["charged"]["rows"], 0);
        let reservation = fixture.reserve("sheets_list").await;
        let id = reservation["grant"]["id"].as_str().unwrap();
        assert_eq!(
            fixture
                .post(
                    "/v1/read-budget/local-error",
                    CORE,
                    json!({"reservation":id,"tool":"sheets_list"}),
                    None
                )
                .await
                .1["result"]["error"]["code"],
            "invalid_input"
        );
        assert_eq!(fixture.status().await["charged"]["calls"], 2);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture
                .post("/v1/tools/sheets_list", CORE, json!({}), Some(id))
                .await
                .0,
            409
        );
    }

    #[tokio::test]
    async fn document_metadata_and_unicode_citations_count_toward_full_visible_output() {
        let fixture = Fixture::new(4096, "é🙂").await;
        fixture.create(2).await;
        let reservation = fixture.reserve("docs_read").await;
        let id = reservation["grant"]["id"].as_str().unwrap();
        let wire = fixture
            .post(
                "/v1/tools/docs_read",
                CORE,
                json!({"document":"notes","tab_id":"t.0","limit":2}),
                Some(id),
            )
            .await
            .1;
        assert_eq!(wire["result"]["ok"], true);
        assert_eq!(wire["result"]["data"]["text"], "é🙂");
        fn strings(value: &Value) -> u64 {
            match value {
                Value::String(s) => s.chars().count() as u64,
                Value::Array(a) => a.iter().map(strings).sum(),
                Value::Object(o) => o.values().map(strings).sum(),
                _ => 0,
            }
        }
        let units = strings(&wire["result"]["data"]);
        assert!(units > 2); // Titles, provenance, coverage and citations are included.
        assert_eq!(
            fixture.status().await["charged"]["document_characters"],
            units
        );
        let text = wire["model_text"].as_str().unwrap();
        assert!(text.len() > text.chars().count());
        assert_eq!(
            fixture.status().await["charged"]["output_bytes"],
            256 + text.len() as u64
        );

        // A tiny body does not make oversized metadata free.
        let fixture = Fixture::new(512, &"é🙂".repeat(500)).await;
        fixture.create(1).await;
        let reservation = fixture.reserve("docs_describe").await;
        let wire = fixture
            .post(
                "/v1/tools/docs_describe",
                CORE,
                json!({"document":"notes"}),
                reservation["grant"]["id"].as_str(),
            )
            .await
            .1;
        assert_eq!(
            wire["result"]["error"]["code"],
            "read_budget_result_too_large"
        );
        assert_eq!(fixture.status().await["charged"]["document_characters"], 0);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
    }
}
