//! Public HTTP seam; synthetic Google server, real bus, approval and durable journal.
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use secretary_core::{journal::Journal, router, writes::Writes, AppState, Registry};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(app: &axum::Router, path: &str, input: Value, operator: bool) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(path)
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
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}
fn change() -> Value {
    json!({"operation_id":"create-one","spreadsheet":"operations","title":"New planning","rows":20,"columns":3,"headers":["Task","=literal"]})
}
async fn app() -> axum::Router {
    let mut registry: Value = serde_json::from_str(include_str!(
        "../../../postman/fixtures/registry-workspace.json"
    ))
    .unwrap();
    registry["spreadsheets"][0]["create_tabs"] = json!(true);
    let registry: Registry = serde_json::from_value(registry).unwrap();
    let source =
        secretary_core::google::GoogleClient::static_token("http://127.0.0.1:1", "synthetic")
            .unwrap();
    let journal = Journal::open(
        &std::env::temp_dir().join(format!("secretary-create-{}", uuid::Uuid::new_v4())),
    )
    .await
    .unwrap();
    router(
        AppState::new(registry, "business-secret".into(), "owner".into(), source)
            .unwrap()
            .with_writes(Writes::new(journal, true), "operator-secret".into())
            .unwrap(),
    )
}
#[tokio::test]
async fn create_requires_registered_permission_and_rejects_authority_injection() {
    let app = app().await;
    let mut invalid = change();
    invalid["actor"] = json!("owner");
    assert_eq!(
        call(&app, "/v1/tools/sheets_preview_create_tab", invalid, false)
            .await
            .0,
        400
    );
}

