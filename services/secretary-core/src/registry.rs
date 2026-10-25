use crate::ApiError;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub version: String,
    pub spreadsheets: Vec<Spreadsheet>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spreadsheet {
    pub alias: String,
    pub id: String,
    pub readable: bool,
    pub views: Vec<View>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub name: String,
    pub sheet_id: i64,
    pub title: String,
    pub kind: Kind,
    pub range: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub merges: Vec<String>,
    pub formulas: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    WeddingCosts,
    GuestEstimates,
    PlanningNotes,
    PreweddingOptions,
    HouseholdItems,
}
impl Registry {
    pub fn validate(&self) -> Result<(), String> {
        if self.version.is_empty() || self.spreadsheets.len() > 20 {
            return Err("invalid registry bounds".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut aliases = std::collections::HashSet::new();
        for s in &self.spreadsheets {
            if s.alias.is_empty()
                || s.id.is_empty()
                || !s
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(&s.id)
                || !aliases.insert(&s.alias)
                || s.views.is_empty()
                || s.views.len() > 10
            {
                return Err("invalid spreadsheet registration".into());
            }
            let mut names = std::collections::HashSet::new();
            for v in &s.views {
                let expected = match v.kind {
                    Kind::WeddingCosts => "A1:G63",
                    Kind::GuestEstimates => "A1:F13",
                    Kind::PlanningNotes => "A3:C26",
                    Kind::PreweddingOptions => "A1:C10",
                    Kind::HouseholdItems => "A1:D16",
                };
                if v.name.is_empty()
                    || !names.insert(&v.name)
                    || v.sheet_id < 0
                    || v.title.is_empty()
                    || v.title.len() > 100
                    || v.range != expected
                    || v.headers.is_empty()
                    || v.merges.len() > 100
                    || v.formulas.len() > 100
                {
                    return Err("invalid bounded view registration".into());
                }
                for cell in v.headers.keys().chain(v.formulas.keys()) {
                    if crate::projection::coordinate(cell).is_none() {
                        return Err("invalid cell coordinate".into());
                    }
                }
                for m in &v.merges {
                    if crate::projection::parse_range(m).is_none() {
                        return Err("invalid merge coordinate".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn lookup(&self, name: &str) -> Result<&Spreadsheet, ApiError> {
        self.spreadsheets
            .iter()
            .find(|s| s.alias == name && s.readable)
            .ok_or(ApiError(
                StatusCode::NOT_FOUND,
                "spreadsheet_not_registered",
            ))
    }
}
