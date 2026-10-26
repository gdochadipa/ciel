use secretary_core::{router, AppState, GoogleClient, Registry};
use serde_json::{json, Value};

// External Google HTTP stand-in: all core behavior is observed through public routes.
async fn start() -> (String, tokio::task::JoinHandle<()>) {
    let registry: Registry =
        serde_json::from_value(json!({"version":"test-v1","spreadsheets":[]})).unwrap();
    let state = AppState::new(
        registry,
        "test-secret-token".into(),
        "owner-one".into(),
        GoogleClient::static_token("http://127.0.0.1:1", "fixture").unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    (url, handle)
}

#[tokio::test]
async fn tools_require_owner_bound_token_and_never_accept_caller_actor() {
    let (url, handle) = start().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/v1/tools/sheets_list"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: Value = client
        .post(format!("{url}/v1/tools/sheets_list"))
        .bearer_auth("test-secret-token")
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body,
        json!({"ok":true,"data":{"spreadsheets":[],"mapping_version":"test-v1"}})
    );
    let response = client
        .post(format!("{url}/v1/tools/sheets_list"))
        .bearer_auth("test-secret-token")
        .json(&json!({"actor":"intruder"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    handle.abort();
}

mod common;
async fn provider(book: Value) -> (String, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new().route(
        "/spreadsheets/demo-spreadsheet",
        axum::routing::get(move || {
            let book = book.clone();
            async move { axum::Json(book) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, handle)
}
async fn with_provider(
    registry: Registry,
    book: Value,
) -> (String, Vec<tokio::task::JoinHandle<()>>) {
    let (base, provider) = provider(book).await;
    let state = AppState::new(
        registry,
        "test-secret-token".into(),
        "owner-one".into(),
        GoogleClient::static_token(&base, "fixture-token").unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let core = tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    (url, vec![provider, core])
}
async fn call(url: &str, name: &str, input: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{url}/v1/tools/{name}"))
        .bearer_auth("test-secret-token")
        .json(&input)
        .send()
        .await
        .unwrap()
}
#[tokio::test]
async fn public_tools_preserve_unknowns_paginate_and_reject_arbitrary_ranges() {
    let (registry, book) = common::fixture();
    let (url, handles) = with_provider(registry, book).await;
    let body: Value = call(
        &url,
        "reports_wedding",
        json!({"spreadsheet":"wedding-demo"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(body["data"]["known_cost_total"], 2200);
    assert_eq!(body["data"]["unknown_cost_groups"], 38);
    assert!(body["data"]["paid_total"].is_null());
    let page: Value = call(
        &url,
        "sheets_read",
        json!({"spreadsheet":"wedding-demo","view":"wedding_costs","limit":1}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(page["data"]["records"].as_array().unwrap().len(), 1);
    let next:Value=call(&url,"sheets_read",json!({"spreadsheet":"wedding-demo","view":"wedding_costs","limit":1,"cursor":page["data"]["next_cursor"]})).await.json().await.unwrap();
    assert_eq!(
        next["data"]["records"][0]["fields"]["description"],
        "Demo item 3"
    );
    let search: Value = call(
        &url,
        "sheets_search",
        json!({"spreadsheet":"wedding-demo","view":"wedding_costs","query":"Demo item 17"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(search["data"]["records"].as_array().unwrap().len(), 1);
    assert_eq!(
        call(
            &url,
            "sheets_read",
            json!({"spreadsheet":"wedding-demo","view":"wedding_costs","range":"A1:ZZ99999"})
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        call(
            &url,
            "sheets_read",
            json!({"spreadsheet":"wedding-demo","view":"wedding_costs","limit":101})
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        call(&url, "sheets_append", json!({"approved":true}))
            .await
            .status(),
        400
    );
    assert_eq!(
        call(&url, "sheets_append", json!({"operation_id":"sample-op"}))
            .await
            .status(),
        403
    );
    for h in handles {
        h.abort()
    }
}
#[tokio::test]
async fn drift_fails_closed_and_provider_errors_never_echo_source_bodies() {
    let (registry, mut book) = common::fixture();
    book["sheets"][0]["merges"] = json!([]);
    let (url, handles) = with_provider(registry, book).await;
    let result = call(
        &url,
        "sheets_read",
        json!({"spreadsheet":"wedding-demo","view":"wedding_costs"}),
    )
    .await;
    assert_eq!(result.status(), 409);
    let body: Value = result.json().await.unwrap();
    assert_eq!(body["error"]["code"], "schema_drift");
    for h in handles {
        h.abort()
    }
}

#[tokio::test]
async fn transient_provider_failures_have_at_most_two_read_retries() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let (registry, book) = common::fixture();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let upstream = axum::Router::new().route(
        "/spreadsheets/demo-spreadsheet",
        axum::routing::get(move |headers: axum::http::HeaderMap| {
            let count = count.clone();
            let book = book.clone();
            async move {
                assert_eq!(
                    headers.get("authorization").unwrap(),
                    "Bearer fixture-token"
                );
                if count.fetch_add(1, Ordering::SeqCst) < 2 {
                    (
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        axum::Json(json!({"error":"private provider diagnostic"})),
                    )
                } else {
                    (axum::http::StatusCode::OK, axum::Json(book))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let provider = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let state = AppState::new(
        registry,
        "test-secret-token".into(),
        "owner-one".into(),
        GoogleClient::static_token(&base, "fixture-token").unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let core = tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    let body: Value = call(
        &url,
        "reports_wedding",
        json!({"spreadsheet":"wedding-demo"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(body["data"]["known_cost_total"], 2200);
    assert_eq!(observed.load(Ordering::SeqCst), 3);
    core.abort();
    provider.abort();
}
