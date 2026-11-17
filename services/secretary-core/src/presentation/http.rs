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
    pub(super) approval_token: Arc<str>,
    pub(super) checkpoints: super::task_native::Checkpoints,
    read_tasks: Option<Arc<dyn crate::application::read_budget::ReadTaskLedger>>,
}
impl AppState {
    pub fn new(
        registry: Registry,
        token: String,
        owner: String,
        sheets: impl SpreadsheetPort + 'static,
    ) -> Result<Self, String> {
        Self::from_bus(CommandBus::new(registry, owner, sheets)?, token)
    }
    fn from_bus(bus: CommandBus, token: String) -> Result<Self, String> {
        if bus.is_read_only() && token.len() < 32 {
            return Err("read-only core token must have at least 32 bytes".into());
        }
        if token.len() < 12 {
            return Err("core token must have at least 12 bytes".into());
        }
        Ok(Self {
            bus,
            token: token.into(),
            approval_token: "".into(),
            checkpoints: Default::default(),
            read_tasks: None,
        })
    }
    /// Capability foundation only; this constructor does not attest host admission.
    pub fn new_read_only(
        registry: Registry,
        token: String,
        owner: String,
        source: impl SpreadsheetPort + 'static,
    ) -> Result<Self, String> {
        Self::from_bus(CommandBus::new_read_only(registry, owner, source)?, token)
    }
    pub fn with_writes(mut self, writes: Writes, approval_token: String) -> Result<Self, String> {
        if writes
            .task_owner()
            .is_some_and(|owner| owner != self.bus.owner.as_ref())
        {
            return Err("write-task ledger owner must match the configured Core owner".into());
        }
        if self.bus.is_read_only() && writes.enabled {
            return Err("read-only profile forbids enabled writes".into());
        }
        if self.bus.is_read_only()
            && (approval_token.len() < 32 || approval_token == self.token.as_ref())
        {
            return Err(
                "read-only recovery requires a distinct operator token of at least 32 bytes".into(),
            );
        }
        if writes.enabled && (approval_token.len() < 12 || approval_token == self.token.as_ref()) {
            return Err(
                "distinct approval token of at least 12 bytes required when writes enabled".into(),
            );
        }
        self.bus = self.bus.with_writes(writes);
        self.approval_token = approval_token.into();
        Ok(self)
    }

    /// Attach trusted cumulative read-task accounting and separate operator auth.
    ///
    /// # Examples
    /// ```no_run
    /// # use secretary_core::{AppState, application::read_budget::ReadTaskLedger};
    /// # fn attach(state: AppState, ledger: impl ReadTaskLedger + 'static, operator: String) -> Result<AppState, String> {
    /// state.with_read_tasks(ledger, operator)
    /// # }
    /// ```
    ///
    /// # Errors
    /// Rejects legacy/duplicate attachment or invalid/conflicting operator tokens.
    pub fn with_read_tasks(
        mut self,
        ledger: impl crate::application::read_budget::ReadTaskLedger + 'static,
        operator_token: String,
    ) -> Result<Self, String> {
        if !self.bus.is_read_only()
            || self.read_tasks.is_some()
            || operator_token.len() < 32
            || operator_token == self.token.as_ref()
            || (!self.approval_token.is_empty() && self.approval_token.as_ref() != operator_token)
        {
            return Err(
                "read tasks require read-only policy and the distinct configured operator token"
                    .into(),
            );
        }
        self.read_tasks = Some(Arc::new(ledger));
        self.approval_token = operator_token.into();
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
pub(super) fn authorized(provided: &str, token: &str) -> bool {
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
        "docs_list" => {
            let _: Empty = parse(input)?;
            Command::DocsList
        }
        "docs_describe" => Command::DocsDescribe(parse(input)?),
        "docs_read" => match parse(input)? {
            DocumentReadRequest::Tab(p) => Command::DocsRead(p),
            DocumentReadRequest::Summary(p) => Command::DocsSummary(p),
            DocumentReadRequest::Many(p) => Command::DocsSummaryMany(p),
        },
        "docs_search" => Command::DocsSearch(parse(input)?),
        "docs_preview_change" => Command::Mutation(MutationCommand::DocsPreview(parse(input)?)),
        "docs_apply" => Command::Mutation(MutationCommand::DocsApply(parse(input)?)),
        "operations_get_history" => Command::Mutation(MutationCommand::History(parse(input)?)),
        "sheets_list" => {
            let _: Empty = parse(input)?;
            Command::List
        }
        "sheets_describe" => Command::Describe(parse(input)?),
        "sheets_read" => match parse(input)? {
            SheetsReadRequest::Rows(p) => Command::Read(p),
            SheetsReadRequest::Summary(p) => Command::SheetSummary(p),
        },
        "sheets_search" => Command::Search(parse(input)?),
        "reports_wedding" => Command::Report(parse(input)?),
        "sheets_preview_create_tab" => {
            Command::Mutation(MutationCommand::TabPreview(parse(input)?))
        }
        "sheets_create_tab" => Command::Mutation(MutationCommand::TabCreate(parse(input)?)),
        "sheets_preview_change" => Command::Mutation(MutationCommand::Preview(parse(input)?)),
        "sheets_append" => Command::Mutation(MutationCommand::Append(parse(input)?)),
        "sheets_update" => Command::Mutation(MutationCommand::Update(parse(input)?)),
        "sheets_get_history" => Command::Mutation(MutationCommand::History(parse(input)?)),
        _ => return Err(CoreError(ErrorKind::NotFound, "unknown_tool")),
    })
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/v1/evidence-cleanup",
            post(super::evidence_cleanup::handle),
        )
        .route("/v1/write-tasks/native", post(super::task_native::control))
        .route("/v1/write-tasks/execute", post(super::task_native::execute))
        .route("/v1/write-tasks/confirm", post(super::task_native::confirm))
        .route("/v1/tools/{name}", post(tool))
        .route("/v1/operations/{id}/approval", post(operator_approval))
        .route("/v1/operations/{id}/reconcile", post(operator_reconcile))
        .route("/v1/read-budget/reserve", post(read_reserve))
        .route("/v1/read-budget/local-error", post(read_local_error))
        .route("/v1/read-budget/operator/{action}", post(read_operator))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(state)
}
// Authentication precedes input parsing. Actor always comes from operator configuration.
async fn tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<axum::response::Response, CoreError> {
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
    let grant = if let Some(ledger) = &state.read_tasks {
        state.bus.check_authority()?;
        let id = headers
            .get("x-ciel-read-reservation")
            .and_then(|value| value.to_str().ok())
            .ok_or(CoreError(ErrorKind::Forbidden, "read_reservation_required"))?;
        Some(ledger.claim(id, &name).await?)
    } else {
        None
    };
    let result = tool_result(&state, &name, body).await;
    if let Some(grant) = grant {
        let ledger = state.read_tasks.as_ref().ok_or(CoreError::invalid())?;
        let wire =
            crate::application::read_budget::settle(ledger.as_ref(), &grant, &name, result).await?;
        state.bus.check_authority()?;
        return Ok((
            [
                ("x-ciel-budgeted-result", "v1"),
                ("content-type", "application/vnd.ciel.read-budget+json"),
            ],
            Json(wire),
        )
            .into_response());
    }
    result.map(|data| Json(json!({"ok":true,"data":data})).into_response())
}

