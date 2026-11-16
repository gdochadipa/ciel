//! Separate removable source payloads from permanent operation identity.
//!
//! This module introduces storage/read semantics only. No runtime cleanup command
//! exists yet: an owner-bound selection and reference check must precede removal.
use super::storage;
use crate::domain::{
    error::{CoreError, ErrorKind},
    operation::{digest, Operation, OperationState, Preview, ResourceKind},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    operation_id: String,
    actor: String,
    resource_kind: ResourceKind,
    spreadsheet: String,
    view: String,
    action: String,
    record_id: String,
    mapping_version: String,
    preview_digest: String,
}
impl From<&Preview> for Identity {
    fn from(p: &Preview) -> Self {
        Self {
            operation_id: p.operation_id.clone(),
            actor: p.actor.clone(),
            resource_kind: p.resource_kind,
            spreadsheet: p.spreadsheet.clone(),
            view: p.view.clone(),
            action: p.action.clone(),
            record_id: p.record_id.clone(),
            mapping_version: p.mapping_version.clone(),
            preview_digest: p.preview_digest.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Removal {
    version: u8,
    cleanup_id: String,
    removed_at: i64,
}

fn encoded(p: &Preview) -> Result<String, CoreError> {
    serde_json::to_string(&Identity::from(p)).map_err(|_| storage())
}

/// DDL and copying are one transaction: a failed upgrade leaves the legacy schema
/// and exact preview bytes intact. DROP COLUMN preserves operation IDs, foreign
/// keys, audit and task/link tables; no parent row is deleted or recreated.
pub(super) async fn initialize(pool: &SqlitePool) -> Result<(), CoreError> {
    let mut tx = pool.begin().await.map_err(|_| storage())?;
    let columns = sqlx::query("PRAGMA table_info(operations)")
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| storage())?;
    let legacy = columns
        .iter()
        .any(|row| row.get::<&str, _>("name") == "preview");
    if legacy {
        // Refuse an unrecognized partial/future layout rather than adopting it.
        if columns.len() != 5 {
            return Err(storage());
        }
        sqlx::query("CREATE TABLE journal_evidence_schema(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=1))").execute(&mut *tx).await.map_err(|_| storage())?;
        sqlx::query("INSERT INTO journal_evidence_schema VALUES(1,1)")
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        sqlx::query("ALTER TABLE operations ADD COLUMN identity TEXT")
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        sqlx::query("ALTER TABLE operations ADD COLUMN evidence_cleanup TEXT")
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        sqlx::query("CREATE TABLE operation_evidence(operation_id TEXT PRIMARY KEY REFERENCES operations(operation_id),preview TEXT NOT NULL)")
            .execute(&mut *tx).await.map_err(|_| storage())?;
        // Bounded batches avoid loading all retained source bodies into memory.
        let mut after: Option<String> = None;
        loop {
            let rows = sqlx::query("SELECT operation_id,request_digest,preview FROM operations WHERE (? IS NULL OR operation_id>?) ORDER BY operation_id LIMIT 64")
                .bind(&after).bind(&after).fetch_all(&mut *tx).await.map_err(|_| storage())?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                let p: Preview = serde_json::from_str(row.get("preview")).map_err(|_| storage())?;
                if p.operation_id != row.get::<&str, _>("operation_id")
                    || p.request_digest() != row.get::<&str, _>("request_digest")
                {
                    return Err(storage());
                }
                sqlx::query("UPDATE operations SET identity=? WHERE operation_id=?")
                    .bind(encoded(&p)?)
                    .bind(&p.operation_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| storage())?;
                sqlx::query("INSERT INTO operation_evidence VALUES(?,?)")
                    .bind(&p.operation_id)
                    .bind(row.get::<&str, _>("preview"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| storage())?;
            }
            after = rows.last().map(|row| row.get("operation_id"));
        }
        sqlx::query("ALTER TABLE operations DROP COLUMN preview")
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
    }
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM journal_evidence_schema WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| storage())?;
    if version != 1 {
        return Err(storage());
    }
    // Fail closed for missing bodies without a valid removal receipt, malformed
    // tombstones, unresolved outcomes or a retained payload/receipt contradiction.
    let mut after: Option<String> = None;
    loop {
        let rows = sqlx::query("SELECT o.*,e.preview FROM operations o LEFT JOIN operation_evidence e USING(operation_id) WHERE (? IS NULL OR o.operation_id>?) ORDER BY o.operation_id LIMIT 64")
            .bind(&after).bind(&after).fetch_all(&mut *tx).await.map_err(|_| storage())?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            public(row)?;
        }
        after = rows.last().map(|row| row.get("operation_id"));
    }
    tx.commit().await.map_err(|_| storage())
}

