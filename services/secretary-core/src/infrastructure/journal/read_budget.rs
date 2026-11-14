//! Durable task quotas in the shared operational journal; no source text is stored.
use super::Journal;
use crate::{
    application::{
        ports::PortFuture,
        read_budget::{ReadAdmission, ReadGrant, ReadTaskLedger, READ_TOOLS},
    },
    domain::{
        error::{CoreError, ErrorKind},
        read_budget::{ReadBudgetPolicy, ReadLimits, ReadUsage},
    },
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, Row, Sqlite, Transaction};
use std::sync::atomic::Ordering;

/// One trusted owner/deployment budget context held by the actual Core journal.
///
/// Construction validates policy and recovers orphan reservations conservatively.
/// A journal instance can attach this context only once; callers cannot reopen it
/// in place to reset counters. No method grants mutation/approval authority.
///
/// # Examples
/// ```no_run
/// # use secretary_core::{journal::{Journal, read_budget::JournalReadTasks}, domain::read_budget::ReadBudgetPolicy};
/// # async fn open(journal: Journal, policy: ReadBudgetPolicy) -> Result<(), secretary_core::CoreError> {
/// let ledger = JournalReadTasks::open(journal, "configured-owner", "deployment-v1", policy).await?;
/// # Ok(()) }
/// ```
pub struct JournalReadTasks {
    journal: Journal,
    scope: String,
    policy: ReadBudgetPolicy,
}

fn storage(_: sqlx::Error) -> CoreError {
    CoreError(ErrorKind::Unavailable, "read_budget_storage")
}
fn denied(code: &'static str) -> CoreError {
    CoreError(ErrorKind::Conflict, code)
}
fn count(row: &SqliteRow, name: &str) -> Result<u64, CoreError> {
    u64::try_from(row.try_get::<i64, _>(name).map_err(storage)?)
        .map_err(|_| denied("read_budget_corrupt"))
}
fn limits(row: &SqliteRow) -> Result<ReadLimits, CoreError> {
    let value: ReadLimits =
        serde_json::from_str(&row.try_get::<String, _>("limits").map_err(storage)?)
            .map_err(|_| denied("read_budget_corrupt"))?;
    value
        .validate()
        .map_err(|_| denied("read_budget_corrupt"))?;
    Ok(value)
}

