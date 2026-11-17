//! Authenticated owner maintenance, intentionally absent from model tool routes.
use super::http::{authorized, AppState};
use crate::{
    application::commands::Command,
    domain::error::{CoreError, ErrorKind},
};
use axum::{extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};
// The operator credential authenticates before parsing; a model Core bearer and
// an approval-shaped JSON body can neither propose nor authorize removal.
pub(super) async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !authorized(token, &state.approval_token) {
        return Err(CoreError(ErrorKind::Unauthorized, "unauthorized"));
    }
    let command = serde_json::from_slice(&body.map_err(|_| CoreError::invalid())?)
        .map_err(|_| CoreError::invalid())?;
    Ok(Json(
        json!({"ok":true,"data":state.bus.execute(Command::Cleanup(command)).await?}),
    ))
}
