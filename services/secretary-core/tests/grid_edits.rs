//! Real HTTP adapter, bus, approval and durable journal against synthetic Sheets.
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::Request,
    routing::{get, post},
    Json, Router,
};
use secretary_core::{journal::Journal, router, writes::Writes, AppState, GoogleClient, Registry};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;
#[derive(Clone)]
struct Provider {
    book: Arc<tokio::sync::Mutex<Value>>,
    sends: Arc<AtomicUsize>,
    lost: Arc<AtomicBool>,
}
async fn read(State(p): State<Provider>) -> Json<Value> {
    Json(p.book.lock().await.clone())
}
async fn write(State(p): State<Provider>, Json(body): Json<Value>) -> String {
    p.sends.fetch_add(1, Ordering::SeqCst);
    let requests = body["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 1);
    let u = &requests[0]["updateCells"];
    assert_eq!(u["fields"], "userEnteredValue");
    assert_eq!(u["start"]["sheetId"], 7);
    let r = u["start"]["rowIndex"].as_u64().unwrap() as usize;
    let c = u["start"]["columnIndex"].as_u64().unwrap() as usize;
    let mut b = p.book.lock().await;
    for (rr, row) in u["rows"].as_array().unwrap().iter().enumerate() {
        for (cc, cell) in row["values"].as_array().unwrap().iter().enumerate() {
            b["sheets"][0]["data"][0]["rowData"][r + rr]["values"][c + cc] = cell.clone();
        }
    }
    if p.lost.swap(false, Ordering::SeqCst) {
        "broken-json".into()
    } else {
        "{}".into()
    }
}
async fn fixture() -> (Provider, String, tokio::task::JoinHandle<()>) {
    let mut rows = vec![json!({"values":[{},{},{},{}]}); 10];
    rows[0]["values"][0] = json!({"userEnteredValue":{"stringValue":"Protected"},"effectiveValue":{"stringValue":"Protected"}});
    rows[1]["values"][1] = json!({"userEnteredValue":{"numberValue":12}});
    rows[2]["values"][2] =
        json!({"userEnteredValue":{"formulaValue":"=1+1"},"effectiveValue":{"numberValue":2}});
    let p = Provider {
        book: Arc::new(tokio::sync::Mutex::new(
            json!({"spreadsheetId":"grid-demo","sheets":[{"properties":{"sheetId":7,"title":"Plan"},"data":[{"rowData":rows}],"merges":[{"startRowIndex":3,"endRowIndex":4,"startColumnIndex":1,"endColumnIndex":3}]}]}),
        )),
        sends: Default::default(),
        lost: Default::default(),
    };
    let server = Router::new()
        .route("/spreadsheets/grid-demo", get(read))
        .route("/spreadsheets/grid-demo:batchUpdate", post(write))
        .with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
    (p, url, task)
}
async fn app(url: &str, writable: Option<&str>, path: &std::path::Path) -> Router {
    configured(url, writable, path, "grid-v1", "grid-demo").await
}
async fn configured(
    url: &str,
    writable: Option<&str>,
    path: &std::path::Path,
    version: &str,
    id: &str,
) -> Router {
    let mut view = json!({"name":"plan","sheet_id":7,"title":"Plan","kind":"grid","range":"A1:D10","headers":{"A1":"Protected"},"merges":[],"formulas":{}});
    if let Some(r) = writable {
        view["writable_range"] = json!(r);
    }
    let registry:Registry=serde_json::from_value(json!({"version":version,"spreadsheets":[{"alias":"demo","id":id,"readable":true,"views":[view]}]})).unwrap();
    router(
        AppState::new(
            registry,
            "business-secret".into(),
            "owner".into(),
            GoogleClient::static_token(url, "synthetic").unwrap(),
        )
        .unwrap()
        .with_writes(
            Writes::new(Journal::open(path).await.unwrap(), true),
            "operator-secret".into(),
        )
        .unwrap(),
    )
}
fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("secretary-grid-{}", uuid::Uuid::new_v4()))
}
async fn call(app: &Router, name: &str, input: Value, operator: bool) -> (u16, Value) {
    let r = app
        .clone()
        .oneshot(
            Request::post(name)
                .header(
                    "authorization",
                    if operator {
                        "Bearer operator-secret"
                    } else {
                        "Bearer business-secret"
                    },
                )
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status().as_u16();
    (
        status,
        serde_json::from_slice(&to_bytes(r.into_body(), 2097152).await.unwrap()).unwrap(),
    )
}
fn change(id: &str, range: &str, values: Value) -> Value {
    json!({"operation_id":id,"spreadsheet":"demo","view":"plan","action":"update","fields":{"range":range,"values":values}})
}
async fn preview(app: &Router, c: Value) -> Value {
    let (status, r) = call(app, "/v1/tools/sheets_preview_change", c, false).await;
    assert_eq!(status, 200, "{r}");
    r["data"].clone()
}
async fn approve(app: &Router, p: &Value) {
    assert_eq!(
        call(
            app,
            &format!(
                "/v1/operations/{}/approval",
                p["operation_id"].as_str().unwrap()
            ),
            json!({"decision":"approve","preview_digest":p["preview_digest"]}),
            true
        )
        .await
        .0,
        200
    )
}
async fn apply(app: &Router, id: &str) -> (u16, Value) {
    call(
        app,
        "/v1/tools/sheets_update",
        json!({"operation_id":id}),
        false,
    )
    .await
}
#[tokio::test]
async fn grid_approval_literal_values_clear_and_replay_preserve_other_cells() {
    let (p, url, server) = fixture().await;
    let app = app(&url, Some("B2:C8"), &path()).await;
    let c = change("grid-one", "B2:C2", json!([["=literal", null]]));
    let intent = preview(&app, c.clone()).await;
    assert_eq!(intent["resource_kind"], "sheets_grid");
    assert_eq!(intent["before"], json!([[{"numberValue":12},null]]));
    assert_eq!(preview(&app, c).await, intent);
    assert_eq!(apply(&app, "grid-one").await.0, 409);
    approve(&app, &intent).await;
    for _ in 0..2 {
        let (_, r) = apply(&app, "grid-one").await;
        assert_eq!(r["data"]["state"], "applied", "{r}");
    }
    assert_eq!(p.sends.load(Ordering::SeqCst), 1);
    let b = p.book.lock().await;
    assert_eq!(
        b["sheets"][0]["data"][0]["rowData"][1]["values"][1]["userEnteredValue"],
        json!({"stringValue":"=literal"})
    );
    assert_eq!(
        b["sheets"][0]["data"][0]["rowData"][2]["values"][2]["userEnteredValue"]["formulaValue"],
        "=1+1"
    );
    server.abort();
}
#[tokio::test]
async fn permission_bounds_formula_merges_and_headers_fail_before_send() {
    let (p, url, server) = fixture().await;
    let readonly = app(&url, None, &path()).await;
    assert_eq!(
        call(
            &readonly,
            "/v1/tools/sheets_preview_change",
            change("denied", "B2:B2", json!([[1]])),
            false
        )
        .await
        .0,
        403
    );
    let app = app(&url, Some("A1:D10"), &path()).await;
    for (range, values) in [
        ("A1:A1", json!([["bad"]])),
        ("C3:C3", json!([["bad"]])),
        ("B4:C4", json!([[1, 2]])),
        ("A1:D100", json!([[1]])),
        ("B2:C2", json!([[1]])),
        ("B2:B2", json!([[{"formulaValue":"=1"}]])),
    ] {
        let (status, r) = call(
            &app,
            "/v1/tools/sheets_preview_change",
            change("denied", range, values),
            false,
        )
        .await;
        assert!(status == 400 || status == 403, "{r}");
    }
    assert_eq!(p.sends.load(Ordering::SeqCst), 0);
    server.abort();
}
#[tokio::test]
async fn changed_intent_or_source_conflicts_and_never_reaches_writer() {
    let (p, url, server) = fixture().await;
    let app = app(&url, Some("B2:C8"), &path()).await;
    let intent = preview(&app, change("stale", "B2:C2", json!([[true, 4]]))).await;
    assert_eq!(
        call(
            &app,
            "/v1/tools/sheets_preview_change",
            change("stale", "B2:C2", json!([[false, 4]])),
            false
        )
        .await
        .0,
        409
    );
    approve(&app, &intent).await;
    p.book.lock().await["sheets"][0]["data"][0]["rowData"][1]["values"][1] =
        json!({"userEnteredValue":{"numberValue":99}});
    assert_eq!(apply(&app, "stale").await.1["data"]["state"], "conflict");
    assert_eq!(p.sends.load(Ordering::SeqCst), 0);
    server.abort();
}
#[tokio::test]
async fn accepted_lost_response_survives_restart_and_only_observes() {
    let (p, url, server) = fixture().await;
    let journal = path();
    let first = app(&url, Some("B2:C8"), &journal).await;
    let intent = preview(&first, change("lost", "B2:C2", json!([[false, 3.5]]))).await;
    approve(&first, &intent).await;
    p.lost.store(true, Ordering::SeqCst);
    assert_eq!(apply(&first, "lost").await.1["data"]["state"], "uncertain");
    assert_eq!(apply(&first, "lost").await.0, 409);
    drop(first);
    let restarted = app(&url, Some("B2:C8"), &journal).await;
    assert_eq!(
        call(&restarted, "/v1/operations/lost/reconcile", json!({}), true)
            .await
            .1["data"]["state"],
        "applied"
    );
    assert_eq!(p.sends.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn changed_registry_or_spreadsheet_identity_cannot_apply_old_approval() {
    let (p, url, server) = fixture().await;
    for (version, id) in [
        ("grid-v2", "grid-demo"),
        ("grid-v1", "different-spreadsheet"),
    ] {
        let journal = path();
        let first = app(&url, Some("B2:C8"), &journal).await;
        let intent = preview(&first, change("mapping", "B2:C2", json!([[1, 2]]))).await;
        approve(&first, &intent).await;
        drop(first);
        let reopened = configured(&url, Some("B2:C8"), &journal, version, id).await;
        assert_eq!(
            apply(&reopened, "mapping").await.1["data"]["state"],
            "conflict"
        );
    }
    assert_eq!(p.sends.load(Ordering::SeqCst), 0);
    server.abort();
}
#[tokio::test]
async fn uncertain_before_state_stays_unresolved_and_divergence_conflicts_without_resend() {
    let (p, url, server) = fixture().await;
    let app = app(&url, Some("B2:C8"), &path()).await;
    let intent = preview(&app, change("observe", "B2:C2", json!([[false, 3.5]]))).await;
    approve(&app, &intent).await;
    p.lost.store(true, Ordering::SeqCst);
    assert_eq!(apply(&app, "observe").await.1["data"]["state"], "uncertain");
    {
        let mut b = p.book.lock().await;
        b["sheets"][0]["data"][0]["rowData"][1]["values"][1] =
            json!({"userEnteredValue":{"numberValue":12}});
        b["sheets"][0]["data"][0]["rowData"][1]["values"][2] = json!({});
    }
    assert_eq!(
        call(&app, "/v1/operations/observe/reconcile", json!({}), true)
            .await
            .1["data"]["state"],
        "reconciliation"
    );
    p.book.lock().await["sheets"][0]["data"][0]["rowData"][1]["values"][1] =
        json!({"userEnteredValue":{"stringValue":"collaborator"}});
    assert_eq!(
        call(&app, "/v1/operations/observe/reconcile", json!({}), true)
            .await
            .1["data"]["state"],
        "conflict"
    );
    assert_eq!(p.sends.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn reconciliation_rejects_another_provider_spreadsheet_with_matching_cells() {
    let (p, url, server) = fixture().await;
    let app = app(&url, Some("B2:C8"), &path()).await;
    let intent = preview(&app, change("identity", "B2:C2", json!([[false, 3.5]]))).await;
    approve(&app, &intent).await;
    p.lost.store(true, Ordering::SeqCst);
    assert_eq!(
        apply(&app, "identity").await.1["data"]["state"],
        "uncertain"
    );
    p.book.lock().await["spreadsheetId"] = json!("another-spreadsheet");
    assert_eq!(
        call(&app, "/v1/operations/identity/reconcile", json!({}), true)
            .await
            .1["data"]["state"],
        "conflict"
    );
    assert_eq!(p.sends.load(Ordering::SeqCst), 1);
    server.abort();
}
