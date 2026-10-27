//! Bounded tab creation. Registry permissions remain operator-owned after creation.
use crate::domain::{
    error::{CoreError, ErrorKind},
    operation::{Preview, ResourceKind},
    registry::Spreadsheet,
    write_model::valid_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTab {
    pub operation_id: String,
    pub spreadsheet: String,
    pub title: String,
    pub rows: u32,
    pub columns: u32,
    #[serde(default)]
    pub headers: Vec<String>,
}
impl CreateTab {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !valid_id(&self.operation_id)
            || self.title.trim() != self.title
            || self.title.is_empty()
            || self.title.chars().count() > 100
            || self
                .title
                .chars()
                .any(|c| c.is_control() || "[]:*?/\\".contains(c))
            || !(1..=1000).contains(&self.rows)
            || !(1..=26).contains(&self.columns)
            || self.rows * self.columns > 10000
            || self.headers.len() > self.columns as usize
            || self
                .headers
                .iter()
                .any(|h| h.len() > 256 || h.chars().any(char::is_control))
        {
            return Err(CoreError::invalid());
        }
        Ok(())
    }
    pub fn fields(&self) -> Value {
        json!({"title":self.title,"rows":self.rows,"columns":self.columns,"headers":self.headers})
    }
}
pub fn catalog<'a>(s: &Spreadsheet, book: &'a Value) -> Result<&'a Vec<Value>, CoreError> {
    if book["spreadsheetId"] != s.id {
        return Err(CoreError::drift());
    }
    let sheets = book["sheets"]
        .as_array()
        .filter(|s| s.len() <= 1000)
        .ok_or_else(CoreError::drift)?;
    let mut ids = std::collections::HashSet::new();
    let mut titles = std::collections::HashSet::new();
    for sheet in sheets {
        let p = &sheet["properties"];
        let id = p["sheetId"]
            .as_i64()
            .filter(|id| *id >= 0 && *id <= i32::MAX as i64)
            .ok_or_else(CoreError::drift)?;
        let title = p["title"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(CoreError::drift)?;
        if !ids.insert(id) || !titles.insert(title.to_lowercase()) {
            return Err(CoreError::drift());
        }
    }
    Ok(sheets)
}
pub fn collision(s: &Spreadsheet, book: &Value, title: &str, id: i64) -> Result<bool, CoreError> {
    Ok(catalog(s, book)?.iter().any(|t| {
        t["properties"]["sheetId"] == id
            || t["properties"]["title"]
                .as_str()
                .is_some_and(|t| t.to_lowercase() == title.to_lowercase())
    }))
}
pub fn prepare(
    c: &CreateTab,
    s: &Spreadsheet,
    owner: &str,
    version: &str,
    book: &Value,
    id: i64,
    expires_at: i64,
) -> Result<Preview, CoreError> {
    c.validate()?;
    if !s.create_tabs {
        return Err(CoreError(ErrorKind::Forbidden, "tab_creation_disabled"));
    }
    if s.views
        .iter()
        .any(|v| v.sheet_id == id || v.title.to_lowercase() == c.title.to_lowercase())
        || collision(s, book, &c.title, id)?
    {
        return Err(CoreError(ErrorKind::Conflict, "tab_collision"));
    }
    Ok(Preview {
        resource_kind: ResourceKind::SheetsTab,
        operation_id: c.operation_id.clone(),
        actor: owner.into(),
        spreadsheet: c.spreadsheet.clone(),
        view: c.title.clone(),
        action: "create_tab".into(),
        record_id: id.to_string(),
        fields: c.fields(),
        before: Value::Null,
        after: json!({"spreadsheet_id":s.id,"sheet_id":id,"title":c.title,"rows":c.rows,"columns":c.columns,"headers":c.headers,"registration":"unregistered","readable":false,"writable":false}),
        mapping_version: version.into(),
        source_revision: crate::domain::projection::fingerprint(book),
        expires_at,
        preview_digest: String::new(),
    })
}
pub fn requests(p: &Preview) -> Value {
    let mut requests = vec![
        json!({"addSheet":{"properties":{"sheetId":p.after["sheet_id"],"title":p.after["title"],"sheetType":"GRID","gridProperties":{"rowCount":p.after["rows"],"columnCount":p.after["columns"]}}}}),
    ];
    if let Some(headers) = p.after["headers"].as_array().filter(|h| !h.is_empty()) {
        requests.push(json!({"updateCells":{"start":{"sheetId":p.after["sheet_id"],"rowIndex":0,"columnIndex":0},"rows":[{"values":headers.iter().map(|h|json!({"userEnteredValue":{"stringValue":h}})).collect::<Vec<_>>()}],"fields":"userEnteredValue"}}));
    }
    json!({"requests":requests,"includeSpreadsheetInResponse":false})
}
