use super::{bus::CommandBus, ports::WriteOutcome, writes::Writes};
use crate::domain::{
    error::CoreError,
    grids,
    operation::{Outcome, Preview, ResourceKind},
    projection,
    write_model::Change,
};
use serde_json::Value;
impl Writes {
    pub(super) async fn grid_preview(
        &self,
        bus: &CommandBus,
        c: Change,
    ) -> Result<Value, CoreError> {
        let s = bus.registry.lookup(&c.spreadsheet)?;
        let v = s
            .views
            .iter()
            .find(|v| v.name == c.view)
            .ok_or_else(CoreError::invalid)?;
        let (range, after) = grids::target(v, &c)?;
        grids::registered_targets(s, v, range)?;
        let book = bus.source.read(s).await?;
        projection::validate(s, &book)?;
        let before = grids::snapshot(v, &book, range)?;
        let p = Preview {
            resource_kind: ResourceKind::SheetsGrid,
            operation_id: c.operation_id,
            actor: bus.owner.to_string(),
            spreadsheet: c.spreadsheet,
            view: c.view,
            action: c.action,
            record_id: format!("{}:{}", s.id, v.sheet_id),
            fields: c.fields,
            before,
            after,
            mapping_version: bus.registry.version.clone(),
            source_revision: projection::fingerprint(&book),
            expires_at: chrono::Utc::now().timestamp() + 300,
            preview_digest: String::new(),
        };
        let p = self.journal.create(p).await?;
        Ok(p.public("previewed"))
    }
    pub(super) async fn apply_grid(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        let s = match bus.registry.lookup(&p.spreadsheet) {
            Ok(s) if bus.registry.version == p.mapping_version => s,
            _ => return Outcome::Conflict,
        };
        let v = match s
            .views
            .iter()
            .find(|v| v.name == p.view && format!("{}:{}", s.id, v.sheet_id) == p.record_id)
        {
            Some(v) => v,
            None => return Outcome::Conflict,
        };
        let c = Change {
            operation_id: p.operation_id.clone(),
            spreadsheet: p.spreadsheet.clone(),
            view: p.view.clone(),
            action: p.action.clone(),
            record_id: None,
            fields: p.fields.clone(),
        };
        let range = match grids::target(v, &c) {
            Ok((range, after)) if after == p.after => range,
            _ => return Outcome::Conflict,
        };
        if grids::registered_targets(s, v, range).is_err() {
            return Outcome::Conflict;
        }
        let book = match bus.source.read(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::KnownFailure,
        };
        if projection::validate(s, &book).is_err()
            || projection::fingerprint(&book) != p.source_revision
            || !matches!(grids::snapshot(v,&book,range),Ok(before) if before==p.before)
        {
            return Outcome::Conflict;
        }
        match bus.source.write_grid(s, p).await {
            WriteOutcome::Rejected => Outcome::KnownFailure,
            WriteOutcome::Uncertain => Outcome::Uncertain,
            WriteOutcome::Accepted => self.observe_grid(bus, p).await,
        }
    }
    pub(super) async fn observe_grid(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        let s = match bus.registry.lookup(&p.spreadsheet) {
            Ok(s) if bus.registry.version == p.mapping_version => s,
            _ => return Outcome::Conflict,
        };
        let v = match s
            .views
            .iter()
            .find(|v| v.name == p.view && format!("{}:{}", s.id, v.sheet_id) == p.record_id)
        {
            Some(v) => v,
            None => return Outcome::Conflict,
        };
        let range = match p.fields["range"].as_str() {
            Some(r)
                if v.writable_range
                    .as_deref()
                    .is_some_and(|a| grids::contains(a, r)) =>
            {
                r
            }
            _ => return Outcome::Conflict,
        };
        let book = match bus.source.read(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::Reconciliation,
        };
        if book["spreadsheetId"].as_str() != Some(s.id.as_str()) {
            return Outcome::Conflict;
        }
        match grids::snapshot(v, &book, range) {
            Ok(now) if now == p.after => Outcome::Applied,
            Ok(now) if now == p.before => Outcome::Reconciliation,
            _ => Outcome::Conflict,
        }
    }
}
