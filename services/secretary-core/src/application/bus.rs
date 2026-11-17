use super::{
    commands::{Command, Search},
    ports::WorkspacePort,
    writes::Writes,
};
use crate::domain::{
    error::{CoreError, ErrorKind},
    projection,
    registry::Registry,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExecutionPolicy {
    Legacy,
    ReadOnly,
}

/// In-process command dispatch; no broker, queue, or transport dependency.
#[derive(Clone)]
pub struct CommandBus {
    pub(crate) registry: Arc<Registry>,
    pub(crate) source: Arc<dyn WorkspacePort>,
    pub(crate) owner: Arc<str>,
    pub(crate) writes: Option<Arc<Writes>>,
    policy: ExecutionPolicy,
    authority: Option<Arc<dyn super::ports::RuntimeAuthority>>,
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
            policy: ExecutionPolicy::Legacy,
            authority: None,
        })
    }
    /// Explicit read-only construction is independent of coordinator enablement.
    pub fn new_read_only(
        registry: Registry,
        owner: String,
        source: impl WorkspacePort + 'static,
    ) -> Result<Self, String> {
        registry.validate_read_only()?;
        let mut bus = Self::new(registry, owner, source)?;
        bus.policy = ExecutionPolicy::ReadOnly;
        Ok(bus)
    }
    pub(crate) fn is_read_only(&self) -> bool {
        self.policy == ExecutionPolicy::ReadOnly
    }
    /// Attach command-boundary authority checks only to the read-only profile.
    ///
    /// # Examples
    /// ```no_run
    /// # use secretary_core::application::{bus::CommandBus, ports::RuntimeAuthority};
    /// # fn attach(bus: CommandBus, guard: impl RuntimeAuthority + 'static) -> Result<(), String> {
    /// let bus = bus.with_runtime_authority(guard)?;
    /// # Ok(()) }
    /// ```
    /// # Errors
    /// Returns an error if the legacy policy or a second guard is supplied.
    pub fn with_runtime_authority(
        mut self,
        authority: impl super::ports::RuntimeAuthority + 'static,
    ) -> Result<Self, String> {
        if !self.is_read_only() || self.authority.is_some() {
            return Err("runtime authority requires an unguarded read-only bus".into());
        }
        self.authority = Some(Arc::new(authority));
        Ok(self)
    }
    pub fn with_writes(mut self, writes: Writes) -> Self {
        self.writes = Some(Arc::new(writes));
        self
    }
    pub(crate) fn task_writes(&self) -> Result<&Writes, CoreError> {
        if self.is_read_only() {
            return Err(CoreError(ErrorKind::Forbidden, "readonly_profile"));
        }
        let writes = self
            .writes
            .as_deref()
            .filter(|writes| writes.enabled && writes.tasks.is_some())
            .ok_or(CoreError(ErrorKind::Unavailable, "write_tasks_unavailable"))?;
        if writes
            .tasks
            .as_ref()
            .is_none_or(|ledger| ledger.owner() != self.owner.as_ref())
        {
            return Err(CoreError(ErrorKind::Unavailable, "write_task_binding"));
        }
        Ok(writes)
    }
    pub async fn execute(&self, command: Command) -> Result<Value, CoreError> {
        self.check_authority()?;
        let result = self.dispatch(command).await;
        self.check_authority()?;
        result
    }

    pub(crate) fn check_authority(&self) -> Result<(), CoreError> {
        self.authority.as_ref().map_or(Ok(()), |guard| {
            guard
                .check()
                .map_err(|code| CoreError(ErrorKind::Unavailable, code))
        })
    }

    async fn dispatch(&self, command: Command) -> Result<Value, CoreError> {
        match command {
            Command::Cleanup(command) => {
                if self.is_read_only() {
                    return Err(CoreError(ErrorKind::Forbidden, "readonly_profile"));
                }
                let writes = self
                    .writes
                    .as_ref()
                    .ok_or(CoreError(ErrorKind::Unavailable, "cleanup_unavailable"))?;
                let ledger = writes
                    .tasks
                    .as_ref()
                    .filter(|ledger| ledger.owner() == self.owner.as_ref())
                    .ok_or(CoreError(ErrorKind::Unavailable, "cleanup_unavailable"))?;
                let _writer = writes.writer.lock().await;
                return ledger.cleanup(&command).await;
            }
            Command::TaskControl(command) => {
                return super::task_authority::control(self, command).await
            }
            Command::TaskExecute {
                member,
                confirmation,
            } => {
                return self
                    .task_writes()?
                    .execute_task(self, &member, confirmation.as_ref())
                    .await
            }
            other => return self.dispatch_standard(other).await,
        }
    }
    async fn dispatch_standard(&self, command: Command) -> Result<Value, CoreError> {
        // Approval credentials and an attached coordinator cannot elevate this policy.
        // Reconciliation is observation-only and keeps its separate operator authority.
        if self.is_read_only()
            && !matches!(
                &command,
                Command::DocsList
                    | Command::DocsDescribe(_)
                    | Command::DocsRead(_)
                    | Command::DocsSummary(_)
                    | Command::DocsSummaryMany(_)
                    | Command::DocsSearch(_)
                    | Command::List
                    | Command::Describe(_)
                    | Command::Read(_)
                    | Command::SheetSummary(_)
                    | Command::Search(_)
                    | Command::Reconcile { .. }
            )
        {
            return Err(CoreError(ErrorKind::Forbidden, "readonly_profile"));
        }
        match command {
            Command::Cleanup(_) | Command::TaskControl(_) | Command::TaskExecute { .. } => {
                unreachable!("task commands routed above")
            }

            Command::DocsList => Ok(
                json!({"documents":self.registry.documents.iter().filter(|d|d.readable).map(|d|json!({"document":d.alias,"id":d.id,"readable":true,"writable":d.writable && self.writes.as_ref().is_some_and(|w|w.enabled)})).collect::<Vec<_>>(),"mapping_version":self.registry.version}),
            ),
            Command::DocsDescribe(p) => super::documents::describe(self, &p.document).await,
            Command::DocsRead(p) => super::documents::read(self, p).await,
            Command::DocsSummary(p) => super::documents::summary_context(self, p).await,
            Command::DocsSummaryMany(p) => super::documents::summary_selected(self, p).await,
            Command::DocsSearch(p) => super::documents::search(self, p).await,
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
            Command::SheetSummary(p) => super::sheets_summary::summary(self, p).await,
            Command::Search(Search::Many(p)) => super::sheets_search::search(self, p).await,
            Command::Search(Search::One(p)) => {
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
    let binding = projection::read_binding(
        &s.id,
        view,
        query.as_deref(),
        &state.registry.version,
        &fingerprint,
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

pub(super) fn literal_match(value: &Value, query: &str) -> bool {
    match value {
        Value::String(s) => s.to_lowercase().contains(query),
        Value::Number(n) => n.to_string().contains(query),
        Value::Bool(b) => b.to_string().contains(query),
        Value::Array(a) => a.iter().any(|v| literal_match(v, query)),
        Value::Object(o) => o.values().any(|v| literal_match(v, query)),
        _ => false,
    }
}
