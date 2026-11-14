//! Read-task accounting port and exact model-text result construction.
use super::ports::PortFuture;
use crate::domain::{
    error::CoreError,
    read_budget::{ReadLimits, ReadUsage},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Server-created reservation; tool input never selects a task or reservation.
///
/// # Examples
/// ```
/// use secretary_core::application::read_budget::ReadGrant;
/// let grant: ReadGrant = serde_json::from_str(r#"{"id":"server-id","rows":2,"document_characters":0,"output_bytes":1024}"#)?;
/// assert_eq!(grant.rows, 2);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadGrant {
    /// Unpredictable server-created reservation ID.
    pub id: String,
    /// Maximum returned spreadsheet rows for this invocation.
    pub rows: u64,
    /// Maximum returned document Unicode scalars.
    pub document_characters: u64,
    /// Complete model-visible UTF-8 output allowance.
    pub output_bytes: u64,
}

/// Reservation decision; denied invocations emit at most one task stop notice.
///
/// # Examples
/// ```
/// use secretary_core::application::read_budget::ReadAdmission;
/// let decision = ReadAdmission { grant: None, model_text: String::new(), code: "read_task_required" };
/// assert!(decision.model_text.is_empty());
/// ```
#[derive(Debug, Serialize)]
pub struct ReadAdmission {
    /// A grant is present only after durable quota reservation.
    pub grant: Option<ReadGrant>,
    /// Exact permitted model-visible text; empty on repeated/no-task denials.
    pub model_text: String,
    /// Bounded UI diagnostic, not additional model-visible text.
    pub code: &'static str,
}

/// Durable read counters, never operation approvals or personal source history.
///
/// # Examples
/// ```no_run
/// # use secretary_core::{application::read_budget::ReadTaskLedger, CoreError};
/// # async fn inspect(ledger: &dyn ReadTaskLedger) -> Result<(), CoreError> {
/// let counters = ledger.status().await?;
/// assert!(counters.get("active").is_some());
/// # Ok(()) }
/// ```
pub trait ReadTaskLedger: Send + Sync {
    /// Create one operator-authorized active task in the configured scope.
    ///
    /// # Errors
    /// Denies invalid/above-ceiling limits, an existing task or storage failure.
    fn create(&self, limits: ReadLimits) -> PortFuture<'_, Result<Value, CoreError>>;
    /// Increase current limits monotonically within the protected policy ceiling.
    ///
    /// # Errors
    /// Denies missing tasks, reductions, insufficient notice capacity or storage failure.
    fn increase(&self, limits: ReadLimits) -> PortFuture<'_, Result<Value, CoreError>>;
    /// Close only a task without outstanding invocations.
    ///
    /// # Errors
    /// Denies outstanding reservations or storage failure.
    fn close(&self) -> PortFuture<'_, Result<Value, CoreError>>;
    /// Inspect bounded counters, without returning source contents or secrets.
    ///
    /// # Errors
    /// Returns sanitized storage/corrupt-counter errors.
    fn status(&self) -> PortFuture<'_, Result<Value, CoreError>>;
    /// Reserve a whole invocation before local validation, IO or model output.
    ///
    /// # Errors
    /// Denies unsupported tools or storage failure; exhaustion is an admission decision.
    fn reserve<'a>(&'a self, tool: &'a str) -> PortFuture<'a, Result<ReadAdmission, CoreError>>;
    /// Claim one pending reservation for its exact read tool, at most once.
    ///
    /// # Errors
    /// Denies wrong/replayed/inactive reservations or storage failure.
    fn claim<'a>(
        &'a self,
        id: &'a str,
        tool: &'a str,
    ) -> PortFuture<'a, Result<ReadGrant, CoreError>>;
    /// Settle actual usage once; repeated settlement cannot release quota twice.
    ///
    /// # Errors
    /// Denies overruns, wrong/replayed reservations or storage failure.
    fn finish<'a>(&'a self, id: &'a str, usage: ReadUsage)
        -> PortFuture<'a, Result<(), CoreError>>;
}

pub(crate) const READ_TOOLS: [&str; 8] = [
    "sheets_list",
    "sheets_describe",
    "sheets_read",
    "sheets_search",
    "docs_list",
    "docs_describe",
    "docs_read",
    "docs_search",
];

pub(crate) fn model_result(result: Result<Value, CoreError>) -> Value {
    match result {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(error) => {
            json!({"ok":false,"error":{"code":error.1,"message":error.1,"retryable":false}})
        }
    }
}

pub(crate) fn measure(tool: &str, result: &Value, text: &str) -> ReadUsage {
    fn rows(value: &Value) -> u64 {
        match value {
            Value::Array(values) => values.iter().map(rows).sum(),
            Value::Object(values) => values
                .iter()
                .map(|(key, value)| {
                    if matches!(key.as_str(), "rows" | "records") {
                        value.as_array().map_or(0, |values| values.len() as u64)
                    } else {
                        rows(value)
                    }
                })
                .sum(),
            _ => 0,
        }
    }
    fn characters(value: &Value) -> u64 {
        match value {
            Value::String(value) => value.chars().count() as u64,
            Value::Array(values) => values.iter().map(characters).sum(),
            Value::Object(values) => values.values().map(characters).sum(),
            _ => 0,
        }
    }
    ReadUsage {
        rows: if tool.starts_with("sheets_") {
            rows(&result["data"])
        } else {
            0
        },
        document_characters: if tool.starts_with("docs_") {
            characters(&result["data"])
        } else {
            0
        },
        output_bytes: text.len() as u64,
    }
}

pub(crate) async fn settle(
    ledger: &dyn ReadTaskLedger,
    grant: &ReadGrant,
    tool: &str,
    result: Result<Value, CoreError>,
) -> Result<Value, CoreError> {
    let mut result = model_result(result);
    let mut text = result.to_string();
    let mut usage = measure(tool, &result, &text);
    if usage.output_bytes > grant.output_bytes
        || usage.rows > grant.rows
        || usage.document_characters > grant.document_characters
        || json!({"result":&result,"model_text":&text})
            .to_string()
            .len()
            > 2_097_152
    {
        result = model_result(Err(CoreError(
            crate::domain::error::ErrorKind::PayloadTooLarge,
            "read_budget_result_too_large",
        )));
        text = result.to_string();
        usage = measure(tool, &result, &text);
    }
    ledger.finish(&grant.id, usage).await?;
    Ok(json!({"result":result,"model_text":text}))
}
