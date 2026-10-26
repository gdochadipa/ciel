use crate::{
    application::{
        bus::CommandBus,
        commands::{Approval, MutationCommand},
        ports::{OperationStore, WriteOutcome},
    },
    domain::{
        error::{CoreError, ErrorKind},
        operation::{OperationState, Outcome, Preview},
        projection, write_model,
    },
};
use serde_json::Value;
use std::sync::Arc;
pub struct Writes {
    pub journal: Arc<dyn OperationStore>,
    pub enabled: bool,
    writer: tokio::sync::Mutex<()>,
}
impl Writes {
    pub fn new(journal: impl OperationStore + 'static, enabled: bool) -> Self {
        Self {
            journal: Arc::new(journal),
            enabled,
            writer: tokio::sync::Mutex::new(()),
        }
    }
    pub async fn handle(
        &self,
        state: &CommandBus,
        command: MutationCommand,
    ) -> Result<Value, CoreError> {
        let result = self.dispatch(state, command).await;
        if let Err(e) = &result {
            self.journal.audit(None, e.1).await?;
        }
        result
    }
    async fn dispatch(
        &self,
        state: &CommandBus,
        command: MutationCommand,
    ) -> Result<Value, CoreError> {
        if let MutationCommand::History(p) = &command {
            let cursor = p
                .cursor
                .as_ref()
                .map(|c| c.parse::<usize>().map_err(|_| CoreError::invalid()))
                .transpose()?
                .unwrap_or(0);
            return self
                .journal
                .history(p.operation_id.as_deref(), p.limit.unwrap_or(50), cursor)
                .await;
        }
        if !self.enabled {
            return Err(CoreError(ErrorKind::Forbidden, "writes_disabled"));
        }
        let _guard = self.writer.lock().await;
        match command {
            MutationCommand::Preview(c) => {
                if !write_model::valid_id(&c.operation_id) {
                    return Err(CoreError::invalid());
                }
                if let Some(old) = self.journal.find(&c.operation_id).await? {
                    if !write_model::matches_request(&old.preview, &c, &state.owner) {
                        return Err(CoreError(ErrorKind::Conflict, "operation_conflict"));
                    }
                    return Ok(old.preview.public(old.state.as_str()));
                }
                let s = state.registry.lookup(&c.spreadsheet)?;
                let v = s
                    .views
                    .iter()
                    .find(|v| v.name == c.view)
                    .ok_or_else(CoreError::invalid)?;
                write_model::validate_fields(v, &c.fields, c.action == "append")?;
                let book = state.sheets.read(s).await?;
                projection::validate(s, &book)?;
                let generated_id = uuid::Uuid::new_v4().to_string();
                let context = write_model::PreparationContext {
                    new_record_id: &generated_id,
                    expires_at: chrono::Utc::now().timestamp() + 300,
                };
                let preview = write_model::prepare(
                    &c,
                    &state.owner,
                    s,
                    v,
                    &book,
                    &state.registry.version,
                    &context,
                )?;
                let p = self.journal.create(preview).await?;
                Ok(p.public("previewed"))
            }
            command @ (MutationCommand::Append(_) | MutationCommand::Update(_)) => {
                let (p, action) = match command {
                    MutationCommand::Append(p) => (p, "append"),
                    MutationCommand::Update(p) => (p, "update"),
                    _ => unreachable!(),
                };
                if !write_model::valid_id(&p.operation_id) {
                    return Err(CoreError::invalid());
                }
                let old = self.journal.get(&p.operation_id).await?;
                if old.preview.actor.as_str() != state.owner.as_ref()
                    || old.preview.action != action
                {
                    return Err(CoreError(ErrorKind::Conflict, "operation_action_mismatch"));
                }
                if old.state == OperationState::Applied {
                    return Ok(old.preview.public("applied"));
                }
                let operation = self.journal.begin_apply(&p.operation_id).await?;
                let outcome = self.apply(state, &operation.preview).await;
                let finished = self.journal.finish(&p.operation_id, outcome).await?;
                Ok(finished.preview.public(finished.state.as_str()))
            }
            MutationCommand::History(_) => unreachable!("history handled before write gate"),
        }
    }
    async fn apply(&self, state: &CommandBus, p: &Preview) -> Outcome {
        if p.mapping_version != state.registry.version {
            return Outcome::Conflict;
        }
        let s = match state.registry.lookup(&p.spreadsheet) {
            Ok(s) => s,
            Err(_) => return Outcome::Conflict,
        };
        let v = match s
            .views
            .iter()
            .find(|v| v.name == p.view && v.structured() && v.write_schema.is_some())
        {
            Some(v) => v,
            None => return Outcome::Conflict,
        };
        let book = match state.sheets.read(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::KnownFailure,
        };
        if projection::validate(s, &book).is_err()
            || projection::fingerprint(&book) != p.source_revision
        {
            return Outcome::Conflict;
        }
        let rows = match write_model::rows(s, v, &book) {
            Ok(rows) => rows,
            Err(_) => return Outcome::Conflict,
        };
        // Preflight detects duplicate identities and retains the exact approved target after reordering.
        let row = if p.action == "update" {
            match rows.iter().find(|r| r.record_id == p.record_id) {
                Some(r) if r.fields == p.before => Some(r.row),
                _ => return Outcome::Conflict,
            }
        } else {
            if rows.iter().any(|r| {
                r.record_id == p.record_id || r.operation_id == p.operation_id || r.row >= 1001
            }) {
                return Outcome::Conflict;
            }
            None
        };
        match state.sheets.write_values(s, v, p, row).await {
            WriteOutcome::Rejected => Outcome::KnownFailure,
            WriteOutcome::Uncertain => Outcome::Uncertain,
            WriteOutcome::Accepted => self.observe(state, p).await,
        }
    }
    async fn observe(&self, state: &CommandBus, p: &Preview) -> Outcome {
        let s = match state.registry.lookup(&p.spreadsheet) {
            Ok(s) => s,
            Err(_) => return Outcome::Reconciliation,
        };
        let v = match s.views.iter().find(|v| v.name == p.view && v.structured()) {
            Some(v) => v,
            None => return Outcome::Reconciliation,
        };
        let book = match state.sheets.read(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::Reconciliation,
        };
        if projection::validate(s, &book).is_err() {
            return Outcome::Reconciliation;
        }
        let rows = match write_model::rows(s, v, &book) {
            Ok(r) => r,
            Err(_) => return Outcome::Reconciliation,
        };
        let found = rows.iter().find(|r| r.operation_id == p.operation_id);
        match found {
            Some(r) if r.record_id == p.record_id && r.fields == p.after => Outcome::Applied,
            Some(_) => Outcome::Conflict,
            None => Outcome::Reconciliation,
        }
    }
    pub async fn approve(&self, id: &str, p: Approval) -> Result<Value, CoreError> {
        if !self.enabled {
            self.journal.audit(Some(id), "writes_disabled").await?;
            return Err(CoreError(ErrorKind::Forbidden, "writes_disabled"));
        }
        let result = async {
            if !matches!(p.decision.as_str(), "approve" | "reject") || !write_model::valid_id(id) {
                return Err(CoreError::invalid());
            }
            let _guard = self.writer.lock().await;
            let op = self
                .journal
                .approve(id, &p.preview_digest, &p.decision)
                .await?;
            Ok(op.preview.public(op.state.as_str()))
        }
        .await;
        if let Err(e) = &result {
            self.journal.audit(Some(id), e.1).await?
        }
        result
    }
    pub async fn reconcile(&self, state: &CommandBus, id: &str) -> Result<Value, CoreError> {
        let _guard = self.writer.lock().await;
        let op = self.journal.get(id).await?;
        if op.state == OperationState::Applied {
            return Ok(op.preview.public(op.state.as_str()));
        }
        if !matches!(
            op.state,
            OperationState::Uncertain | OperationState::Reconciliation | OperationState::Applying
        ) {
            return Err(CoreError(ErrorKind::Conflict, "operation_not_reconcilable"));
        }
        let outcome = self.observe(state, &op.preview).await;
        let op = self.journal.finish(id, outcome).await?;
        Ok(op.preview.public(op.state.as_str()))
    }
}
