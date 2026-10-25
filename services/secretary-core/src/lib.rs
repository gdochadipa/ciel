pub mod google;
pub mod projection;
pub mod registry;
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
pub use google::GoogleClient;
pub use registry::Registry;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<Registry>,
    pub google: GoogleClient,
    token: Arc<str>,
    pub owner: Arc<str>,
}
impl AppState {
    pub fn new(
        registry: Registry,
        token: String,
        owner: String,
        google: GoogleClient,
    ) -> Result<Self, String> {
        registry.validate()?;
        if token.len() < 12 || owner.is_empty() {
            return Err(
                "core token must have at least 12 bytes and owner must be configured".into(),
            );
        }
        Ok(Self {
            registry: Arc::new(registry),
            google,
            token: token.into(),
            owner: owner.into(),
        })
    }
}
pub mod error;
pub use error::ApiError;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Describe {
    spreadsheet: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    spreadsheet: String,
    view: String,
    limit: Option<usize>,
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    spreadsheet: String,
    view: String,
    query: String,
    limit: Option<usize>,
    cursor: Option<String>,
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/tools/{name}", post(tool))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(state)
}
// Authentication precedes input parsing. Actor always comes from operator configuration.
async fn tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, ApiError> {
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
        return Err(ApiError(StatusCode::UNAUTHORIZED, "unauthorized"));
    }
    let body = body.map_err(|_| ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request_too_large"))?;
    let input: Value = serde_json::from_slice(&body).map_err(|_| ApiError::invalid())?;
    let data = match name.as_str() {
        "sheets_list" => {
            let _: Empty = parse(input)?;
            json!({"spreadsheets":state.registry.spreadsheets.iter().filter(|s|s.readable).map(|s|json!({"spreadsheet":s.alias,"id":s.id,"readable":true,"writable":false})).collect::<Vec<_>>(),"mapping_version":state.registry.version})
        }
        "sheets_describe" => {
            let p: Describe = parse(input)?;
            let s = state.registry.lookup(&p.spreadsheet)?;
            json!({"spreadsheet":s.alias,"id":s.id,"mapping_version":state.registry.version,"views":s.views,"writable":false})
        }
        "sheets_read" => {
            let p: Read = parse(input)?;
            read(&state, &p.spreadsheet, &p.view, None, p.limit, p.cursor).await?
        }
        "sheets_search" => {
            let p: Search = parse(input)?;
            if p.query.is_empty() || p.query.len() > 256 {
                return Err(ApiError::invalid());
            }
            read(
                &state,
                &p.spreadsheet,
                &p.view,
                Some(p.query),
                p.limit,
                p.cursor,
            )
            .await?
        }
        "reports_wedding" => {
            let p: Describe = parse(input)?;
            let s = state.registry.lookup(&p.spreadsheet)?;
            let workbook = state.google.read(s).await?;
            projection::validate(s, &workbook)?;
            projection::report(s, &workbook, &state.registry.version)?
        }
        "sheets_preview_change" | "sheets_append" | "sheets_update" | "sheets_get_history" => {
            return Err(ApiError(StatusCode::FORBIDDEN, "writes_disabled"))
        }
        _ => return Err(ApiError(StatusCode::NOT_FOUND, "unknown_tool")),
    };
    Ok(Json(json!({"ok":true,"data":data})))
}
fn parse<T: serde::de::DeserializeOwned>(input: Value) -> Result<T, ApiError> {
    serde_json::from_value(input).map_err(|_| ApiError::invalid())
}
async fn read(
    state: &AppState,
    spreadsheet: &str,
    view: &str,
    query: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
) -> Result<Value, ApiError> {
    let limit = limit.unwrap_or(50);
    if limit == 0 || limit > 100 {
        return Err(ApiError::invalid());
    }
    let s = state.registry.lookup(spreadsheet)?;
    let v = s
        .views
        .iter()
        .find(|v| v.name == view)
        .ok_or_else(ApiError::invalid)?;
    let workbook = state.google.read(s).await?;
    projection::validate(s, &workbook)?;
    let fingerprint = projection::fingerprint(&workbook);
    // Cursor binds the source revision and full query, preventing pagination over changed data.
    use sha2::{Digest, Sha256};
    let binding = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}|{}|{}|{}",
                s.id,
                view,
                query.as_deref().unwrap_or(""),
                state.registry.version,
                fingerprint
            )
            .as_bytes()
        )
    );
    let offset = if let Some(c) = cursor {
        let (b, n) = c.split_once(':').ok_or_else(ApiError::invalid)?;
        if b != binding {
            return Err(ApiError(StatusCode::CONFLICT, "stale_cursor"));
        }
        n.parse::<usize>().map_err(|_| ApiError::invalid())?
    } else {
        0
    };
    let mut records = projection::project(s, v, &workbook, &state.registry.version)?;
    if let Some(q) = query {
        let q = q.to_lowercase();
        records.retain(|r| literal_match(&r["fields"], &q));
    }
    if offset > records.len() {
        return Err(ApiError::invalid());
    }
    let end = offset.saturating_add(limit).min(records.len());
    Ok(
        json!({"records":records[offset..end],"next_cursor":if end<records.len(){Some(format!("{binding}:{end}"))}else{None},"source_revision":fingerprint,"mapping_version":state.registry.version,"retrieved_at":chrono::Utc::now().to_rfc3339()}),
    )
}

fn literal_match(value: &Value, query: &str) -> bool {
    match value {
        Value::String(s) => s.to_lowercase().contains(query),
        Value::Number(n) => n.to_string().contains(query),
        Value::Array(a) => a.iter().any(|v| literal_match(v, query)),
        Value::Object(o) => o.values().any(|v| literal_match(v, query)),
        _ => false,
    }
}
