mod common;
use common::fixture;
use secretary_core::{
    application::{
        bus::CommandBus,
        commands::{Command, Read},
        ports::{PortFuture, SpreadsheetPort, WriteOutcome},
    },
    domain::{
        error::CoreError,
        operation::Preview,
        registry::{Spreadsheet, View},
    },
};
use serde_json::Value;
struct FixtureSource(Value);
impl SpreadsheetPort for FixtureSource {
    fn read<'a>(&'a self, _: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async { Ok(self.0.clone()) })
    }
    fn write_values<'a>(
        &'a self,
        _: &'a Spreadsheet,
        _: &'a View,
        _: &'a Preview,
        _: Option<usize>,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async { panic!("read commands must not send mutations") })
    }
}
#[tokio::test]
async fn typed_read_commands_preserve_authoritative_records_and_reject_unregistered_sources() {
    let (registry, book) = fixture();
    let bus = CommandBus::new(registry, "configured-owner".into(), FixtureSource(book)).unwrap();
    let listed = bus.execute(Command::List).await.unwrap();
    assert_eq!(listed["spreadsheets"][0]["spreadsheet"], "wedding-demo");
    let result = bus
        .execute(Command::Read(Read {
            spreadsheet: "wedding-demo".into(),
            view: "wedding_costs".into(),
            limit: Some(100),
            cursor: None,
        }))
        .await
        .unwrap();
    assert_eq!(result["records"].as_array().unwrap().len(), 46);
    assert_eq!(result["records"][0]["fields"]["cost"]["amount"], 100);
    assert!(bus
        .execute(Command::Read(Read {
            spreadsheet: "forbidden".into(),
            view: "wedding_costs".into(),
            limit: None,
            cursor: None
        }))
        .await
        .is_err());
}

#[tokio::test]
async fn public_http_dispatch_preserves_authentication_input_rejection_and_read_only_policy() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::json;
    use tower::ServiceExt;
    let (registry, book) = fixture();
    let app = secretary_core::router(
        secretary_core::AppState::new(
            registry,
            "synthetic-core-token".into(),
            "configured-owner".into(),
            FixtureSource(book),
        )
        .unwrap(),
    );
    for (tool, token, input, status, code) in [
        (
            "sheets_list",
            "wrong-token",
            json!({}),
            401,
            Some("unauthorized"),
        ),
        (
            "sheets_list",
            "synthetic-core-token",
            json!({"actor":"forged"}),
            400,
            Some("invalid_input"),
        ),
        (
            "sheets_append",
            "synthetic-core-token",
            json!({"operation_id":"test-op"}),
            403,
            Some("writes_disabled"),
        ),
        ("sheets_list", "synthetic-core-token", json!({}), 200, None),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/tools/{tool}"))
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        if let Some(code) = code {
            assert_eq!(body["error"]["code"], code);
        } else {
            assert_eq!(
                body["data"]["spreadsheets"][0]["spreadsheet"],
                "wedding-demo"
            );
        }
    }
}

#[tokio::test]
async fn denied_typed_mutation_cannot_reach_source_and_history_remains_available() {
    use secretary_core::{
        application::{
            commands::{Approval, Commit, History, MutationCommand},
            writes::Writes,
        },
        journal::Journal,
        write_model::Change,
    };
    use serde_json::json;
    let (registry, book) = fixture();
    let dir = std::env::temp_dir().join(format!("secretary-bus-{}", uuid::Uuid::new_v4()));
    let journal = Journal::open(&dir).await.unwrap();
    let writes = Writes::new(journal, true);
    let bus = CommandBus::new(registry, "configured-owner".into(), FixtureSource(book))
        .unwrap()
        .with_writes(writes);
    let change:Change=serde_json::from_value(json!({"operation_id":"denied-op","spreadsheet":"wedding-demo","view":"tasks","action":"append","fields":{"title":"Sample task","status":"todo"}})).unwrap();
    let preview = bus
        .execute(Command::Mutation(MutationCommand::Preview(change)))
        .await
        .unwrap();
    bus.execute(Command::Approve {
        operation_id: "denied-op".into(),
        approval: Approval {
            decision: "reject".into(),
            preview_digest: preview["preview_digest"].as_str().unwrap().into(),
        },
    })
    .await
    .unwrap();
    assert!(bus
        .execute(Command::Mutation(MutationCommand::Append(Commit {
            operation_id: "denied-op".into()
        })))
        .await
        .is_err());
    let history = bus
        .execute(Command::Mutation(MutationCommand::History(History {
            operation_id: Some("denied-op".into()),
            limit: None,
            cursor: None,
        })))
        .await
        .unwrap();
    assert_eq!(history["operation"]["state"], "rejected");
}

#[tokio::test]
async fn http_operator_decisions_require_the_distinct_operator_credential() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        Router,
    };
    use secretary_core::{journal::Journal, router, writes::Writes, AppState};
    use serde_json::json;
    use tower::ServiceExt;
    async fn post(app: &Router, path: &str, token: &str, body: Value) -> (u16, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status().as_u16();
        (
            status,
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap(),
        )
    }
    let (registry, book) = fixture();
    let dir =
        std::env::temp_dir().join(format!("secretary-http-dispatch-{}", uuid::Uuid::new_v4()));
    let journal = Journal::open(&dir).await.unwrap();
    let state = AppState::new(
        registry,
        "synthetic-core-token".into(),
        "configured-owner".into(),
        FixtureSource(book),
    )
    .unwrap();
    assert!(state
        .clone()
        .with_writes(
            Writes::new(journal.clone(), true),
            "synthetic-core-token".into()
        )
        .is_err());
    let app = router(
        state
            .with_writes(
                Writes::new(journal, true),
                "synthetic-operator-token".into(),
            )
            .unwrap(),
    );
    let (status,preview)=post(&app,"/v1/tools/sheets_preview_change","synthetic-core-token",json!({"operation_id":"operator-op","spreadsheet":"wedding-demo","view":"tasks","action":"append","fields":{"title":"Sample task","status":"todo"}})).await;
    assert_eq!(status, 200);
    let decision = json!({"decision":"reject","preview_digest":preview["data"]["preview_digest"]});
    assert_eq!(
        post(
            &app,
            "/v1/operations/operator-op/approval",
            "synthetic-core-token",
            decision.clone()
        )
        .await
        .0,
        401
    );
    let (status, result) = post(
        &app,
        "/v1/operations/operator-op/approval",
        "synthetic-operator-token",
        decision,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["data"]["state"], "rejected");
    assert_eq!(
        post(
            &app,
            "/v1/tools/sheets_append",
            "synthetic-core-token",
            json!({"operation_id":"operator-op"})
        )
        .await
        .0,
        409
    );
}
