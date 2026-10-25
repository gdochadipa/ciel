use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0,Json(json!({"ok":false,"error":{"code":self.1,"message":self.1,"retryable":self.0==StatusCode::BAD_GATEWAY}}))).into_response()
    }
}
impl ApiError {
    pub fn invalid() -> Self {
        Self(StatusCode::BAD_REQUEST, "invalid_input")
    }
    pub fn drift() -> Self {
        Self(StatusCode::CONFLICT, "schema_drift")
    }
    pub fn upstream() -> Self {
        Self(StatusCode::BAD_GATEWAY, "provider_unavailable")
    }
}
