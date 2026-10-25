//! Durable immutable operation journal. SQLite is authoritative for intent and approval, never sheet cells.
use crate::ApiError;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
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
fn digest(v: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).unwrap_or_default())
    )
}
#[derive(Clone, Debug)]
pub struct Operation {
    pub preview: Preview,
    pub state: String,
}
#[derive(Clone)]
pub struct Journal {
    pool: SqlitePool,
    directory: PathBuf,
    _lock: std::sync::Arc<std::fs::File>,
}
fn storage() -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, "journal_unavailable")
}
fn conflict() -> ApiError {
    ApiError(StatusCode::CONFLICT, "operation_conflict")
}
impl Journal {
    pub async fn open(directory: &Path) -> Result<Self, ApiError> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if let Ok(meta) = std::fs::symlink_metadata(directory) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(storage());
            }
        } else {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|_| storage())?;
        }
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| storage())?;
        let lockpath = directory.join("secretary.lock");
        if let Ok(meta) = std::fs::symlink_metadata(&lockpath) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(storage());
            }
        }
        use std::os::unix::fs::OpenOptionsExt;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(lockpath)
            .map_err(|_| storage())?;
        lock.try_lock().map_err(|_| storage())?;
        let db = directory.join("secretary.sqlite3");
        if let Ok(meta) = std::fs::symlink_metadata(&db) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(storage());
            }
        } else {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&db)
                .map_err(|_| storage())?;
        }
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| storage())?;
        let options = SqliteConnectOptions::new()
            .filename(&db)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|_| storage())?;
        sqlx::query("CREATE TABLE IF NOT EXISTS operations(operation_id TEXT PRIMARY KEY, request_digest TEXT NOT NULL, preview TEXT NOT NULL, state TEXT NOT NULL, updated_at INTEGER NOT NULL)").execute(&pool).await.map_err(|_|storage())?;
        sqlx::query("CREATE TABLE IF NOT EXISTS audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT, operation_id TEXT, event TEXT NOT NULL, occurred_at INTEGER NOT NULL)").execute(&pool).await.map_err(|_|storage())?;
        let journal = Self {
            pool,
            directory: directory.to_owned(),
            _lock: std::sync::Arc::new(lock),
        };
        // A process may have stopped after sending a request. Never resend that intent.
        sqlx::query(
            "UPDATE operations SET state='reconciliation',updated_at=? WHERE state='applying'",
        )
        .bind(chrono::Utc::now().timestamp())
        .execute(&journal.pool)
        .await
        .map_err(|_| storage())?;
        journal.audit(None, "startup_recovery").await?;
        Ok(journal)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub async fn audit(&self, id: Option<&str>, event: &str) -> Result<(), ApiError> {
        sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,?,?)")
            .bind(id)
            .bind(event)
            .bind(chrono::Utc::now().timestamp())
            .execute(&self.pool)
            .await
            .map_err(|_| storage())?;
        Ok(())
    }
    pub async fn find(&self, id: &str) -> Result<Option<Operation>, ApiError> {
        let row = sqlx::query("SELECT preview,state FROM operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| storage())?;
        row.map(|r| {
            Ok(Operation {
                preview: serde_json::from_str(r.get::<&str, _>("preview"))
                    .map_err(|_| storage())?,
                state: r.get("state"),
            })
        })
        .transpose()
    }
    pub async fn get(&self, id: &str) -> Result<Operation, ApiError> {
        self.find(id)
            .await?
            .ok_or(ApiError(StatusCode::NOT_FOUND, "operation_not_found"))
    }
    pub async fn create(&self, mut p: Preview) -> Result<Preview, ApiError> {
        let mut tx = self.pool.begin().await.map_err(|_| storage())?;
        if let Some(row) =
            sqlx::query("SELECT request_digest,preview FROM operations WHERE operation_id=?")
                .bind(&p.operation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| storage())?
        {
            if row.get::<&str, _>("request_digest") != p.request_digest() {
                return Err(conflict());
            }
            return serde_json::from_str(row.get::<&str, _>("preview")).map_err(|_| storage());
        }
        p.preview_digest.clear();
        p.preview_digest = digest(&serde_json::to_value(&p).map_err(|_| storage())?);
        let encoded = serde_json::to_string(&p).map_err(|_| storage())?;
        sqlx::query("INSERT INTO operations(operation_id,request_digest,preview,state,updated_at) VALUES(?,?,?,'previewed',?)").bind(&p.operation_id).bind(p.request_digest()).bind(encoded).bind(chrono::Utc::now().timestamp()).execute(&mut *tx).await.map_err(|_|storage())?;
        sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,'previewed',?)")
            .bind(&p.operation_id)
            .bind(chrono::Utc::now().timestamp())
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        tx.commit().await.map_err(|_| storage())?;
        Ok(p)
    }
    async fn transition(
        &self,
        id: &str,
        expected: &[&str],
        state: &str,
    ) -> Result<Operation, ApiError> {
        let mut tx = self.pool.begin().await.map_err(|_| storage())?;
        let row = sqlx::query("SELECT state FROM operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| storage())?
            .ok_or(ApiError(StatusCode::NOT_FOUND, "operation_not_found"))?;
        if !expected.contains(&row.get::<&str, _>("state")) {
            return Err(conflict());
        }
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE operation_id=?")
            .bind(state)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,?,?)")
            .bind(id)
            .bind(state)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        tx.commit().await.map_err(|_| storage())?;
        self.get(id).await
    }
    pub async fn approve(
        &self,
        id: &str,
        preview_digest: &str,
        decision: &str,
    ) -> Result<Operation, ApiError> {
        let operation = self.get(id).await?;
        if operation.preview.preview_digest != preview_digest {
            self.audit(Some(id), "approval_digest_mismatch").await?;
            return Err(conflict());
        }
        if operation.preview.expires_at <= chrono::Utc::now().timestamp()
            && ["previewed", "approved"].contains(&operation.state.as_str())
        {
            self.transition(id, &["previewed", "approved"], "expired")
                .await?;
            return Err(ApiError(StatusCode::CONFLICT, "approval_expired"));
        }
        let state = match decision {
            "approve" => "approved",
            "reject" => "rejected",
            _ => return Err(ApiError::invalid()),
        };
        if operation.state == state {
            return Ok(operation);
        }
        self.transition(id, &["previewed"], state).await
    }
    pub async fn begin_apply(&self, id: &str) -> Result<Operation, ApiError> {
        let operation = self.get(id).await?;
        if operation.preview.expires_at <= chrono::Utc::now().timestamp()
            && operation.state == "approved"
        {
            self.transition(id, &["approved"], "expired").await?;
            return Err(ApiError(StatusCode::CONFLICT, "approval_expired"));
        }
        self.transition(id, &["approved"], "applying").await
    }
    pub async fn finish(&self, id: &str, state: &str) -> Result<Operation, ApiError> {
        if ![
            "applied",
            "known_failure",
            "uncertain",
            "conflict",
            "reconciliation",
        ]
        .contains(&state)
        {
            return Err(ApiError::invalid());
        }
        self.transition(id, &["applying", "uncertain", "reconciliation"], state)
            .await
    }
    pub async fn history(
        &self,
        operation: Option<&str>,
        limit: usize,
        cursor: usize,
    ) -> Result<Value, ApiError> {
        if limit == 0 || limit > 100 || cursor > 1_000_000 {
            return Err(ApiError::invalid());
        }
        if let Some(id) = operation {
            let o = self.get(id).await?;
            let audit=sqlx::query("SELECT sequence,event,occurred_at FROM audit WHERE operation_id=? ORDER BY sequence DESC LIMIT ? OFFSET ?").bind(id).bind((limit+1)as i64).bind(cursor as i64).fetch_all(&self.pool).await.map_err(|_|storage())?;
            let more = audit.len() > limit;
            let entries:Vec<_>=audit.into_iter().take(limit).map(|r|json!({"sequence":r.get::<i64,_>("sequence"),"event":r.get::<String,_>("event"),"occurred_at":r.get::<i64,_>("occurred_at")})).collect();
            return Ok(
                json!({"operation":o.preview.public(&o.state),"events":entries,"next_cursor":if more{Some((cursor+limit).to_string())}else{None}}),
            );
        }
        let rows=sqlx::query("SELECT preview,state FROM operations ORDER BY updated_at DESC,operation_id LIMIT ? OFFSET ?").bind((limit+1)as i64).bind(cursor as i64).fetch_all(&self.pool).await.map_err(|_|storage())?;
        let more = rows.len() > limit;
        let mut records = Vec::new();
        for row in rows.into_iter().take(limit) {
            let p: Preview = serde_json::from_str(row.get("preview")).map_err(|_| storage())?;
            records.push(p.public(row.get("state")))
        }
        Ok(
            json!({"operations":records,"next_cursor":if more{Some((cursor+limit).to_string())}else{None}}),
        )
    }
}
