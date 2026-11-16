//! Shared parent fixture exercises the real native router/bus/journal.
use super::*;

async fn grant_scope(f: &Fixture, scope: Value) -> Value {
    let proposal = control(
        &f.app,
        json!({"action":"propose","context":context(),"scope":scope}),
    )
    .await;
    control(&f.app,json!({"action":"decide","task_id":proposal["task_id"],"context":context(),"scope_digest":proposal["scope_digest"],"grant_revision":0,"decision":"approve"})).await
}
async fn close(f: &Fixture, task: &Value) {
    control(&f.app,json!({"action":"end","task_id":task["task_id"],"context":context(),"grant_revision":task["grant_revision"],"reason":"completed"})).await;
}
async fn undo_grant(f: &Fixture, selection: Value) -> Value {
    let mut selected = scope();
    selected["objective"] = json!("Undo only the selected retained original edits");
    selected["undo_of"] = selection;
    grant_scope(f, selected).await
}
fn inverse(task: &Value, operation: &str, original: &str) -> Value {
    json!({"action":"capture","context":context(),"task_id":task["task_id"],"change":{"kind":"inverse","operation_id":operation,"original_id":original}})
}

#[tokio::test]
async fn native_undo_round_trips_grid_docs_and_compound_under_new_scope_and_preserves_originals() {
    for profile in 0..3 {
        let f = fixture(true).await;
        let forward = grant(&f).await;
        let original = if profile == 2 {
            capture_compound(&f, &forward, "original").await
        } else {
            capture(&f, &forward, "original", profile == 1).await
        };
        assert_eq!(
            dispatch(&f, &forward, "original").await["data"]["state"],
            "applied"
        );
        close(&f, &forward).await;
        if profile == 1 {
            f.source.book.lock().await["sheets"][0]["data"][0]["rowData"][1]["values"][0] = json!({"userEnteredValue":{"stringValue":"human outside range"},"effectiveValue":{"stringValue":"human outside range"}});
        } else {
            let mut doc = f.source.doc.lock().await;
            doc["tabs"].as_array_mut().unwrap().push(json!({"tabProperties":{"tabId":"t.1","title":"Other"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"human other tab\n"}}]}}]}}}));
            doc["revisionId"] = json!("human-before-undo-capture");
        }
        let selected = if profile == 0 {
            json!({"kind":"operation","operation_id":"original"})
        } else {
            json!({"kind":"task","task_id":forward["task_id"]})
        };
        let undo = undo_grant(&f, selected).await;
        let captured = control(&f.app, inverse(&undo, "inverse", "original")).await;
        assert_eq!(captured["before"], original["after"]);
        assert_eq!(captured["after"], original["before"]);
        let reads = f.source.reads.load(Ordering::SeqCst);
        assert_eq!(
            control(&f.app, inverse(&undo, "inverse", "original")).await,
            captured
        );
        assert_eq!(f.source.reads.load(Ordering::SeqCst), reads);
        assert_eq!(
            dispatch(&f, &undo, "inverse").await["data"]["state"],
            "applied"
        );
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 2);
        assert_eq!(
            control(&f.app, inverse(&undo, "inverse", "original")).await["state"],
            "applied"
        );
        let (status, denied) = value(
            post(
                &f.app,
                "/v1/write-tasks/native",
                inverse(&undo, "duplicate", "original"),
                NATIVE,
            )
            .await,
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(denied["error"]["code"], "undo_attempt_blocked");
        assert!(f.journal.find("duplicate").await.unwrap().is_none());
        let record = f.journal.get("original").await.unwrap();
        assert_eq!(record.state.as_str(), "applied");
        assert_eq!(record.preview.preview_digest, original["preview_digest"]);
        assert_eq!(record.preview.before, original["before"]);
        let observed = control(
            &f.app,
            json!({"action":"review","task_id":undo["task_id"],"context":context()}),
        )
        .await;
        assert_eq!(observed["admitted_operations"], 1);
        assert!(observed["undo_next"].is_null());
        assert_eq!(f.source.sends.load(Ordering::SeqCst), 2);
        if profile == 1 {
            assert_eq!(
                f.source.book.lock().await["sheets"][0]["data"][0]["rowData"][1]["values"][0]
                    ["userEnteredValue"]["stringValue"],
                "human outside range"
            );
        } else {
            assert_eq!(
                f.source.doc.lock().await["tabs"][1]["documentTab"]["body"]["content"][0]
                    ["paragraph"]["elements"][0]["textRun"]["content"],
                "human other tab\n"
            );
        }
    }
}

