//! One registered view, shared command bus and existing Sheets adapter.
use super::{bus::CommandBus, commands::SheetSummary};
use crate::domain::{
    error::{CoreError, ErrorKind},
    projection, sheets_summary,
};
use serde_json::Value;
pub async fn summary(bus: &CommandBus, p: SheetSummary) -> Result<Value, CoreError> {
    let max_records = p.max_records.unwrap_or(25);
    if !(1..=100).contains(&max_records) {
        return Err(CoreError::invalid());
    }
    let s = bus.registry.lookup(&p.spreadsheet)?;
    let v = s
        .views
        .iter()
        .find(|v| v.name == p.view)
        .ok_or_else(CoreError::invalid)?;
    tokio::time::timeout(std::time::Duration::from_secs(4), async {
        let book = bus.source.read(s).await?;
        projection::validate(s, &book)?;
        let records = projection::project(s, v, &book, &bus.registry.version)?;
        let packet = sheets_summary::context(
            s,
            v,
            &records,
            &bus.registry.version,
            &projection::fingerprint(&book),
            &chrono::Utc::now().to_rfc3339(),
            max_records,
        );
        if serde_json::to_vec(&packet)
            .map_err(|_| CoreError::invalid())?
            .len()
            > 65536
        {
            return Err(CoreError(
                ErrorKind::PayloadTooLarge,
                "sheets_summary_context_too_large",
            ));
        }
        Ok(packet)
    })
    .await
    .map_err(|_| CoreError(ErrorKind::ProviderUnavailable, "sheets_summary_timeout"))?
}
