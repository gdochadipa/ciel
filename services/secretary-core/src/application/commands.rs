use crate::domain::write_model::Change;
use serde::Deserialize;

/// Typed in-process application requests, including reads and mutations.
/// HTTP names and JSON decoding belong to presentation.
pub enum Command {
    DocsList,
    DocsDescribe(DocumentRequest),
    DocsRead(DocumentRead),
    DocsSummary(DocumentSummary),
    DocsSummaryMany(MultiDocumentSummary),
    DocsSearch(DocumentSearch),
    List,
    Describe(Describe),
    Read(Read),
    Search(Search),
    Report(Describe),
    Mutation(MutationCommand),
    Approve {
        operation_id: String,
        approval: Approval,
    },
    Reconcile {
        operation_id: String,
    },
}
/// Requests served by the mutation coordinator, including read-only operation history.
pub enum MutationCommand {
    TabPreview(crate::domain::tabs::CreateTab),
    TabCreate(Commit),
    DocsPreview(crate::domain::documents::DocChange),
    DocsApply(Commit),
    Preview(Change),
    Append(Commit),
    Update(Commit),
    History(History),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRequest {
    pub document: String,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum DocumentReadRequest {
    Tab(DocumentRead),
    Summary(DocumentSummary),
    Many(MultiDocumentSummary),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryContextMode {
    SummaryContext,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSummary {
    pub document: String,
    pub mode: SummaryContextMode,
    #[serde(default, deserialize_with = "non_null_option")]
    pub max_characters: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiDocumentSummary {
    pub documents: Vec<String>,
    pub mode: SummaryContextMode,
    #[serde(default, deserialize_with = "non_null_option")]
    pub max_characters: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRead {
    pub document: String,
    pub tab_id: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum DocumentSearch {
    One(SingleDocumentSearch),
    Many(MultiDocumentSearch),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiDocumentSearch {
    pub documents: Vec<String>,
    pub query: String,
    #[serde(default, deserialize_with = "non_null_option")]
    pub limit: Option<usize>,
    #[serde(default, deserialize_with = "non_null_option")]
    pub cursor: Option<String>,
    #[serde(default = "default_case_sensitive")]
    pub case_sensitive: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleDocumentSearch {
    pub document: String,
    pub query: String,
    pub tab_id: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    #[serde(default = "default_case_sensitive")]
    pub case_sensitive: bool,
}
fn non_null_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
fn default_case_sensitive() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Describe {
    pub spreadsheet: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub spreadsheet: String,
    pub view: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Search {
    One(SingleSearch),
    Many(SelectedSearch),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchTarget {
    pub spreadsheet: String,
    pub view: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSearch {
    pub targets: Vec<SearchTarget>,
    pub query: String,
    #[serde(default, deserialize_with = "non_null_option")]
    pub limit: Option<usize>,
    #[serde(default, deserialize_with = "non_null_option")]
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleSearch {
    pub spreadsheet: String,
    pub view: String,
    pub query: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub operation_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub operation_id: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub decision: String,
    pub preview_digest: String,
}
