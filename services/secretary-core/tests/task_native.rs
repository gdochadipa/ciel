//! Real HTTP router/bus/SQLite, controlled source IO; no live Google/host claims.
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use futures_util::StreamExt;
use secretary_core::{
    application::ports::{PortFuture, WorkspacePort, WriteOutcome},
    domain::{
        operation::Preview,
        registry::{Document, Spreadsheet, View},
    },
    journal::{write_tasks::JournalWriteTasks, Journal},
    router,
    writes::Writes,
    AppState, CoreError, Registry,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;
const CORE: &str = "synthetic-core-token";
const NATIVE: &str = "synthetic-native-token";
#[derive(Clone)]
struct Source {
    doc: Arc<tokio::sync::Mutex<Value>>,
    book: Arc<tokio::sync::Mutex<Value>>,
    sends: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
    outcome: Arc<AtomicUsize>,
    send_started: Arc<tokio::sync::Notify>,
    send_release: Arc<tokio::sync::Notify>,
}
impl WorkspacePort for Source {
    fn read<'a>(&'a self, _: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.book.lock().await.clone())
        })
    }
    fn write_values<'a>(
        &'a self,
        _: &'a Spreadsheet,
        _: &'a View,
        _: &'a Preview,
        _: Option<usize>,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async { panic!("structured writes outside task profile") })
    }
    fn read_document<'a>(&'a self, _: &'a Document) -> PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.doc.lock().await.clone())
        })
    }
    fn write_document<'a>(
        &'a self,
        _: &'a Document,
        p: &'a Preview,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async move {
            self.sends.fetch_add(1, Ordering::SeqCst);
            if self.outcome.load(Ordering::SeqCst) == 3 {
                self.send_started.notify_one();
                self.send_release.notified().await;
            }
            if self.outcome.load(Ordering::SeqCst) == 1 {
                return WriteOutcome::Rejected;
            }
            let mut doc = self.doc.lock().await;
            doc["tabs"][0]["documentTab"]["body"]["content"][0]["paragraph"]["elements"][0]
                ["textRun"]["content"] = p.after["text"].clone();
            doc["revisionId"] = json!(uuid::Uuid::new_v4().to_string());
            if self.outcome.load(Ordering::SeqCst) == 2 {
                WriteOutcome::Uncertain
            } else {
                WriteOutcome::Accepted
            }
        })
    }
    fn write_grid<'a>(
        &'a self,
        _: &'a Spreadsheet,
        p: &'a Preview,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async move {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let mut book = self.book.lock().await;
            for (i, value) in p.after[0].as_array().unwrap().iter().enumerate() {
                book["sheets"][0]["data"][0]["rowData"][0]["values"][i] =
                    json!({"userEnteredValue":value,"effectiveValue":value});
            }
            WriteOutcome::Accepted
        })
    }
}
struct Fixture {
    app: axum::Router,
    journal: Journal,
    source: Source,
    _dir: tempfile::TempDir,
}
fn context() -> Value {
    json!({"gateway_instance":"gateway","agent_id":"agent","session_key":"agent:owner","session_id":"incarnation"})
}
fn scope() -> Value {
    json!({"objective":"Correct sample text and cells","mapping_version":"v1","max_operations":10,"targets":[{"kind":"document","document":"notes","document_id":"doc","tab_id":"t.0"},{"kind":"grid","spreadsheet":"sheet","spreadsheet_id":"book","view":"grid","sheet_id":7,"range":"B2:C3"}]})
}
async fn fixture(enabled: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path()).await.unwrap();
    let tasks = JournalWriteTasks::open(journal.clone(), "owner", "deployment")
        .await
        .unwrap();
    let source = Source {
        doc: Arc::new(tokio::sync::Mutex::new(
            json!({"documentId":"doc","revisionId":"r1","title":"Sample","tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"old note\n"}}]}}]}}}]}),
        )),
        book: Arc::new(tokio::sync::Mutex::new(
            json!({"spreadsheetId":"book","sheets":[{"properties":{"sheetId":7,"title":"Grid"},"data":[{"startRow":1,"startColumn":1,"rowData":[{"values":[{"userEnteredValue":{"stringValue":"old"},"effectiveValue":{"stringValue":"old"}},{"userEnteredValue":{"boolValue":true},"effectiveValue":{"boolValue":true}}]},{"values":[{},{}]}]}]}]}),
        )),
        sends: Arc::new(AtomicUsize::new(0)),
        reads: Arc::new(AtomicUsize::new(0)),
        outcome: Arc::new(AtomicUsize::new(0)),
        send_started: Arc::new(tokio::sync::Notify::new()),
        send_release: Arc::new(tokio::sync::Notify::new()),
    };
    let registry:Registry=serde_json::from_value(json!({"version":"v1","documents":[{"alias":"notes","id":"doc","readable":true,"writable":true}],"spreadsheets":[{"alias":"sheet","id":"book","readable":true,"views":[{"name":"grid","sheet_id":7,"title":"Grid","kind":"grid","range":"B2:C3","writable_range":"B2:C3","headers":{},"merges":[],"formulas":{}}]}]})).unwrap();
    let app = router(
        AppState::new(registry, CORE.into(), "owner".into(), source.clone())
            .unwrap()
            .with_writes(Writes::with_task_ledger(tasks, enabled), NATIVE.into())
            .unwrap(),
    );
    Fixture {
        app,
        journal,
        source,
        _dir: dir,
    }
}
async fn post(
    app: &axum::Router,
    path: &str,
    input: Value,
    token: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn value(response: axum::response::Response) -> (u16, Value) {
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 262144).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn control(app: &axum::Router, input: Value) -> Value {
    let (status, body) = value(post(app, "/v1/write-tasks/native", input, NATIVE).await).await;
    assert_eq!(status, 200, "{body}");
    body["data"].clone()
}
async fn grant(f: &Fixture) -> Value {
    let p = control(
        &f.app,
        json!({"action":"propose","context":context(),"scope":scope()}),
    )
    .await;
    control(&f.app,json!({"action":"decide","task_id":p["task_id"],"context":context(),"scope_digest":p["scope_digest"],"grant_revision":0,"decision":"approve"})).await
}
async fn capture(f: &Fixture, task: &Value, id: &str, grid: bool) -> Value {
    let change = if grid {
        json!({"kind":"grid","operation_id":id,"spreadsheet":"sheet","view":"grid","action":"update","fields":{"range":"B2:C2","values":[["new",false]]}})
    } else {
        json!({"kind":"document","operation_id":id,"document":"notes","tab_id":"t.0","action":"replace_text","fields":{"find":"old","replacement":"new"}})
    };
    control(
        &f.app,
        json!({"action":"capture","task_id":task["task_id"],"context":context(),"change":change}),
    )
    .await
}
async fn capture_compound(f: &Fixture, task: &Value, id: &str) -> Value {
    control(&f.app, json!({"action":"capture","task_id":task["task_id"],"context":context(),"change":{
        "kind":"document","operation_id":id,"document":"notes","tab_id":"t.0","action":"replace_text_batch",
        "fields":{"replacements":[{"find":"old","replacement":"new"},{"find":"note","replacement":"record"}]}
    }})).await
}
async fn capture_doc(f: &Fixture, task: &Value, id: &str, compound: bool) -> Value {
    if compound {
        capture_compound(f, task, id).await
    } else {
        capture(f, task, id, false).await
    }
}
async fn dispatch(f: &Fixture, task: &Value, id: &str) -> Value {
    let mut stream = post(&f.app, "/v1/write-tasks/execute", member(task, id), NATIVE)
        .await
        .into_body()
        .into_data_stream();
    let first: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
    if first["type"] != "ready" {
        return first;
    }
    assert_eq!(
        post(
            &f.app,
            "/v1/write-tasks/confirm",
            first["challenge"].clone(),
            NATIVE
        )
        .await
        .status(),
        200
    );
    serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap()
}

#[tokio::test]
async fn compound_members_keep_exact_replay_counts_and_partial_outcomes_on_human_drift() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    let first = capture_compound(&f, &task, "batch").await;
    assert_eq!(capture_compound(&f, &task, "batch").await, first);
    assert_eq!(
        dispatch(&f, &task, "batch").await["data"]["state"],
        "applied"
    );
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
    let next = json!({"action":"capture","task_id":task["task_id"],"context":context(),"change":{
        "kind":"document","operation_id":"following","document":"notes","tab_id":"t.0","action":"replace_text_batch",
        "fields":{"replacements":[{"find":"new","replacement":"next"},{"find":"record","replacement":"memo"}]}
    }});
    let fresh = control(&f.app, next.clone()).await;
    assert_eq!(fresh["before"], first["after"]);
    assert_ne!(fresh["source_revision"], first["source_revision"]);
    let mut changed = next;
    changed["change"]["fields"]["replacements"][0]["replacement"] = json!("different");
    let (status, refused) =
        value(post(&f.app, "/v1/write-tasks/native", changed, NATIVE).await).await;
    assert_eq!(status, 409);
    assert_eq!(refused["error"]["code"], "operation_conflict");
    f.source.doc.lock().await["revisionId"] = json!("human-revision");
    assert_eq!(
        dispatch(&f, &task, "following").await["data"]["state"],
        "conflict"
    );
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.journal.get("batch").await.unwrap().state.as_str(),
        "applied"
    );
    assert_eq!(
        f.journal.get("following").await.unwrap().state.as_str(),
        "conflict"
    );
    let review = control(
        &f.app,
        json!({"action":"review","task_id":task["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(review["admitted_operations"], 2);
}

#[tokio::test]
async fn compound_finish_persistence_failure_recovers_by_observation_without_another_send() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    capture_compound(&f, &task, "interrupted-batch").await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f._dir.path().join("secretary.sqlite3").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER fail_compound_finish BEFORE UPDATE OF state ON operations WHEN NEW.state='applied' BEGIN SELECT RAISE(ABORT,'synthetic finish failure'); END").execute(&pool).await.unwrap();
    let failed = dispatch(&f, &task, "interrupted-batch").await;
    assert_eq!(failed["ok"], false);
    assert_eq!(
        f.journal
            .get("interrupted-batch")
            .await
            .unwrap()
            .state
            .as_str(),
        "applying"
    );
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM write_task_dispatches WHERE operation_id='interrupted-batch'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER fail_compound_finish")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let Fixture {
        app,
        journal,
        source,
        _dir,
    } = f;
    drop(app);
    drop(journal);
    let recovered = Journal::open(_dir.path()).await.unwrap();
    assert_eq!(
        recovered
            .get("interrupted-batch")
            .await
            .unwrap()
            .state
            .as_str(),
        "reconciliation"
    );
    let tasks = JournalWriteTasks::open(recovered.clone(), "owner", "deployment")
        .await
        .unwrap();
    let registry = serde_json::from_value(json!({"version":"v1","spreadsheets":[],"documents":[{"alias":"notes","id":"doc","readable":true,"writable":true}]})).unwrap();
    let app = router(
        AppState::new(registry, CORE.into(), "owner".into(), source.clone())
            .unwrap()
            .with_writes(Writes::with_task_ledger(tasks, true), NATIVE.into())
            .unwrap(),
    );
    for _ in 0..2 {
        let (status, observed) = value(
            post(
                &app,
                "/v1/operations/interrupted-batch/reconcile",
                json!({}),
                NATIVE,
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{observed}");
        assert_eq!(observed["data"]["state"], "applied");
        assert_eq!(source.sends.load(Ordering::SeqCst), 1);
    }
    assert_eq!(
        recovered
            .get("interrupted-batch")
            .await
            .unwrap()
            .preview
            .after["text"],
        "new record\n"
    );
}
fn member(task: &Value, id: &str) -> Value {
    json!({"task_id":task["task_id"],"context":context(),"grant_revision":task["grant_revision"],"operation_id":id})
}
async fn cancel(f: &Fixture, task: &Value) {
    control(
        &f.app,
        json!({"action":"end","task_id":task["task_id"],"context":context(),"reason":"cancelled","grant_revision":task["grant_revision"]}),
    )
    .await;
}

#[tokio::test]
async fn native_controls_authenticate_before_decode_and_disabled_routes_have_no_effects() {
    let f = fixture(false).await;
    let (status, body) = value(
        post(
            &f.app,
            "/v1/write-tasks/native",
            json!({"forged":true}),
            CORE,
        )
        .await,
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthorized");
    let (status, body) = value(
        post(
            &f.app,
            "/v1/write-tasks/native",
            json!({"action":"propose","context":context(),"scope":scope()}),
            NATIVE,
        )
        .await,
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "write_tasks_unavailable");
    assert_eq!(f.source.reads.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn owner_grant_admits_and_dispatches_exactly_one_docs_or_grid_member_after_preflight() {
    for profile in 0..3 {
        let grid = profile == 1;
        let f = fixture(true).await;
        let task = grant(&f).await;
        let p = if profile == 2 {
            capture_compound(&f, &task, "one").await
        } else {
            capture(&f, &task, "one", grid).await
        };
        let response = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "one"),
            NATIVE,
        )
        .await;
        assert_eq!(response.status(), 200);
        let mut stream = response.into_body().into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            f.source.sends.load(Ordering::SeqCst),
            0,
            "provider sent without native confirmation"
        );
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["challenge"]["preview_digest"], p["preview_digest"]);
        assert_eq!(f.source.reads.load(Ordering::SeqCst), 2);
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.journal.get("one").await.unwrap().state.as_str(),
            "applying"
        );
        let challenge = ready["challenge"].clone();
        let mut altered = challenge.clone();
        altered["member"]["context"]["session_id"] = json!("other");
        assert_eq!(
            post(&f.app, "/v1/write-tasks/confirm", altered, NATIVE)
                .await
                .status(),
            409
        );
        assert_eq!(
            post(&f.app, "/v1/write-tasks/confirm", challenge.clone(), CORE)
                .await
                .status(),
            401
        );
        assert_eq!(
            post(&f.app, "/v1/write-tasks/confirm", challenge.clone(), NATIVE)
                .await
                .status(),
            200
        );
        assert_eq!(
            post(&f.app, "/v1/write-tasks/confirm", challenge, NATIVE)
                .await
                .status(),
            409
        );
        let result: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(result["data"]["state"], "applied", "{result}");
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
        assert!(stream.next().await.is_none());
        let replay = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "one"),
            NATIVE,
        )
        .await;
        let body = to_bytes(replay.into_body(), 262144).await.unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn cancellation_between_ready_and_confirm_prevents_the_provider_send() {
    for compound in [false, true] {
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "cancel", compound).await;
        let mut stream = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "cancel"),
            NATIVE,
        )
        .await
        .into_body()
        .into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        cancel(&f, &task).await;
        assert_eq!(
            post(
                &f.app,
                "/v1/write-tasks/confirm",
                ready["challenge"].clone(),
                NATIVE
            )
            .await
            .status(),
            200
        );
        let result: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(result["data"]["state"], "known_failure");
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.journal.get("cancel").await.unwrap().state.as_str(),
            "known_failure"
        );
    }
}
#[tokio::test]
async fn cancellation_after_accepted_dispatch_keeps_the_one_outcome_and_denies_following_work() {
    for compound in [false, true] {
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "accepted", compound).await;
        f.source.outcome.store(3, Ordering::SeqCst);
        let mut stream = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "accepted"),
            NATIVE,
        )
        .await
        .into_body()
        .into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            post(
                &f.app,
                "/v1/write-tasks/confirm",
                ready["challenge"].clone(),
                NATIVE
            )
            .await
            .status(),
            200
        );
        let result = tokio::spawn(async move { stream.next().await.unwrap().unwrap() });
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            f.source.send_started.notified(),
        )
        .await
        .unwrap();
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
        // The receipt precedes the provider send. Cancellation may revoke following
        // work while this already accepted effect is waiting for its source response.
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            f._dir.path().join("secretary.sqlite3").display()
        ))
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM write_task_dispatches WHERE operation_id='accepted'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        cancel(&f, &task).await;
        f.source.send_release.notify_one();
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), result)
            .await
            .unwrap()
            .unwrap();
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["data"]["state"], "applied");
        assert_eq!(
            f.journal.get("accepted").await.unwrap().state.as_str(),
            "applied"
        );
        let reads = f.source.reads.load(Ordering::SeqCst);
        let (status, failure) = value(post(&f.app, "/v1/write-tasks/native", json!({
        "action":"capture", "task_id":task["task_id"], "context":context(),
        "change":{"kind":"document","operation_id":"following","document":"notes","tab_id":"t.0","action":"replace_text","fields":{"find":"new","replacement":"next"}}
    }), NATIVE).await).await;
        assert_eq!(status, 409);
        assert_eq!(failure["error"]["code"], "write_task_not_active");
        assert_eq!(f.source.reads.load(Ordering::SeqCst), reads);
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
        pool.close().await;
    }
}

