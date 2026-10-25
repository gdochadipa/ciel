use crate::{
    google::WriteOutcome,
    journal::Journal,
    projection,
    write_model::{self, Change},
    ApiError, AppState,
};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
pub struct Writes {
    pub journal: Journal,
    pub enabled: bool,
    approval_token: Arc<str>,
    writer: tokio::sync::Mutex<()>,
}
impl Writes {
    pub fn new(
        journal: Journal,
        enabled: bool,
        approval_token: String,
        core_token: &str,
    ) -> Result<Self, String> {
        if enabled && (approval_token.len() < 12 || approval_token == core_token) {
            return Err(
                "distinct approval token of at least 12 bytes required when writes enabled".into(),
            );
        }
        Ok(Self {
            journal,
            enabled,
            approval_token: approval_token.into(),
            writer: tokio::sync::Mutex::new(()),
        })
    }
    pub fn authorize_operator(&self, provided: &str) -> bool {
        if self.approval_token.len() < 12 {
            return false;
        }
        use sha2::{Digest, Sha256};
        let a = Sha256::digest(provided.as_bytes());
        let b = Sha256::digest(format!("Bearer {}", self.approval_token).as_bytes());
        a.iter().zip(b.iter()).fold(0u8, |n, (a, b)| n | (a ^ b)) == 0
    }
    pub async fn tool(
        &self,
        state: &AppState,
        name: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        let result = self.dispatch(state, name, input).await;
        if let Err(e) = &result {
            self.journal.audit(None, e.1).await?;
        }
        result
    }
    async fn dispatch(
        &self,
        state: &AppState,
        name: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        if name == "sheets_get_history" {
            let p: History = serde_json::from_value(input).map_err(|_| ApiError::invalid())?;
            let cursor = p
                .cursor
                .map(|c| c.parse::<usize>().map_err(|_| ApiError::invalid()))
                .transpose()?
                .unwrap_or(0);
            return self
                .journal
                .history(p.operation_id.as_deref(), p.limit.unwrap_or(50), cursor)
                .await;
        }
        if !self.enabled {
            return Err(ApiError(StatusCode::FORBIDDEN, "writes_disabled"));
        }
        let _guard = self.writer.lock().await;
        match name {
            "sheets_preview_change" => {
                let c: Change = serde_json::from_value(input).map_err(|_| ApiError::invalid())?;
                if !write_model::valid_id(&c.operation_id) {
                    return Err(ApiError::invalid());
                }
                if let Some(old) = self.journal.find(&c.operation_id).await? {
                    if !write_model::matches_request(&old.preview, &c, &state.owner) {
                        return Err(ApiError(StatusCode::CONFLICT, "operation_conflict"));
                    }
                    return Ok(old.preview.public(&old.state));
                }
                let s = state.registry.lookup(&c.spreadsheet)?;
                let v = s
                    .views
                    .iter()
                    .find(|v| v.name == c.view)
                    .ok_or_else(ApiError::invalid)?;
                write_model::validate_fields(v, &c.fields, c.action == "append")?;
                let book = state.google.read(s).await?;
                projection::validate(s, &book)?;
                let preview =
                    write_model::prepare(&c, &state.owner, s, v, &book, &state.registry.version)?;
                let p = self.journal.create(preview).await?;
                Ok(p.public("previewed"))
            }
            "sheets_append" | "sheets_update" => {
                let p: Commit = serde_json::from_value(input).map_err(|_| ApiError::invalid())?;
                if !write_model::valid_id(&p.operation_id) {
                    return Err(ApiError::invalid());
                }
                let old = self.journal.get(&p.operation_id).await?;
                let action = if name == "sheets_append" {
                    "append"
                } else {
                    "update"
                };
                if old.preview.actor.as_str() != state.owner.as_ref()
                    || old.preview.action != action
                {
                    return Err(ApiError(StatusCode::CONFLICT, "operation_action_mismatch"));
                }
                if old.state == "applied" {
                    return Ok(old.preview.public("applied"));
                }
                let operation = self.journal.begin_apply(&p.operation_id).await?;
                let outcome = self.apply(state, &operation.preview).await;
                let finished = self.journal.finish(&p.operation_id, outcome).await?;
                Ok(finished.preview.public(&finished.state))
            }
            _ => Err(ApiError(StatusCode::NOT_FOUND, "unknown_tool")),
        }
    }
    async fn apply(&self, state: &AppState, p: &crate::journal::Preview) -> &'static str {
        if p.mapping_version != state.registry.version {
            return "conflict";
        }
        let s = match state.registry.lookup(&p.spreadsheet) {
            Ok(s) => s,
            Err(_) => return "conflict",
        };
        let v = match s
            .views
            .iter()
            .find(|v| v.name == p.view && v.structured() && v.write_schema.is_some())
        {
            Some(v) => v,
            None => return "conflict",
        };
        let book = match state.google.read(s).await {
            Ok(b) => b,
            Err(_) => return "known_failure",
        };
        if projection::validate(s, &book).is_err()
            || projection::fingerprint(&book) != p.source_revision
        {
            return "conflict";
        }
        let rows = match write_model::rows(s, v, &book) {
            Ok(rows) => rows,
            Err(_) => return "conflict",
        };
        // Preflight detects duplicate identities and retains the exact approved target after reordering.
        let row = if p.action == "update" {
            match rows.iter().find(|r| r.record_id == p.record_id) {
                Some(r) if r.fields == p.before => Some(r.row),
                _ => return "conflict",
            }
        } else {
            if rows.iter().any(|r| {
                r.record_id == p.record_id || r.operation_id == p.operation_id || r.row >= 1001
            }) {
                return "conflict";
            }
            None
        };
        match state.google.write_values(s, v, p, row).await {
            WriteOutcome::Rejected => "known_failure",
            WriteOutcome::Uncertain => "uncertain",
            WriteOutcome::Accepted => self.observe(state, p).await,
        }
    }
    async fn observe(&self, state: &AppState, p: &crate::journal::Preview) -> &'static str {
        let s = match state.registry.lookup(&p.spreadsheet) {
            Ok(s) => s,
            Err(_) => return "reconciliation",
        };
        let v = match s.views.iter().find(|v| v.name == p.view && v.structured()) {
            Some(v) => v,
            None => return "reconciliation",
        };
        let book = match state.google.read(s).await {
            Ok(b) => b,
            Err(_) => return "reconciliation",
        };
        if projection::validate(s, &book).is_err() {
            return "reconciliation";
        }
        let rows = match write_model::rows(s, v, &book) {
            Ok(r) => r,
            Err(_) => return "reconciliation",
        };
        let found = rows.iter().find(|r| r.operation_id == p.operation_id);
        match found {
            Some(r) if r.record_id == p.record_id && r.fields == p.after => "applied",
            Some(_) => "conflict",
            None => "reconciliation",
        }
    }
    pub async fn approve(&self, id: &str, input: Value) -> Result<Value, ApiError> {
        if !self.enabled {
            self.journal.audit(Some(id), "writes_disabled").await?;
            return Err(ApiError(StatusCode::FORBIDDEN, "writes_disabled"));
        }
        let result = async {
            let p: Approval = serde_json::from_value(input).map_err(|_| ApiError::invalid())?;
            if !matches!(p.decision.as_str(), "approve" | "reject") || !write_model::valid_id(id) {
                return Err(ApiError::invalid());
            }
            let _guard = self.writer.lock().await;
            let op = self
                .journal
                .approve(id, &p.preview_digest, &p.decision)
                .await?;
            Ok(op.preview.public(&op.state))
        }
        .await;
        if let Err(e) = &result {
            self.journal.audit(Some(id), e.1).await?
        }
        result
    }
    pub async fn reconcile(
        &self,
        state: &AppState,
        id: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        let _: Empty = serde_json::from_value(input).map_err(|_| ApiError::invalid())?;
        let _guard = self.writer.lock().await;
        let op = self.journal.get(id).await?;
        if op.state == "applied" {
            return Ok(op.preview.public(&op.state));
        }
        if !["uncertain", "reconciliation", "applying"].contains(&op.state.as_str()) {
            return Err(ApiError(StatusCode::CONFLICT, "operation_not_reconcilable"));
        }
        let outcome = self.observe(state, &op.preview).await;
        let op = self.journal.finish(id, outcome).await?;
        Ok(op.preview.public(&op.state))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    operation_id: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Commit {
    operation_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    decision: String,
    preview_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