pub(super) async fn insert(tx: &mut Transaction<'_, Sqlite>, p: &Preview) -> Result<(), CoreError> {
    sqlx::query("INSERT INTO operations(operation_id,request_digest,identity,state,updated_at) VALUES(?,?,?,'previewed',?)")
        .bind(&p.operation_id).bind(p.request_digest()).bind(encoded(p)?).bind(chrono::Utc::now().timestamp())
        .execute(&mut **tx).await.map_err(|_| storage())?;
    sqlx::query("INSERT INTO operation_evidence VALUES(?,?)")
        .bind(&p.operation_id)
        .bind(serde_json::to_string(p).map_err(|_| storage())?)
        .execute(&mut **tx)
        .await
        .map_err(|_| storage())?;
    Ok(())
}

fn metadata(row: &SqliteRow) -> Result<(Identity, OperationState), CoreError> {
    let identity: Identity = serde_json::from_str(row.try_get("identity").map_err(|_| storage())?)
        .map_err(|_| storage())?;
    let request: &str = row.try_get("request_digest").map_err(|_| storage())?;
    let is_digest =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if identity.operation_id != row.get::<&str, _>("operation_id")
        || !crate::domain::write_model::valid_id(&identity.operation_id)
        || identity.actor.is_empty()
        || !is_digest(&identity.preview_digest)
        || !is_digest(request)
    {
        return Err(storage());
    }
    Ok((identity, row.get::<&str, _>("state").parse()?))
}

fn removal(row: &SqliteRow, state: OperationState) -> Result<Option<Removal>, CoreError> {
    let body: Option<&str> = row.try_get("preview").map_err(|_| storage())?;
    let receipt: Option<&str> = row.try_get("evidence_cleanup").map_err(|_| storage())?;
    match (body, receipt) {
        (Some(_), None) => Ok(None),
        (None, Some(receipt)) => {
            let receipt: Removal = serde_json::from_str(receipt).map_err(|_| storage())?;
            if receipt.version != 1
                || !crate::domain::write_model::valid_id(&receipt.cleanup_id)
                || receipt.removed_at <= 0
                || !matches!(
                    state,
                    OperationState::Applied
                        | OperationState::KnownFailure
                        | OperationState::Rejected
                        | OperationState::Expired
                )
            {
                return Err(storage());
            }
            Ok(Some(receipt))
        }
        _ => Err(storage()),
    }
}

pub(super) fn retained(row: &SqliteRow) -> Result<Operation, CoreError> {
    let (identity, state) = metadata(row)?;
    if removal(row, state)?.is_some() {
        return Err(CoreError(
            ErrorKind::Conflict,
            "operation_evidence_unavailable",
        ));
    }
    let preview: Preview = serde_json::from_str(row.get("preview")).map_err(|_| storage())?;
    // Identity and request digests survive payload removal. A split record must
    // never let a payload from a different intent confer execution authority.
    if Identity::from(&preview) != identity
        || preview.request_digest() != row.get::<&str, _>("request_digest")
    {
        return Err(storage());
    }
    let mut sealed = preview.clone();
    sealed.preview_digest.clear();
    if preview.preview_digest != digest(&serde_json::to_value(sealed).map_err(|_| storage())?) {
        return Err(storage());
    }
    Ok(Operation { preview, state })
}

pub(super) fn public(row: &SqliteRow) -> Result<Value, CoreError> {
    let (identity, state) = metadata(row)?;
    if let Some(receipt) = removal(row, state)? {
        let mut out = serde_json::to_value(identity).map_err(|_| storage())?;
        out["state"] = json!(state.as_str());
        out["request_digest"] = json!(row.get::<&str, _>("request_digest"));
        out["evidence"] = json!({"status":"unavailable","cleanup_id":receipt.cleanup_id,"removed_at":receipt.removed_at});
        if out["resource_kind"] == "google_docs" {
            let object = out.as_object_mut().ok_or_else(storage)?;
            let document = object.remove("spreadsheet").ok_or_else(storage)?;
            let tab = object.remove("view").ok_or_else(storage)?;
            object.insert("document".into(), document);
            object.insert("tab_id".into(), tab);
        }
        return Ok(out);
    }
    let operation = retained(row)?;
    Ok(operation.preview.public(operation.state.as_str()))
}

// No removal writer is exported here. Tests use private synthetic journals to
// exercise the future cleanup representation, never an operator's evidence.
#[cfg(test)]
mod tests;
