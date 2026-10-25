use crate::{
    journal::Preview,
    projection,
    registry::{FieldType, Spreadsheet, View},
    ApiError,
};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub operation_id: String,
    pub spreadsheet: String,
    pub view: String,
    pub action: String,
    pub record_id: Option<String>,
    pub fields: Value,
}
#[derive(Clone, Debug)]
pub struct StructuredRow {
    pub row: usize,
    pub record_id: String,
    pub operation_id: String,
    pub fields: Value,
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn validate_fields(view: &View, fields: &Value, append: bool) -> Result<(), ApiError> {
    let schema = view
        .write_schema
        .as_ref()
        .filter(|_| view.structured())
        .ok_or(ApiError(StatusCode::FORBIDDEN, "view_read_only"))?;
    let fields = fields
        .as_object()
        .filter(|f| !f.is_empty())
        .ok_or_else(ApiError::invalid)?;
    for (name, value) in fields {
        let spec = schema.fields.get(name).ok_or_else(ApiError::invalid)?;
        if value.is_null() && !spec.required {
            continue;
        }
        let good = match spec.kind {
            FieldType::Text => value
                .as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.len() <= 2048),
            FieldType::PositiveInteger => value
                .as_i64()
                .is_some_and(|n| n > 0 && n <= 9_007_199_254_740_991),
            FieldType::Currency => value
                .as_str()
                .is_some_and(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())),
            FieldType::Date => value.as_str().is_some_and(|s| {
                s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
            }),
            FieldType::TaskStatus => value.as_str().is_some_and(|s| {
                ["todo", "in_progress", "blocked", "done", "canceled"].contains(&s)
            }),
        };
        if !good {
            return Err(ApiError::invalid());
        }
    }
    if append
        && schema
            .fields
            .iter()
            .any(|(name, spec)| spec.required && !fields.contains_key(name))
    {
        return Err(ApiError::invalid());
    }
    Ok(())
}
pub fn rows(_s: &Spreadsheet, v: &View, book: &Value) -> Result<Vec<StructuredRow>, ApiError> {
    let schema = v
        .write_schema
        .as_ref()
        .filter(|_| v.structured())
        .ok_or(ApiError(StatusCode::FORBIDDEN, "view_read_only"))?;
    let cells = projection::cells(projection::sheet(book, v)?);
    let mut rows = Vec::new();
    let mut ids = HashSet::new();
    let mut ops = HashSet::new();
    for row in 2..=1001 {
        let mut values = serde_json::Map::new();
        let mut any = false;
        for col in b'A'..=b'Z' {
            let value = projection::at(&cells, &format!("{}{row}", col as char));
            if !value.is_null() && value != json!("") {
                any = true;
                let owned =
                    col <= b'B' || schema.fields.values().any(|f| f.column.as_bytes() == [col]);
                if !owned {
                    return Err(ApiError::drift());
                }
            }
        }
        if !any {
            continue;
        }
        let id = projection::at(&cells, &format!("A{row}"));
        let op = projection::at(&cells, &format!("B{row}"));
        let id = id
            .as_str()
            .filter(|s| valid_id(s))
            .ok_or_else(ApiError::drift)?
            .to_owned();
        let op = op
            .as_str()
            .filter(|s| valid_id(s))
            .ok_or_else(ApiError::drift)?
            .to_owned();
        if !ids.insert(id.clone()) || !ops.insert(op.clone()) {
            return Err(ApiError(StatusCode::CONFLICT, "duplicate_record_identity"));
        }
        for (name, spec) in &schema.fields {
            let value = projection::at(&cells, &format!("{}{row}", spec.column));
            values.insert(
                name.clone(),
                if value == json!("") {
                    Value::Null
                } else {
                    value
                },
            );
        }
        let fields = Value::Object(values);
        validate_fields(v, &fields, true).map_err(|_| ApiError::drift())?;
        rows.push(StructuredRow {
            row,
            record_id: id,
            operation_id: op,
            fields,
        });
    }
    Ok(rows)
}
pub fn prepare(
    change: &Change,
    owner: &str,
    s: &Spreadsheet,
    v: &View,
    book: &Value,
    version: &str,
) -> Result<Preview, ApiError> {
    if !valid_id(&change.operation_id)
        || !matches!(change.action.as_str(), "append" | "update")
        || change.action == "append" && change.record_id.is_some()
        || change.action == "update" && !change.record_id.as_deref().is_some_and(valid_id)
    {
        return Err(ApiError::invalid());
    }
    validate_fields(v, &change.fields, change.action == "append")?;
    if v.kind == crate::registry::Kind::StructuredPayments {
        let cost_group = change.fields.get("cost_group").and_then(Value::as_str);
        if let Some(group) = cost_group {
            let mut found = false;
            for costs in s
                .views
                .iter()
                .filter(|v| v.kind == crate::registry::Kind::WeddingCosts)
            {
                if projection::project(s, costs, book, version)?
                    .iter()
                    .any(|r| r["fields"]["cost_group"].as_str() == Some(group))
                {
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(ApiError(StatusCode::BAD_REQUEST, "unknown_cost_group"));
            }
        }
    }
    let rows = rows(s, v, book)?;
    let (id, before, mut after) = if change.action == "update" {
        let row = rows
            .iter()
            .find(|r| Some(&r.record_id) == change.record_id.as_ref())
            .ok_or(ApiError(StatusCode::NOT_FOUND, "record_not_found"))?;
        (
            row.record_id.clone(),
            row.fields.clone(),
            row.fields.clone(),
        )
    } else {
        // Appending beyond the bounded table would become unobservable to reconciliation.
        if rows.iter().any(|r| r.row >= 1001) || rows.len() >= 1000 {
            return Err(ApiError(StatusCode::CONFLICT, "table_full"));
        }
        let mut empty = serde_json::Map::new();
        for name in v.write_schema.as_ref().unwrap().fields.keys() {
            empty.insert(name.clone(), Value::Null);
        }
        (
            uuid::Uuid::new_v4().to_string(),
            Value::Null,
            Value::Object(empty),
        )
    };
    for (k, v) in change.fields.as_object().unwrap() {
        after[k] = v.clone()
    }
    Ok(Preview {
        operation_id: change.operation_id.clone(),
        actor: owner.into(),
        spreadsheet: change.spreadsheet.clone(),
        view: change.view.clone(),
        action: change.action.clone(),
        record_id: id,
        fields: change.fields.clone(),
        before,
        after,
        mapping_version: version.into(),
        source_revision: projection::fingerprint(book),
        expires_at: chrono::Utc::now().timestamp() + 300,
        preview_digest: String::new(),
    })
}
pub fn matches_request(p: &Preview, c: &Change, owner: &str) -> bool {
    p.operation_id == c.operation_id
        && p.actor == owner
        && p.spreadsheet == c.spreadsheet
        && p.view == c.view
        && p.action == c.action
        && p.fields == c.fields
        && (c.action == "append" && c.record_id.is_none()
            || c.action == "update" && c.record_id.as_ref() == Some(&p.record_id))
}