impl JournalReadTasks {
    /// Bind validated policy and recover pending reservations after journal restart.
    ///
    /// # Errors
    /// Denies unsafe scope, duplicate attachment, changed policy, legacy state or IO.
    pub async fn open(
        journal: Journal,
        owner: &str,
        generation: &str,
        policy: ReadBudgetPolicy,
    ) -> Result<Self, CoreError> {
        policy
            .validate()
            .map_err(|code| CoreError(ErrorKind::InvalidInput, code))?;
        if owner.is_empty()
            || owner.len() > 256
            || owner.chars().any(char::is_control)
            || !crate::domain::service_config::revision(generation)
        {
            return Err(denied("read_budget_scope_invalid"));
        }
        if journal.budget_claimed.swap(true, Ordering::AcqRel) {
            return Err(denied("read_budget_already_attached"));
        }
        let scope = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(owner, generation))
                    .map_err(|_| denied("read_budget_scope_invalid"))?
            )
        );
        let policy_hash = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&policy).map_err(|_| denied("budget_policy_invalid"))?
            )
        );
        let mut tx = journal.pool.begin().await.map_err(storage)?;
        for statement in [
            "CREATE TABLE IF NOT EXISTS read_budget_scopes(scope TEXT PRIMARY KEY, policy TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS read_tasks(id TEXT PRIMARY KEY,scope TEXT NOT NULL,active INTEGER NOT NULL,limits TEXT NOT NULL,calls INTEGER NOT NULL DEFAULT 0,rows INTEGER NOT NULL DEFAULT 0,characters INTEGER NOT NULL DEFAULT 0,bytes INTEGER NOT NULL DEFAULT 256,notice INTEGER NOT NULL DEFAULT 0)",
            "CREATE UNIQUE INDEX IF NOT EXISTS read_task_active_scope ON read_tasks(scope) WHERE active=1",
            "CREATE TABLE IF NOT EXISTS read_reservations(id TEXT PRIMARY KEY,task TEXT NOT NULL,tool TEXT NOT NULL,state TEXT NOT NULL,rows INTEGER NOT NULL,characters INTEGER NOT NULL,bytes INTEGER NOT NULL)",
            "CREATE INDEX IF NOT EXISTS read_reservation_task_state ON read_reservations(task,state)",
        ] { sqlx::query(statement).execute(&mut *tx).await.map_err(storage)?; }
        // No source values, model prose, token bodies or key hashes enter tables.
        sqlx::query("INSERT OR IGNORE INTO read_budget_scopes(scope,policy) VALUES(?,?)")
            .bind(&scope)
            .bind(&policy_hash)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let stored: String =
            sqlx::query_scalar("SELECT policy FROM read_budget_scopes WHERE scope=?")
                .bind(&scope)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        if stored != policy_hash {
            return Err(denied("read_budget_policy_changed"));
        }
        let orphaned = sqlx::query("SELECT r.* FROM read_reservations r JOIN read_tasks t ON t.id=r.task WHERE t.scope=? AND r.state IN ('pending','executing') LIMIT 17")
            .bind(&scope).fetch_all(&mut *tx).await.map_err(storage)?;
        if orphaned.len() > 16 {
            return Err(denied("read_budget_corrupt"));
        }
        for row in orphaned {
            sqlx::query("UPDATE read_tasks SET rows=rows+?,characters=characters+?,bytes=bytes+? WHERE id=?")
                .bind(count(&row,"rows")? as i64).bind(count(&row,"characters")? as i64).bind(count(&row,"bytes")? as i64)
                .bind(row.get::<String,_>("task")).execute(&mut *tx).await.map_err(storage)?;
            sqlx::query("UPDATE read_reservations SET state='lost' WHERE id=?")
                .bind(row.get::<String, _>("id"))
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(Self {
            journal,
            scope,
            policy,
        })
    }

    async fn transaction(&self) -> Result<Transaction<'_, Sqlite>, CoreError> {
        let mut tx = self.journal.pool.begin().await.map_err(storage)?;
        // Acquire SQLite's writer lock before reading counters; cancellation drops
        // the sqlx transaction and rolls it back instead of leaking a manual BEGIN.
        sqlx::query("UPDATE read_budget_scopes SET policy=policy WHERE scope=?")
            .bind(&self.scope)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        Ok(tx)
    }
    async fn active(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Option<SqliteRow>, CoreError> {
        sqlx::query("SELECT * FROM read_tasks WHERE scope=? AND active=1")
            .bind(&self.scope)
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)
    }
    async fn reserved(
        tx: &mut Transaction<'_, Sqlite>,
        task: &str,
    ) -> Result<SqliteRow, CoreError> {
        sqlx::query("SELECT COUNT(*) AS outstanding,COALESCE(SUM(rows),0) AS rows,COALESCE(SUM(characters),0) AS characters,COALESCE(SUM(bytes),0) AS bytes FROM read_reservations WHERE task=? AND state IN ('pending','executing')")
            .bind(task).fetch_one(&mut **tx).await.map_err(storage)
    }
    fn grant(row: &SqliteRow) -> Result<ReadGrant, CoreError> {
        Ok(ReadGrant {
            id: row.try_get("id").map_err(storage)?,
            rows: count(row, "rows")?,
            document_characters: count(row, "characters")?,
            output_bytes: count(row, "bytes")?,
        })
    }
}

