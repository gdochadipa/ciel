//! Durable immutable operation journal. SQLite is authoritative for intent and approval, never sheet cells.
use crate::domain::error::ErrorKind;
pub use crate::domain::operation::Preview;
use crate::domain::{
    error::CoreError,
    operation::{digest, Operation, OperationEvent, OperationState, Outcome},
};
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
pub struct Journal {
    pool: SqlitePool,
    directory: PathBuf,
    _lock: std::sync::Arc<std::fs::File>,
}
fn storage() -> CoreError {
    CoreError(ErrorKind::Unavailable, "journal_unavailable")
}
fn conflict() -> CoreError {
    CoreError(ErrorKind::Conflict, "operation_conflict")
}
impl Journal {
    pub async fn open(directory: &Path) -> Result<Self, CoreError> {
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
            .truncate(false)
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
        sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE state=?")
            .bind(
                OperationState::Applying
                    .transition(OperationEvent::Recover)?
                    .as_str(),
            )
            .bind(chrono::Utc::now().timestamp())
            .bind(OperationState::Applying.as_str())
            .execute(&journal.pool)
            .await
            .map_err(|_| storage())?;
        journal.audit(None, "startup_recovery").await?;
        Ok(journal)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub async fn audit(&self, id: Option<&str>, event: &str) -> Result<(), CoreError> {
        sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,?,?)")
            .bind(id)
            .bind(event)
            .bind(chrono::Utc::now().timestamp())
            .execute(&self.pool)
            .await
            .map_err(|_| storage())?;
        Ok(())
    }
    pub async fn find(&self, id: &str) -> Result<Option<Operation>, CoreError> {
        let row = sqlx::query("SELECT preview,state FROM operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| storage())?;
        row.map(|r| {
            Ok(Operation {
                preview: serde_json::from_str(r.get::<&str, _>("preview"))
                    .map_err(|_| storage())?,
                state: r.get::<&str, _>("state").parse()?,
            })
        })
        .transpose()
    }
    pub async fn get(&self, id: &str) -> Result<Operation, CoreError> {
        self.find(id)
            .await?
            .ok_or(CoreError(ErrorKind::NotFound, "operation_not_found"))
    }
    pub async fn create(&self, mut p: Preview) -> Result<Preview, CoreError> {
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
    async fn transition(&self, id: &str, event: OperationEvent) -> Result<Operation, CoreError> {
        let mut tx = self.pool.begin().await.map_err(|_| storage())?;
        let row = sqlx::query("SELECT state FROM operations WHERE operation_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| storage())?
            .ok_or(CoreError(ErrorKind::NotFound, "operation_not_found"))?;
        let current: OperationState = row.get::<&str, _>("state").parse()?;
        let state = current.transition(event)?;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE operations SET state=?,updated_at=? WHERE operation_id=?")
            .bind(state.as_str())
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| storage())?;
        sqlx::query("INSERT INTO audit(operation_id,event,occurred_at) VALUES(?,?,?)")
            .bind(id)
            .bind(state.as_str())
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
    ) -> Result<Operation, CoreError> {
        let operation = self.get(id).await?;
        let event = match operation.approval_event(
            preview_digest,
            decision,
            chrono::Utc::now().timestamp(),
        ) {
            Ok(event) => event,
            Err(error) => {
                if error.1 == "operation_conflict" {
                    self.audit(Some(id), "approval_digest_mismatch").await?;
                }
                return Err(error);
            }
        };
        if matches!(event, OperationEvent::Expire) {
            self.transition(id, event).await?;
            return Err(CoreError(ErrorKind::Conflict, "approval_expired"));
        }
        self.transition(id, event).await
    }
    pub async fn begin_apply(&self, id: &str) -> Result<Operation, CoreError> {
        let operation = self.get(id).await?;
        let event = operation.apply_event(chrono::Utc::now().timestamp());
        if matches!(event, OperationEvent::Expire) {
            self.transition(id, event).await?;
            return Err(CoreError(ErrorKind::Conflict, "approval_expired"));
        }
        self.transition(id, event).await
    }
    pub async fn finish(&self, id: &str, outcome: Outcome) -> Result<Operation, CoreError> {
        self.transition(id, OperationEvent::Finish(outcome)).await
    }
    pub async fn history(
        &self,
        operation: Option<&str>,
        limit: usize,
        cursor: usize,
    ) -> Result<Value, CoreError> {
        if limit == 0 || limit > 100 || cursor > 1_000_000 {
            return Err(CoreError::invalid());
        }
        if let Some(id) = operation {
            let o = self.get(id).await?;
            let audit=sqlx::query("SELECT sequence,event,occurred_at FROM audit WHERE operation_id=? ORDER BY sequence DESC LIMIT ? OFFSET ?").bind(id).bind((limit+1)as i64).bind(cursor as i64).fetch_all(&self.pool).await.map_err(|_|storage())?;
            let more = audit.len() > limit;
            let entries:Vec<_>=audit.into_iter().take(limit).map(|r|json!({"sequence":r.get::<i64,_>("sequence"),"event":r.get::<String,_>("event"),"occurred_at":r.get::<i64,_>("occurred_at")})).collect();
            return Ok(
                json!({"operation":o.preview.public(o.state.as_str()),"events":entries,"next_cursor":if more{Some((cursor+limit).to_string())}else{None}}),
            );
        }
        let rows=sqlx::query("SELECT preview,state FROM operations ORDER BY updated_at DESC,operation_id LIMIT ? OFFSET ?").bind((limit+1)as i64).bind(cursor as i64).fetch_all(&self.pool).await.map_err(|_|storage())?;
        let more = rows.len() > limit;
        let mut records = Vec::new();
        for row in rows.into_iter().take(limit) {
            let p: Preview = serde_json::from_str(row.get("preview")).map_err(|_| storage())?;
            let state: OperationState = row.get::<&str, _>("state").parse()?;
            records.push(p.public(state.as_str()))
        }
        Ok(
            json!({"operations":records,"next_cursor":if more{Some((cursor+limit).to_string())}else{None}}),
        )
    }
}

impl crate::application::ports::OperationStore for Journal {
    fn audit<'a>(
        &'a self,
        id: Option<&'a str>,
        event: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<(), CoreError>> {
        Box::pin(self.audit(id, event))
    }
    fn find<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Option<Operation>, CoreError>> {
        Box::pin(self.find(id))
    }
    fn get<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.get(id))
    }
    fn create(
        &self,
        p: Preview,
    ) -> crate::application::ports::PortFuture<'_, Result<Preview, CoreError>> {
        Box::pin(self.create(p))
    }
    fn approve<'a>(
        &'a self,
        id: &'a str,
        digest: &'a str,
        decision: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.approve(id, digest, decision))
    }
    fn begin_apply<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.begin_apply(id))
    }
    fn finish<'a>(
        &'a self,
        id: &'a str,
        outcome: Outcome,
    ) -> crate::application::ports::PortFuture<'a, Result<Operation, CoreError>> {
        Box::pin(self.finish(id, outcome))
    }
    fn history<'a>(
        &'a self,
        id: Option<&'a str>,
        limit: usize,
        cursor: usize,
    ) -> crate::application::ports::PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(self.history(id, limit, cursor))
    }
}
