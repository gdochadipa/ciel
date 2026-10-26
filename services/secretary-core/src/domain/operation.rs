use super::error::{CoreError, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub operation_id: String,
    pub actor: String,
    pub spreadsheet: String,
    pub view: String,
    pub action: String,
    pub record_id: String,
    pub fields: Value,
    pub before: Value,
    pub after: Value,
    pub mapping_version: String,
    pub source_revision: String,
    pub expires_at: i64,
    pub preview_digest: String,
}
impl Preview {
    pub fn request_digest(&self) -> String {
        digest(
            &json!({"operation_id":self.operation_id,"actor":self.actor,"spreadsheet":self.spreadsheet,"view":self.view,"action":self.action,"record_id":if self.action=="update"{Some(&self.record_id)}else{None},"fields":self.fields}),
        )
    }
    pub fn public(&self, state: &str) -> Value {
        let mut out = serde_json::to_value(self).unwrap_or(Value::Null);
        out["state"] = json!(state);
        out["expires_at"] = json!(chrono::DateTime::from_timestamp(self.expires_at, 0)
            .map(|t| t.to_rfc3339())
            .unwrap_or_default());
        out
    }
}
pub(crate) fn digest(v: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).unwrap_or_default())
    )
}
#[derive(Clone, Debug)]
pub struct Operation {
    pub preview: Preview,
    pub state: OperationState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Previewed,
    Approved,
    Rejected,
    Expired,
    Applying,
    Applied,
    KnownFailure,
    Uncertain,
    Conflict,
    Reconciliation,
}
#[derive(Clone, Copy, Debug)]
pub enum OperationEvent {
    Approve,
    Reject,
    Expire,
    BeginApply,
    Finish(Outcome),
    Recover,
}
#[derive(Clone, Copy, Debug)]
pub enum Outcome {
    Applied,
    KnownFailure,
    Uncertain,
    Conflict,
    Reconciliation,
}
impl OperationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Previewed => "previewed",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Applying => "applying",
            Self::Applied => "applied",
            Self::KnownFailure => "known_failure",
            Self::Uncertain => "uncertain",
            Self::Conflict => "conflict",
            Self::Reconciliation => "reconciliation",
        }
    }
    pub fn transition(self, event: OperationEvent) -> Result<Self, CoreError> {
        use OperationEvent::*;
        match (self, event) {
            (Self::Previewed, Approve) | (Self::Approved, Approve) => Ok(Self::Approved),
            (Self::Previewed, Reject) | (Self::Rejected, Reject) => Ok(Self::Rejected),
            (Self::Previewed | Self::Approved, Expire) => Ok(Self::Expired),
            (Self::Approved, BeginApply) => Ok(Self::Applying),
            (Self::Applying, Recover) => Ok(Self::Reconciliation),
            (Self::Applying, Finish(outcome)) => Ok(outcome.state()),
            (
                Self::Uncertain | Self::Reconciliation,
                Finish(outcome @ (Outcome::Applied | Outcome::Conflict | Outcome::Reconciliation)),
            ) => Ok(outcome.state()),
            _ => Err(CoreError(ErrorKind::Conflict, "operation_conflict")),
        }
    }
}
impl std::str::FromStr for OperationState {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "previewed" => Ok(Self::Previewed),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            "applying" => Ok(Self::Applying),
            "applied" => Ok(Self::Applied),
            "known_failure" => Ok(Self::KnownFailure),
            "uncertain" => Ok(Self::Uncertain),
            "conflict" => Ok(Self::Conflict),
            "reconciliation" => Ok(Self::Reconciliation),
            _ => Err(CoreError(ErrorKind::Unavailable, "journal_unavailable")),
        }
    }
}
impl Outcome {
    pub fn state(self) -> OperationState {
        match self {
            Self::Applied => OperationState::Applied,
            Self::KnownFailure => OperationState::KnownFailure,
            Self::Uncertain => OperationState::Uncertain,
            Self::Conflict => OperationState::Conflict,
            Self::Reconciliation => OperationState::Reconciliation,
        }
    }
}

impl Operation {
    pub fn approval_event(
        &self,
        preview_digest: &str,
        decision: &str,
        now: i64,
    ) -> Result<OperationEvent, CoreError> {
        if self.preview.preview_digest != preview_digest {
            return Err(CoreError(ErrorKind::Conflict, "operation_conflict"));
        }
        if self.preview.expires_at <= now
            && matches!(
                self.state,
                OperationState::Previewed | OperationState::Approved
            )
        {
            return Ok(OperationEvent::Expire);
        }
        match decision {
            "approve" => Ok(OperationEvent::Approve),
            "reject" => Ok(OperationEvent::Reject),
            _ => Err(CoreError::invalid()),
        }
    }
    pub fn apply_event(&self, now: i64) -> OperationEvent {
        if self.preview.expires_at <= now && self.state == OperationState::Approved {
            OperationEvent::Expire
        } else {
            OperationEvent::BeginApply
        }
    }
}
