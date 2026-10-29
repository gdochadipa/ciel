//! Public Core HTTP + actual Google adapter; only provider HTTP is synthetic.
use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::Request,
    routing::get,
    Json, Router,
};
use secretary_core::{router, AppState, GoogleClient, Registry};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
#[derive(Clone)]
struct Provider {
    books: Arc<tokio::sync::Mutex<BTreeMap<String, Value>>>,
    reads: Arc<AtomicUsize>,
}
fn book(id: &str, tab: i64, title: &str, rows: &[(&str, i64)]) -> Value {
    json!({"spreadsheetId":id,"sheets":[{"properties":{"sheetId":tab,"title":title},"data":[{"startRow":0,"startColumn":0,"rowData":rows.iter().map(|(text,n)|json!({"values":[{"effectiveValue":{"stringValue":text}},{"effectiveValue":{"numberValue":n}}]})).collect::<Vec<_>>()}]}]})
}
async fn read(State(p): State<Provider>, Path(id): Path<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    p.reads.fetch_add(1, Ordering::SeqCst);
    let data = p.books.lock().await.get(&id).unwrap().clone();
    if data["__fail"] == true {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(json!({"message":"PRIVATE_PROVIDER_CANARY"})),
        )
            .into_response();
    }
    if data["__slow"] == true {
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    }
    Json(data).into_response()
}
async fn fixture() -> (Router, Provider, tokio::task::JoinHandle<()>) {
    fixture_with_late_profile(false).await
}
async fn fixture_with_late_profile(
    late_profile: bool,
) -> (Router, Provider, tokio::task::JoinHandle<()>) {
    let p = Provider {
        books: Arc::new(tokio::sync::Mutex::new(BTreeMap::from([
            (
                "first-id".into(),
                book("first-id", 7, "First", &[("Cat", 10), ("cat tail", 20)]),
            ),
            (
                "second-id".into(),
                book("second-id", 8, "Second", &[("other", 30), ("cat", 40)]),
            ),
        ]))),
        reads: Default::default(),
    };
    if late_profile {
        p.books.lock().await.get_mut("second-id").unwrap()["sheets"][0]["data"][0]["rowData"][0]
            ["values"][0]["effectiveValue"] = json!({"stringValue":"Item"});
    }
    let provider = Router::new()
        .route("/spreadsheets/{id}", get(read))
        .with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let view = |name: &str, id: i64, title: &str, range: &str| json!({"name":name,"sheet_id":id,"title":title,"kind":"grid","range":range,"headers":{},"merges":[],"formulas":{}});
    let registry:Registry=serde_json::from_value(json!({"version":"selected-v1","spreadsheets":[
        {"alias":"first","id":"first-id","readable":true,"views":[view("all",7,"First","A1:B2"),view("tail",7,"First","A2:B2")]},
        {"alias":"second","id":"second-id","readable":true,"views":[if late_profile {json!({"name":"all","sheet_id":8,"title":"Second","kind":"household_items","range":"A1:D16","headers":{"A1":"Item"},"merges":[],"formulas":{}})} else {view("all",8,"Second","A1:B2")}]},
        {"alias":"hidden","id":"hidden-id","readable":false,"views":[view("all",9,"Hidden","A1:B2")]}
    ]})).unwrap();
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
            Request::post("/v1/tools/sheets_search")
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
async fn selected_views_page_in_target_order_and_read_each_workbook_once() {
    let (app, p, task) = fixture().await;
    let targets = json!([{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"},{"spreadsheet":"first","view":"tail"}]);
    let (status, first) = search(&app, json!({"targets":targets,"query":"CAT","limit":1})).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(p.reads.load(Ordering::SeqCst), 2);
    assert_eq!(first["data"]["sources"].as_array().unwrap().len(), 3);
    assert_eq!(
        first["data"]["records"][0]["fields"],
        json!({"A":"Cat","B":10})
    );
    assert_eq!(first["data"]["records"][0]["spreadsheet"], "first");
    let (_, next) = search(
        &app,
        json!({"targets":targets,"query":"CAT","limit":3,"cursor":first["data"]["next_cursor"]}),
    )
    .await;
    let rows = next["data"]["records"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["fields"]["B"], 20);
    assert_eq!(rows[1]["spreadsheet"], "second");
    assert_eq!(rows[1]["source"]["spreadsheet_id"], "second-id");
    assert_eq!(rows[2]["view"], "tail");
    assert!(next["data"]["next_cursor"].is_null());
    assert_eq!(next["data"]["coverage"], "registered_views_only");
    task.abort();
}

#[tokio::test]
async fn selected_views_reject_invalid_or_hidden_targets_before_any_provider_io() {
    let (app, p, task) = fixture().await;
    let target = json!({"spreadsheet":"first","view":"all"});
    for (input, expected) in [
        (json!({"targets":[],"query":"cat"}), 400),
        (json!({"targets":[target,target],"query":"cat"}), 400),
        (
            json!({"targets":[target,target,target,target],"query":"cat"}),
            400,
        ),
        (
            json!({"targets":[target,{"spreadsheet":"hidden","view":"all"}],"query":"cat"}),
            404,
        ),
        (
            json!({"targets":[target,{"spreadsheet":"missing","view":"all"}],"query":"cat"}),
            404,
        ),
        (
            json!({"targets":[target,{"spreadsheet":"second","view":"missing"}],"query":"cat"}),
            400,
        ),
        (
            json!({"targets":[target],"spreadsheet":"first","view":"all","query":"cat"}),
            400,
        ),
        (
            json!({"targets":[{"spreadsheet":"first","view":"all","range":"A1:Z99"}],"query":"cat"}),
            400,
        ),
        (json!({"targets":[null],"query":"cat"}), 400),
        (
            json!({"targets":[{"spreadsheet":"x".repeat(101),"view":"all"}],"query":"cat"}),
            400,
        ),
        (json!({"targets":[target],"query":" "}), 400),
        (json!({"targets":[target],"query":"é".repeat(129)}), 400),
        (json!({"targets":[target],"query":"cat","limit":101}), 400),
        (json!({"targets":[target],"query":"cat","limit":null}), 400),
        (json!({"targets":[target],"query":"cat","cursor":null}), 400),
    ] {
        let (status, body) = search(&app, input).await;
        assert_eq!(status, expected, "{body}");
        assert_eq!(p.reads.load(Ordering::SeqCst), 0);
    }
    let denied = app
        .clone()
        .oneshot(
            Request::post("/v1/tools/sheets_search")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"targets":[target],"query":"cat"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), 401);
    assert_eq!(p.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn selected_views_cursors_bind_all_books_exact_query_target_order_and_positions() {
    let (app, p, task) = fixture().await;
    let targets =
        json!([{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"}]);
    let (_, first) = search(&app, json!({"targets":targets,"query":"cat","limit":1})).await;
    let cursor = first["data"]["next_cursor"].as_str().unwrap();
    for input in [
        json!({"targets":[{"spreadsheet":"second","view":"all"},{"spreadsheet":"first","view":"all"}],"query":"cat","cursor":cursor}),
        json!({"targets":[{"spreadsheet":"first","view":"tail"},{"spreadsheet":"second","view":"all"}],"query":"cat","cursor":cursor}),
        json!({"targets":targets,"query":"CAT","cursor":cursor}),
    ] {
        assert_eq!(search(&app, input).await.1["error"]["code"], "stale_cursor");
    }
    let binding = cursor.split(':').next().unwrap();
    for position in ["2:0", "0:99999", "a:0", "0:0:0"] {
        assert_eq!(
            search(
                &app,
                json!({"targets":targets,"query":"cat","cursor":format!("{binding}:{position}")})
            )
            .await
            .0,
            400
        );
    }
    p.books.lock().await.get_mut("second-id").unwrap()["sheets"][0]["data"][0]["rowData"][0]
        ["values"][0]["effectiveValue"]["stringValue"] = json!("edited unreturned row");
    assert_eq!(
        search(
            &app,
            json!({"targets":targets,"query":"cat","cursor":cursor})
        )
        .await
        .1["error"]["code"],
        "stale_cursor"
    );
    task.abort();
}

#[tokio::test]
async fn selected_views_keep_typed_literal_matching_and_refuse_oversized_whole_pages() {
    let (app, p, task) = fixture().await;
    let targets =
        json!([{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"}]);
    let (_, number) = search(&app, json!({"targets":targets,"query":"40"})).await;
    assert_eq!(number["data"]["records"].as_array().unwrap().len(), 1);
    assert_eq!(number["data"]["records"][0]["fields"]["B"], 40);
    {
        let mut books = p.books.lock().await;
        books.get_mut("first-id").unwrap()["sheets"][0]["data"][0]["rowData"][0]["values"][0]
            ["effectiveValue"] = json!({"boolValue":true});
    }
    let (_, boolean) = search(&app, json!({"targets":targets,"query":"TRUE"})).await;
    assert_eq!(boolean["data"]["records"][0]["fields"]["A"], true);
    let (_, none) = search(&app, json!({"targets":targets,"query":"absent"})).await;
    assert_eq!(none["data"]["records"], json!([]));
    assert!(none["data"]["next_cursor"].is_null());
    p.books.lock().await.get_mut("second-id").unwrap()["sheets"][0]["data"][0]["rowData"][0]
        ["values"][0]["effectiveValue"] =
        json!({"stringValue":format!("cat{}","x".repeat(140_000))});
    let (status, oversized) = search(&app, json!({"targets":targets,"query":"cat"})).await;
    assert_eq!(status, 413);
    assert_eq!(oversized["error"]["code"], "sheets_search_page_too_large");
    assert!(oversized.get("data").is_none());
    task.abort();
}

#[tokio::test]
async fn selected_views_provider_failures_identity_drift_and_timeout_never_return_partial_data() {
    let (app, p, task) = fixture().await;
    let input = json!({"targets":[{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"}],"query":"cat"});
    p.books.lock().await.get_mut("second-id").unwrap()["__fail"] = json!(true);
    let (status, error) = search(&app, input.clone()).await;
    assert_eq!(status, 502);
    assert!(error.get("data").is_none());
    assert!(!error.to_string().contains("CANARY"));
    {
        let mut books = p.books.lock().await;
        let b = books.get_mut("second-id").unwrap();
        b["__fail"] = json!(false);
        b["spreadsheetId"] = json!("wrong-id");
    }
    let (status, error) = search(&app, input.clone()).await;
    assert_eq!(status, 409);
    assert!(error.get("data").is_none());
    {
        let mut books = p.books.lock().await;
        let b = books.get_mut("second-id").unwrap();
        b["spreadsheetId"] = json!("second-id");
        b["__slow"] = json!(true);
    }
    let (status, error) =
        tokio::time::timeout(std::time::Duration::from_secs(6), search(&app, input))
            .await
            .expect("aggregate deadline must precede adapter timeout");
    assert_eq!(status, 502);
    assert_eq!(error["error"]["code"], "sheets_search_timeout");
    assert!(error.get("data").is_none());
    task.abort();
}

#[tokio::test]
async fn selected_views_reject_profile_projection_failure_beyond_the_current_page() {
    let (app, p, task) = fixture_with_late_profile(true).await;
    let (legacy_status, _) = search(
        &app,
        json!({"spreadsheet":"second","view":"all","query":"cat"}),
    )
    .await;
    assert_eq!(legacy_status, 409);
    let (status,error)=search(&app,json!({"targets":[{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"}],"query":"cat","limit":1})).await;
    assert_eq!(
        status, 409,
        "all selected projections must succeed even if first target fills page: {error}"
    );
    assert!(error.get("data").is_none());
    {
        let mut books = p.books.lock().await;
        let rows = books.get_mut("second-id").unwrap()["sheets"][0]["data"][0]["rowData"]
            .as_array_mut()
            .unwrap();
        rows.resize(16, json!({"values":[]}));
        for (index, row) in rows.iter_mut().enumerate().skip(2) {
            *row = json!({"values":[{"effectiveValue":{"stringValue":if index==2 {"cat item"} else {"Other"}}},{"effectiveValue":{"numberValue":50}}]});
        }
    }
    let (status,valid)=search(&app,json!({"targets":[{"spreadsheet":"first","view":"all"},{"spreadsheet":"second","view":"all"}],"query":"cat","limit":3})).await;
    assert_eq!(status, 200, "{valid}");
    assert_eq!(valid["data"]["records"].as_array().unwrap().len(), 3);
    assert_eq!(
        valid["data"]["records"][2]["fields"]["description"],
        "cat item"
    );
    task.abort();
}
