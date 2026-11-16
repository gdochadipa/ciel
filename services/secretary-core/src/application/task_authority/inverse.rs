//! Fresh inverse capture under the caller's shared writer; execution stays one member.
use super::{CommandBus, CoreError, TaskContext, TaskLedger, TaskScope};
use crate::domain::compensation::{Inverse, InverseChange};
use serde_json::Value;

pub(super) async fn capture(
    bus: &CommandBus,
    ledger: &dyn TaskLedger,
    scope: &TaskScope,
    task_id: &str,
    context: &TaskContext,
    operation_id: &str,
    original_id: &str,
) -> Result<Value, CoreError> {
    let writes = bus.task_writes()?;
    let original = ledger
        .inverse_original(task_id, context, original_id, operation_id)
        .await?;
    let inverse = Inverse::from_applied(&original, operation_id, &bus.owner)?;
    // Typed physical/mapping permission is checked before provider access.
    scope.permits(&original.preview, &bus.owner)?;
    let preview = if let Some(previous) = writes.journal.find(operation_id).await? {
        inverse.validate_preview(&previous.preview)?;
        previous.preview
    } else {
        let fresh = match inverse.change() {
            InverseChange::Grid(change) => writes.capture_grid(bus, change.clone()).await?,
            InverseChange::Document(change) => writes.capture_document(bus, change.clone()).await?,
        };
        inverse.validate_preview(&fresh)?;
        fresh
    };
    let operation = ledger
        .admit_inverse(task_id, context, original_id, preview)
        .await?;
    Ok(operation.preview.public(operation.state.as_str()))
}
