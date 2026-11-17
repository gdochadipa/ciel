//! One SQLite maintenance decision, not a provider workflow or a second executor.
use super::{db, decode, denied, encode, task, JournalWriteTasks};
use crate::domain::{
    cleanup::{self, CleanupCommand, CleanupDecision, CleanupSelection},
    error::{CoreError, ErrorKind},
    operation::OperationState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;

const MAX_MEMBERS: usize = 200;
const MAX_PENDING: i64 = 64;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    selection: CleanupSelection,
    members: Value,
    tasks: Value,
    estimated_reusable_payload_bytes: u64,
    applied_originals_losing_evidence: u64,
    inverse_payloads: u64,
}

impl JournalWriteTasks {
    /// Preview/decide exact owner-authorized evidence removal, with no provider IO.
    /// This trusted port must be called under the application's shared writer.
    /// # Errors
    /// Rejects unsafe selection/references, stale process/digest, storage failure
    /// or a conflicting decision. A committed replay only observes its receipt.
    pub async fn cleanup(&self, command: &CleanupCommand) -> Result<Value, CoreError> {
        let mut tx = self.transaction().await?;
        let result = match command {
            CleanupCommand::Preview { selection } => {
                let selection = selection.canonical(chrono::Utc::now().timestamp())?;
                let plan = self.cleanup_plan(&mut tx, &selection).await?;
                let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM evidence_cleanups WHERE instance=? AND decision='pending'")
                    .bind(&self.instance).fetch_one(&mut *tx).await.map_err(db)?;
                if count >= MAX_PENDING {
                    return Err(denied("cleanup_preview_capacity"));
                }
                let id = uuid::Uuid::new_v4().to_string();
                let digest = cleanup::preview_digest(
                    &id,
                    &self.owner,
                    &self.generation,
                    &self.instance,
                    &serde_json::to_value(&plan).map_err(|_| super::storage())?,
                );
                sqlx::query("INSERT INTO evidence_cleanups VALUES(?,?,?,?,?,?,'pending',?,NULL)")
                    .bind(&id)
                    .bind(&self.owner)
                    .bind(&self.generation)
                    .bind(&self.instance)
                    .bind(encode(&plan)?)
                    .bind(&digest)
                    .bind(chrono::Utc::now().timestamp())
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                super::audit(&mut tx, None, &format!("evidence_cleanup_previewed:{id}")).await?;
                self.cleanup_review(&mut tx, &id).await?
            }
            CleanupCommand::Inspect { cleanup_id } => {
                self.cleanup_review(&mut tx, cleanup_id).await?
            }
            CleanupCommand::Decide {
                cleanup_id,
                preview_digest,
                decision,
            } => {
                let row = self.cleanup_record(&mut tx, cleanup_id).await?;
                let stored = row.get::<&str, _>("decision");
                if preview_digest != row.get::<&str, _>("digest") {
                    return Err(denied("cleanup_digest_mismatch"));
                }
                if stored != "pending" {
                    let expected = match decision {
                        CleanupDecision::Approve => "applied",
                        CleanupDecision::Reject => "rejected",
                    };
                    if stored != expected {
                        return Err(denied("cleanup_decision_conflict"));
                    }
                    decode(row.get("result"))?
                } else {
                    if row.get::<&str, _>("instance") != self.instance {
                        return Err(denied("cleanup_confirmation_required"));
                    }
                    match decision {
                        CleanupDecision::Reject => {
                            let result = json!({"cleanup_id":cleanup_id,"preview_digest":preview_digest,"decision":"rejected","removed_operations":0});
                            sqlx::query("UPDATE evidence_cleanups SET decision='rejected',result=? WHERE id=?")
                                .bind(encode(&result)?).bind(cleanup_id).execute(&mut *tx).await.map_err(db)?;
                            super::audit(
                                &mut tx,
                                None,
                                &format!("evidence_cleanup_rejected:{cleanup_id}"),
                            )
                            .await?;
                            result
                        }
                        CleanupDecision::Approve => {
                            let plan: Plan = decode(row.get("plan"))?;
                            let fresh = self.cleanup_plan(&mut tx, &plan.selection).await?;
                            if encode(&fresh)? != encode(&plan)? {
                                return Err(denied("cleanup_selection_changed"));
                            }
                            self.remove_evidence(&mut tx, cleanup_id, preview_digest, &plan)
                                .await?
                        }
                    }
                }
            }
        };
        #[cfg(test)]
        if matches!(
            command,
            CleanupCommand::Decide {
                decision: CleanupDecision::Approve,
                ..
            }
        ) {
            tests::crash_boundary(self, "before_commit", &result);
        }
        tx.commit().await.map_err(db)?;
        #[cfg(test)]
        if matches!(
            command,
            CleanupCommand::Decide {
                decision: CleanupDecision::Approve,
                ..
            }
        ) {
            tests::crash_boundary(self, "after_commit", &result);
        }
        Ok(result)
    }