impl ReadTaskLedger for JournalReadTasks {
    fn create(&self, requested: ReadLimits) -> PortFuture<'_, Result<Value, CoreError>> {
        Box::pin(async move {
            requested
                .validate()
                .map_err(|code| CoreError(ErrorKind::InvalidInput, code))?;
            if !requested.within(&self.policy.ceiling) {
                return Err(denied("read_budget_ceiling"));
            }
            let mut tx = self.transaction().await?;
            if self.active(&mut tx).await?.is_some() {
                return Err(denied("read_task_already_active"));
            }
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO read_tasks(id,scope,active,limits) VALUES(?,?,1,?)")
                .bind(&id)
                .bind(&self.scope)
                .bind(
                    serde_json::to_string(&requested)
                        .map_err(|_| denied("budget_limits_invalid"))?,
                )
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(json!({"task_id":id,"active":true,"limits":requested,"notice_bytes_reserved":256}))
        })
    }

    fn increase(&self, requested: ReadLimits) -> PortFuture<'_, Result<Value, CoreError>> {
        Box::pin(async move {
            requested
                .validate()
                .map_err(|code| CoreError(ErrorKind::InvalidInput, code))?;
            let mut tx = self.transaction().await?;
            let row = self
                .active(&mut tx)
                .await?
                .ok_or(denied("read_task_required"))?;
            if !requested.within(&self.policy.ceiling) || !limits(&row)?.within(&requested) {
                return Err(denied("read_budget_increase_invalid"));
            }
            if requested != limits(&row)? && count(&row, "notice")? != 0 {
                let reserved = Self::reserved(&mut tx, &row.get::<String, _>("id")).await?;
                if requested
                    .output_bytes
                    .saturating_sub(count(&row, "bytes")?)
                    .saturating_sub(count(&reserved, "bytes")?)
                    < 256
                {
                    return Err(denied("read_budget_notice_capacity"));
                }
                // A real authorized increase may need another stop notice.
                // Reserve its bytes rather than resetting the old notice for free.
                sqlx::query("UPDATE read_tasks SET bytes=bytes+256,notice=0 WHERE id=?")
                    .bind(row.get::<String, _>("id"))
                    .execute(&mut *tx)
                    .await
                    .map_err(storage)?;
            }
            sqlx::query("UPDATE read_tasks SET limits=? WHERE id=?")
                .bind(
                    serde_json::to_string(&requested)
                        .map_err(|_| denied("budget_limits_invalid"))?,
                )
                .bind(row.get::<String, _>("id"))
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            self.status().await
        })
    }

    fn close(&self) -> PortFuture<'_, Result<Value, CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            if let Some(row) = self.active(&mut tx).await? {
                let id = row.get::<String, _>("id");
                if count(&Self::reserved(&mut tx, &id).await?, "outstanding")? != 0 {
                    return Err(denied("read_task_in_flight"));
                }
                sqlx::query("UPDATE read_tasks SET active=0 WHERE id=?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await
                    .map_err(storage)?;
            }
            tx.commit().await.map_err(storage)?;
            Ok(json!({"active":false}))
        })
    }

    fn status(&self) -> PortFuture<'_, Result<Value, CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            let result = if let Some(row) = self.active(&mut tx).await? {
                let id = row.get::<String, _>("id");
                let reserved = Self::reserved(&mut tx, &id).await?;
                json!({"active":true,"task_id":id,"limits":limits(&row)?,"charged":{"calls":count(&row,"calls")?,"rows":count(&row,"rows")?,"document_characters":count(&row,"characters")?,"output_bytes":count(&row,"bytes")?},
                "reserved":{"rows":count(&reserved,"rows")?,"document_characters":count(&reserved,"characters")?,"output_bytes":count(&reserved,"bytes")?},"in_flight":count(&reserved,"outstanding")?})
            } else {
                json!({"active":false})
            };
            tx.commit().await.map_err(storage)?;
            Ok(result)
        })
    }

    fn reserve<'a>(&'a self, tool: &'a str) -> PortFuture<'a, Result<ReadAdmission, CoreError>> {
        Box::pin(async move {
            if !READ_TOOLS.contains(&tool) {
                return Err(CoreError::invalid());
            }
            let mut tx = self.transaction().await?;
            let Some(row) = self.active(&mut tx).await? else {
                return Ok(ReadAdmission {
                    grant: None,
                    model_text: String::new(),
                    code: "read_task_required",
                });
            };
            let id = row.get::<String, _>("id");
            let limits = limits(&row)?;
            let reserved = Self::reserved(&mut tx, &id).await?;
            let remaining_rows = limits
                .rows
                .saturating_sub(count(&row, "rows")?)
                .saturating_sub(count(&reserved, "rows")?);
            let remaining_chars = limits
                .document_characters
                .saturating_sub(count(&row, "characters")?)
                .saturating_sub(count(&reserved, "characters")?);
            let remaining_bytes = limits
                .output_bytes
                .saturating_sub(count(&row, "bytes")?)
                .saturating_sub(count(&reserved, "bytes")?);
            let rows = if matches!(tool, "sheets_read" | "sheets_search") {
                remaining_rows.min(100)
            } else {
                0
            };
            let chars = if tool.starts_with("docs_") {
                remaining_chars.min(1_048_576)
            } else {
                0
            };
            if count(&row, "calls")? >= limits.calls
                || count(&reserved, "outstanding")? >= limits.in_flight
                || remaining_bytes < 256
                || (matches!(tool, "sheets_read" | "sheets_search") && rows == 0)
                || (tool.starts_with("docs_") && chars == 0)
            {
                let notice = if count(&row, "notice")? == 0 {
                    sqlx::query("UPDATE read_tasks SET notice=1 WHERE id=?")
                        .bind(&id)
                        .execute(&mut *tx)
                        .await
                        .map_err(storage)?;
                    json!({"ok":false,"error":{"code":"read_budget_increase_required","message":"Ask the owner for an operator-authorized budget increase","retryable":false}}).to_string()
                } else {
                    String::new()
                };
                tx.commit().await.map_err(storage)?;
                return Ok(ReadAdmission {
                    grant: None,
                    model_text: notice,
                    code: "read_budget_exhausted",
                });
            }
            let grant = ReadGrant {
                id: uuid::Uuid::new_v4().to_string(),
                rows,
                document_characters: chars,
                output_bytes: remaining_bytes.min(self.policy.request_output_bytes),
            };
            sqlx::query("INSERT INTO read_reservations(id,task,tool,state,rows,characters,bytes) VALUES(?,?,?,'pending',?,?,?)")
            .bind(&grant.id).bind(&id).bind(tool).bind(grant.rows as i64).bind(grant.document_characters as i64).bind(grant.output_bytes as i64).execute(&mut *tx).await.map_err(storage)?;
            sqlx::query("UPDATE read_tasks SET calls=calls+1 WHERE id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(ReadAdmission {
                grant: Some(grant),
                model_text: String::new(),
                code: "admitted",
            })
        })
    }

    fn claim<'a>(
        &'a self,
        id: &'a str,
        tool: &'a str,
    ) -> PortFuture<'a, Result<ReadGrant, CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            let row=sqlx::query("SELECT r.* FROM read_reservations r JOIN read_tasks t ON t.id=r.task WHERE r.id=? AND r.tool=? AND r.state='pending' AND t.scope=? AND t.active=1")
            .bind(id).bind(tool).bind(&self.scope).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or(denied("read_reservation_invalid"))?;
            let grant = Self::grant(&row)?;
            sqlx::query(
                "UPDATE read_reservations SET state='executing' WHERE id=? AND state='pending'",
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(grant)
        })
    }

    fn finish<'a>(
        &'a self,
        id: &'a str,
        usage: ReadUsage,
    ) -> PortFuture<'a, Result<(), CoreError>> {
        Box::pin(async move {
            let mut tx = self.transaction().await?;
            let row=sqlx::query("SELECT r.* FROM read_reservations r JOIN read_tasks t ON t.id=r.task WHERE r.id=? AND r.state='executing' AND t.scope=? AND t.active=1")
            .bind(id).bind(&self.scope).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or(denied("read_reservation_invalid"))?;
            let grant = Self::grant(&row)?;
            if usage.rows > grant.rows
                || usage.document_characters > grant.document_characters
                || usage.output_bytes > grant.output_bytes
            {
                return Err(denied("read_reservation_overrun"));
            }
            sqlx::query("UPDATE read_tasks SET rows=rows+?,characters=characters+?,bytes=bytes+? WHERE id=?")
            .bind(usage.rows as i64).bind(usage.document_characters as i64).bind(usage.output_bytes.max(256) as i64).bind(row.get::<String,_>("task")).execute(&mut *tx).await.map_err(storage)?;
            sqlx::query("UPDATE read_reservations SET state='settled' WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(())
        })
    }
}
