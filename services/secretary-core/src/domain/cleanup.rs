//! Owner-reviewed journal maintenance; never provider mutation authority.
use super::{
    error::CoreError,
    operation::{digest, OperationState},
    write_model::valid_id,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// Bounded whole completed tasks and explicitly selected standalone legacy records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupSelection {
    pub task_ids: Vec<String>,
    pub operation_ids: Vec<String>,
    /// Only evidence completed/updated at or before this Unix timestamp is selected.
    pub completed_before: i64,
}
impl CleanupSelection {
    /// Canonicalize an exact selection without silently truncating it.
    /// # Errors
    /// Rejects empty/duplicate IDs, over20 selectors, invalid IDs or future cutoff.
    pub fn canonical(&self, now: i64) -> Result<Self, CoreError> {
        if self.task_ids.len() + self.operation_ids.len() == 0
            || self.task_ids.len() + self.operation_ids.len() > 20
            || self.completed_before <= 0
            || self.completed_before > now
        {
            return Err(CoreError::invalid());
        }
        let tasks: HashSet<_> = self.task_ids.iter().collect();
        let operations: HashSet<_> = self.operation_ids.iter().collect();
        if tasks.len() != self.task_ids.len()
            || operations.len() != self.operation_ids.len()
            || self
                .task_ids
                .iter()
                .any(|id| uuid::Uuid::parse_str(id).map_or(true, |uuid| uuid.to_string() != *id))
            || self.operation_ids.iter().any(|id| !valid_id(id))
        {
            return Err(CoreError::invalid());
        }
        let mut selection = self.clone();
        selection.task_ids.sort();
        selection.operation_ids.sort();
        Ok(selection)
    }
}

/// One authenticated operator action; model-facing tool routes do not decode it.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CleanupCommand {
    Preview {
        selection: CleanupSelection,
    },
    Inspect {
        cleanup_id: String,
    },
    Decide {
        cleanup_id: String,
        preview_digest: String,
        decision: CleanupDecision,
    },
}
/// A decision applies to the exact persisted cleanup preview once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupDecision {
    Approve,
    Reject,
}

pub(crate) fn eligible(state: OperationState) -> bool {
    matches!(
        state,
        OperationState::Applied
            | OperationState::KnownFailure
            | OperationState::Rejected
            | OperationState::Expired
    )
}
pub(crate) fn preview_digest(
    id: &str,
    owner: &str,
    generation: &str,
    instance: &str,
    plan: &Value,
) -> String {
    digest(&serde_json::json!([
        "ciel.evidence-cleanup.v1",
        id,
        owner,
        generation,
        instance,
        plan
    ]))
}
