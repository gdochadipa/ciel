//! Trusted native task authority and single-member execution through the shared bus.
use super::{
    bus::CommandBus,
    ports::{OperationStore, PortFuture},
};
use crate::domain::{
    error::{CoreError, ErrorKind},
    operation::{Operation, Preview},
    write_task::{TaskContext, TaskDecision, TaskEnd, TaskScope},
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
mod inverse;

/// Scoped persistence, sharing the exact operation store used for execution.
///
/// Native authentication belongs to the presentation/host boundary.
/// ```no_run
/// use secretary_core::application::task_authority::TaskLedger;
/// fn operations(ledger: &dyn TaskLedger) { let _ = ledger.operations(); }
/// ```
pub trait TaskLedger: Send + Sync {
    /// Configured owner paired to this store; composition must match the bus.
    fn owner(&self) -> &str;
    /// Return the paired operation store; adapters must never substitute a journal.
    fn operations(&self) -> Arc<dyn OperationStore>;
    /// Propose bounded scope. Errors reject invalid context/scope or a live task.
    fn propose<'a>(
        &'a self,
        context: &'a TaskContext,
        scope: &'a TaskScope,
    ) -> PortFuture<'a, Result<Value, CoreError>>;
    /// Inspect an exact conversation without renewing or revoking a grant.
    /// Errors reject a new/reset conversation or unavailable record.
    fn review<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
    ) -> PortFuture<'a, Result<Value, CoreError>>;
    /// Apply the captured owner decision. Errors reject stale/replayed authority.
    fn decide<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        digest: &'a str,
        revision: u64,
        decision: TaskDecision,
    ) -> PortFuture<'a, Result<Value, CoreError>>;
    /// End permission without deleting evidence. Errors reject invalid completion.
    fn end<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        reason: TaskEnd,
        revision: u64,
    ) -> PortFuture<'a, Result<Value, CoreError>>;
    /// Revoke only the exact host context. Errors report storage/identity failure.
    fn invalidate<'a>(&'a self, context: &'a TaskContext) -> PortFuture<'a, Result<(), CoreError>>;
    /// Load active scope before source access. Errors deny stale host/grant state.
    fn scope<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
    ) -> PortFuture<'a, Result<TaskScope, CoreError>>;
    /// Admit source captured by the application. Errors reject scope/replay drift.
    fn admit<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        preview: Preview,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    /// Load an original within the exact current undo selection before source IO.
    /// Errors reject stale authority, a different original or unavailable evidence.
    fn inverse_original<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        original_id: &'a str,
        operation_id: &'a str,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    /// Atomically retain fresh inverse intent and its authoritative original link.
    /// Errors reject changed evidence, ordering, duplicate attempts or stale grants.
    fn admit_inverse<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        original_id: &'a str,
        preview: Preview,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    /// Reject unsent intent without closing or replenishing the task grant.
    /// Errors reject wrong membership, revision or a member already dispatched.
    fn discard<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        operation_id: &'a str,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    /// Persist applying under the exact current revision. Errors grant no dispatch.
    fn claim<'a>(&'a self, binding: &'a TaskMember)
        -> PortFuture<'a, Result<Operation, CoreError>>;
    /// Linearize one accepted effect after native confirmation and fresh preflight.
    /// Errors include cancellation/revision changes and repeated dispatch attempts.
    fn permit<'a>(
        &'a self,
        binding: &'a TaskMember,
        preview: &'a Preview,
    ) -> PortFuture<'a, Result<(), CoreError>>;
}

/// Exact immutable member selected by a trusted current native invocation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMember {
    /// Server-created task identity.
    pub task_id: String,
    /// Captured native conversation/process descriptors, not standalone proof.
    pub context: TaskContext,
    /// Owner-approved grant revision, never silently upgraded.
    pub grant_revision: u64,
    /// One admitted operation, never a list of remaining work.
    pub operation_id: String,
}

