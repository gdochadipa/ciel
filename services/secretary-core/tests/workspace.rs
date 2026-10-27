//! Public bus/HTTP tests for generic grids and Docs using real journal persistence.
use secretary_core::{
    application::{
        commands::{Approval, Command, Commit, DocumentRead, MutationCommand},
        ports::{PortFuture, WorkspacePort, WriteOutcome},
    },
    domain::{
        documents::DocChange,
        operation::{Preview, ResourceKind},
        registry::{Document, Spreadsheet, View},
    },
    journal::Journal,
    router,
    writes::Writes,
    AppState, Registry,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[derive(Clone)]
struct Source {
    document: Arc<tokio::sync::Mutex<Value>>,
    sends: Arc<AtomicUsize>,
}
fn fixture() -> Value {
    json!({"documentId":"demo-document","title":"Independent notes","revisionId":"rev-1","tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},"documentTab":{"body":{"content":[{"sectionBreak":{}},{"paragraph":{"elements":[{"textRun":{"content":"Hello sample\n"}}]}}]}}},{"tabProperties":{"tabId":"t.1","title":"Other"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"Other content\n"}}]}}]}}}]})
}
impl WorkspacePort for Source {
    fn read<'a>(
        &'a self,
        _s: &'a Spreadsheet,
    ) -> PortFuture<'a, Result<Value, secretary_core::CoreError>> {
        Box::pin(async {
            Ok(
                json!({"spreadsheetId":"generic-spreadsheet","sheets":[{"properties":{"sheetId":7,"title":"Operations"},"data":[{"startRow":1,"startColumn":1,"rowData":[{"values":[{"effectiveValue":{"stringValue":"Independent"}},{"effectiveValue":{"numberValue":12}},{"effectiveValue":{"boolValue":true}}]},{"values":[{"effectiveValue":{"stringValue":"Another"}},{"userEnteredValue":{"formulaValue":"=2*3"},"effectiveValue":{"numberValue":6}}]}]}]}]}),
            )
        })
    }
    fn write_values<'a>(
        &'a self,
        _s: &'a Spreadsheet,
        _v: &'a View,
        _p: &'a Preview,
        _row: Option<usize>,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async { panic!("generic grids are read-only") })
    }
    fn read_document<'a>(
        &'a self,
        _d: &'a Document,
    ) -> PortFuture<'a, Result<Value, secretary_core::CoreError>> {
        Box::pin(async { Ok(self.document.lock().await.clone()) })
    }
    fn write_document<'a>(
        &'a self,
        _d: &'a Document,
        p: &'a Preview,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async move {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let mut doc = self.document.lock().await;
            assert_eq!(doc["revisionId"], p.source_revision);
            assert_eq!(p.resource_kind, ResourceKind::GoogleDocs);
            let tab = doc["tabs"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|t| t["tabProperties"]["tabId"] == p.view)
                .unwrap();
            tab["documentTab"]["body"]["content"] =
                json!([{"paragraph":{"elements":[{"textRun":{"content":p.after["text"]}}]}}]);
            doc["revisionId"] = json!("rev-2");
            WriteOutcome::Accepted
        })
    }
}
async fn state() -> (AppState, Source) {
    let registry: Registry = serde_json::from_str(include_str!(
        "../../../postman/fixtures/registry-workspace.json"
    ))
    .unwrap();
    let source = Source {
        document: Arc::new(tokio::sync::Mutex::new(fixture())),
        sends: Arc::new(AtomicUsize::new(0)),
    };
    let journal = Journal::open(
        &std::env::temp_dir().join(format!("secretary-workspace-{}", uuid::Uuid::new_v4())),
    )
    .await
    .unwrap();
    (
        AppState::new(
            registry,
            "business-token-secret".into(),
            "owner".into(),
            source.clone(),
        )
        .unwrap()
        .with_writes(Writes::new(journal, true), "operator-token-secret".into())
        .unwrap(),
        source,
    )
}
fn change(id: &str) -> DocChange {
    DocChange {
        operation_id: id.into(),
        document: "notes".into(),
        tab_id: "t.0".into(),
        action: "append_text".into(),
        fields: json!({"text":"Added"}),
    }
}
#[tokio::test]
async fn generic_grid_has_no_wedding_headers_or_fixed_template_and_keeps_source_types() {
    let (state, _) = state().await;
    let result = state
        .bus
        .execute(Command::Read(secretary_core::application::commands::Read {
            spreadsheet: "operations".into(),
            view: "overview".into(),
            limit: Some(2),
            cursor: None,
        }))
        .await
        .unwrap();
    assert_eq!(
        result["records"][0]["fields"],
        json!({"B":"Independent","C":12,"D":true})
    );
    assert_eq!(result["records"][1]["fields"]["C"], 6);
    assert_eq!(
        result["records"][0]["source"]["range"],
        "'Operations'!B2:D2"
    );
    assert!(state
        .bus
        .execute(Command::Report(
            secretary_core::application::commands::Describe {
                spreadsheet: "operations".into()
            }
        ))
        .await
        .is_err());
    assert!(state
        .bus
        .execute(Command::Mutation(MutationCommand::Preview(
            secretary_core::write_model::Change {
                operation_id: "no-grid-write".into(),
                spreadsheet: "operations".into(),
                view: "overview".into(),
                action: "append".into(),
                record_id: None,
                fields: json!({"text":"No"})
            }
        )))
        .await
        .is_err());
}
#[tokio::test]
async fn docs_preview_approval_commit_is_shared_fsm_and_replay_does_not_send_twice() {
    let (state, source) = state().await;
    let p = state
        .bus
        .execute(Command::Mutation(MutationCommand::DocsPreview(change(
            "docs-op",
        ))))
        .await
        .unwrap();
    assert_eq!(p["document"], "notes");
    assert_eq!(p["tab_id"], "t.0");
    assert!(p.get("spreadsheet").is_none());
    assert!(state
        .bus
        .execute(Command::Mutation(MutationCommand::DocsApply(Commit {
            operation_id: "docs-op".into()
        })))
        .await
        .is_err());
    state
        .bus
        .execute(Command::Approve {
            operation_id: "docs-op".into(),
            approval: Approval {
                decision: "approve".into(),
                preview_digest: p["preview_digest"].as_str().unwrap().into(),
            },
        })
        .await
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            state
                .bus
                .execute(Command::Mutation(MutationCommand::DocsApply(Commit {
                    operation_id: "docs-op".into()
                })))
                .await
                .unwrap()["state"],
            "applied"
        );
    }
    assert_eq!(source.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        source.document.lock().await["tabs"][1]["documentTab"]["body"]["content"][0]["paragraph"]
            ["elements"][0]["textRun"]["content"],
        "Other content\n"
    );
}
#[tokio::test]
async fn docs_cursor_binds_revision_and_conflict_never_reaches_writer() {
    let (state, source) = state().await;
    let read = state
        .bus
        .execute(Command::DocsRead(DocumentRead {
            document: "notes".into(),
            tab_id: "t.0".into(),
            limit: Some(2),
            cursor: None,
        }))
        .await
        .unwrap();
    let p = state
        .bus
        .execute(Command::Mutation(MutationCommand::DocsPreview(change(
            "stale-op",
        ))))
        .await
        .unwrap();
    state
        .bus
        .execute(Command::Approve {
            operation_id: "stale-op".into(),
            approval: Approval {
                decision: "approve".into(),
                preview_digest: p["preview_digest"].as_str().unwrap().into(),
            },
        })
        .await
        .unwrap();
    source.document.lock().await["revisionId"] = json!("external-revision");
    assert!(state
        .bus
        .execute(Command::DocsRead(DocumentRead {
            document: "notes".into(),
            tab_id: "t.0".into(),
            limit: Some(2),
            cursor: Some(read["next_cursor"].as_str().unwrap().into())
        }))
        .await
        .is_err());
    assert_eq!(
        state
            .bus
            .execute(Command::Mutation(MutationCommand::DocsApply(Commit {
                operation_id: "stale-op".into()
            })))
            .await
            .unwrap()["state"],
        "conflict"
    );
    assert_eq!(source.sends.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn public_docs_tools_reject_unregistered_resources_and_unknown_authority_fields() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (state, _) = state().await;
    let app = router(state);
    for (token, input, status) in [
        ("wrong", json!({"document":"notes"}), 401),
        (
            "business-token-secret",
            json!({"document":"not-registered"}),
            404,
        ),
        (
            "business-token-secret",
            json!({"document":"notes","actor":"owner"}),
            400,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/v1/tools/docs_describe")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
    }
}
#[test]
fn legacy_sheet_preview_digest_and_json_remain_compatible() {
    let old = json!({"operation_id":"old-op","actor":"owner","spreadsheet":"legacy","view":"table","action":"append","record_id":"record","fields":{"title":"Old"},"before":null,"after":{"title":"Old"},"mapping_version":"v1","source_revision":"old","expires_at":99,"preview_digest":"digest"});
    let p: Preview = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(p.resource_kind, ResourceKind::Sheets);
    assert_eq!(
        p.request_digest(),
        "8b7891e27dade133e26586f5e5364c5af20f8c7525ac515c565639c9cce4d518"
    );
    assert_eq!(serde_json::to_value(p).unwrap(), old);
}
#[test]
fn docs_edit_rejects_ambiguous_text_tables_suggestions_and_missing_revision() {
    use secretary_core::domain::documents::{prepare, snapshot};
    let doc = Document {
        alias: "notes".into(),
        id: "demo-document".into(),
        readable: true,
        writable: true,
    };
    let mut c = change("strict-op");
    c.action = "replace_text".into();
    c.fields = json!({"find":"missing","replacement":"x"});
    assert!(prepare(
        &c,
        &doc,
        &snapshot(&doc, &fixture()).unwrap(),
        "owner",
        "v1",
        100
    )
    .is_err());
    for alteration in ["suggestions", "missing_revision", "table", "header"] {
        let mut value = fixture();
        match alteration {
            "suggestions" => {
                value["tabs"][0]["documentTab"]["body"]["content"][1]["paragraph"]["elements"][0]
                    ["suggestedInsertionIds"] = json!(["suggestion"])
            }
            "missing_revision" => value["revisionId"] = Value::Null,
            "table" => {
                value["tabs"][0]["documentTab"]["body"]["content"][1] =
                    json!({"table":{"tableRows":[]}})
            }
            _ => value["tabs"][0]["documentTab"]["headers"] = json!({"header":{"content":[]}}),
        }
        assert!(
            prepare(
                &change("strict-op"),
                &doc,
                &snapshot(&doc, &value).unwrap(),
                "owner",
                "v1",
                100
            )
            .is_err(),
            "{alteration}"
        );
    }
}
