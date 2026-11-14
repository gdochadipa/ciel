//! Explicit profile enforcement through Core HTTP and an external Google stand-in.
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use secretary_core::{journal::Journal, router, writes::Writes, AppState, GoogleClient, Registry};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

const CORE: &str = "synthetic-core-bearer-aaaaaaaaaaaaaaaaaaaaaaaa";
const OPERATOR: &str = "synthetic-operator-bearer-bbbbbbbbbbbbbbbbbbbb";

#[derive(Clone, Default)]
struct Provider {
    reads: Arc<AtomicUsize>,
    mutations: Arc<AtomicUsize>,
}
async fn sheet(State(provider): State<Provider>) -> Json<Value> {
    provider.reads.fetch_add(1, Ordering::SeqCst);
    Json(json!({"spreadsheetId":"generic-spreadsheet","sheets":[{
        "properties":{"sheetId":7,"title":"Operations"},
        "data":[{"startRow":1,"startColumn":1,"rowData":[{"values":[
            {"effectiveValue":{"stringValue":"Independent"}},
            {"effectiveValue":{"numberValue":12}},
            {"effectiveValue":{"boolValue":true}}
        ]}]}]
    }]}))
}
async fn document(State(provider): State<Provider>) -> Json<Value> {
    provider.reads.fetch_add(1, Ordering::SeqCst);
    Json(
        json!({"documentId":"demo-document","title":"Notes","revisionId":"rev-1",
        "tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},
            "documentTab":{"body":{"content":[{"paragraph":{"elements":[
                {"textRun":{"content":"Hello sample\n"}}
            ]}}]}}}]}),
    )
}
async fn unexpected_provider_request(State(provider): State<Provider>) -> StatusCode {
    provider.mutations.fetch_add(1, Ordering::SeqCst);
    StatusCode::METHOD_NOT_ALLOWED
}
struct Fixture {
    app: Router,
    provider: Provider,
    server: tokio::task::JoinHandle<()>,
    directory: std::path::PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let provider = Provider::default();
        let upstream = Router::new()
            .route("/spreadsheets/generic-spreadsheet", get(sheet))
            .route("/documents/demo-document", get(document))
            .fallback(unexpected_provider_request)
            .method_not_allowed_fallback(unexpected_provider_request)
            .with_state(provider.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let registry: Registry =
            serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
        let directory =
            std::env::temp_dir().join(format!("secretary-readonly-{}", uuid::Uuid::new_v4()));
        let journal = Journal::open(&directory).await.unwrap();
        let state = AppState::new_read_only(
            registry,
            CORE.into(),
            "configured-owner".into(),
            GoogleClient::static_token(&url, "synthetic-provider-token").unwrap(),
        )
        .unwrap()
        .with_writes(Writes::new(journal, false), OPERATOR.into())
        .unwrap();
        Self {
            app: router(state),
            provider,
            server,
            directory,
        }
    }
    async fn post(&self, path: &str, token: &str, input: Value) -> (u16, Value) {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[tokio::test]
async fn readonly_profile_blocks_mutation_and_approval_routes_before_provider_access() {
    let f = Fixture::new().await;
    for (tool, input) in [
        (
            "sheets_preview_change",
            json!({"operation_id":"blocked-grid","spreadsheet":"operations","view":"overview","action":"update","fields":{"range":"B2:B2","values":[["Changed"]]}}),
        ),
        (
            "sheets_preview_create_tab",
            json!({"operation_id":"blocked-tab","spreadsheet":"operations","title":"New","rows":2,"columns":2}),
        ),
        (
            "docs_preview_change",
            json!({"operation_id":"blocked-doc","document":"notes","tab_id":"t.0","action":"append_text","fields":{"text":"Changed"}}),
        ),
        ("sheets_append", json!({"operation_id":"blocked-grid"})),
        ("sheets_update", json!({"operation_id":"blocked-grid"})),
        ("sheets_create_tab", json!({"operation_id":"blocked-tab"})),
        ("docs_apply", json!({"operation_id":"blocked-doc"})),
        ("operations_get_history", json!({})),
        ("sheets_get_history", json!({})),
        ("reports_wedding", json!({"spreadsheet":"operations"})),
    ] {
        let (status, body) = f.post(&format!("/v1/tools/{tool}"), CORE, input).await;
        assert_eq!(status, 403, "{tool}: {body}");
        assert_eq!(body["error"]["code"], "readonly_profile");
    }
    for token in [CORE, "wrong-operator", OPERATOR] {
        let (status, body) = f
            .post(
                "/v1/operations/blocked-grid/approval",
                token,
                json!({"decision":"approve","preview_digest":"synthetic-digest"}),
            )
            .await;
        assert_eq!(status, if token == OPERATOR { 403 } else { 401 });
        assert_eq!(
            body["error"]["code"],
            if token == OPERATOR {
                "readonly_profile"
            } else {
                "unauthorized"
            }
        );
    }
    // Recovery remains an operator-only observation path, not a write capability.
    let (status, _) = f
        .post("/v1/operations/blocked-grid/reconcile", CORE, json!({}))
        .await;
    assert_eq!(status, 401);
    let (status, body) = f
        .post("/v1/operations/blocked-grid/reconcile", OPERATOR, json!({}))
        .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "operation_not_found");
    assert_eq!(f.provider.reads.load(Ordering::SeqCst), 0);
    assert_eq!(f.provider.mutations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn readonly_profile_preserves_registered_sheets_docs_and_summary_reads() {
    let f = Fixture::new().await;
    let (status, body) = f
        .post("/v1/tools/sheets_list", "wrong-core", json!({}))
        .await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthorized");
    let (status, body) = f
        .post(
            "/v1/tools/sheets_list",
            CORE,
            json!({"actor":"forged-owner"}),
        )
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_input");
    for (tool, input) in [
        ("sheets_list", json!({})),
        ("sheets_describe", json!({"spreadsheet":"operations"})),
        (
            "sheets_read",
            json!({"spreadsheet":"operations","view":"overview"}),
        ),
        (
            "sheets_search",
            json!({"spreadsheet":"operations","view":"overview","query":"Independent"}),
        ),
        (
            "sheets_read",
            json!({"spreadsheet":"operations","view":"overview","mode":"summary_context"}),
        ),
        ("docs_list", json!({})),
        ("docs_describe", json!({"document":"notes"})),
        ("docs_read", json!({"document":"notes","tab_id":"t.0"})),
        ("docs_search", json!({"document":"notes","query":"sample"})),
        (
            "docs_read",
            json!({"document":"notes","mode":"summary_context"}),
        ),
        (
            "docs_read",
            json!({"documents":["notes"],"mode":"summary_context"}),
        ),
    ] {
        let (status, body) = f.post(&format!("/v1/tools/{tool}"), CORE, input).await;
        assert_eq!(status, 200, "{tool}: {body}");
        assert_eq!(body["ok"], true);
        if tool == "sheets_list" {
            assert_eq!(body["data"]["spreadsheets"][0]["writable"], false);
            assert_eq!(body["data"]["spreadsheets"][0]["create_tabs"], false);
        }
        if tool == "docs_list" {
            assert_eq!(body["data"]["documents"][0]["writable"], false);
        }
    }
    let (status, body) = f
        .post(
            "/v1/tools/docs_read",
            CORE,
            json!({"document":"notes","tab_id":"t.0"}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["text"], "Hello sample\n");
    let (status, body) = f
        .post(
            "/v1/tools/docs_describe",
            CORE,
            json!({"document":"unregistered"}),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "document_not_registered");
    assert!(f.provider.reads.load(Ordering::SeqCst) > 0);
    assert_eq!(f.provider.mutations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn readonly_profile_refuses_enabled_coordinators_and_shared_or_short_recovery_tokens() {
    let directory =
        std::env::temp_dir().join(format!("secretary-readonly-token-{}", uuid::Uuid::new_v4()));
    let journal = Journal::open(&directory).await.unwrap();
    let registry: Registry =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    let source =
        GoogleClient::static_token("http://127.0.0.1:1", "synthetic-provider-token").unwrap();
    assert!(AppState::new_read_only(
        registry.clone(),
        "legacy-length-token".into(),
        "configured-owner".into(),
        source.clone()
    )
    .is_err());
    assert!(AppState::new(
        registry,
        "legacy-length-token".into(),
        "configured-owner".into(),
        source
    )
    .is_ok());
    let state = || {
        let registry: Registry =
            serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
        AppState::new_read_only(
            registry,
            CORE.into(),
            "configured-owner".into(),
            GoogleClient::static_token("http://127.0.0.1:1", "synthetic-provider-token").unwrap(),
        )
        .unwrap()
    };
    for (enabled, token) in [(true, OPERATOR), (false, CORE), (false, "short")] {
        assert!(state()
            .with_writes(Writes::new(journal.clone(), enabled), token.into())
            .is_err());
    }
    assert!(state()
        .with_writes(Writes::new(journal.clone(), false), OPERATOR.into())
        .is_ok());
    drop(journal);
    let _ = std::fs::remove_dir_all(directory);
}
