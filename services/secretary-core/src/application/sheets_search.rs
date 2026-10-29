//! Explicit multi-view read orchestration through the existing bus/Workspace adapter.
use super::{
    bus::{literal_match, CommandBus},
    commands::SelectedSearch,
};
use crate::domain::{
    error::{CoreError, ErrorKind},
    projection,
};
use serde_json::{json, Value};
use std::collections::{btree_map::Entry, BTreeMap, HashSet};
struct Book {
    data: Value,
    fingerprint: String,
}
pub async fn search(bus: &CommandBus, p: SelectedSearch) -> Result<Value, CoreError> {
    let limit = p.limit.unwrap_or(50);
    if !(1..=100).contains(&limit)
        || p.query.trim().is_empty()
        || p.query.len() > 256
        || p.targets.is_empty()
        || p.targets.len() > 3
        || p.cursor
            .as_ref()
            .is_some_and(|c| c.is_empty() || c.len() > 8192)
    {
        return Err(CoreError::invalid());
    }
    let mut unique = HashSet::new();
    for t in &p.targets {
        if [&t.spreadsheet, &t.view]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 100)
            || !unique.insert((&t.spreadsheet, &t.view))
        {
            return Err(CoreError::invalid());
        }
        let s = bus.registry.lookup(&t.spreadsheet)?;
        if !s.views.iter().any(|v| v.name == t.view) {
            return Err(CoreError::invalid());
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(4), selected(bus, &p, limit))
        .await
        .map_err(|_| CoreError(ErrorKind::ProviderUnavailable, "sheets_search_timeout"))?
}
async fn selected(bus: &CommandBus, p: &SelectedSearch, limit: usize) -> Result<Value, CoreError> {
    let mut books = BTreeMap::new();
    for t in &p.targets {
        if let Entry::Vacant(entry) = books.entry(t.spreadsheet.clone()) {
            let s = bus.registry.lookup(&t.spreadsheet)?;
            let data = bus.source.read(s).await?;
            projection::validate(s, &data)?;
            let fingerprint = projection::fingerprint(&data);
            entry.insert(Book { data, fingerprint });
        }
    }
    let retrieved_at = chrono::Utc::now().to_rfc3339();
    let mut sources = Vec::new();
    let mut identity = Vec::new();
    for t in &p.targets {
        let s = bus.registry.lookup(&t.spreadsheet)?;
        let v = s
            .views
            .iter()
            .find(|v| v.name == t.view)
            .ok_or_else(CoreError::invalid)?;
        identity.push(json!([
            t.spreadsheet,
            t.view,
            s.id,
            v,
            books[&t.spreadsheet].fingerprint
        ]));
        sources.push(json!({"spreadsheet":t.spreadsheet,"view":t.view,"spreadsheet_id":s.id,"sheet_id":v.sheet_id,"registered_range":format!("'{}'!{}",v.title.replace('\'',"''"),v.range),"source_revision":books[&t.spreadsheet].fingerprint,"mapping_version":bus.registry.version,"retrieved_at":retrieved_at}));
    }
    let binding = projection::fingerprint(&json!([
        "sheets_search_many_lowercase_v1",
        identity,
        p.query,
        bus.registry.version
    ]));
    let (start, offset) = if let Some(cursor) = &p.cursor {
        let parts: Vec<_> = cursor.split(':').collect();
        if parts.len() != 3 {
            return Err(CoreError::invalid());
        }
        if parts[0] != binding {
            return Err(CoreError(ErrorKind::Conflict, "stale_cursor"));
        }
        let start = parts[1]
            .parse::<usize>()
            .map_err(|_| CoreError::invalid())?;
        let offset = parts[2]
            .parse::<usize>()
            .map_err(|_| CoreError::invalid())?;
        if start >= p.targets.len() {
            return Err(CoreError::invalid());
        }
        (start, offset)
    } else {
        (0, 0)
    };
    let mut records = Vec::new();
    let mut next_cursor = None;
    let query = p.query.to_lowercase();
    // Every selected projection must succeed, even outside the returned page.
    for (index, t) in p.targets.iter().enumerate() {
        let s = bus.registry.lookup(&t.spreadsheet)?;
        let v = s
            .views
            .iter()
            .find(|v| v.name == t.view)
            .ok_or_else(CoreError::invalid)?;
        let mut matching =
            projection::project(s, v, &books[&t.spreadsheet].data, &bus.registry.version)?;
        if index < start || next_cursor.is_some() {
            continue;
        }
        matching.retain(|r| literal_match(&r["fields"], &query));
        let from = if index == start { offset } else { 0 };
        if from > matching.len() {
            return Err(CoreError::invalid());
        }
        for (position, mut record) in matching.into_iter().enumerate().skip(from) {
            if records.len() == limit {
                next_cursor = Some(format!("{binding}:{index}:{position}"));
                break;
            }
            record["spreadsheet"] = json!(t.spreadsheet);
            record["view"] = json!(t.view);
            records.push(record);
        }
    }
    let result = json!({"records":records,"sources":sources,"next_cursor":next_cursor,"coverage":"registered_views_only","match_mode":"literal_case_insensitive_lowercase"});
    if serde_json::to_vec(&result)
        .map_err(|_| CoreError::invalid())?
        .len()
        > 131072
    {
        return Err(CoreError(
            ErrorKind::PayloadTooLarge,
            "sheets_search_page_too_large",
        ));
    }
    Ok(result)
}