/// Host confirmation after Core source preflight, before accepting one effect.
/// ```no_run
/// use secretary_core::application::task_authority::DispatchConfirmation;
/// fn host(_guard: &dyn DispatchConfirmation) {}
/// ```
pub trait DispatchConfirmation: Send + Sync {
    /// Await exact current-invocation confirmation. Failure guarantees no send.
    /// # Errors
    /// Returns refusal, cancellation, unavailable transport or timeout.
    fn confirm<'a>(
        &'a self,
        member: &'a TaskMember,
        preview_digest: &'a str,
    ) -> PortFuture<'a, Result<(), CoreError>>;
}

pub use crate::domain::write_task::TaskCapture;

enum ForwardCapture {
    Grid(crate::domain::write_model::Change),
    Document(crate::domain::documents::DocChange),
}

/// Typed native control requests; never decoded from a model-facing tool route.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskControl {
    /// Propose an exact registered scope for native owner approval.
    Propose {
        context: TaskContext,
        scope: TaskScope,
    },
    /// Inspect retained scope for fresh confirmation without resetting usage.
    Review {
        task_id: String,
        context: TaskContext,
    },
    /// Capture source under the shared writer and atomically admit exact intent.
    Capture {
        task_id: String,
        context: TaskContext,
        change: TaskCapture,
    },
    /// Retain spent identity/evidence while permitting another in-scope approach.
    Discard {
        task_id: String,
        context: TaskContext,
        operation_id: String,
    },
    /// Resolve the captured proposal/reconfirmation, bound to its revision/digest.
    Decide {
        task_id: String,
        context: TaskContext,
        scope_digest: String,
        grant_revision: u64,
        decision: TaskDecision,
    },
    /// End an unfinished task without manufacturing success for its members.
    End {
        task_id: String,
        context: TaskContext,
        reason: TaskEnd,
        grant_revision: u64,
    },
    /// Trusted lifecycle notification; not relied upon as the final send fence.
    Invalidate { context: TaskContext },
}

pub(super) async fn control(bus: &CommandBus, command: TaskControl) -> Result<Value, CoreError> {
    let writes = bus.task_writes()?;
    let ledger = writes
        .tasks
        .as_ref()
        .ok_or(CoreError(ErrorKind::Unavailable, "write_tasks_unavailable"))?;
    match command {
        TaskControl::Propose { context, scope } => {
            scope.registered(&bus.registry)?;
            ledger.propose(&context, &scope).await
        }
        TaskControl::Review { task_id, context } => ledger.review(&task_id, &context).await,
        TaskControl::Capture {
            task_id,
            context,
            change,
        } => {
            let _writer = writes.writer.lock().await;
            let scope = ledger.scope(&task_id, &context).await?;
            scope.registered(&bus.registry)?;
            scope.permits_capture(&change)?;
            let change = match change {
                TaskCapture::Grid(c) => ForwardCapture::Grid(c),
                TaskCapture::Document(c) => ForwardCapture::Document(c),
                TaskCapture::Inverse {
                    operation_id,
                    original_id,
                } => {
                    return inverse::capture(
                        bus,
                        ledger.as_ref(),
                        &scope,
                        &task_id,
                        &context,
                        &operation_id,
                        &original_id,
                    )
                    .await;
                }
            };
            let operation_id = match &change {
                ForwardCapture::Grid(c) => &c.operation_id,
                ForwardCapture::Document(c) => &c.operation_id,
            };
            if !crate::domain::write_model::valid_id(operation_id) {
                return Err(CoreError::invalid());
            }
            let previous = match &change {
                ForwardCapture::Grid(c) => writes.journal.find(&c.operation_id).await?,
                ForwardCapture::Document(c) => writes.journal.find(&c.operation_id).await?,
            };
            if let Some(previous) = previous {
                let matches = match &change {
                    ForwardCapture::Grid(c) => crate::domain::write_model::matches_request(
                        &previous.preview,
                        c,
                        &bus.owner,
                    ),
                    ForwardCapture::Document(c) => {
                        crate::domain::documents::matches(&previous.preview, c, &bus.owner)
                    }
                };
                if !matches {
                    return Err(CoreError(ErrorKind::Conflict, "operation_conflict"));
                }
                let op = ledger.admit(&task_id, &context, previous.preview).await?;
                return Ok(op.preview.public(op.state.as_str()));
            }
            let preview = match change {
                ForwardCapture::Grid(c) => writes.capture_grid(bus, c).await?,
                ForwardCapture::Document(c) => writes.capture_document(bus, c).await?,
            };
            let op = ledger.admit(&task_id, &context, preview).await?;
            Ok(op.preview.public(op.state.as_str()))
        }
        TaskControl::Discard {
            task_id,
            context,
            operation_id,
        } => {
            let _writer = writes.writer.lock().await;
            let op = ledger.discard(&task_id, &context, &operation_id).await?;
            Ok(op.preview.public(op.state.as_str()))
        }
        TaskControl::Decide {
            task_id,
            context,
            scope_digest,
            grant_revision,
            decision,
        } => {
            if matches!(decision, TaskDecision::Approve) {
                let review = ledger.review(&task_id, &context).await?;
                let scope: TaskScope = serde_json::from_value(review["scope"].clone())
                    .map_err(|_| CoreError::invalid())?;
                scope.registered(&bus.registry)?;
            }
            ledger
                .decide(&task_id, &context, &scope_digest, grant_revision, decision)
                .await
        }
        TaskControl::End {
            task_id,
            context,
            reason,
            grant_revision,
        } => ledger.end(&task_id, &context, reason, grant_revision).await,
        TaskControl::Invalidate { context } => {
            ledger.invalidate(&context).await?;
            Ok(serde_json::json!({"invalidated":true}))
        }
    }
}

