use super::error::CoreError;
use crate::domain::error::ErrorKind;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub version: String,
    #[serde(default)]
    pub spreadsheets: Vec<Spreadsheet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<Document>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub alias: String,
    pub id: String,
    pub readable: bool,
    #[serde(default)]
    pub writable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spreadsheet {
    #[serde(default)]
    pub create_tabs: bool,
    pub alias: String,
    pub id: String,
    pub readable: bool,
    pub views: Vec<View>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writable_range: Option<String>,
    pub name: String,
    pub sheet_id: i64,
    pub title: String,
    pub kind: Kind,
    pub range: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub merges: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_schema: Option<WriteSchema>,
    pub formulas: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Grid,
    WeddingCosts,
    GuestEstimates,
    PlanningNotes,
    PreweddingOptions,
    HouseholdItems,
    StructuredTasks,
    StructuredPayments,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSchema {
    pub fields: std::collections::BTreeMap<String, FieldSpec>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    pub column: String,
    pub kind: FieldType,
    pub required: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    PositiveInteger,
    Currency,
    Date,
    TaskStatus,
}
impl View {
    pub fn structured(&self) -> bool {
        matches!(self.kind, Kind::StructuredTasks | Kind::StructuredPayments)
    }
    pub fn validate_write_schema(&self) -> Result<(), String> {
        if !self.structured() {
            if self.write_schema.is_some() {
                return Err("legacy writes forbidden".into());
            }
            return Ok(());
        }
        if !self.merges.is_empty()
            || !self.formulas.is_empty()
            || !matches!(self.title.as_str(), "AI_Tasks" | "AI_Payments")
        {
            return Err("structured tables cannot contain merged/formula targets".into());
        }
        if self.kind == Kind::StructuredTasks && self.title != "AI_Tasks"
            || self.kind == Kind::StructuredPayments && self.title != "AI_Payments"
        {
            return Err("structured table title mismatch".into());
        }
        let schema = self
            .write_schema
            .as_ref()
            .ok_or("structured schema required")?;
        if schema.fields.is_empty()
            || schema.fields.len() > 24
            || self.headers.get("A1").map(String::as_str) != Some("record_id")
            || self.headers.get("B1").map(String::as_str) != Some("operation_id")
        {
            return Err("structured identity headers required".into());
        }
        let mut columns = std::collections::BTreeSet::new();
        for (name, field) in &schema.fields {
            if name.is_empty()
                || name.len() > 64
                || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                || matches!(
                    name.as_str(),
                    "record_id" | "operation_id" | "actor" | "approved"
                )
                || field.column.len() != 1
                || !field.column.bytes().all(|b| (b'C'..=b'Z').contains(&b))
                || !columns.insert(field.column.clone())
                || self.headers.get(&format!("{}1", field.column)) != Some(name)
            {
                return Err("invalid allowed field schema".into());
            }
        }
        // Append owns one contiguous table, with no gaps that might contain protected cells.
        for n in 0..schema.fields.len() {
            if !columns.contains(&((b'C' + n as u8) as char).to_string()) {
                return Err("structured columns must be contiguous".into());
            }
        }
        if self.headers.len() != schema.fields.len() + 2 {
            return Err("unexpected structured header".into());
        }
        let required = match self.kind {
            Kind::StructuredTasks => vec![
                ("title", FieldType::Text),
                ("status", FieldType::TaskStatus),
            ],
            Kind::StructuredPayments => vec![
                ("amount", FieldType::PositiveInteger),
                ("currency", FieldType::Currency),
                ("payment_date", FieldType::Date),
                ("cost_group", FieldType::Text),
            ],
            _ => unreachable!(),
        };
        for (name, kind) in required {
            let field = schema
                .fields
                .get(name)
                .ok_or("required domain field missing")?;
            if !field.required
                || std::mem::discriminant(&field.kind) != std::mem::discriminant(&kind)
            {
                return Err("required domain field schema invalid".into());
            }
        }
        Ok(())
    }
}
impl Registry {
    pub fn validate(&self) -> Result<(), String> {
        if self.version.is_empty() || self.spreadsheets.len() + self.documents.len() > 20 {
            return Err("invalid registry bounds".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut aliases = std::collections::HashSet::new();
        for doc in &self.documents {
            if doc.alias.is_empty()
                || doc.alias.len() > 100
                || doc.id.is_empty()
                || doc.id.len() > 200
                || !doc
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(&doc.id)
                || !aliases.insert(&doc.alias)
                || doc.writable && !doc.readable
            {
                return Err("invalid document registration".into());
            }
        }
        for s in &self.spreadsheets {
            if s.alias.is_empty()
                || s.id.is_empty()
                || !s
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(&s.id)
                || !aliases.insert(&s.alias)
                || s.create_tabs && !s.readable
                || s.views.is_empty()
                || s.views.len() > 10
            {
                return Err("invalid spreadsheet registration".into());
            }
            let mut names = std::collections::HashSet::new();
            for v in &s.views {
                v.validate_write_schema()?;
                if let Some(range) = &v.writable_range {
                    if !s.readable
                        || v.kind != Kind::Grid
                        || !crate::domain::grids::contains(&v.range, range)
                    {
                        return Err(
                            "grid writable range must be contained in a readable grid".into()
                        );
                    }
                }
                let expected = match v.kind {
                    Kind::Grid => v.range.as_str(),
                    Kind::WeddingCosts => "A1:G63",
                    Kind::GuestEstimates => "A1:F13",
                    Kind::PlanningNotes => "A3:C26",
                    Kind::PreweddingOptions => "A1:C10",
                    Kind::HouseholdItems => "A1:D16",
                    Kind::StructuredTasks | Kind::StructuredPayments => "A1:Z1001",
                };
                if v.name.is_empty()
                    || !names.insert(&v.name)
                    || v.sheet_id < 0
                    || v.title.is_empty()
                    || v.title.len() > 100
                    || v.range != expected
                    || (v.kind != Kind::Grid && v.headers.is_empty())
                    || v.merges.len() > 100
                    || v.formulas.len() > 100
                {
                    return Err("invalid bounded view registration".into());
                }
                if v.kind == Kind::Grid {
                    let (r, c, rr, cc) = crate::domain::projection::parse_range(&v.range)
                        .ok_or("invalid bounded grid range")?;
                    if (rr - r).checked_mul(cc - c).is_none_or(|n| n > 10000)
                        || v.write_schema.is_some()
                    {
                        return Err("grid reads require a bounded read-only range".into());
                    }
                }
                for cell in v.headers.keys().chain(v.formulas.keys()) {
                    if crate::domain::projection::coordinate(cell).is_none() {
                        return Err("invalid cell coordinate".into());
                    }
                }
                for m in &v.merges {
                    if crate::domain::projection::parse_range(m).is_none() {
                        return Err("invalid merge coordinate".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn lookup(&self, name: &str) -> Result<&Spreadsheet, CoreError> {
        self.spreadsheets
            .iter()
            .find(|s| s.alias == name && s.readable)
            .ok_or(CoreError(ErrorKind::NotFound, "spreadsheet_not_registered"))
    }
    pub fn document(&self, name: &str) -> Result<&Document, CoreError> {
        self.documents
            .iter()
            .find(|d| d.alias == name && d.readable)
            .ok_or(CoreError(ErrorKind::NotFound, "document_not_registered"))
    }
}