async fn tool_result(
    state: &AppState,
    name: &str,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Value, CoreError> {
    let body = match body {
        Ok(b) => b,
        Err(_) => {
            if let Some(w) = &state.bus.writes {
                w.journal.audit(None, "request_too_large").await?
            }
            return Err(CoreError(ErrorKind::PayloadTooLarge, "request_too_large"));
        }
    };
    let parsed = if state.read_tasks.is_some() {
        crate::infrastructure::config::decode::<Value>(&body).map_err(|_| ())
    } else {
        serde_json::from_slice::<Value>(&body).map_err(|_| ())
    };
    let input: Value = match parsed {
        Ok(v) => v,
        Err(_) => {
            if let Some(w) = &state.bus.writes {
                w.journal.audit(None, "invalid_input").await?
            }
            return Err(CoreError::invalid());
        }
    };
    let command = match decode_tool(name, input) {
        Ok(command) => command,
        Err(error) => {
            if let Some(writes) = &state.bus.writes {
                writes.journal.audit(None, error.1).await?;
            }
            return Err(error);
        }
    };
    state.bus.execute(command).await
}

fn read_ledger(
    state: &AppState,
) -> Result<&dyn crate::application::read_budget::ReadTaskLedger, CoreError> {
    state
        .read_tasks
        .as_deref()
        .ok_or(CoreError(ErrorKind::Forbidden, "read_budget_disabled"))
}

fn read_auth(state: &AppState, headers: &HeaderMap, operator: bool) -> Result<(), CoreError> {
    let token = if operator {
        state.approval_token.as_ref()
    } else {
        state.token.as_ref()
    };
    if !authorized(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or(""),
        token,
    ) {
        return Err(CoreError(ErrorKind::Unauthorized, "unauthorized"));
    }
    Ok(())
}

fn read_body<T: serde::de::DeserializeOwned>(
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<T, CoreError> {
    crate::infrastructure::config::decode(&body.map_err(|_| CoreError::invalid())?)
        .map_err(|_| CoreError::invalid())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadReserve {
    tool: String,
}

async fn read_reserve(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    read_auth(&state, &headers, false)?;
    state.bus.check_authority()?;
    let input: ReadReserve = read_body(body)?;
    let result = read_ledger(&state)?.reserve(&input.tool).await?;
    state.bus.check_authority()?;
    Ok(Json(
        serde_json::to_value(result).map_err(|_| CoreError::invalid())?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadLocalError {
    reservation: String,
    tool: String,
}

async fn read_local_error(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<axum::response::Response, CoreError> {
    read_auth(&state, &headers, false)?;
    state.bus.check_authority()?;
    let input: ReadLocalError = read_body(body)?;
    let ledger = read_ledger(&state)?;
    let grant = ledger.claim(&input.reservation, &input.tool).await?;
    let wire = crate::application::read_budget::settle(
        ledger,
        &grant,
        &input.tool,
        Err(CoreError::invalid()),
    )
    .await?;
    state.bus.check_authority()?;
    Ok((
        [
            ("x-ciel-budgeted-result", "v1"),
            ("content-type", "application/vnd.ciel.read-budget+json"),
        ],
        Json(wire),
    )
        .into_response())
}

async fn read_operator(
    State(state): State<AppState>,
    Path(action): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    read_auth(&state, &headers, true)?;
    state.bus.check_authority()?;
    let ledger = read_ledger(&state)?;
    let result = match action.as_str() {
        "create" => ledger.create(read_body(body)?).await?,
        "increase" => ledger.increase(read_body(body)?).await?,
        "close" | "status" => {
            let _: Empty = read_body(body)?;
            if action == "close" {
                ledger.close().await?
            } else {
                ledger.status().await?
            }
        }
        _ => return Err(CoreError::invalid()),
    };
    state.bus.check_authority()?;
    Ok(Json(result))
}
pub(super) fn parse<T: serde::de::DeserializeOwned>(input: Value) -> Result<T, CoreError> {
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