#[tokio::test]
async fn dropped_native_request_retires_checkpoint_without_reopening_applying_or_sending() {
    for compound in [false, true] {
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "drop", compound).await;
        let mut stream = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "drop"),
            NATIVE,
        )
        .await
        .into_body()
        .into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        drop(stream);
        assert_eq!(
            post(
                &f.app,
                "/v1/write-tasks/confirm",
                ready["challenge"].clone(),
                NATIVE
            )
            .await
            .status(),
            409
        );
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.journal.get("drop").await.unwrap().state.as_str(),
            "applying"
        );
        let result = to_bytes(
            post(
                &f.app,
                "/v1/write-tasks/execute",
                member(&task, "drop"),
                NATIVE,
            )
            .await
            .into_body(),
            262144,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&result).unwrap()["ok"],
            false
        );
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        let stored = f.journal.get("drop").await.unwrap().preview;
        let binding: secretary_core::application::task_authority::TaskMember =
            serde_json::from_value(member(&task, "drop")).unwrap();
        let Fixture {
            app,
            journal,
            source,
            _dir,
        } = f;
        drop(app);
        drop(journal);
        let recovered = Journal::open(_dir.path()).await.unwrap();
        assert_eq!(
            recovered.get("drop").await.unwrap().state.as_str(),
            "reconciliation"
        );
        assert_eq!(
            recovered.get("drop").await.unwrap().preview.fields,
            stored.fields
        );
        let ledger = JournalWriteTasks::open(recovered, "owner", "deployment")
            .await
            .unwrap();
        use secretary_core::application::task_authority::TaskLedger;
        assert_eq!(
            TaskLedger::claim(&ledger, &binding).await.err().unwrap().1,
            "write_task_not_active"
        );
        assert_eq!(source.sends.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn stale_context_and_out_of_scope_capture_do_not_read_sources() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    let mut wrong = context();
    wrong["session_id"] = json!("new");
    let body = json!({"action":"capture","task_id":task["task_id"],"context":wrong,"change":{"kind":"document","operation_id":"wrong","document":"notes","tab_id":"t.0","action":"replace_text","fields":{"find":"old","replacement":"new"}}});
    let (status, error) =
        value(post(&f.app, "/v1/write-tasks/native", body.clone(), NATIVE).await).await;
    assert_eq!(status, 409);
    assert_eq!(error["error"]["code"], "write_task_context");
    let mut outside = body;
    outside["context"] = context();
    outside["change"]["tab_id"] = json!("t.other");
    assert_eq!(
        post(&f.app, "/v1/write-tasks/native", outside, NATIVE)
            .await
            .status(),
        403
    );
    assert_eq!(f.source.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn drift_stale_revision_and_uncertain_outcomes_never_create_a_second_send() {
    for compound in [false, true] {
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "drift", compound).await;
        let mut stale = member(&task, "drift");
        stale["grant_revision"] = json!(0);
        let bytes = to_bytes(
            post(&f.app, "/v1/write-tasks/execute", stale, NATIVE)
                .await
                .into_body(),
            262144,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["error"]["code"],
            "write_task_stale_member"
        );
        assert_eq!(f.source.reads.load(Ordering::SeqCst), 1);
        f.source.doc.lock().await["revisionId"] = json!("human-change");
        let bytes = to_bytes(
            post(
                &f.app,
                "/v1/write-tasks/execute",
                member(&task, "drift"),
                NATIVE,
            )
            .await
            .into_body(),
            262144,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["data"]["state"],
            "conflict"
        );
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "lost", compound).await;
        f.source.outcome.store(2, Ordering::SeqCst);
        let mut stream = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "lost"),
            NATIVE,
        )
        .await
        .into_body()
        .into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            post(
                &f.app,
                "/v1/write-tasks/confirm",
                ready["challenge"].clone(),
                NATIVE
            )
            .await
            .status(),
            200
        );
        let result: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(result["data"]["state"], "uncertain");
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
        let result = to_bytes(
            post(
                &f.app,
                "/v1/write-tasks/execute",
                member(&task, "lost"),
                NATIVE,
            )
            .await
            .into_body(),
            262144,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&result).unwrap()["error"]["code"],
            "write_task_not_active"
        );
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn confirmation_timeout_is_not_a_task_ttl_and_the_nonce_cannot_be_used_later() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    capture(&f, &task, "timeout", false).await;
    let mut stream = post(
        &f.app,
        "/v1/write-tasks/execute",
        member(&task, "timeout"),
        NATIVE,
    )
    .await
    .into_body()
    .into_data_stream();
    let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
    let result: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
    assert_eq!(result["data"]["state"], "known_failure");
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
    assert_eq!(
        post(
            &f.app,
            "/v1/write-tasks/confirm",
            ready["challenge"].clone(),
            NATIVE
        )
        .await
        .status(),
        409
    );
    let review = control(
        &f.app,
        json!({"action":"review","task_id":task["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(review["phase"], "suspended");
    assert_eq!(review["admitted_operations"], 1);
    let resumed=control(&f.app,json!({"action":"decide","task_id":task["task_id"],"context":context(),"scope_digest":review["scope_digest"],"grant_revision":review["grant_revision"],"decision":"approve"})).await;
    assert_eq!(resumed["phase"], "active");
    assert_eq!(resumed["admitted_operations"], 1);
}

#[tokio::test]
async fn gateway_reconfirmation_keeps_counts_and_old_callbacks_cannot_renew_the_grant() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    capture(&f, &task, "old-preview", false).await;
    let mut replacement = context();
    replacement["gateway_instance"] = json!("replacement");
    let review = control(
        &f.app,
        json!({"action":"review","task_id":task["task_id"],"context":replacement}),
    )
    .await;
    assert_eq!(review["phase"], "active");
    assert_eq!(review["requires_confirmation"], true);
    assert_eq!(review["admitted_operations"], 1);
    let old = json!({"action":"decide","task_id":task["task_id"],"context":context(),"scope_digest":task["scope_digest"],"grant_revision":0,"decision":"approve"});
    let resumed=control(&f.app,json!({"action":"decide","task_id":task["task_id"],"context":replacement,"scope_digest":review["scope_digest"],"grant_revision":review["grant_revision"],"decision":"approve"})).await;
    assert_eq!(resumed["phase"], "active");
    assert_eq!(resumed["admitted_operations"], 1);
    assert_eq!(
        f.journal.get("old-preview").await.unwrap().state.as_str(),
        "rejected"
    );
    assert_eq!(
        post(&f.app, "/v1/write-tasks/native", old, NATIVE)
            .await
            .status(),
        409
    );
    assert_eq!(post(&f.app,"/v1/write-tasks/native",json!({"action":"end","task_id":task["task_id"],"context":context(),"reason":"cancelled","grant_revision":task["grant_revision"]}),NATIVE).await.status(),409);
    let unchanged = control(
        &f.app,
        json!({"action":"review","task_id":task["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(unchanged["phase"], "active");
    assert_eq!(unchanged["grant_revision"], resumed["grant_revision"]);
}

/// Started only by the actual-SDK Node integration suite after its official build.
#[tokio::test]
#[ignore = "requires the explicitly invoked synthetic SDK harness"]
async fn native_sdk_fixture_server() {
    assert_eq!(
        std::env::var("CIEL_TASK_NATIVE_FIXTURE").as_deref(),
        Ok("synthetic")
    );
    let f = fixture(true).await;
    let source = f.source.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let stop = Arc::new(std::sync::Mutex::new(Some(stop)));
    let app=f.app.clone().route("/__fixture",axum::routing::post(move |axum::Json(input):axum::Json<Value>| {
        let source=source.clone();let stop=stop.clone();async move {
            if input["action"]=="uncertain" {source.outcome.store(2,Ordering::SeqCst);}
            if input["action"]=="stop" {if let Some(stop)=stop.lock().unwrap().take() {stop.send(()).unwrap();}}
            axum::Json(json!({"sends":source.sends.load(Ordering::SeqCst),"reads":source.reads.load(Ordering::SeqCst)}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!(
        "CIEL_TASK_NATIVE_LISTENING http://{}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    drop(f);
}

#[tokio::test]
async fn discarded_capture_is_retained_and_an_in_scope_revision_needs_no_new_owner_decision() {
    let f = fixture(true).await;
    let task = grant(&f).await;
    let first = capture(&f, &task, "discard", false).await;
    let before = f.source.reads.load(Ordering::SeqCst);
    let repeated = capture(&f, &task, "discard", false).await;
    assert_eq!(first, repeated);
    assert_eq!(f.source.reads.load(Ordering::SeqCst), before);
    let rejected=control(&f.app,json!({"action":"discard","task_id":task["task_id"],"context":context(),"operation_id":"discard"})).await;
    assert_eq!(rejected["state"], "rejected");
    let next = capture(&f, &task, "fresh", false).await;
    assert_eq!(next["state"], "previewed");
    let review = control(
        &f.app,
        json!({"action":"review","task_id":task["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(review["grant_revision"], 1);
    assert_eq!(review["admitted_operations"], 2);
    assert_eq!(review["phase"], "active");
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn accepted_receipt_failure_rolls_back_before_any_provider_send() {
    for compound in [false, true] {
        let f = fixture(true).await;
        let task = grant(&f).await;
        capture_doc(&f, &task, "rollback", compound).await;
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            f._dir.path().join("secretary.sqlite3").display()
        ))
        .await
        .unwrap();
        sqlx::query("CREATE TRIGGER reject_dispatch BEFORE INSERT ON write_task_dispatches BEGIN SELECT RAISE(FAIL,'synthetic failure'); END").execute(&pool).await.unwrap();
        let mut stream = post(
            &f.app,
            "/v1/write-tasks/execute",
            member(&task, "rollback"),
            NATIVE,
        )
        .await
        .into_body()
        .into_data_stream();
        let ready: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            post(
                &f.app,
                "/v1/write-tasks/confirm",
                ready["challenge"].clone(),
                NATIVE
            )
            .await
            .status(),
            200
        );
        let result: Value = serde_json::from_slice(&stream.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(result["data"]["state"], "known_failure");
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM write_task_dispatches")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let audit: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE event='task_dispatch_accepted'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(audit, 0);
        pool.close().await;
    }
}

#[tokio::test]
async fn readonly_policy_cannot_acquire_task_authority_with_operator_credentials() {
    let f = fixture(true).await;
    let registry:Registry=serde_json::from_value(json!({"version":"v1","documents":[{"alias":"notes","id":"doc","readable":true}],"spreadsheets":[]})).unwrap();
    let state = AppState::new_read_only(registry, "c".repeat(32), "owner".into(), f.source.clone())
        .unwrap()
        .with_writes(Writes::new(f.journal.clone(), false), "n".repeat(32))
        .unwrap();
    let app = router(state);
    let (status, error) = value(
        post(
            &app,
            "/v1/write-tasks/native",
            json!({"action":"propose","context":context(),"scope":scope()}),
            &"n".repeat(32),
        )
        .await,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(error["error"]["code"], "readonly_profile");
    assert_eq!(f.source.reads.load(Ordering::SeqCst), 0);
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_new_legacy_operation_cannot_bypass_the_scoped_coordinator() {
    let f = fixture(true).await;
    let body = json!({"operation_id":"legacy-bypass","document":"notes","tab_id":"t.0","action":"replace_text","fields":{"find":"old","replacement":"new"}});
    let (status, error) =
        value(post(&f.app, "/v1/tools/docs_preview_change", body, CORE).await).await;
    assert_eq!(status, 403);
    assert_eq!(error["error"]["code"], "task_dispatch_required");
    assert_eq!(f.source.reads.load(Ordering::SeqCst), 0);
    let (status, error) = value(
        post(
            &f.app,
            "/v1/operations/legacy-bypass/approval",
            json!({"decision":"approve","preview_digest":"a".repeat(64)}),
            NATIVE,
        )
        .await,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(error["error"]["code"], "task_dispatch_required");
    assert!(f.journal.find("legacy-bypass").await.unwrap().is_none());
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn scoped_journal_cannot_be_composed_under_another_configured_owner() {
    let f = fixture(true).await;
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path()).await.unwrap();
    let tasks = JournalWriteTasks::open(journal, "other-owner", "deployment")
        .await
        .unwrap();
    let registry:Registry=serde_json::from_value(json!({"version":"v1","documents":[{"alias":"notes","id":"doc","readable":true,"writable":true}],"spreadsheets":[]})).unwrap();
    let state = AppState::new(registry, CORE.into(), "owner".into(), f.source.clone()).unwrap();
    let result = state.with_writes(Writes::with_task_ledger(tasks, true), NATIVE.into());
    match result {
        Err(error) => assert_eq!(
            error,
            "write-task ledger owner must match the configured Core owner"
        ),
        Ok(_) => panic!("accepted a foreign task owner"),
    }
    assert_eq!(f.source.reads.load(Ordering::SeqCst), 0);
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 0);
}
