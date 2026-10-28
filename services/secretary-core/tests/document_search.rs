//! Public HTTP + real Google GET adapter; search must work with writes disabled.
use axum::{
    body::{to_bytes, Body},
    extract::{Query, State},
    http::Request,
    routing::{get, post},
    Json, Router,
};
use secretary_core::{router, AppState, GoogleClient, Registry};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
#[derive(Clone)]
struct Provider {
    book: Arc<tokio::sync::Mutex<Value>>,
    reads: Arc<AtomicUsize>,
    writes: Arc<AtomicUsize>,
}
fn tab(id: &str, title: &str, text: &str) -> Value {
    json!({"tabProperties":{"tabId":id,"title":title},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":text}}]}}]}}})
}
async fn read(State(p): State<Provider>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    assert_eq!(
        q.get("includeTabsContent").map(String::as_str),
        Some("true")
    );
    p.reads.fetch_add(1, Ordering::SeqCst);
    Json(p.book.lock().await.clone())
}
async fn write(State(p): State<Provider>) -> Json<Value> {
    p.writes.fetch_add(1, Ordering::SeqCst);
    Json(json!({}))
}
async fn fixture() -> (Router, Provider, tokio::task::JoinHandle<()>) {
    let mut other = tab("t.1", "Other", "Cat cat.\n");
    other["childTabs"] = json!([tab("t.2", "Nested", "cat-tail\n")]);
    let p = Provider {
        book: Arc::new(tokio::sync::Mutex::new(
            json!({"documentId":"demo-document","title":"Search notes","revisionId":"rev-1","tabs":[tab("t.0","Main","😀 cat cat\n"),other]}),
        )),
        reads: Default::default(),
        writes: Default::default(),
    };
    let provider = Router::new()
        .route("/documents/demo-document", get(read))
        .route("/documents/demo-document:batchUpdate", post(write))
        .with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let registry:Registry=serde_json::from_value(json!({"version":"search-v1","spreadsheets":[],"documents":[{"alias":"notes","id":"demo-document","readable":true,"writable":false},{"alias":"hidden","id":"hidden-document","readable":false,"writable":false}]})).unwrap();
    let app = router(
        AppState::new(
            registry,
            "business-secret".into(),
            "owner".into(),
            GoogleClient::static_token(&url, "synthetic").unwrap(),
        )
        .unwrap(),
    );
    (app, p, task)
}
async fn search(app: &Router, input: Value) -> (u16, Value) {
    let r = app
        .clone()
        .oneshot(
            Request::post("/v1/tools/docs_search")
                .header("authorization", "Bearer business-secret")
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
#[tokio::test]
async fn literal_search_pages_across_tabs_with_unicode_offsets_and_no_writes() {
    let (app, p, task) = fixture().await;
    let (status, first) = search(&app, json!({"document":"notes","query":"cat","limit":2})).await;
    assert_eq!(status, 200, "{first}");
    let data = &first["data"];
    assert_eq!(data["matches"].as_array().unwrap().len(), 2);
    assert_eq!(data["matches"][0]["start_character"], 2);
    assert_eq!(data["matches"][0]["end_character"], 5);
    assert_eq!(data["matches"][1]["start_character"], 6);
    assert_eq!(data["matches"][0]["tab_id"], "t.0");
    assert_eq!(data["matches"][0]["text"], "😀 cat cat\n");
    assert_eq!(data["source"]["document_id"], "demo-document");
    let (_, next) = search(
        &app,
        json!({"document":"notes","query":"cat","limit":2,"cursor":data["next_cursor"]}),
    )
    .await;
    assert_eq!(next["data"]["matches"][0]["tab_id"], "t.1");
    assert_eq!(next["data"]["matches"][1]["tab_id"], "t.2");
    assert!(next["data"]["next_cursor"].is_null());
    assert_eq!(p.writes.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn filtering_case_literal_and_snippet_bounds_follow_projected_text() {
    let (app, p, task) = fixture().await;
    let (_, filtered) = search(
        &app,
        json!({"document":"notes","query":"cat","tab_id":"t.2"}),
    )
    .await;
    assert_eq!(filtered["data"]["matches"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["data"]["matches"][0]["tab_title"], "Nested");
    for (query, count) in [("Cat", 1), (".", 1), ("not present", 0)] {
        let (status, result) = search(&app, json!({"document":"notes","query":query})).await;
        assert_eq!(status, 200);
        assert_eq!(result["data"]["matches"].as_array().unwrap().len(), count);
        assert!(result["data"]["next_cursor"].is_null());
    }
    let text = format!("{}cat{}", "😀".repeat(200), "é".repeat(200));
    p.book.lock().await["tabs"][0] = tab("t.0", &"T".repeat(1000), &text);
    let (_, result) = search(
        &app,
        json!({"document":"notes","query":"cat","tab_id":"t.0"}),
    )
    .await;
    let hit = &result["data"]["matches"][0];
    assert_eq!(hit["start_character"], 200);
    assert_eq!(hit["snippet_start_character"], 120);
    assert_eq!(hit["snippet_end_character"], 283);
    assert_eq!(hit["text"].as_str().unwrap().chars().count(), 163);
    assert_eq!(hit["tab_title"].as_str().unwrap().chars().count(), 200);
    assert_eq!(p.writes.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn cursor_is_bound_to_query_tab_and_full_source_not_just_revision() {
    let (app, p, task) = fixture().await;
    let (_, first) = search(&app, json!({"document":"notes","query":"cat","limit":1})).await;
    let cursor = first["data"]["next_cursor"].as_str().unwrap();
    for input in [
        json!({"document":"notes","query":"Cat","cursor":cursor}),
        json!({"document":"notes","query":"cat","tab_id":"t.0","cursor":cursor}),
    ] {
        let (status, r) = search(&app, input).await;
        assert_eq!(status, 409);
        assert_eq!(r["error"]["code"], "stale_cursor");
    }
    let binding = cursor.split_once(':').unwrap().0;
    for suffix in ["bad", "999999999", "-1"] {
        assert_eq!(
            search(
                &app,
                json!({"document":"notes","query":"cat","cursor":format!("{binding}:{suffix}")})
            )
            .await
            .0,
            400
        );
    }
    p.book.lock().await["title"] = json!("Changed title, same revision");
    let (status, r) = search(
        &app,
        json!({"document":"notes","query":"cat","cursor":cursor}),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(r["error"]["code"], "stale_cursor");
    assert_eq!(p.writes.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn invalid_inputs_unregistered_and_unauthenticated_requests_never_read_provider() {
    let (app, p, task) = fixture().await;
    for input in [
        json!({"document":"notes","query":" "}),
        json!({"document":"notes","query":"😀".repeat(65)}),
        json!({"document":"notes","query":"cat","limit":0}),
        json!({"document":"notes","query":"cat","limit":51}),
        json!({"document":"notes","query":"cat","tab_id":""}),
        json!({"document":"notes","query":"cat","url":"http://example.invalid"}),
    ] {
        assert_eq!(search(&app, input).await.0, 400);
    }
    assert_eq!(
        search(&app, json!({"document":"unregistered","query":"cat"}))
            .await
            .0,
        404
    );
    assert_eq!(
        search(&app, json!({"document":"hidden","query":"cat"}))
            .await
            .0,
        404
    );
    let response = app
        .oneshot(
            Request::post("/v1/tools/docs_search")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"document":"notes","query":"cat"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(p.reads.load(Ordering::SeqCst), 0);
    assert_eq!(p.writes.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn missing_tab_and_wrong_provider_document_identity_fail_closed() {
    let (app, p, task) = fixture().await;
    assert_eq!(
        search(
            &app,
            json!({"document":"notes","query":"cat","tab_id":"missing"})
        )
        .await
        .0,
        404
    );
    p.book.lock().await["documentId"] = json!("another-document");
    assert_ne!(
        search(&app, json!({"document":"notes","query":"cat"}))
            .await
            .0,
        200
    );
    assert_eq!(p.writes.load(Ordering::SeqCst), 0);
    task.abort();
}
