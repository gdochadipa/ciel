use crate::{
    application::{bus::CommandBus, commands::*, ports::SpreadsheetPort, writes::Writes},
    domain::{
        error::{CoreError, ErrorKind},
        registry::Registry,
        write_model,
    },
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Clone)]
pub struct AppState {
    pub bus: CommandBus,
    token: Arc<str>,
    approval_token: Arc<str>,
}
impl AppState {
    pub fn new(
        registry: Registry,
        token: String,
        owner: String,
        sheets: impl SpreadsheetPort + 'static,
    ) -> Result<Self, String> {
        if token.len() < 12 {
            return Err("core token must have at least 12 bytes".into());
        }
        Ok(Self {
            bus: CommandBus::new(registry, owner, sheets)?,
            token: token.into(),
            approval_token: "".into(),
        })
    }
    pub fn with_writes(mut self, writes: Writes, approval_token: String) -> Result<Self, String> {
        if writes.enabled && (approval_token.len() < 12 || approval_token == self.token.as_ref()) {
            return Err(
                "distinct approval token of at least 12 bytes required when writes enabled".into(),
            );
        }
        self.bus = self.bus.with_writes(writes);
        self.approval_token = approval_token.into();
        Ok(self)
    }
}
impl IntoResponse for CoreError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::ProviderUnavailable => StatusCode::BAD_GATEWAY,
            ErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        };
        (status,Json(json!({"ok":false,"error":{"code":self.1,"message":self.1,"retryable":self.0==ErrorKind::ProviderUnavailable}}))).into_response()
    }
}
fn authorized(provided: &str, token: &str) -> bool {
    if token.len() < 12 {
        return false;
    }
    use sha2::{Digest, Sha256};
    let a = Sha256::digest(provided.as_bytes());
    let b = Sha256::digest(format!("Bearer {token}").as_bytes());
    a.iter().zip(b.iter()).fold(0u8, |n, (a, b)| n | (a ^ b)) == 0
}
fn decode_tool(name: &str, input: Value) -> Result<Command, CoreError> {
    Ok(match name {
        "sheets_list" => {
            let _: Empty = parse(input)?;
            Command::List
        }
        "sheets_describe" => Command::Describe(parse(input)?),
        "sheets_read" => Command::Read(parse(input)?),
        "sheets_search" => Command::Search(parse(input)?),
        "reports_wedding" => Command::Report(parse(input)?),
        "sheets_preview_change" => Command::Mutation(MutationCommand::Preview(parse(input)?)),
        "sheets_append" => Command::Mutation(MutationCommand::Append(parse(input)?)),
        "sheets_update" => Command::Mutation(MutationCommand::Update(parse(input)?)),
        "sheets_get_history" => Command::Mutation(MutationCommand::History(parse(input)?)),
        _ => return Err(CoreError(ErrorKind::NotFound, "unknown_tool")),
    })
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/tools/{name}", post(tool))
        .route("/v1/operations/{id}/approval", post(operator_approval))
        .route("/v1/operations/{id}/reconcile", post(operator_reconcile))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(state)
}
// Authentication precedes input parsing. Actor always comes from operator configuration.
async fn tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    let expected = format!("Bearer {}", state.token);
    let provided = headers
        .get("authorization")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    use sha2::{Digest, Sha256};
    let a = Sha256::digest(provided.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    let diff = a.iter().zip(b.iter()).fold(0u8, |n, (x, y)| n | (x ^ y));
    if diff != 0 {
        if let Some(writes) = &state.bus.writes {
            writes.journal.audit(None, "denied_core_auth").await?;
        }
        return Err(CoreError(ErrorKind::Unauthorized, "unauthorized"));
    }
    let body = match body {
        Ok(b) => b,
        Err(_) => {
            if let Some(w) = &state.bus.writes {
                w.journal.audit(None, "request_too_large").await?
            }
            return Err(CoreError(ErrorKind::PayloadTooLarge, "request_too_large"));
        }
    };
    let input: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            if let Some(w) = &state.bus.writes {
                w.journal.audit(None, "invalid_input").await?
            }
            return Err(CoreError::invalid());
        }
    };
    let command = match decode_tool(&name, input) {
        Ok(command) => command,
        Err(error) => {
            if let Some(writes) = &state.bus.writes {
                writes.journal.audit(None, error.1).await?;
            }
            return Err(error);
        }
    };
    let data = state.bus.execute(command).await?;
    Ok(Json(json!({"ok":true,"data":data})))
}
fn parse<T: serde::de::DeserializeOwned>(input: Value) -> Result<T, CoreError> {
    serde_json::from_value(input).map_err(|_| CoreError::invalid())
}
async fn operator_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    operator(&state, &id, headers, body, false).await
}
async fn operator_reconcile(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    operator(&state, &id, headers, body, true).await
}
async fn operator(
    state: &AppState,
    id: &str,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
    reconcile: bool,
) -> Result<Json<Value>, CoreError> {
    let writes = state
        .bus
        .writes
        .as_ref()
        .ok_or(CoreError(ErrorKind::Forbidden, "writes_disabled"))?;
    if !authorized(
        headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or(""),
        &state.approval_token,
    ) {
        writes.journal.audit(None, "denied_operator_auth").await?;
        return Err(CoreError(ErrorKind::Unauthorized, "unauthorized"));
    }
    if !write_model::valid_id(id) {
        writes.journal.audit(None, "invalid_operation_id").await?;
        return Err(CoreError::invalid());
    }
    let body = match body {
        Ok(b) => b,
        Err(_) => {
            writes.journal.audit(None, "request_too_large").await?;
            return Err(CoreError(ErrorKind::PayloadTooLarge, "request_too_large"));
        }
    };
    let input = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            writes.journal.audit(None, "invalid_input").await?;
            return Err(CoreError::invalid());
        }
    };
    let decoded: Result<Command, CoreError> = if reconcile {
        parse::<Empty>(input).map(|_| Command::Reconcile {
            operation_id: id.into(),
        })
    } else {
        parse(input).map(|approval| Command::Approve {
            operation_id: id.into(),
            approval,
        })
    };
    let command = match decoded {
        Ok(command) => command,
        Err(error) => {
            writes.journal.audit(Some(id), error.1).await?;
            return Err(error);
        }
    };
    let data = match state.bus.execute(command).await {
        Ok(v) => v,
        Err(e) => {
            writes.journal.audit(Some(id), e.1).await?;
            return Err(e);
        }
    };
    Ok(Json(json!({"ok":true,"data":data})))
}
