use super::{commands::Command, ports::WorkspacePort, writes::Writes};
use crate::domain::{
    error::{CoreError, ErrorKind},
    projection,
    registry::Registry,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// In-process command dispatch; no broker, queue, or transport dependency.
#[derive(Clone)]
pub struct CommandBus {
    pub(crate) registry: Arc<Registry>,
    pub(crate) source: Arc<dyn WorkspacePort>,
    pub(crate) owner: Arc<str>,
    pub(crate) writes: Option<Arc<Writes>>,
}
impl CommandBus {
    pub fn new(
        registry: Registry,
        owner: String,
        sheets: impl WorkspacePort + 'static,
    ) -> Result<Self, String> {
        registry.validate()?;
        if owner.is_empty() {
            return Err("owner must be configured".into());
        }
        Ok(Self {
            registry: Arc::new(registry),
            source: Arc::new(sheets),
            owner: owner.into(),
            writes: None,
        })
    }
    pub fn with_writes(mut self, writes: Writes) -> Self {
        self.writes = Some(Arc::new(writes));
        self
    }
    pub async fn execute(&self, command: Command) -> Result<Value, CoreError> {
        match command {
            Command::DocsList => Ok(
                json!({"documents":self.registry.documents.iter().filter(|d|d.readable).map(|d|json!({"document":d.alias,"id":d.id,"readable":true,"writable":d.writable && self.writes.as_ref().is_some_and(|w|w.enabled)})).collect::<Vec<_>>(),"mapping_version":self.registry.version}),
            ),
            Command::DocsDescribe(p) => super::documents::describe(self, &p.document).await,
            Command::DocsRead(p) => super::documents::read(self, p).await,
            Command::List => Ok(
                json!({"spreadsheets":self.registry.spreadsheets.iter().filter(|s|s.readable).map(|s|json!({"spreadsheet":s.alias,"id":s.id,"readable":true,"create_tabs":s.create_tabs && self.writes.as_ref().is_some_and(|w|w.enabled),"writable":self.writes.as_ref().is_some_and(|w|w.enabled)&&s.views.iter().any(|v|v.structured() || v.writable_range.is_some())})).collect::<Vec<_>>(),"mapping_version":self.registry.version}),
            ),
            Command::Describe(p) => {
                let s = self.registry.lookup(&p.spreadsheet)?;
                Ok(
                    json!({"spreadsheet":s.alias,"id":s.id,"mapping_version":self.registry.version,"views":s.views,"create_tabs":s.create_tabs && self.writes.as_ref().is_some_and(|w|w.enabled),"writable":self.writes.as_ref().is_some_and(|w|w.enabled)&&s.views.iter().any(|v|v.structured() || v.writable_range.is_some())}),
                )
            }
            Command::Read(p) => read(self, &p.spreadsheet, &p.view, None, p.limit, p.cursor).await,
            Command::Search(p) => {
                if p.query.is_empty() || p.query.len() > 256 {
                    return Err(CoreError::invalid());
                }
                read(
                    self,
                    &p.spreadsheet,
                    &p.view,
                    Some(p.query),
                    p.limit,
                    p.cursor,
                )
                .await
            }
            Command::Report(p) => {
                let s = self.registry.lookup(&p.spreadsheet)?;
                let book = self.source.read(s).await?;
                projection::validate(s, &book)?;
                projection::report(s, &book, &self.registry.version)
            }
            Command::Mutation(command) => self.writes()?.handle(self, command).await,
            Command::Approve {
                operation_id,
                approval,
            } => self.writes()?.approve(&operation_id, approval).await,
            Command::Reconcile { operation_id } => {
                self.writes()?.reconcile(self, &operation_id).await
            }
        }
    }
    fn writes(&self) -> Result<&Writes, CoreError> {
        self.writes
            .as_deref()
            .ok_or(CoreError(ErrorKind::Forbidden, "writes_disabled"))
    }
}
async fn read(
    state: &CommandBus,
    spreadsheet: &str,
    view: &str,
    query: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
) -> Result<Value, CoreError> {
    let limit = limit.unwrap_or(50);
    if limit == 0 || limit > 100 {
        return Err(CoreError::invalid());
    }
    let s = state.registry.lookup(spreadsheet)?;
    let v = s
        .views
        .iter()
        .find(|v| v.name == view)
        .ok_or_else(CoreError::invalid)?;
    let workbook = state.source.read(s).await?;
    projection::validate(s, &workbook)?;
    let fingerprint = projection::fingerprint(&workbook);
    // Cursor binds the source revision and full query, preventing pagination over changed data.
    use sha2::{Digest, Sha256};
    let binding = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}|{}|{}|{}",
                s.id,
                view,
                query.as_deref().unwrap_or(""),
                state.registry.version,
                fingerprint
            )
            .as_bytes()
        )
    );
    let offset = if let Some(c) = cursor {
        let (b, n) = c.split_once(':').ok_or_else(CoreError::invalid)?;
        if b != binding {
            return Err(CoreError(ErrorKind::Conflict, "stale_cursor"));
        }
        n.parse::<usize>().map_err(|_| CoreError::invalid())?
    } else {
        0
    };
    let mut records = projection::project(s, v, &workbook, &state.registry.version)?;
    if let Some(q) = query {
        let q = q.to_lowercase();
        records.retain(|r| literal_match(&r["fields"], &q));
    }
    if offset > records.len() {
        return Err(CoreError::invalid());
    }
    let end = offset.saturating_add(limit).min(records.len());
    Ok(
        json!({"records":records[offset..end],"next_cursor":if end<records.len(){Some(format!("{binding}:{end}"))}else{None},"source_revision":fingerprint,"mapping_version":state.registry.version,"retrieved_at":chrono::Utc::now().to_rfc3339()}),
    )
}

fn literal_match(value: &Value, query: &str) -> bool {
    match value {
        Value::String(s) => s.to_lowercase().contains(query),
        Value::Number(n) => n.to_string().contains(query),
        Value::Bool(b) => b.to_string().contains(query),
        Value::Array(a) => a.iter().any(|v| literal_match(v, query)),
        Value::Object(o) => o.values().any(|v| literal_match(v, query)),
        _ => false,
    }
}
