//! Google Docs queries and controlled mutations through the shared bus and journal.
use super::{
    bus::CommandBus,
    commands::{DocumentRead, DocumentSearch},
    ports::WriteOutcome,
    writes::Writes,
};
use crate::domain::{
    documents::{self, DocChange, Snapshot},
    error::{CoreError, ErrorKind},
    operation::{OperationState, Outcome, Preview, ResourceKind},
    projection::fingerprint,
    write_model::valid_id,
};
use serde_json::{json, Value};
pub async fn load(bus: &CommandBus, alias: &str) -> Result<Snapshot, CoreError> {
    let document = bus.registry.document(alias)?;
    documents::snapshot(document, &bus.source.read_document(document).await?)
}
pub async fn describe(bus: &CommandBus, alias: &str) -> Result<Value, CoreError> {
    let snapshot = load(bus, alias).await?;
    Ok(
        json!({"document":alias,"document_id":snapshot.id,"title":snapshot.title,"revision_id":snapshot.revision,"tabs":snapshot.tabs.iter().map(|t|json!({"tab_id":t.id,"title":t.title,"editable_structure":t.editable,"characters":t.text.chars().count()})).collect::<Vec<_>>(),"mapping_version":bus.registry.version,"source_fingerprint":snapshot.fingerprint}),
    )
}
pub async fn read(bus: &CommandBus, p: DocumentRead) -> Result<Value, CoreError> {
    let limit = p.limit.unwrap_or(2000);
    if !(1..=10000).contains(&limit) || !documents::valid_tab(&p.tab_id) {
        return Err(CoreError::invalid());
    }
    let snapshot = load(bus, &p.document).await?;
    let tab = snapshot
        .tabs
        .iter()
        .find(|t| t.id == p.tab_id)
        .ok_or(CoreError(ErrorKind::NotFound, "document_tab_not_found"))?;
    let binding = fingerprint(&json!([
        snapshot.id,
        p.tab_id,
        snapshot.fingerprint,
        bus.registry.version
    ]));
    let offset = if let Some(cursor) = p.cursor {
        let (revision, offset) = cursor.split_once(':').ok_or_else(CoreError::invalid)?;
        if revision != binding {
            return Err(CoreError(ErrorKind::Conflict, "stale_cursor"));
        }
        offset.parse::<usize>().map_err(|_| CoreError::invalid())?
    } else {
        0
    };
    let chars: Vec<_> = tab.text.chars().collect();
    if offset > chars.len() {
        return Err(CoreError::invalid());
    }
    let end = offset.saturating_add(limit).min(chars.len());
    Ok(
        json!({"text":chars[offset..end].iter().collect::<String>(),"next_cursor":if end<chars.len(){Some(format!("{binding}:{end}"))}else{None},"source":{"document_id":snapshot.id,"tab_id":p.tab_id,"revision_id":snapshot.revision,"source_fingerprint":snapshot.fingerprint,"mapping_version":bus.registry.version,"start_character":offset,"end_character":end,"retrieved_at":chrono::Utc::now().to_rfc3339()},"editable_structure":tab.editable,"coverage":"body_text_only"}),
    )
}
pub async fn search(bus: &CommandBus, p: DocumentSearch) -> Result<Value, CoreError> {
    let limit = p.limit.unwrap_or(10);
    if !(1..=50).contains(&limit)
        || p.query.trim().is_empty()
        || p.query.len() > 256
        || p.tab_id
            .as_deref()
            .is_some_and(|id| !documents::valid_tab(id))
    {
        return Err(CoreError::invalid());
    }
    let snapshot = load(bus, &p.document).await?;
    let mut identity = json!([
        "docs_search",
        snapshot.id,
        snapshot.fingerprint,
        bus.registry.version,
        p.query,
        p.tab_id
    ]);
    if !p.case_sensitive {
        identity
            .as_array_mut()
            .unwrap()
            .push(json!("unicode_casefold_2.10.0"));
    }
    let binding = fingerprint(&identity);
    let offset = if let Some(cursor) = p.cursor {
        let (revision, offset) = cursor.split_once(':').ok_or_else(CoreError::invalid)?;
        if revision != binding {
            return Err(CoreError(ErrorKind::Conflict, "stale_cursor"));
        }
        offset.parse::<usize>().map_err(|_| CoreError::invalid())?
    } else {
        0
    };
    let (matches, more) = documents::search(
        &snapshot,
        &p.query,
        p.tab_id.as_deref(),
        offset,
        limit,
        p.case_sensitive,
    )?;
    Ok(
        json!({"matches":matches,"next_cursor":if more {Some(format!("{binding}:{}",offset+limit))} else {None},
        "source":{"document":p.document,"document_id":snapshot.id,"title":snapshot.title.chars().take(200).collect::<String>(),"revision_id":snapshot.revision,"source_fingerprint":snapshot.fingerprint,"mapping_version":bus.registry.version,"retrieved_at":chrono::Utc::now().to_rfc3339()},
        "coverage":"body_text_only","match_mode":if p.case_sensitive {"literal_case_sensitive"} else {"literal_case_insensitive"},"offset_unit":"unicode_scalar"}),
    )
}
impl Writes {
    pub(super) async fn docs_preview(
        &self,
        bus: &CommandBus,
        c: DocChange,
    ) -> Result<Value, CoreError> {
        if !valid_id(&c.operation_id) {
            return Err(CoreError::invalid());
        }
        if let Some(old) = self.journal.find(&c.operation_id).await? {
            if !documents::matches(&old.preview, &c, &bus.owner) {
                return Err(CoreError(ErrorKind::Conflict, "operation_conflict"));
            }
            return Ok(old.preview.public(old.state.as_str()));
        }
        let registered = bus.registry.document(&c.document)?;
        let snapshot = load(bus, &c.document).await?;
        let preview = documents::prepare(
            &c,
            registered,
            &snapshot,
            &bus.owner,
            &bus.registry.version,
            chrono::Utc::now().timestamp() + 300,
        )?;
        let preview = self.journal.create(preview).await?;
        Ok(preview.public("previewed"))
    }
    pub(super) async fn docs_apply(&self, bus: &CommandBus, id: &str) -> Result<Value, CoreError> {
        if !valid_id(id) {
            return Err(CoreError::invalid());
        }
        let old = self.journal.get(id).await?;
        if old.preview.resource_kind != ResourceKind::GoogleDocs
            || old.preview.actor != bus.owner.as_ref()
        {
            return Err(CoreError(ErrorKind::Conflict, "operation_action_mismatch"));
        }
        if old.state == OperationState::Applied {
            return Ok(old.preview.public("applied"));
        }
        let op = self.journal.begin_apply(id).await?;
        let outcome = self.apply_document(bus, &op.preview).await;
        let finished = self.journal.finish(id, outcome).await?;
        Ok(finished.preview.public(finished.state.as_str()))
    }
    async fn apply_document(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        if p.mapping_version != bus.registry.version {
            return Outcome::Conflict;
        }
        let registered = match bus.registry.document(&p.spreadsheet) {
            Ok(d) if d.writable && d.id == p.record_id => d,
            _ => return Outcome::Conflict,
        };
        let current = match load(bus, &p.spreadsheet).await {
            Ok(s) => s,
            Err(_) => return Outcome::KnownFailure,
        };
        let tab = match current.tabs.iter().find(|t| t.id == p.view) {
            Some(t) if t.editable => t,
            _ => return Outcome::Conflict,
        };
        if current.revision != p.source_revision
            || p.before["text"].as_str() != Some(tab.text.as_str())
        {
            return Outcome::Conflict;
        }
        match bus.source.write_document(registered, p).await {
            WriteOutcome::Rejected => Outcome::KnownFailure,
            WriteOutcome::Uncertain => Outcome::Uncertain,
            WriteOutcome::Accepted => self.observe_document(bus, p).await,
        }
    }
    pub(super) async fn observe_document(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        let registered = match bus.registry.document(&p.spreadsheet) {
            Ok(d) if d.id == p.record_id && p.mapping_version == bus.registry.version => d,
            _ => return Outcome::Conflict,
        };
        let current = match bus
            .source
            .read_document(registered)
            .await
            .and_then(|v| documents::snapshot(registered, &v))
        {
            Ok(s) => s,
            Err(_) => return Outcome::Reconciliation,
        };
        let tab = match current.tabs.iter().find(|t| t.id == p.view) {
            Some(t) => t,
            None => return Outcome::Conflict,
        };
        if p.after["text"].as_str() == Some(tab.text.as_str()) {
            Outcome::Applied
        } else if p.before["text"].as_str() == Some(tab.text.as_str()) {
            Outcome::Reconciliation
        } else {
            Outcome::Conflict
        }
    }
}