#[derive(Clone)]
struct Provider {
    book: std::sync::Arc<tokio::sync::Mutex<Value>>,
    sends: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
async fn provider() -> (String, Provider, tokio::task::JoinHandle<()>) {
    use axum::{
        extract::{Query, State},
        routing::{get, post},
        Json,
    };
    async fn read(
        State(s): State<Provider>,
        Query(q): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        let mut book = s.book.lock().await.clone();
        if let Some(range) = q.get("ranges") {
            let title = range
                .split("'!")
                .next()
                .unwrap()
                .trim_start_matches('\'')
                .replace("''", "'");
            book["sheets"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["properties"]["title"] == title);
        } else {
            for sheet in book["sheets"].as_array_mut().unwrap() {
                sheet.as_object_mut().unwrap().remove("data");
            }
        }
        Json(book)
    }
    async fn write(State(s): State<Provider>, Json(body): Json<Value>) -> (u16, String) {
        s.sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let properties = &body["requests"][0]["addSheet"]["properties"];
        assert_eq!(properties["sheetType"], "GRID");
        let mut book = s.book.lock().await;
        if book["sheets"].as_array().unwrap().iter().any(|t| {
            t["properties"]["sheetId"] == properties["sheetId"]
                || t["properties"]["title"] == properties["title"]
        }) {
            return (400, "{}".into());
        }
        let mut sheet = json!({"properties":properties});
        if body["requests"].as_array().unwrap().len() == 2 {
            let update = &body["requests"][1]["updateCells"];
            assert_eq!(update["start"]["sheetId"], properties["sheetId"]);
            assert_eq!(update["fields"], "userEnteredValue");
            assert_eq!(
                update["rows"][0]["values"][1]["userEnteredValue"],
                json!({"stringValue":"=literal"})
            );
            sheet["data"] = json!([{"rowData":update["rows"]}]);
        }
        book["sheets"].as_array_mut().unwrap().push(sheet);
        if s.lost.swap(false, std::sync::atomic::Ordering::SeqCst) {
            (200, "truncated-json".into())
        } else {
            (200, "{}".into())
        }
    }
    let s = Provider {
        book: std::sync::Arc::new(tokio::sync::Mutex::new(
            json!({"spreadsheetId":"generic-spreadsheet","sheets":[{"properties":{"sheetId":7,"title":"Operations","sheetType":"GRID","gridProperties":{"rowCount":100,"columnCount":10}}}]}),
        )),
        sends: Default::default(),
        lost: Default::default(),
    };
    let server = axum::Router::new()
        .route("/spreadsheets/generic-spreadsheet", get(read))
        .route(
            "/spreadsheets/generic-spreadsheet:batchUpdate",
            post(|state, body| async move {
                let (status, body) = write(state, body).await;
                (axum::http::StatusCode::from_u16(status).unwrap(), body)
            }),
        )
        .with_state(s.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
    (url, s, handle)
}
async fn connected(
    url: &str,
    permission: bool,
    enabled: bool,
) -> (axum::Router, std::path::PathBuf) {
    let mut registry: Value = serde_json::from_str(include_str!(
        "../../../postman/fixtures/registry-workspace.json"
    ))
    .unwrap();
    registry["spreadsheets"][0]["create_tabs"] = json!(permission);
    let source = secretary_core::GoogleClient::static_token(url, "synthetic").unwrap();
    let path = std::env::temp_dir().join(format!("secretary-create-{}", uuid::Uuid::new_v4()));
    let journal = Journal::open(&path).await.unwrap();
    (
        router(
            AppState::new(
                serde_json::from_value(registry).unwrap(),
                "business-secret".into(),
                "owner".into(),
                source,
            )
            .unwrap()
            .with_writes(Writes::new(journal, enabled), "operator-secret".into())
            .unwrap(),
        ),
        path,
    )
}
async fn approve(app: &axum::Router, id: &str, p: &Value) {
    assert_eq!(
        call(
            app,
            &format!("/v1/operations/{id}/approval"),
            json!({"decision":"approve","preview_digest":p["preview_digest"]}),
            true
        )
        .await
        .0,
        200
    )
}
#[tokio::test]
async fn approved_tab_create_is_atomic_literal_bounded_unregistered_and_replayed_once() {
    let (url, s, server) = provider().await;
    let (app, _) = connected(&url, true, true).await;
    let (status, result) = call(&app, "/v1/tools/sheets_preview_create_tab", change(), false).await;
    assert_eq!(status, 200, "{result}");
    let p = &result["data"];
    assert_eq!(p["after"]["registration"], "unregistered");
    assert_eq!(p["after"]["readable"], false);
    assert_eq!(p["after"]["writable"], false);
    assert_eq!(
        call(&app, "/v1/tools/sheets_preview_create_tab", change(), false)
            .await
            .1["data"],
        *p
    );
    let mut altered = change();
    altered["columns"] = json!(4);
    assert_eq!(
        call(&app, "/v1/tools/sheets_preview_create_tab", altered, false)
            .await
            .0,
        409
    );
    assert_eq!(
        call(
            &app,
            "/v1/tools/sheets_create_tab",
            json!({"operation_id":"create-one"}),
            false
        )
        .await
        .0,
        409
    );
    approve(&app, "create-one", p).await;
    for _ in 0..2 {
        let r = call(
            &app,
            "/v1/tools/sheets_create_tab",
            json!({"operation_id":"create-one"}),
            false,
        )
        .await;
        assert_eq!(r.1["data"]["state"], "applied", "{r:?}");
    }
    assert_eq!(s.sends.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        s.book.lock().await["sheets"][0]["properties"]["title"],
        "Operations"
    );
    assert_eq!(
        call(
            &app,
            "/v1/tools/sheets_describe",
            json!({"spreadsheet":"operations"}),
            false
        )
        .await
        .1["data"]["views"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.abort();
}
#[tokio::test]
async fn lost_response_survives_restart_and_reconciles_without_another_send() {
    let (url, s, server) = provider().await;
    let (app, path) = connected(&url, true, true).await;
    let (_, r) = call(&app, "/v1/tools/sheets_preview_create_tab", change(), false).await;
    approve(&app, "create-one", &r["data"]).await;
    s.lost.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        call(
            &app,
            "/v1/tools/sheets_create_tab",
            json!({"operation_id":"create-one"}),
            false
        )
        .await
        .1["data"]["state"],
        "uncertain"
    );
    assert_eq!(
        call(
            &app,
            "/v1/tools/sheets_create_tab",
            json!({"operation_id":"create-one"}),
            false
        )
        .await
        .0,
        409
    );
    drop(app);
    let mut registry: Value = serde_json::from_str(include_str!(
        "../../../postman/fixtures/registry-workspace.json"
    ))
    .unwrap();
    registry["spreadsheets"][0]["create_tabs"] = json!(true);
    let app = router(
        AppState::new(
            serde_json::from_value(registry).unwrap(),
            "business-secret".into(),
            "owner".into(),
            secretary_core::GoogleClient::static_token(&url, "synthetic").unwrap(),
        )
        .unwrap()
        .with_writes(
            Writes::new(Journal::open(&path).await.unwrap(), true),
            "operator-secret".into(),
        )
        .unwrap(),
    );
    assert_eq!(
        call(&app, "/v1/operations/create-one/reconcile", json!({}), true)
            .await
            .1["data"]["state"],
        "applied"
    );
    assert_eq!(s.sends.load(std::sync::atomic::Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn title_id_collisions_and_registration_gates_never_send() {
    let (url, s, server) = provider().await;
    for (permission, enabled) in [(false, true), (true, false)] {
        let (app, _) = connected(&url, permission, enabled).await;
        assert_eq!(
            call(&app, "/v1/tools/sheets_preview_create_tab", change(), false)
                .await
                .0,
            403
        );
    }
    let (app, _) = connected(&url, true, true).await;
    for (key, value) in [
        ("title", json!("operations")),
        ("title", json!("Bad/name")),
        ("rows", json!(1001)),
        ("columns", json!(27)),
        ("rows", json!(1000)),
        ("headers", json!(["a", "b", "c", "d"])),
    ] {
        let mut c = change();
        c[key] = value;
        if key == "rows" && c[key] == 1000 {
            c["columns"] = json!(26);
        }
        let (status, r) = call(&app, "/v1/tools/sheets_preview_create_tab", c, false).await;
        assert!(status == 400 || status == 409, "{r}");
    }
    for collision in ["title", "id"] {
        let mut c = change();
        let id = format!("collision-{collision}");
        c["operation_id"] = json!(id);
        let (_, r) = call(&app, "/v1/tools/sheets_preview_create_tab", c, false).await;
        let p = &r["data"];
        approve(&app, &id, p).await;
        let props = if collision == "id" {
            json!({"sheetId":p["after"]["sheet_id"],"title":"Other title"})
        } else {
            json!({"sheetId":123456,"title":"NEW PLANNING"})
        };
        s.book.lock().await["sheets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"properties":props}));
        assert_eq!(
            call(
                &app,
                "/v1/tools/sheets_create_tab",
                json!({"operation_id":id}),
                false
            )
            .await
            .1["data"]["state"],
            "conflict"
        );
        s.book.lock().await["sheets"].as_array_mut().unwrap().pop();
    }
    assert_eq!(s.sends.load(std::sync::atomic::Ordering::SeqCst), 0);
    server.abort();
}