// The executor supplies this guard only for task members; legacy apply keeps its
// existing approval/FSM behavior. One accepted member may finish after revocation.
pub(super) trait EffectGuard: Send + Sync {
    fn accept<'a>(&'a self, preview: &'a Preview) -> PortFuture<'a, Result<(), CoreError>>;
}
// Native confirmation commits exactly one intent. The following durable permit
// orders cancellation before/after acceptance; it never authorizes later members.
struct MemberEffect<'a> {
    ledger: &'a dyn TaskLedger,
    member: &'a TaskMember,
    native: &'a dyn DispatchConfirmation,
}
impl EffectGuard for MemberEffect<'_> {
    fn accept<'a>(&'a self, preview: &'a Preview) -> PortFuture<'a, Result<(), CoreError>> {
        Box::pin(async move {
            let result = async {
                self.ledger
                    .scope(&self.member.task_id, &self.member.context)
                    .await?;
                self.native
                    .confirm(self.member, &preview.preview_digest)
                    .await?;
                self.ledger.permit(self.member, preview).await
            }
            .await;
            if let Err(error) = &result {
                self.ledger
                    .operations()
                    .audit(Some(&self.member.operation_id), error.1)
                    .await?;
            }
            result
        })
    }
}
impl super::writes::Writes {
    pub(super) async fn execute_task(
        &self,
        bus: &CommandBus,
        member: &TaskMember,
        native: &dyn DispatchConfirmation,
    ) -> Result<Value, CoreError> {
        let _writer = self.writer.lock().await;
        let ledger = self
            .tasks
            .as_ref()
            .ok_or(CoreError(ErrorKind::Unavailable, "write_tasks_unavailable"))?;
        let op = ledger.claim(member).await?;
        let guard = MemberEffect {
            ledger: ledger.as_ref(),
            member,
            native,
        };
        let outcome = match op.preview.resource_kind {
            crate::domain::operation::ResourceKind::SheetsGrid => {
                self.apply_grid_checked(bus, &op.preview, Some(&guard))
                    .await
            }
            crate::domain::operation::ResourceKind::GoogleDocs => {
                self.apply_document_checked(bus, &op.preview, Some(&guard))
                    .await
            }
            _ => crate::domain::operation::Outcome::KnownFailure,
        };
        let result = self.journal.finish(&member.operation_id, outcome).await?;
        Ok(result.preview.public(result.state.as_str()))
    }
}
