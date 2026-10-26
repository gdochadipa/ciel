//! Full public API acceptance against an external Google-only HTTP stand-in.
//! Requires localhost binds and the real Reqwest production dependency graph.
mod common;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use secretary_core::{journal::Journal, router, writes::Writes, AppState, GoogleClient};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[derive(Clone)]
struct Provider {
    book: Arc<tokio::sync::Mutex<Value>>,
    sends: Arc<AtomicUsize>,
}
async fn read(State(p): State<Provider>) -> Json<Value> {
    Json(p.book.lock().await.clone())
}
async fn append(
    State(p): State<Provider>,
    Path(_range): Path<String>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    p.sends.fetch_add(1, Ordering::SeqCst);
    let mut book = p.book.lock().await;
    let values = body["values"][0].as_array().unwrap();
    for (c, value) in values.iter().enumerate() {
        book["sheets"][5]["data"][0]["rowData"][1]["values"][c] = common::cell(value.clone());
    }
    // The provider accepted the row but the response is ambiguous to Core.
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"private_upstream_detail":"not exposed"})),
    )
}
async fn call(url: &str, path: &str, token: &str, input: Value) -> (StatusCode, Value) {
    let r = reqwest::Client::new()
        .post(format!("{url}{path}"))
        .bearer_auth(token)
        .json(&input)
        .send()
        .await
        .unwrap();
    (r.status(), r.json().await.unwrap())
}
#[tokio::test]
async fn accepted_append_with_lost_response_is_reconciled_once_without_reissuing() {
    let (registry, book) = common::fixture();
    let legacy = book["sheets"][0].clone();
    let provider = Provider {
        book: Arc::new(tokio::sync::Mutex::new(book)),
        sends: Arc::new(AtomicUsize::new(0)),
    };
    let upstream = Router::new()
        .route("/spreadsheets/demo-spreadsheet", get(read))
        .route(
            "/spreadsheets/demo-spreadsheet/values/{*range}",
            post(append),
        )
        .with_state(provider.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let google = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let dir = std::env::temp_dir().join(format!("secretary-http-{}", uuid::Uuid::new_v4()));
    let journal = Journal::open(&dir).await.unwrap();
    let state = AppState::new(
        registry,
        "business-token-secret".into(),
        "configured-owner".into(),
        GoogleClient::static_token(&base, "fixture-token").unwrap(),
    )
    .unwrap()
    .with_writes(Writes::new(journal, true), "operator-token-secret".into())
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let core = tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    let request = json!({"operation_id":"sample-op","spreadsheet":"wedding-demo","view":"tasks","action":"append","fields":{"title":"Sample task","status":"todo"}});
    let (_, preview) = call(
        &url,
        "/v1/tools/sheets_preview_change",
        "business-token-secret",
        request.clone(),
    )
    .await;
    assert_eq!(preview["data"]["actor"], "configured-owner");
    assert_eq!(preview["data"]["state"], "previewed");
    assert_eq!(
        call(
            &url,
            "/v1/tools/sheets_append",
            "business-token-secret",
            json!({"operation_id":"sample-op"})
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            &url,
            "/v1/operations/sample-op/approval",
            "business-token-secret",
            json!({"decision":"approve","preview_digest":preview["data"]["preview_digest"]})
        )
        .await
        .0,
        401
    );
    assert_eq!(
        call(
            &url,
            "/v1/operations/sample-op/approval",
            "operator-token-secret",
            json!({"decision":"approve","preview_digest":preview["data"]["preview_digest"]})
        )
        .await
        .0,
        200
    );
    let (_, result) = call(
        &url,
        "/v1/tools/sheets_append",
        "business-token-secret",
        json!({"operation_id":"sample-op"}),
    )
    .await;
    assert_eq!(result["data"]["state"], "uncertain");
    assert_eq!(
        call(
            &url,
            "/v1/tools/sheets_append",
            "business-token-secret",
            json!({"operation_id":"sample-op"})
        )
        .await
        .0,
        409
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    let (_, result) = call(
        &url,
        "/v1/operations/sample-op/reconcile",
        "operator-token-secret",
        json!({}),
    )
    .await;
    assert_eq!(result["data"]["state"], "applied");
    let (_, again) = call(
        &url,
        "/v1/tools/sheets_preview_change",
        "business-token-secret",
        request.clone(),
    )
    .await;
    assert_eq!(
        again["data"]["preview_digest"],
        preview["data"]["preview_digest"]
    );
    let mut altered = request;
    altered["fields"]["title"] = json!("Changed payload");
    assert_eq!(
        call(
            &url,
            "/v1/tools/sheets_preview_change",
            "business-token-secret",
            altered
        )
        .await
        .0,
        409
    );
    let (_, applied) = call(
        &url,
        "/v1/tools/sheets_append",
        "business-token-secret",
        json!({"operation_id":"sample-op"}),
    )
    .await;
    assert_eq!(applied["data"]["state"], "applied");
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(provider.book.lock().await["sheets"][0], legacy);
    core.abort();
    google.abort();
}
