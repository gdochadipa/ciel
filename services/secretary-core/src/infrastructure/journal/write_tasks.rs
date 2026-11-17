//! Scoped authority metadata in the existing journal, not a second execution FSM.
//! No HTTP/native grant issuer is installed by opening this backend.
use super::{storage, Journal};
use crate::domain::{
    compensation::Inverse,
    error::{CoreError, ErrorKind},
    operation::{digest, Operation, OperationEvent, OperationState, Preview},
    write_model::valid_id,
    write_task::{TaskContext, TaskScope, UndoSource},
};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteRow, Row, Sqlite, Transaction};
use std::sync::atomic::Ordering;

fn denied(code: &'static str) -> CoreError {
    CoreError(ErrorKind::Conflict, code)
}
fn db(_: sqlx::Error) -> CoreError {
    storage()
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String, CoreError> {
    serde_json::to_string(value).map_err(|_| storage())
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, CoreError> {
    serde_json::from_str(value).map_err(|_| storage())
}

pub use crate::domain::write_task::{TaskDecision, TaskEnd};
mod cleanup;

/// One owner/deployment's scoped task authority in the shared SQLite journal.
///
/// Callers are trusted application/host composition, not model input. Opening this
/// backend does not authenticate native callbacks or qualify a controlled runtime.
/// All existing legacy approval/apply entry points reject linked task operations.
///
/// # Examples
/// ```no_run
/// # async fn open(journal: secretary_core::journal::Journal)
/// # -> Result<(), secretary_core::CoreError> {
/// use secretary_core::journal::write_tasks::JournalWriteTasks;
/// let tasks = JournalWriteTasks::open(journal, "owner", "deployment-v1").await?;
/// # Ok(()) }
/// ```
pub struct JournalWriteTasks {
    journal: Journal,
    owner: String,
    generation: String,
    instance: String,
}

impl JournalWriteTasks {
    /// Bind this ledger once to the owner/deployment and suspend earlier grants.
    ///
    /// Retained previews/FSM digests are untouched. Restarted applying members are
    /// reconciled by ordinary Journal recovery; no provider write is performed.
    ///
    /// # Errors
    /// Rejects read-only state, changed owner/deployment, foreign legacy records,
    /// duplicate attachment or storage failure.
    pub async fn open(journal: Journal, owner: &str, generation: &str) -> Result<Self, CoreError> {
        if owner.trim().is_empty()
            || owner.len() > 256
            || owner.chars().any(char::is_control)
            || !crate::domain::service_config::revision(generation)
        {
            return Err(CoreError::invalid());
        }
        if journal.write_tasks_claimed.swap(true, Ordering::SeqCst) {
            return Err(denied("write_tasks_already_attached"));
        }
        let mut tx = journal.pool.begin().await.map_err(db)?;
        let readonly: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='readonly_runtime_binding'")
            .fetch_one(&mut *tx).await.map_err(db)?;
        if readonly != 0 {
            return Err(CoreError(ErrorKind::Forbidden, "readonly_profile"));
        }
        sqlx::query("CREATE TABLE IF NOT EXISTS write_task_binding(singleton INTEGER PRIMARY KEY CHECK(singleton=1),owner TEXT NOT NULL,generation TEXT NOT NULL,schema_version INTEGER NOT NULL CHECK(schema_version=1))")
            .execute(&mut *tx).await.map_err(db)?;
        if let Some(row) = sqlx::query(
            "SELECT owner,generation,schema_version FROM write_task_binding WHERE singleton=1",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            if row.get::<String, _>("owner") != owner
                || row.get::<String, _>("generation") != generation
                || row.get::<i64, _>("schema_version") != 1
            {
                return Err(denied("write_task_binding"));
            }
        } else {
            let foreign:i64=sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE json_type(identity,'$.actor') IS NOT 'text' OR json_extract(identity,'$.actor')<>?")
                .bind(owner).fetch_one(&mut *tx).await.map_err(db)?;
            if foreign != 0 {
                return Err(denied("write_task_legacy_owner"));
            }
            sqlx::query("INSERT INTO write_task_binding VALUES(1,?,?,1)")
                .bind(owner)
                .bind(generation)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("CREATE TABLE IF NOT EXISTS evidence_cleanups(id TEXT PRIMARY KEY,owner TEXT NOT NULL,generation TEXT NOT NULL,instance TEXT NOT NULL,plan TEXT NOT NULL,digest TEXT NOT NULL,decision TEXT NOT NULL CHECK(decision IN ('pending','applied','rejected')),created_at INTEGER NOT NULL,result TEXT)")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS write_tasks(id TEXT PRIMARY KEY,conversation TEXT NOT NULL,context TEXT NOT NULL,scope TEXT NOT NULL,digest TEXT NOT NULL,phase TEXT NOT NULL CHECK(phase IN ('proposed','active','suspended','closed')),revision INTEGER NOT NULL,instance TEXT NOT NULL,admitted INTEGER NOT NULL DEFAULT 0,prompt_expires INTEGER NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,ended_at INTEGER,reason TEXT)")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS write_task_ended_contexts(conversation TEXT NOT NULL,context TEXT NOT NULL,ended_at INTEGER NOT NULL,PRIMARY KEY(conversation,context))")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS write_tasks_one_unfinished ON write_tasks(conversation) WHERE phase<>'closed'").execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS write_task_operations(operation_id TEXT PRIMARY KEY REFERENCES operations(operation_id),task_id TEXT NOT NULL REFERENCES write_tasks(id),ordinal INTEGER NOT NULL,grant_revision INTEGER NOT NULL,preview_digest TEXT NOT NULL,resource_key TEXT NOT NULL,UNIQUE(task_id,ordinal))")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS write_task_dispatches(operation_id TEXT PRIMARY KEY REFERENCES operations(operation_id),task_id TEXT NOT NULL REFERENCES write_tasks(id),grant_revision INTEGER NOT NULL,preview_digest TEXT NOT NULL,accepted_at INTEGER NOT NULL)")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS operation_compensations(inverse_id TEXT PRIMARY KEY REFERENCES operations(operation_id),original_id TEXT NOT NULL REFERENCES operations(operation_id),original_digest TEXT NOT NULL,inverse_digest TEXT NOT NULL,CHECK(inverse_id<>original_id))")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("CREATE INDEX IF NOT EXISTS compensation_original ON operation_compensations(original_id)")
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE write_tasks SET phase='suspended',revision=revision+1,reason='restart',updated_at=? WHERE phase IN ('proposed','active')")
            .bind(chrono::Utc::now().timestamp()).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, None, "write_tasks_attached").await?;
        tx.commit().await.map_err(db)?;
        Ok(Self {
            journal,
            owner: owner.into(),
            generation: generation.into(),
            instance: uuid::Uuid::new_v4().to_string(),
        })
    }

    async fn transaction(&self) -> Result<Transaction<'_, Sqlite>, CoreError> {
        let mut tx = self.journal.pool.begin().await.map_err(db)?;
        // Take SQLite's writer lock before reading grant, count or member state.
        sqlx::query("UPDATE write_task_binding SET generation=generation WHERE singleton=1")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        Ok(tx)
    }

    fn conversation(&self, context: &TaskContext) -> String {
        digest(&json!([
            self.owner,
            self.generation,
            context.agent_id,
            context.session_key
        ]))
    }

    fn scope_digest(&self, id: &str, context: &TaskContext, scope: &TaskScope) -> String {
        digest(&json!([
            "ciel.write-task.v1",
            id,
            self.owner,
            self.generation,
            context.agent_id,
            context.session_key,
            context.session_id,
            scope
        ]))
    }

    fn checked_scope(&self, row: &SqliteRow) -> Result<TaskScope, CoreError> {
        let context: TaskContext = decode(row.get("context"))?;
        context.validate()?;
        let scope: TaskScope = decode(row.get("scope"))?;
        scope.validate()?;
        if self.scope_digest(row.get("id"), &context, &scope) != row.get::<&str, _>("digest")
            || self.conversation(&context) != row.get::<&str, _>("conversation")
        {
            return Err(storage());
        }
        Ok(scope)
    }

    /// Persist a server-created task proposal with exact scope for owner review.
    ///
    /// # Errors
    /// Rejects invalid identity/scope or an unfinished task in that conversation.
    pub async fn propose(
        &self,
        context: &TaskContext,
        scope: &TaskScope,
    ) -> Result<Value, CoreError> {
        context.validate()?;
        scope.validate()?;
        let mut tx = self.transaction().await?;
        let conversation = self.conversation(context);
        // A detached notification can overtake an already sent proposal. Keep
        // the exact ended identity even when there was no task to close yet.
        let ended: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM write_task_ended_contexts WHERE conversation=? AND context=?",
        )
        .bind(&conversation)
        .bind(encode(context)?)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if ended != 0 {
            return Err(denied("write_task_context_ended"));
        }
        if sqlx::query("SELECT id FROM write_tasks WHERE conversation=? AND phase<>'closed'")
            .bind(&conversation)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .is_some()
        {
            return Err(denied("write_task_already_active"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let hash = self.scope_digest(&id, context, scope);
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO write_tasks(id,conversation,context,scope,digest,phase,revision,instance,prompt_expires,created_at,updated_at) VALUES(?,?,?,?,?,'proposed',0,?,?,?,?)")
            .bind(&id).bind(conversation).bind(encode(context)?).bind(encode(scope)?).bind(hash).bind(&self.instance).bind(now+60).bind(now).bind(now)
            .execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, None, &format!("write_task_proposed:{id}")).await?;
        let result = summary(&task(&mut tx, &id).await?)?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }

    /// Apply a trusted owner decision or fresh confirmation after suspension.
    ///
    /// This API is not model-callable. Native prompt age is bounded for initial
    /// approval; an active task itself has no time countdown. Repeated approval of
    /// the current grant is idempotent and cannot reset operation counts.
    ///
    /// # Errors
    /// Rejects changed scope/conversation, expired initial dialog, closed task or
    /// unresolved earlier writes. Restart resumption requires this explicit call.
    pub async fn decide(
        &self,
        id: &str,
        context: &TaskContext,
        hash: &str,
        expected_revision: u64,
        decision: TaskDecision,
    ) -> Result<Value, CoreError> {
        context.validate()?;
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, id).await?;
        self.checked_scope(&row)?;
        let old: TaskContext = decode(row.get("context"))?;
        if !old.same_conversation(context) || row.get::<String, _>("digest") != hash {
            return Err(denied("write_task_context"));
        }
        let phase = row.get::<&str, _>("phase");
        if phase == "closed" {
            return Err(denied("write_task_closed"));
        }
        let revision = u64::try_from(row.get::<i64, _>("revision")).map_err(|_| storage())?;
        if phase == "active"
            && matches!(decision, TaskDecision::Approve)
            && expected_revision.checked_add(1) == Some(revision)
        {
            self.active(&row, context)?;
            return summary(&row);
        }
        if expected_revision != revision {
            return Err(denied("write_task_stale_decision"));
        }
        if matches!(decision, TaskDecision::Reject) {
            close(&mut tx, id, "rejected").await?;
        } else if phase == "active" && old == *context {
            self.active(&row, context)?;
        } else {
            if phase == "proposed"
                && (old != *context || row.get::<&str, _>("instance") != self.instance)
            {
                return Err(denied("write_task_context"));
            }
            if phase == "proposed"
                && row.get::<i64, _>("prompt_expires") <= chrono::Utc::now().timestamp()
            {
                return Err(denied("write_task_prompt_expired"));
            }
            let unresolved:i64=sqlx::query_scalar("SELECT COUNT(*) FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id WHERE l.task_id=? AND o.state IN ('applying','uncertain','reconciliation','conflict')")
                .bind(id).fetch_one(&mut *tx).await.map_err(db)?;
            if unresolved != 0 {
                return Err(denied("write_task_recovery_required"));
            }
            retire_previews(&mut tx, id).await?;
            sqlx::query("UPDATE write_tasks SET phase='active',context=?,instance=?,revision=revision+1,reason=NULL,updated_at=? WHERE id=?")
                .bind(encode(context)?).bind(&self.instance).bind(chrono::Utc::now().timestamp()).bind(id).execute(&mut *tx).await.map_err(db)?;
            audit(&mut tx, None, &format!("write_task_authorized:{id}")).await?;
        }
        let result = summary(&task(&mut tx, id).await?)?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }

    fn active(&self, row: &SqliteRow, context: &TaskContext) -> Result<(), CoreError> {
        context.validate()?;
        self.checked_scope(row)?;
        if row.get::<&str, _>("phase") != "active"
            || row.get::<&str, _>("instance") != self.instance
        {
            return Err(denied("write_task_not_active"));
        }
        if decode::<TaskContext>(row.get("context"))? != *context {
            return Err(denied("write_task_context"));
        }
        Ok(())
    }

    /// Admit immutable captured intent under the scope without a per-edit prompt.
    ///
    /// The application must prepare from current registrations/source under the
    /// shared writer boundary. Admission never performs a provider write. Exact
    /// replay returns the persisted operation without changing its authority.
    ///
    /// # Errors
    /// Rejects stale context, out-of-scope/unsupported inverse, reused legacy ID,
    /// unresolved predecessor, quota exhaustion, stale capture or altered replay.
    pub async fn admit(
        &self,
        id: &str,
        context: &TaskContext,
        p: Preview,
    ) -> Result<Operation, CoreError> {
        self.admit_bound(id, context, p, None).await
    }

    /// Atomically admit a fresh inverse and retain its authoritative original link.
    ///
    /// Load/capture source through the application before calling this method.
    /// The original task must be closed; applied members require reverse order.
    /// Failed, expired or rejected attempts retain their links but permit a new
    /// attempt under fresh task authority. Pending/applied inverses block duplicates.
    /// Redo (compensating an inverse) is outside this initial profile.
    ///
    /// # Examples
    /// ```no_run
    /// # async fn example(tasks: &secretary_core::journal::write_tasks::JournalWriteTasks,
    /// # context: &secretary_core::domain::write_task::TaskContext,
    /// # preview: secretary_core::domain::operation::Preview) -> Result<(), secretary_core::CoreError> {
    /// tasks.admit_inverse("undo-task", context, "original-id", preview).await?;
    /// # Ok(()) }
    /// ```
    /// # Errors
    /// Rejects unsupported/unresolved originals, stale task authority, changed
    /// inverse evidence, out-of-order undo, duplicate inverse or storage failure.
    pub async fn admit_inverse(
        &self,
        id: &str,
        context: &TaskContext,
        original_id: &str,
        p: Preview,
    ) -> Result<Operation, CoreError> {
        self.admit_bound(id, context, p, Some(original_id)).await
    }

    async fn admit_bound(
        &self,
        id: &str,
        context: &TaskContext,
        mut p: Preview,
        original_id: Option<&str>,
    ) -> Result<Operation, CoreError> {
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, id).await?;
        self.active(&row, context)?;
        let scope = self.checked_scope(&row)?;
        check_undo_scope(&mut tx, &scope, original_id).await?;
        scope.permits(&p, &self.owner)?;
        if !valid_id(&p.operation_id) || p.source_revision.is_empty() || encode(&p)?.len() > 393216
        {
            return Err(CoreError::invalid());
        }
        p.preview_digest.clear();
        p.preview_digest = digest(&serde_json::to_value(&p).map_err(|_| storage())?);
        let original = if let Some(original_id) = original_id {
            Some(check_inverse(&mut tx, original_id, &p, &self.owner).await?)
        } else {
            None
        };
        let prior_link: Option<String> = sqlx::query_scalar(
            "SELECT original_id FROM operation_compensations WHERE inverse_id=?",
        )
        .bind(&p.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if let Some(old)=sqlx::query("SELECT o.*,e.preview,l.task_id FROM operations o LEFT JOIN operation_evidence e USING(operation_id) LEFT JOIN write_task_operations l ON l.operation_id=o.operation_id WHERE o.operation_id=?")
            .bind(&p.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            super::evidence::retained(&old)?;
            if old.get::<Option<String>,_>("task_id").as_deref()!=Some(id) || old.get::<&str,_>("preview")!=encode(&p)? || prior_link.as_deref()!=original_id {
                return Err(denied("operation_conflict"));
            }
            return Ok(Operation {preview:p,state:old.get::<&str,_>("state").parse()?});
        }
        let now = chrono::Utc::now().timestamp();
        if p.expires_at <= now || p.expires_at > now + 300 {
            return Err(denied("write_task_capture_expired"));
        }
        let admitted = row.get::<i64, _>("admitted");
        if admitted >= i64::from(scope.max_operations) {
            return Err(denied("write_task_limit"));
        }
        let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id WHERE l.task_id=? AND (o.state IN ('previewed','applying','uncertain','reconciliation','conflict') OR (o.state='known_failure' AND l.grant_revision=?))")
            .bind(id).bind(row.get::<i64,_>("revision")).fetch_one(&mut *tx).await.map_err(db)?;
        if pending != 0 {
            return Err(denied("write_task_predecessor"));
        }
        // A separate task/ID cannot evade an unresolved earlier write to this
        // physical source, including a pre-existing legacy operation.
        source_clear(&mut tx, &p).await?;
        super::evidence::insert(&mut tx, &p).await?;
        sqlx::query("INSERT INTO write_task_operations VALUES(?,?,?,?,?,?)")
            .bind(&p.operation_id)
            .bind(id)
            .bind(admitted + 1)
            .bind(row.get::<i64, _>("revision"))
            .bind(&p.preview_digest)
            .bind(physical_id(&p))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE write_tasks SET admitted=admitted+1,updated_at=? WHERE id=?")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit(&mut tx, Some(&p.operation_id), "previewed").await?;
        if let Some(original) = original {
            sqlx::query("INSERT INTO operation_compensations VALUES(?,?,?,?)")
                .bind(&p.operation_id)
                .bind(&original.preview.operation_id)
                .bind(&original.preview.preview_digest)
                .bind(&p.preview_digest)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            audit(&mut tx, Some(&p.operation_id), "compensation_linked").await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(Operation {
            preview: p,
            state: OperationState::Previewed,
        })
    }

    /// Discard an unsent preview when the agent refines its in-scope approach.
    ///
    /// The immutable spent ID/evidence and operation count remain retained. The
    /// active task still permits a fresh capture without another owner approval.
    ///
    /// # Errors
    /// Rejects stale authority, wrong membership and any already-dispatched member.
    pub async fn discard(
        &self,
        id: &str,
        context: &TaskContext,
        operation_id: &str,
    ) -> Result<Operation, CoreError> {
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, id).await?;
        self.active(&row, context)?;
        let member=sqlx::query("SELECT o.*,e.preview,l.grant_revision FROM operations o LEFT JOIN operation_evidence e USING(operation_id) JOIN write_task_operations l ON l.operation_id=o.operation_id WHERE l.task_id=? AND o.operation_id=?")
            .bind(id).bind(operation_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||denied("write_task_membership"))?;
        if member.get::<i64, _>("grant_revision") != row.get::<i64, _>("revision") {
            return Err(denied("write_task_stale_member"));
        }
        let operation = super::evidence::retained(&member)?;
        let before = operation.state;
        let state = before.transition(OperationEvent::Reject)?;
        if before != state {
            sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE operation_id=?")
                .bind(state.as_str())
                .bind(chrono::Utc::now().timestamp())
                .bind(operation_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            audit(&mut tx, Some(operation_id), "rejected").await?;
        }
        let preview = operation.preview;
        tx.commit().await.map_err(db)?;
        Ok(Operation { preview, state })
    }

    /// Atomically derive approval from the live grant and persist applying.
    ///
    /// Returns only once for a member. A future executor must also enforce live
    /// native invocation authority and current-source preflight before its send.
    ///
    /// # Errors
    /// Rejects stale/revoked grants, changed scope, wrong membership, spent member,
    /// stale capture or unresolved source. Never authorizes a legacy operation.
    pub async fn claim(
        &self,
        id: &str,
        context: &TaskContext,
        operation_id: &str,
    ) -> Result<Operation, CoreError> {
        self.claim_bound(id, context, operation_id, None).await
    }

    async fn claim_bound(
        &self,
        id: &str,
        context: &TaskContext,
        operation_id: &str,
        revision: Option<u64>,
    ) -> Result<Operation, CoreError> {
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, id).await?;
        self.active(&row, context)?;
        if revision.is_some_and(|expected| {
            u64::try_from(row.get::<i64, _>("revision")).ok() != Some(expected)
        }) {
            return Err(denied("write_task_stale_member"));
        }

        let member=sqlx::query("SELECT o.*,e.preview,l.grant_revision,l.preview_digest AS member_digest FROM operations o LEFT JOIN operation_evidence e USING(operation_id) JOIN write_task_operations l ON l.operation_id=o.operation_id WHERE l.task_id=? AND o.operation_id=?")
            .bind(id).bind(operation_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||denied("write_task_membership"))?;
        let p = super::evidence::retained(&member)?.preview;
        let mut sealed = p.clone();
        sealed.preview_digest.clear();
        if p.preview_digest != member.get::<&str, _>("member_digest")
            || p.preview_digest != digest(&serde_json::to_value(sealed).map_err(|_| storage())?)
        {
            return Err(storage());
        }
        if member.get::<i64, _>("grant_revision") != row.get::<i64, _>("revision") {
            return Err(denied("write_task_stale_member"));
        }
        let scope = self.checked_scope(&row)?;
        scope.permits(&p, &self.owner)?;
        check_inverse_link(&mut tx, &p, &self.owner, &scope).await?;
        let state: OperationState = member.get::<&str, _>("state").parse()?;
        let approved = state.transition(OperationEvent::Approve)?;
        source_clear(&mut tx, &p).await?;
        let now = chrono::Utc::now().timestamp();
        if p.expires_at <= now {
            approved.transition(OperationEvent::Expire)?;
            sqlx::query("UPDATE operations SET state='expired',updated_at=? WHERE operation_id=?")
                .bind(now)
                .bind(operation_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            audit(&mut tx, Some(operation_id), "expired").await?;
            tx.commit().await.map_err(db)?;
            return Err(denied("write_task_capture_expired"));
        }
        let state = approved.transition(OperationEvent::BeginApply)?;
        sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE operation_id=?")
            .bind(state.as_str())
            .bind(now)
            .bind(operation_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit(&mut tx, Some(operation_id), "approved").await?;
        audit(&mut tx, Some(operation_id), "task_authorized").await?;
        audit(&mut tx, Some(operation_id), "applying").await?;
        tx.commit().await.map_err(db)?;
        Ok(Operation { preview: p, state })
    }

    async fn permit_dispatch(
        &self,
        member: &crate::application::task_authority::TaskMember,
        expected: &Preview,
    ) -> Result<(), CoreError> {
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, &member.task_id).await?;
        self.active(&row, &member.context)?;
        if u64::try_from(row.get::<i64, _>("revision")).ok() != Some(member.grant_revision) {
            return Err(denied("write_task_stale_member"));
        }
        let link=sqlx::query("SELECT grant_revision,preview_digest FROM write_task_operations WHERE task_id=? AND operation_id=?")
            .bind(&member.task_id).bind(&member.operation_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||denied("write_task_membership"))?;
        let operation = load_operation(&mut tx, &member.operation_id).await?;
        if operation.state != OperationState::Applying
            || encode(&operation.preview)? != encode(expected)?
            || link.get::<i64, _>("grant_revision") != row.get::<i64, _>("revision")
            || link.get::<&str, _>("preview_digest") != expected.preview_digest
        {
            return Err(denied("write_task_stale_member"));
        }
        let scope = self.checked_scope(&row)?;
        scope.permits(expected, &self.owner)?;
        if expected.expires_at <= chrono::Utc::now().timestamp() {
            return Err(denied("write_task_capture_expired"));
        }
        check_inverse_link(&mut tx, expected, &self.owner, &scope).await?;
        source_clear_except(&mut tx, expected, &member.operation_id).await?;
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM write_task_dispatches WHERE operation_id=?")
                .bind(&member.operation_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if exists != 0 {
            return Err(denied("write_task_dispatch_spent"));
        }
        sqlx::query("INSERT INTO write_task_dispatches VALUES(?,?,?,?,?)")
            .bind(&member.operation_id)
            .bind(&member.task_id)
            .bind(row.get::<i64, _>("revision"))
            .bind(&expected.preview_digest)
            .bind(chrono::Utc::now().timestamp())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit(
            &mut tx,
            Some(&member.operation_id),
            "task_dispatch_accepted",
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    /// Revoke future execution, retaining all evidence and uncertain outcomes.
    ///
    /// # Errors
    /// Rejects another conversation; completion also rejects unresolved members.
    pub async fn end(
        &self,
        id: &str,
        context: &TaskContext,
        reason: TaskEnd,
    ) -> Result<Value, CoreError> {
        self.end_bound(id, context, reason, None).await
    }
    async fn end_bound(
        &self,
        id: &str,
        context: &TaskContext,
        reason: TaskEnd,
        expected: Option<u64>,
    ) -> Result<Value, CoreError> {
        context.validate()?;
        let mut tx = self.transaction().await?;
        let row = task(&mut tx, id).await?;
        if let Some(expected) = expected {
            let revision = u64::try_from(row.get::<i64, _>("revision")).map_err(|_| storage())?;
            let replay = row.get::<&str, _>("phase") == "closed"
                && expected.checked_add(1) == Some(revision);
            if decode::<TaskContext>(row.get("context"))? != *context
                || (revision != expected && !replay)
            {
                return Err(denied("write_task_stale_decision"));
            }
        }
        if !decode::<TaskContext>(row.get("context"))?.same_conversation(context) {
            return Err(denied("write_task_context"));
        }
        if row.get::<&str, _>("phase") != "closed" {
            if matches!(reason, TaskEnd::Completed) {
                let unresolved:i64=sqlx::query_scalar("SELECT COUNT(*) FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id WHERE l.task_id=? AND o.state NOT IN ('applied','known_failure','rejected','expired')")
                    .bind(id).fetch_one(&mut *tx).await.map_err(db)?;
                if unresolved != 0 {
                    return Err(denied("write_task_recovery_required"));
                }
            }
            close(
                &mut tx,
                id,
                match reason {
                    TaskEnd::Completed => "completed",
                    TaskEnd::Cancelled => "cancelled",
                },
            )
            .await?;
        }
        let result = summary(&task(&mut tx, id).await?)?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }

    /// Revoke the exact ended host context on a trusted lifecycle event.
    ///
    /// Detached old session-end notifications cannot cancel a newer conversation
    /// incarnation or a freshly confirmed grant in a replacement Gateway process.
    ///
    /// # Errors
    /// Rejects invalid host context or storage failure. This is not a model tool.
    /// A retained exact-context receipt also rejects delayed future proposals;
    /// it never selects a successor or clears operation recovery evidence.
    pub async fn invalidate_context(&self, context: &TaskContext) -> Result<(), CoreError> {
        context.validate()?;
        let mut tx = self.transaction().await?;
        let inserted = sqlx::query("INSERT OR IGNORE INTO write_task_ended_contexts(conversation,context,ended_at) VALUES(?,?,?)")
            .bind(self.conversation(context)).bind(encode(context)?).bind(chrono::Utc::now().timestamp())
            .execute(&mut *tx).await.map_err(db)?.rows_affected();
        if inserted != 0 {
            audit(&mut tx, None, "write_task_context_ended").await?;
        }
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM write_tasks WHERE conversation=? AND context=? AND phase<>'closed'",
        )
        .bind(self.conversation(context))
        .bind(encode(context)?)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        for id in ids {
            close(&mut tx, &id, "conversation_reset").await?;
        }
        tx.commit().await.map_err(db)
    }

    /// Read bounded inverse references without exposing retained source contents.
    ///
    /// # Examples
    /// ```no_run
    /// # async fn example(tasks: &secretary_core::journal::write_tasks::JournalWriteTasks)
    /// # -> Result<(), secretary_core::CoreError> {
    /// let page = tasks.inverse_history("original-id", 20, 0).await?;
    /// # Ok(()) }
    /// ```
    /// # Errors
    /// Rejects invalid ID/pagination, nonexistent original or storage failure.
    pub async fn inverse_history(
        &self,
        original_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Value, CoreError> {
        if !valid_id(original_id) || !(1..=20).contains(&limit) || offset > 100_000 {
            return Err(CoreError::invalid());
        }
        let mut tx = self.journal.pool.begin().await.map_err(db)?;
        let row = sqlx::query("SELECT o.*,e.preview FROM operations o LEFT JOIN operation_evidence e USING(operation_id) WHERE o.operation_id=?")
            .bind(original_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or(CoreError(ErrorKind::NotFound, "operation_not_found"))?;
        let original = super::evidence::public(&row)?;
        if original["actor"] != self.owner {
            return Err(denied("undo_unavailable"));
        }
        // Immutable IDs keep outcome updates from moving references between pages.
        let rows=sqlx::query("SELECT c.inverse_id,c.original_digest,c.inverse_digest,o.state,o.evidence_cleanup,l.task_id,l.grant_revision,l.ordinal FROM operation_compensations c JOIN operations o ON o.operation_id=c.inverse_id JOIN write_task_operations l ON l.operation_id=c.inverse_id WHERE c.original_id=? ORDER BY c.inverse_id LIMIT ? OFFSET ?")
            .bind(original_id).bind(i64::from(limit)+1).bind(i64::from(offset)).fetch_all(&mut *tx).await.map_err(db)?;
        let more = rows.len() > limit as usize;
        let attempts:Vec<Value>=rows.iter().take(limit as usize).map(|row|json!({"operation_id":row.get::<String,_>("inverse_id"),"task_id":row.get::<String,_>("task_id"),"grant_revision":row.get::<i64,_>("grant_revision"),"original_digest":row.get::<String,_>("original_digest"),"preview_digest":row.get::<String,_>("inverse_digest"),"state":row.get::<String,_>("state"),"evidence":if row.get::<Option<String>,_>("evidence_cleanup").is_some(){"unavailable"}else{"retained"}})).collect();
        tx.commit().await.map_err(db)?;
        Ok(
            json!({"original_id":original_id,"original_state":original["state"],"original_evidence":original.get("evidence").cloned().unwrap_or(json!({"status":"retained"})),"attempts":attempts,"next_offset":if more {Some(offset+limit)} else {None}}),
        )
    }

    /// Read compact owner-scoped task history without source previews.
    ///
    /// # Errors
    /// Rejects a limit outside 1–20, excessive offset or storage failure.
    pub async fn history(&self, limit: u32, offset: u32) -> Result<Value, CoreError> {
        if !(1..=20).contains(&limit) || offset > 100_000 {
            return Err(CoreError::invalid());
        }
        let mut tx = self.journal.pool.begin().await.map_err(db)?;
        let rows =
            sqlx::query("SELECT * FROM write_tasks ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
                .bind(i64::from(limit) + 1)
                .bind(i64::from(offset))
                .fetch_all(&mut *tx)
                .await
                .map_err(db)?;
        let more = rows.len() > limit as usize;
        let mut tasks = Vec::new();
        for row in rows.iter().take(limit as usize) {
            let mut record = summary(row)?;
            let counts=sqlx::query("SELECT o.state,COUNT(*) AS count FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id WHERE l.task_id=? GROUP BY o.state")
                .bind(row.get::<&str,_>("id")).fetch_all(&mut *tx).await.map_err(db)?;
            let mut outcomes = serde_json::Map::new();
            for count in counts {
                let state: OperationState = count.get::<&str, _>("state").parse()?;
                outcomes.insert(state.as_str().into(), json!(count.get::<i64, _>("count")));
            }
            record["operation_states"] = Value::Object(outcomes);
            let unavailable: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM write_task_operations l JOIN operations o USING(operation_id) WHERE l.task_id=? AND o.evidence_cleanup IS NOT NULL")
                .bind(row.get::<&str,_>("id")).fetch_one(&mut *tx).await.map_err(db)?;
            record["unavailable_evidence"] = json!(unavailable);
            tasks.push(record);
        }
        tx.commit().await.map_err(db)?;
        Ok(json!({"tasks":tasks,"next_offset":if more {Some(offset+limit)} else {None}}))
    }
}

async fn undo_next(
    tx: &mut Transaction<'_, Sqlite>,
    source: &UndoSource,
) -> Result<Value, CoreError> {
    let row = match source {
        UndoSource::Task { task_id } => {
            task(tx, task_id).await?;
            sqlx::query("SELECT o.*,e.preview FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id LEFT JOIN operation_evidence e ON e.operation_id=o.operation_id WHERE l.task_id=? AND o.state NOT IN ('known_failure','rejected','expired') AND NOT EXISTS(SELECT 1 FROM operation_compensations c JOIN operations i ON i.operation_id=c.inverse_id WHERE c.original_id=o.operation_id AND i.state='applied') ORDER BY l.ordinal DESC LIMIT 1")
                .bind(task_id).fetch_optional(&mut **tx).await.map_err(db)?
        }
        UndoSource::Operation { operation_id } => {
            let exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE operation_id=?")
                    .bind(operation_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(db)?;
            if exists == 0 {
                return Err(CoreError(ErrorKind::NotFound, "operation_not_found"));
            }
            sqlx::query("SELECT o.*,e.preview FROM operations o LEFT JOIN operation_evidence e USING(operation_id) WHERE o.operation_id=? AND NOT EXISTS(SELECT 1 FROM operation_compensations c JOIN operations i ON i.operation_id=c.inverse_id WHERE c.original_id=o.operation_id AND i.state='applied')")
                .bind(operation_id).fetch_optional(&mut **tx).await.map_err(db)?
        }
    };
    let Some(row) = row else {
        return Ok(Value::Null);
    };
    let original = super::evidence::public(&row)?;
    let original_id: String = row.get("operation_id");
    let attempt = sqlx::query("SELECT c.inverse_id,o.state FROM operation_compensations c JOIN operations o ON o.operation_id=c.inverse_id WHERE c.original_id=? AND o.state NOT IN ('known_failure','expired','rejected') LIMIT 1")
        .bind(&original_id).fetch_optional(&mut **tx).await.map_err(db)?;
    Ok(
        json!({"original_id":original_id,"state":row.get::<String,_>("state"),"evidence":if original.get("evidence").is_some(){"unavailable"}else{"retained"},"blocking_inverse":attempt.map(|r|json!({"operation_id":r.get::<String,_>("inverse_id"),"state":r.get::<String,_>("state")}))}),
    )
}

async fn check_undo_scope(
    tx: &mut Transaction<'_, Sqlite>,
    scope: &TaskScope,
    original_id: Option<&str>,
) -> Result<(), CoreError> {
    // Existing trusted library scopes omitted this optional selector. Public
    // native inverse capture below requires it before any source access.
    let Some(selection) = &scope.undo_of else {
        return Ok(());
    };
    let original_id = original_id.ok_or_else(|| denied("write_task_undo_scope"))?;
    let matches = match selection {
        UndoSource::Operation { operation_id } => operation_id == original_id,
        UndoSource::Task { task_id } => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM write_task_operations WHERE task_id=? AND operation_id=?",
            )
            .bind(task_id)
            .bind(original_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?
                == 1
        }
    };
    if !matches {
        return Err(denied("write_task_undo_scope"));
    }
    Ok(())
}

async fn check_inverse_link(
    tx: &mut Transaction<'_, Sqlite>,
    p: &Preview,
    owner: &str,
    scope: &TaskScope,
) -> Result<(), CoreError> {
    if let Some(link) = sqlx::query("SELECT original_id,original_digest,inverse_digest FROM operation_compensations WHERE inverse_id=?")
            .bind(&p.operation_id).fetch_optional(&mut **tx).await.map_err(db)? {
            check_undo_scope(tx, scope, Some(link.get("original_id"))).await?;
            let original = check_inverse(tx, link.get("original_id"), p, owner).await?;
            if original.preview.preview_digest != link.get::<&str,_>("original_digest") || p.preview_digest != link.get::<&str,_>("inverse_digest") {
                return Err(storage());
            }
        } else if scope.undo_of.is_some() {
            return Err(denied("write_task_undo_scope"));
        }
    Ok(())
}

async fn load_operation(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<Operation, CoreError> {
    let row = sqlx::query("SELECT o.*,e.preview FROM operations o LEFT JOIN operation_evidence e USING(operation_id) WHERE o.operation_id=?")
        .bind(id).fetch_optional(&mut **tx).await.map_err(db)?
        .ok_or(CoreError(ErrorKind::NotFound, "operation_not_found"))?;
    super::evidence::retained(&row)
}

async fn check_inverse(
    tx: &mut Transaction<'_, Sqlite>,
    original_id: &str,
    p: &Preview,
    owner: &str,
) -> Result<Operation, CoreError> {
    if !valid_id(original_id) {
        return Err(CoreError::invalid());
    }
    let original = load_operation(tx, original_id).await?;
    Inverse::from_applied(&original, &p.operation_id, owner)?.validate_preview(p)?;
    check_original_ready(tx, original_id, &p.operation_id).await?;
    Ok(original)
}

async fn check_original_ready(
    tx: &mut Transaction<'_, Sqlite>,
    original_id: &str,
    inverse_id: &str,
) -> Result<(), CoreError> {
    let nested: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM operation_compensations WHERE inverse_id=?")
            .bind(original_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
    if nested != 0 {
        return Err(denied("undo_inverse_not_supported"));
    }
    if let Some(member)=sqlx::query("SELECT l.task_id,l.ordinal,t.phase FROM write_task_operations l JOIN write_tasks t ON t.id=l.task_id WHERE l.operation_id=?")
        .bind(original_id).fetch_optional(&mut **tx).await.map_err(db)? {
        if member.get::<&str,_>("phase")!="closed" { return Err(denied("undo_original_task_active")); }
        let later:i64=sqlx::query_scalar("SELECT COUNT(*) FROM write_task_operations l JOIN operations o ON o.operation_id=l.operation_id WHERE l.task_id=? AND l.ordinal>? AND (o.state IN ('applying','uncertain','reconciliation','conflict') OR (o.state='applied' AND NOT EXISTS(SELECT 1 FROM operation_compensations c JOIN operations i ON i.operation_id=c.inverse_id WHERE c.original_id=l.operation_id AND i.state='applied')))")
            .bind(member.get::<&str,_>("task_id")).bind(member.get::<i64,_>("ordinal")).fetch_one(&mut **tx).await.map_err(db)?;
        if later!=0 { return Err(denied("undo_reverse_order")); }
    }
    let attempts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM operation_compensations c JOIN operations o ON o.operation_id=c.inverse_id WHERE c.original_id=? AND c.inverse_id<>? AND o.state NOT IN ('known_failure','expired','rejected')")
        .bind(original_id).bind(inverse_id).fetch_one(&mut **tx).await.map_err(db)?;
    if attempts != 0 {
        return Err(denied("undo_attempt_blocked"));
    }
    Ok(())
}

fn resource_kind(p: &Preview) -> &'static str {
    if p.resource_kind == crate::domain::operation::ResourceKind::GoogleDocs {
        "google_docs"
    } else {
        "sheets_grid"
    }
}
fn physical_id(p: &Preview) -> &str {
    if resource_kind(p) == "google_docs" {
        &p.record_id
    } else {
        p.record_id
            .rsplit_once(':')
            .map_or(p.record_id.as_str(), |(id, _)| id)
    }
}
async fn task(tx: &mut Transaction<'_, Sqlite>, id: &str) -> Result<SqliteRow, CoreError> {
    sqlx::query("SELECT * FROM write_tasks WHERE id=?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(CoreError(ErrorKind::NotFound, "write_task_not_found"))
}
fn summary(row: &SqliteRow) -> Result<Value, CoreError> {
    Ok(
        json!({"schema":"ciel.write-task.v1","task_id":row.get::<String,_>("id"),"scope":decode::<TaskScope>(row.get("scope"))?,"scope_digest":row.get::<String,_>("digest"),"phase":row.get::<String,_>("phase"),"grant_revision":row.get::<i64,_>("revision"),"admitted_operations":row.get::<i64,_>("admitted"),"prompt_expires_at":row.get::<i64,_>("prompt_expires"),"created_at":row.get::<i64,_>("created_at"),"ended_at":row.get::<Option<i64>,_>("ended_at"),"reason":row.get::<Option<String>,_>("reason")}),
    )
}
async fn audit(
    tx: &mut Transaction<'_, Sqlite>,
    id: Option<&str>,
    event: &str,
) -> Result<(), CoreError> {
    sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,?,?)")
        .bind(id)
        .bind(event)
        .bind(chrono::Utc::now().timestamp())
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}
async fn close(tx: &mut Transaction<'_, Sqlite>, id: &str, reason: &str) -> Result<(), CoreError> {
    retire_previews(tx, id).await?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE write_tasks SET phase='closed',revision=revision+1,reason=?,ended_at=?,updated_at=? WHERE id=? AND phase<>'closed'")
        .bind(reason).bind(now).bind(now).bind(id).execute(&mut **tx).await.map_err(db)?;
    audit(tx, None, &format!("write_task_closed:{id}:{reason}")).await
}

async fn retire_previews(tx: &mut Transaction<'_, Sqlite>, id: &str) -> Result<(), CoreError> {
    let ids=sqlx::query_scalar::<_,String>("SELECT o.operation_id FROM operations o JOIN write_task_operations l ON l.operation_id=o.operation_id WHERE l.task_id=? AND o.state='previewed' ORDER BY l.ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(db)?;
    let state = OperationState::Previewed.transition(OperationEvent::Reject)?;
    for operation in ids {
        sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE operation_id=?")
            .bind(state.as_str())
            .bind(chrono::Utc::now().timestamp())
            .bind(&operation)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        audit(tx, Some(&operation), "rejected").await?;
    }
    Ok(())
}

async fn source_clear(tx: &mut Transaction<'_, Sqlite>, p: &Preview) -> Result<(), CoreError> {
    source_clear_except(tx, p, "").await
}
async fn source_clear_except(
    tx: &mut Transaction<'_, Sqlite>,
    p: &Preview,
    except: &str,
) -> Result<(), CoreError> {
    // Legacy structured/tab records do not retain the physical spreadsheet ID
    // in record_id. Conservatively fence Sheets while any such write is unresolved.
    let prefix = format!("{}:", physical_id(p));
    let blocked:i64=sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE operation_id<>? AND state IN ('applying','uncertain','reconciliation','conflict') AND ((json_extract(identity,'$.resource_kind')='google_docs' AND ?='google_docs' AND json_extract(identity,'$.record_id')=?) OR (?<>'google_docs' AND COALESCE(json_extract(identity,'$.resource_kind'),'sheets')<>'google_docs' AND (COALESCE(json_extract(identity,'$.resource_kind'),'sheets')<>'sheets_grid' OR substr(json_extract(identity,'$.record_id'),1,length(?))=?)))")
        .bind(except).bind(resource_kind(p)).bind(&p.record_id).bind(resource_kind(p)).bind(&prefix).bind(&prefix).fetch_one(&mut **tx).await.map_err(db)?;
    if blocked != 0 {
        return Err(denied("write_task_source_unresolved"));
    }
    Ok(())
}

pub(super) async fn linked(journal: &Journal, id: &str) -> Result<bool, CoreError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='write_task_operations'",
    )
    .fetch_one(&journal.pool)
    .await
    .map_err(db)?;
    if exists == 0 {
        return Ok(false);
    }
    Ok(
        sqlx::query("SELECT operation_id FROM write_task_operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&journal.pool)
            .await
            .map_err(db)?
            .is_some(),
    )
}

pub(super) async fn linked_in(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<bool, CoreError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='write_task_operations'",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    if exists == 0 {
        return Ok(false);
    }
    Ok(
        sqlx::query("SELECT operation_id FROM write_task_operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .is_some(),
    )
}

pub(super) async fn suspend_on_outcome(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    state: OperationState,
) -> Result<(), CoreError> {
    if state == OperationState::Applied {
        return Ok(());
    }
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='write_task_operations'",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    if exists == 0 {
        return Ok(());
    }
    sqlx::query("UPDATE write_tasks SET phase='suspended',revision=revision+1,reason=?,updated_at=? WHERE phase='active' AND id IN (SELECT task_id FROM write_task_operations WHERE operation_id=?)")
        .bind(state.as_str()).bind(chrono::Utc::now().timestamp()).bind(id).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}

impl crate::application::task_authority::TaskLedger for JournalWriteTasks {
    fn cleanup<'a>(
        &'a self,
        command: &'a crate::domain::cleanup::CleanupCommand,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(self.cleanup(command))
    }

    fn owner(&self) -> &str {
        &self.owner
    }
    fn operations(&self) -> std::sync::Arc<dyn crate::application::ports::OperationStore> {
        std::sync::Arc::new(self.journal.clone())
    }
    fn propose<'a>(
        &'a self,
        context: &'a TaskContext,
        scope: &'a TaskScope,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(self.propose(context, scope))
    }
    fn review<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async move {
            context.validate()?;
            let mut tx = self.transaction().await?;
            let row = task(&mut tx, id).await?;
            let scope = self.checked_scope(&row)?;
            let old: TaskContext = decode(row.get("context"))?;
            if !old.same_conversation(context) {
                return Err(denied("write_task_context"));
            }
            // Inspection cannot revoke a replacement grant when an old network
            // request arrives late. Rebinding happens only in revision-bound decide.
            let mut summary = summary(&row)?;
            summary["requires_confirmation"] = json!(
                old != *context
                    || row.get::<&str, _>("instance") != self.instance
                    || row.get::<&str, _>("phase") == "suspended"
            );
            if let Some(selection) = &scope.undo_of {
                summary["undo_next"] = undo_next(&mut tx, selection).await?;
            }
            tx.commit().await.map_err(db)?;
            Ok(summary)
        })
    }
    fn decide<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        hash: &'a str,
        revision: u64,
        decision: TaskDecision,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(self.decide(id, context, hash, revision, decision))
    }
    fn end<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        reason: TaskEnd,
        revision: u64,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(self.end_bound(id, context, reason, Some(revision)))
    }
    fn invalidate<'a>(
        &'a self,
        context: &'a TaskContext,
    ) -> crate::application::ports::PortFuture<'a, Result<(), CoreError>> {
        Box::pin(self.invalidate_context(context))
    }
    fn scope<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
    ) -> crate::application::ports::PortFuture<'a, Result<TaskScope, CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            let row = task(&mut tx, id).await?;
            self.active(&row, context)?;
            let scope = self.checked_scope(&row)?;
            tx.commit().await.map_err(db)?;
            Ok(scope)
        })
    }
    fn admit<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        preview: Preview,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.admit(id, context, preview))
    }
    fn inverse_original<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        original_id: &'a str,
        operation_id: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            let row = task(&mut tx, id).await?;
            self.active(&row, context)?;
            let scope = self.checked_scope(&row)?;
            if scope.undo_of.is_none() {
                return Err(denied("write_task_undo_scope"));
            }
            check_undo_scope(&mut tx, &scope, Some(original_id)).await?;
            let original = load_operation(&mut tx, original_id).await?;
            Inverse::from_applied(&original, operation_id, &self.owner)?;
            check_original_ready(&mut tx, original_id, operation_id).await?;
            tx.commit().await.map_err(db)?;
            Ok(original)
        })
    }
    fn admit_inverse<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        original_id: &'a str,
        preview: Preview,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.admit_inverse(id, context, original_id, preview))
    }
    fn discard<'a>(
        &'a self,
        id: &'a str,
        context: &'a TaskContext,
        operation_id: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.discard(id, context, operation_id))
    }
    fn claim<'a>(
        &'a self,
        member: &'a crate::application::task_authority::TaskMember,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.claim_bound(
            &member.task_id,
            &member.context,
            &member.operation_id,
            Some(member.grant_revision),
        ))
    }
    fn permit<'a>(
        &'a self,
        member: &'a crate::application::task_authority::TaskMember,
        preview: &'a Preview,
    ) -> crate::application::ports::PortFuture<'a, Result<(), CoreError>> {
        Box::pin(self.permit_dispatch(member, preview))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> TaskContext {
        TaskContext {
            gateway_instance: "gateway".into(),
            agent_id: "agent".into(),
            session_key: "conversation".into(),
            session_id: "incarnation".into(),
        }
    }
    fn scope() -> TaskScope {
        decode(r#"{"objective":"Correct spelling","mapping_version":"v1","max_operations":2,"targets":[{"kind":"document","document":"notes","document_id":"document-id","tab_id":"t.0"}]}"#).unwrap()
    }
    fn preview() -> Preview {
        Preview {
            resource_kind: crate::domain::operation::ResourceKind::GoogleDocs,
            operation_id: "edit".into(),
            actor: "owner".into(),
            spreadsheet: "notes".into(),
            view: "t.0".into(),
            action: "replace_text".into(),
            record_id: "document-id".into(),
            fields: json!({"find":"old","replacement":"new"}),
            before: json!({"text":"old\n"}),
            after: json!({"text":"new\n"}),
            mapping_version: "v1".into(),
            source_revision: "current".into(),
            expires_at: chrono::Utc::now().timestamp() + 300,
            preview_digest: String::new(),
        }
    }

    #[tokio::test]
    async fn initial_dialog_expires_but_active_permission_has_no_five_minute_countdown() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal.clone(), "owner", "v1")
            .await
            .unwrap();
        let ctx = context();
        let proposal = ledger.propose(&ctx, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        sqlx::query("UPDATE write_tasks SET prompt_expires=1 WHERE id=?")
            .bind(id)
            .execute(&journal.pool)
            .await
            .unwrap();
        assert_eq!(
            ledger
                .decide(id, &ctx, hash, 0, TaskDecision::Approve)
                .await
                .err()
                .unwrap()
                .1,
            "write_task_prompt_expired"
        );
        assert_eq!(
            ledger.history(1, 0).await.unwrap()["tasks"][0]["phase"],
            "proposed"
        );
        ledger.end(id, &ctx, TaskEnd::Cancelled).await.unwrap();
        let proposal = ledger.propose(&ctx, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        ledger
            .decide(id, &ctx, hash, 0, TaskDecision::Approve)
            .await
            .unwrap();
        // Age metadata after authorization rather than modifying the system clock
        // or the immutable source capture. Only captures retain a freshness limit.
        sqlx::query("UPDATE write_tasks SET created_at=1,updated_at=1,prompt_expires=1 WHERE id=?")
            .bind(id)
            .execute(&journal.pool)
            .await
            .unwrap();
        ledger.admit(id, &ctx, preview()).await.unwrap();
        ledger.claim(id, &ctx, "edit").await.unwrap();
        assert_eq!(
            ledger.history(20, 0).await.unwrap()["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["task_id"] == id)
                .unwrap()["grant_revision"],
            1
        );
    }

    #[tokio::test]
    async fn changed_persisted_scope_is_not_authorized_under_an_old_digest() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal.clone(), "owner", "v1")
            .await
            .unwrap();
        let ctx = context();
        let proposal = ledger.propose(&ctx, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        let mut altered = scope();
        altered.max_operations = 1000;
        sqlx::query("UPDATE write_tasks SET scope=? WHERE id=?")
            .bind(encode(&altered).unwrap())
            .bind(id)
            .execute(&journal.pool)
            .await
            .unwrap();
        assert_eq!(
            ledger
                .decide(id, &ctx, hash, 0, TaskDecision::Approve)
                .await
                .err()
                .unwrap()
                .1,
            "journal_unavailable"
        );
        assert!(journal.find("edit").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn legacy_transition_rechecks_membership_after_an_optimistic_entry_check() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal.clone(), "owner", "v1")
            .await
            .unwrap();
        let ctx = context();
        // Model the interleaving: an entry check sees no operation yet, then the
        // other request atomically persists task intent before the transition.
        assert!(!linked(&journal, "edit").await.unwrap());
        let proposal = ledger.propose(&ctx, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        ledger
            .decide(id, &ctx, hash, 0, TaskDecision::Approve)
            .await
            .unwrap();
        ledger.admit(id, &ctx, preview()).await.unwrap();
        let legacy = journal.transition("edit", OperationEvent::Approve).await;
        assert_eq!(
            journal.get("edit").await.unwrap().state,
            OperationState::Previewed,
            "a stale legacy entry check must not authorize the task-bound intent"
        );
        assert_eq!(
            legacy
                .expect_err("legacy path must reject task authority")
                .1,
            "task_authority_required"
        );
        ledger.claim(id, &ctx, "edit").await.unwrap();
    }

    #[tokio::test]
    async fn ended_context_cannot_create_a_delayed_proposal_after_invalidation() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal.clone(), "owner", "v1")
            .await
            .unwrap();
        let ctx = context();
        ledger.invalidate_context(&ctx).await.unwrap();
        assert_eq!(
            ledger.propose(&ctx, &scope()).await.unwrap_err().1,
            "write_task_context_ended"
        );
        assert_eq!(ledger.history(20, 0).await.unwrap()["tasks"], json!([]));
        assert!(journal.find("edit").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn ended_receipts_survive_restart_and_do_not_select_a_successor() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = context();
        {
            let journal = Journal::open(directory.path()).await.unwrap();
            let ledger = JournalWriteTasks::open(journal, "owner", "v1")
                .await
                .unwrap();
            ledger.invalidate_context(&ctx).await.unwrap();
            ledger.invalidate_context(&ctx).await.unwrap();
        }
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal.clone(), "owner", "v1")
            .await
            .unwrap();
        assert_eq!(
            ledger.propose(&ctx, &scope()).await.unwrap_err().1,
            "write_task_context_ended"
        );
        let mut next = ctx.clone();
        next.session_id = "successor".into();
        let proposal = ledger.propose(&next, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        ledger
            .decide(id, &next, hash, 0, TaskDecision::Approve)
            .await
            .unwrap();
        ledger.invalidate_context(&ctx).await.unwrap();
        ledger.admit(id, &next, preview()).await.unwrap();
        assert_eq!(
            ledger.history(20, 0).await.unwrap()["tasks"][0]["admitted_operations"],
            1
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM write_task_ended_contexts")
            .fetch_one(&journal.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn a_late_gateway_end_cannot_retire_a_freshly_confirmed_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let ledger = JournalWriteTasks::open(journal, "owner", "v1")
            .await
            .unwrap();
        let old = context();
        let proposal = ledger.propose(&old, &scope()).await.unwrap();
        let id = proposal["task_id"].as_str().unwrap();
        let hash = proposal["scope_digest"].as_str().unwrap();
        ledger
            .decide(id, &old, hash, 0, TaskDecision::Approve)
            .await
            .unwrap();
        let mut replacement = old.clone();
        replacement.gateway_instance = "replacement-gateway".into();
        ledger
            .decide(id, &replacement, hash, 1, TaskDecision::Approve)
            .await
            .unwrap();
        ledger.invalidate_context(&old).await.unwrap();
        ledger.admit(id, &replacement, preview()).await.unwrap();
        assert_eq!(
            ledger.history(20, 0).await.unwrap()["tasks"][0]["phase"],
            "active"
        );
        assert_eq!(
            ledger.propose(&old, &scope()).await.unwrap_err().1,
            "write_task_context_ended"
        );
    }
}
