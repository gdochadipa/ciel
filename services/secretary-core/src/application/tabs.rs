use super::{bus::CommandBus, ports::WriteOutcome, writes::Writes};
use crate::domain::{
    error::{CoreError, ErrorKind},
    operation::{OperationState, Outcome, Preview, ResourceKind},
    tabs::{self, CreateTab},
    write_model::valid_id,
};
use serde_json::Value;
impl Writes {
    pub(super) async fn tab_preview(
        &self,
        bus: &CommandBus,
        c: CreateTab,
    ) -> Result<Value, CoreError> {
        c.validate()?;
        if let Some(old) = self.journal.find(&c.operation_id).await? {
            let p = &old.preview;
            if p.resource_kind != ResourceKind::SheetsTab
                || p.actor != bus.owner.as_ref()
                || p.spreadsheet != c.spreadsheet
                || p.fields != c.fields()
            {
                return Err(CoreError(ErrorKind::Conflict, "operation_conflict"));
            }
            return Ok(p.public(old.state.as_str()));
        }
        let s = bus.registry.lookup(&c.spreadsheet)?;
        if !s.create_tabs {
            return Err(CoreError(ErrorKind::Forbidden, "tab_creation_disabled"));
        }
        let book = bus.source.tab_catalog(s).await?;
        // Allocate once, persist before approval, never let the provider choose a new ID on replay.
        let id = (uuid::Uuid::new_v4().as_u128() % i32::MAX as u128) as i64;
        let p = tabs::prepare(
            &c,
            s,
            &bus.owner,
            &bus.registry.version,
            &book,
            id,
            chrono::Utc::now().timestamp() + 300,
        )?;
        let p = self.journal.create(p).await?;
        Ok(p.public("previewed"))
    }
    pub(super) async fn tab_create(&self, bus: &CommandBus, id: &str) -> Result<Value, CoreError> {
        if !valid_id(id) {
            return Err(CoreError::invalid());
        }
        let old = self.journal.get(id).await?;
        if old.preview.resource_kind != ResourceKind::SheetsTab
            || old.preview.actor != bus.owner.as_ref()
        {
            return Err(CoreError(ErrorKind::Conflict, "operation_action_mismatch"));
        }
        if old.state == OperationState::Applied {
            return Ok(old.preview.public("applied"));
        }
        let op = self.journal.begin_apply(id).await?;
        let outcome = self.apply_tab(bus, &op.preview).await;
        let op = self.journal.finish(id, outcome).await?;
        Ok(op.preview.public(op.state.as_str()))
    }
    async fn apply_tab(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        let s = match bus.registry.lookup(&p.spreadsheet) {
            Ok(s)
                if s.create_tabs
                    && s.id == p.after["spreadsheet_id"]
                    && bus.registry.version == p.mapping_version =>
            {
                s
            }
            _ => return Outcome::Conflict,
        };
        let book = match bus.source.tab_catalog(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::KnownFailure,
        };
        if !matches!(
            tabs::collision(
                s,
                &book,
                &p.view,
                p.after["sheet_id"].as_i64().unwrap_or(-1)
            ),
            Ok(false)
        ) {
            return Outcome::Conflict;
        }
        match bus.source.create_tab(s, p).await {
            WriteOutcome::Rejected => Outcome::KnownFailure,
            WriteOutcome::Uncertain => Outcome::Uncertain,
            WriteOutcome::Accepted => self.observe_tab(bus, p).await,
        }
    }
    pub(super) async fn observe_tab(&self, bus: &CommandBus, p: &Preview) -> Outcome {
        let s = match bus.registry.lookup(&p.spreadsheet) {
            Ok(s)
                if s.id == p.after["spreadsheet_id"]
                    && bus.registry.version == p.mapping_version =>
            {
                s
            }
            _ => return Outcome::Conflict,
        };
        let book = match bus.source.tab_catalog(s).await {
            Ok(b) => b,
            Err(_) => return Outcome::Reconciliation,
        };
        let sheets = match tabs::catalog(s, &book) {
            Ok(s) => s,
            Err(_) => return Outcome::Reconciliation,
        };
        let sheet = sheets
            .iter()
            .find(|t| t["properties"]["sheetId"] == p.after["sheet_id"]);
        let sheet = match sheet {
            Some(t) => t,
            None => {
                return if sheets.iter().any(|t| {
                    t["properties"]["title"]
                        .as_str()
                        .is_some_and(|t| t.to_lowercase() == p.view.to_lowercase())
                }) {
                    Outcome::Conflict
                } else {
                    Outcome::Reconciliation
                }
            }
        };
        let props = &sheet["properties"];
        if props["title"] != p.after["title"]
            || props["sheetType"] != "GRID"
            || props["gridProperties"]["rowCount"] != p.after["rows"]
            || props["gridProperties"]["columnCount"] != p.after["columns"]
        {
            return Outcome::Conflict;
        }
        let headers = match bus.source.tab_headers(s, p).await {
            Ok(h) => h,
            Err(CoreError(ErrorKind::Conflict, _)) => return Outcome::Conflict,
            Err(_) => return Outcome::Reconciliation,
        };
        if headers == p.after["headers"] {
            Outcome::Applied
        } else {
            Outcome::Conflict
        }
    }
}