#[tokio::test]
async fn native_undo_selection_reverse_order_and_forward_profile_guards_precede_source_io() {
    let f = fixture(true).await;
    let forward = grant(&f).await;
    capture(&f, &forward, "grid", true).await;
    assert_eq!(
        dispatch(&f, &forward, "grid").await["data"]["state"],
        "applied"
    );
    capture_compound(&f, &forward, "doc").await;
    assert_eq!(
        dispatch(&f, &forward, "doc").await["data"]["state"],
        "applied"
    );
    let reads = f.source.reads.load(Ordering::SeqCst);
    let (_, denied) = value(
        post(
            &f.app,
            "/v1/write-tasks/native",
            inverse(&forward, "bad", "doc"),
            NATIVE,
        )
        .await,
    )
    .await;
    assert_eq!(denied["error"]["code"], "write_task_undo_scope");
    assert_eq!(f.source.reads.load(Ordering::SeqCst), reads);
    close(&f, &forward).await;
    let undo = undo_grant(&f, json!({"kind":"task","task_id":forward["task_id"]})).await;
    for (operation, original, code) in [
        ("bad-order", "grid", "undo_reverse_order"),
        ("outside", "foreign", "write_task_undo_scope"),
    ] {
        let (_, denied) = value(
            post(
                &f.app,
                "/v1/write-tasks/native",
                inverse(&undo, operation, original),
                NATIVE,
            )
            .await,
        )
        .await;
        assert_eq!(denied["error"]["code"], code);
        assert!(f.journal.find(operation).await.unwrap().is_none());
    }
    let (_, denied) = value(post(&f.app, "/v1/write-tasks/native", json!({"action":"capture","task_id":undo["task_id"],"context":context(),"change":{"kind":"document","operation_id":"forward-in-undo","document":"notes","tab_id":"t.0","action":"replace_text","fields":{"find":"new","replacement":"next"}}}), NATIVE).await).await;
    assert_eq!(denied["error"]["code"], "write_task_undo_scope");
    assert!(f.journal.find("forward-in-undo").await.unwrap().is_none());
    let before = control(
        &f.app,
        json!({"action":"review","task_id":undo["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(before["undo_next"]["original_id"], "doc");
    assert_eq!(before["admitted_operations"], 0);
    assert_eq!(f.source.reads.load(Ordering::SeqCst), reads);
    control(&f.app, inverse(&undo, "undo-doc", "doc")).await;
    assert_eq!(
        dispatch(&f, &undo, "undo-doc").await["data"]["state"],
        "applied"
    );
    let next = control(
        &f.app,
        json!({"action":"review","task_id":undo["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(next["undo_next"]["original_id"], "grid");
    control(&f.app, inverse(&undo, "undo-grid", "grid")).await;
    assert_eq!(
        dispatch(&f, &undo, "undo-grid").await["data"]["state"],
        "applied"
    );
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn native_undo_source_drift_and_lost_response_never_resend_or_refill_original_evidence() {
    for lost in [false, true] {
        let f = fixture(true).await;
        let forward = grant(&f).await;
        capture_compound(&f, &forward, "original").await;
        assert_eq!(
            dispatch(&f, &forward, "original").await["data"]["state"],
            "applied"
        );
        close(&f, &forward).await;
        let undo = undo_grant(&f, json!({"kind":"operation","operation_id":"original"})).await;
        control(&f.app, inverse(&undo, "inverse", "original")).await;
        if lost {
            f.source.outcome.store(2, Ordering::SeqCst);
        } else {
            f.source.doc.lock().await["revisionId"] = json!("human-change");
        }
        let result = dispatch(&f, &undo, "inverse").await;
        assert_eq!(
            result["data"]["state"],
            if lost { "uncertain" } else { "conflict" }
        );
        assert_eq!(
            f.source.sends.load(Ordering::SeqCst),
            if lost { 2 } else { 1 }
        );
        assert_eq!(dispatch(&f, &undo, "inverse").await["ok"], false);
        assert_eq!(
            f.journal.get("original").await.unwrap().state.as_str(),
            "applied"
        );
        if lost {
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
                recovered.get("inverse").await.unwrap().state.as_str(),
                "uncertain"
            );
            let tasks = JournalWriteTasks::open(recovered.clone(), "owner", "deployment")
                .await
                .unwrap();
            let registry=serde_json::from_value(json!({"version":"v1","spreadsheets":[],"documents":[{"alias":"notes","id":"doc","readable":true,"writable":true}]})).unwrap();
            let app = router(
                AppState::new(registry, CORE.into(), "owner".into(), source.clone())
                    .unwrap()
                    .with_writes(Writes::with_task_ledger(tasks, true), NATIVE.into())
                    .unwrap(),
            );
            let (_, replay) = value(
                post(
                    &app,
                    "/v1/write-tasks/execute",
                    member(&undo, "inverse"),
                    NATIVE,
                )
                .await,
            )
            .await;
            assert_eq!(replay["error"]["code"], "write_task_not_active");
            let (_, observed) =
                value(post(&app, "/v1/operations/inverse/reconcile", json!({}), NATIVE).await)
                    .await;
            assert_eq!(observed["data"]["state"], "applied");
            assert_eq!(source.sends.load(Ordering::SeqCst), 2);
            assert_eq!(
                recovered.get("original").await.unwrap().state.as_str(),
                "applied"
            );
            assert_eq!(
                recovered.get("inverse").await.unwrap().state.as_str(),
                "applied"
            );
        }
    }
}

#[tokio::test]
async fn unavailable_original_remains_visible_but_native_capture_and_reconcile_do_no_source_io() {
    use sqlx::Connection;
    let f = fixture(true).await;
    let forward = grant(&f).await;
    capture_compound(&f, &forward, "original").await;
    assert_eq!(
        dispatch(&f, &forward, "original").await["data"]["state"],
        "applied"
    );
    close(&f, &forward).await;
    // Seed a future cleanup result only in this disposable fixture. There is no
    // runtime cleanup route or authority to remove real journal evidence yet.
    let mut conn = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new()
            .filename(f._dir.path().join("secretary.sqlite3")),
    )
    .await
    .unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("DELETE FROM operation_evidence WHERE operation_id='original'")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE operations SET evidence_cleanup=? WHERE operation_id='original'")
        .bind(json!({"version":1,"cleanup_id":"fixture-cleanup","removed_at":100}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    drop(conn);
    let reads = f.source.reads.load(Ordering::SeqCst);
    let undo = undo_grant(&f, json!({"kind":"task","task_id":forward["task_id"]})).await;
    let review = control(
        &f.app,
        json!({"action":"review","task_id":undo["task_id"],"context":context()}),
    )
    .await;
    assert_eq!(review["undo_next"]["original_id"], "original");
    assert_eq!(review["undo_next"]["evidence"], "unavailable");
    let (_, denied) = value(
        post(
            &f.app,
            "/v1/write-tasks/native",
            inverse(&undo, "inverse", "original"),
            NATIVE,
        )
        .await,
    )
    .await;
    assert_eq!(denied["error"]["code"], "operation_evidence_unavailable");
    assert!(f.journal.find("inverse").await.unwrap().is_none());
    let (_, denied) = value(
        post(
            &f.app,
            "/v1/operations/original/reconcile",
            json!({}),
            NATIVE,
        )
        .await,
    )
    .await;
    assert_eq!(denied["error"]["code"], "operation_evidence_unavailable");
    let (_, history) = value(
        post(
            &f.app,
            "/v1/tools/operations_get_history",
            json!({"operation_id":"original"}),
            CORE,
        )
        .await,
    )
    .await;
    assert_eq!(
        history["data"]["operation"]["evidence"]["status"],
        "unavailable"
    );
    assert_eq!(history["data"]["operation"]["state"], "applied");
    assert_eq!(f.source.reads.load(Ordering::SeqCst), reads);
    assert_eq!(f.source.sends.load(Ordering::SeqCst), 1);
}