    async fn cleanup_record(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
    ) -> Result<sqlx::sqlite::SqliteRow, CoreError> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(CoreError::invalid());
        }
        let row = sqlx::query("SELECT * FROM evidence_cleanups WHERE id=?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .ok_or(CoreError(ErrorKind::NotFound, "cleanup_not_found"))?;
        if row.get::<&str, _>("owner") != self.owner
            || row.get::<&str, _>("generation") != self.generation
        {
            return Err(denied("cleanup_binding"));
        }
        let plan: Plan = decode(row.get("plan"))?;
        if cleanup::preview_digest(
            id,
            &self.owner,
            &self.generation,
            row.get("instance"),
            &serde_json::to_value(&plan).map_err(|_| super::storage())?,
        ) != row.get::<&str, _>("digest")
        {
            return Err(super::storage());
        }
        Ok(row)
    }
    async fn cleanup_review(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
    ) -> Result<Value, CoreError> {
        let row = self.cleanup_record(tx, id).await?;
        Ok(
            json!({"cleanup_id":id,"preview_digest":row.get::<&str,_>("digest"),"decision":row.get::<&str,_>("decision"),"requires_fresh_preview":row.get::<&str,_>("decision")=="pending" && row.get::<&str,_>("instance")!=self.instance,"plan":decode::<Value>(row.get("plan"))?,"result":row.get::<Option<&str>,_>("result").map(decode::<Value>).transpose()?,"limits":{"max_selectors":20,"max_operations":MAX_MEMBERS},"notice":"Removal ends retained evidence and conditional undo coverage. IDs, target identity, outcomes, links and native-memory observation references remain. Payload bytes are not physical file shrinkage; no native notes, backups or WAL are deleted."}),
        )
    }
    async fn cleanup_plan(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        selection: &CleanupSelection,
    ) -> Result<Plan, CoreError> {
        let active: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM write_tasks WHERE phase<>'closed'")
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if active != 0 {
            return Err(denied("cleanup_active_task"));
        }
        // Legacy recovery and task recovery both retain their source evidence.
        let unresolved:i64=sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE state NOT IN ('applied','known_failure','rejected','expired')").fetch_one(&mut **tx).await.map_err(db)?;
        if unresolved != 0 {
            return Err(denied("cleanup_recovery_required"));
        }
        let mut rows = Vec::new();
        let mut summaries = Vec::new();
        for id in &selection.task_ids {
            let selected = task(tx, id).await?;
            let scope = self.checked_scope(&selected)?;
            if selected.get::<&str, _>("phase") != "closed"
                || selected
                    .get::<Option<i64>, _>("ended_at")
                    .is_none_or(|time| time > selection.completed_before)
            {
                return Err(denied("cleanup_task_not_completed"));
            }
            summaries.push(json!({"task_id":id,"objective":scope.objective,"reason":selected.get::<Option<&str>,_>("reason"),"ended_at":selected.get::<Option<i64>,_>("ended_at"),"admitted_operations":selected.get::<i64,_>("admitted")}));
            let mut members=sqlx::query("SELECT o.*,e.preview,l.task_id FROM write_task_operations l JOIN operations o USING(operation_id) LEFT JOIN operation_evidence e USING(operation_id) WHERE l.task_id=? ORDER BY o.operation_id LIMIT 201")
                .bind(id).fetch_all(&mut **tx).await.map_err(db)?;
            rows.append(&mut members);
            if rows.len() > MAX_MEMBERS {
                return Err(denied("cleanup_selection_limit"));
            }
        }
        for id in &selection.operation_ids {
            let row=sqlx::query("SELECT o.*,e.preview,l.task_id FROM operations o LEFT JOIN operation_evidence e USING(operation_id) LEFT JOIN write_task_operations l USING(operation_id) WHERE o.operation_id=?")
                .bind(id).fetch_optional(&mut **tx).await.map_err(db)?.ok_or(CoreError(ErrorKind::NotFound,"operation_not_found"))?;
            if row.get::<Option<&str>, _>("task_id").is_some() {
                return Err(denied("cleanup_whole_task_required"));
            }
            if row.get::<i64, _>("updated_at") > selection.completed_before {
                return Err(denied("cleanup_age_mismatch"));
            }
            rows.push(row);
        }
        if rows.is_empty() {
            return Err(denied("cleanup_empty_selection"));
        }
        if rows.len() > MAX_MEMBERS {
            return Err(denied("cleanup_selection_limit"));
        }
        rows.sort_by(|a, b| a.get::<&str, _>("operation_id").cmp(b.get("operation_id")));
        let selected: HashSet<&str> = rows.iter().map(|r| r.get("operation_id")).collect();
        let mut members = Vec::new();
        let mut bytes = 0u64;
        let mut originals = 0u64;
        let mut inverses = 0u64;
        for row in &rows {
            let operation = super::super::evidence::retained(row)?;
            if operation.preview.actor != self.owner || !cleanup::eligible(operation.state) {
                return Err(denied("cleanup_operation_unavailable"));
            }
            let p = &operation.preview;
            let links=sqlx::query("SELECT original_id,inverse_id FROM operation_compensations WHERE original_id=? OR inverse_id=? LIMIT 201")
                .bind(&p.operation_id).bind(&p.operation_id).fetch_all(&mut **tx).await.map_err(db)?;
            if links.len() > MAX_MEMBERS {
                return Err(denied("cleanup_dependency_reference"));
            }
            if links.iter().any(|link| {
                !selected.contains(link.get::<&str, _>("original_id"))
                    || !selected.contains(link.get::<&str, _>("inverse_id"))
            }) {
                return Err(denied("cleanup_dependency_reference"));
            }
            let inverse = links
                .iter()
                .any(|link| link.get::<&str, _>("inverse_id") == p.operation_id);
            inverses += u64::from(inverse);
            originals += u64::from(!inverse && operation.state == OperationState::Applied);
            let identity = super::super::evidence::cleanup_identity(row)?;
            // Native note fences use only these permanent history fields. Their
            // reference is retained, not guessed absent or deleted. Consumers
            // needing the source payload (undo/recovery) are fenced above.
            if p.resource_kind == crate::domain::operation::ResourceKind::GoogleDocs {
                let old = p.public(operation.state.as_str());
                for key in [
                    "operation_id",
                    "resource_kind",
                    "document",
                    "tab_id",
                    "record_id",
                    "mapping_version",
                    "state",
                ] {
                    if old.get(key) != identity.get(key) {
                        return Err(denied("cleanup_native_memory_reference"));
                    }
                }
            }
            let payload_bytes = row.get::<&str, _>("preview").len() as u64;
            bytes = bytes
                .checked_add(payload_bytes)
                .ok_or_else(super::storage)?;
            members.push(json!({"identity":identity,"task_id":row.get::<Option<&str>,_>("task_id"),"payload_bytes":payload_bytes,"inverse":inverse}));
        }
        let members = json!(members);
        if encode(&members)?.len() > 262144 {
            return Err(denied("cleanup_selection_limit"));
        }
        let plan = Plan {
            selection: selection.clone(),
            members,
            tasks: json!(summaries),
            estimated_reusable_payload_bytes: bytes,
            applied_originals_losing_evidence: originals,
            inverse_payloads: inverses,
        };
        if encode(&plan)?.len() > 327680 {
            return Err(denied("cleanup_selection_limit"));
        }
        Ok(plan)
    }
    async fn remove_evidence(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        digest: &str,
        plan: &Plan,
    ) -> Result<Value, CoreError> {
        let before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
        let now = chrono::Utc::now().timestamp();
        let members = plan.members.as_array().ok_or_else(super::storage)?;
        for member in members {
            let operation_id = member["identity"]["operation_id"]
                .as_str()
                .ok_or_else(super::storage)?;
            let receipt = json!({"version":1,"cleanup_id":id,"removed_at":now});
            let removed = sqlx::query("DELETE FROM operation_evidence WHERE operation_id=?")
                .bind(operation_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
            if removed.rows_affected() != 1 {
                return Err(denied("cleanup_selection_changed"));
            }
            sqlx::query("UPDATE operations SET evidence_cleanup=? WHERE operation_id=? AND evidence_cleanup IS NULL")
                .bind(encode(&receipt)?).bind(operation_id).execute(&mut **tx).await.map_err(db)?;
            super::audit(tx, Some(operation_id), &format!("evidence_removed:{id}")).await?;
        }
        let after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
        let result = json!({"cleanup_id":id,"preview_digest":digest,"decision":"applied","removed_operations":members.len(),"removed_payload_bytes":plan.estimated_reusable_payload_bytes,"page_size":page_size,"reusable_pages_before":before,"reusable_pages_after":after,"explicit_physical_shrink":false,"explicit_wal_truncation":false,"backups_deleted":false,"native_memory_deleted":false});
        sqlx::query("UPDATE evidence_cleanups SET decision='applied',result=? WHERE id=? AND decision='pending'")
            .bind(encode(&result)?).bind(id).execute(&mut **tx).await.map_err(db)?;
        super::audit(tx, None, &format!("evidence_cleanup_approved:{id}")).await?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
