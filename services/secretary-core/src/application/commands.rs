use crate::domain::write_model::Change;
use serde::Deserialize;

/// Typed in-process application requests, including reads and mutations.
/// HTTP names and JSON decoding belong to presentation.
pub enum Command {
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
    Preview(Change),
    Append(Commit),
    Update(Commit),
    History(History),
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
#[serde(deny_unknown_fields)]
pub struct Search {
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
