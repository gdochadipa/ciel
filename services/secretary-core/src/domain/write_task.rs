//! Machine-enforceable bounds for conversational authority; no host identity is inferred.
use super::{
    compensation::validate_reversible,
    error::{CoreError, ErrorKind},
    grids::contains,
    operation::{Preview, ResourceKind},
    projection::parse_range,
};
use serde::{Deserialize, Serialize};

fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

/// Host-supplied identity of one conversation incarnation, never tool arguments.
///
/// These strings describe a trusted caller; parsing them is not authentication.
/// A future host adapter must assert current native invocation authority as well.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContext {
    /// Current Gateway process incarnation, minted by the trusted adapter.
    pub gateway_instance: String,
    /// Host-resolved agent identifier.
    pub agent_id: String,
    /// Host-resolved logical conversation key.
    pub session_key: String,
    /// Host-resolved incarnation replaced on new/reset.
    pub session_id: String,
}

impl TaskContext {
    /// Validate bounded identity fields without asserting their provenance.
    ///
    /// # Errors
    /// Rejects missing, oversized or control-containing identities.
    pub fn validate(&self) -> Result<(), CoreError> {
        if !bounded(&self.gateway_instance, 256)
            || !bounded(&self.agent_id, 256)
            || !bounded(&self.session_key, 512)
            || !bounded(&self.session_id, 256)
        {
            return Err(CoreError(ErrorKind::InvalidInput, "write_task_context"));
        }
        Ok(())
    }

    /// Compare the same conversation across a Gateway process restart.
    pub fn same_conversation(&self, other: &Self) -> bool {
        self.agent_id == other.agent_id
            && self.session_key == other.session_key
            && self.session_id == other.session_id
    }
}

/// An exact registered target with a supported reversible action class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriteTarget {
    /// Literal grid updates inside one approved rectangle.
    Grid {
        /// Registered alias.
        spreadsheet: String,
        /// Physical spreadsheet identity.
        spreadsheet_id: String,
        /// Registered view.
        view: String,
        /// Physical sheet identity.
        sheet_id: i64,
        /// Approved A1 rectangle, intersected with current registration on capture.
        range: String,
    },
    /// Unique plain-text replacements in one selected Docs tab.
    Document {
        /// Registered alias.
        document: String,
        /// Physical document identity.
        document_id: String,
        /// Selected registered source tab.
        tab_id: String,
    },
}

impl WriteTarget {
    fn valid(&self) -> bool {
        match self {
            Self::Grid {
                spreadsheet,
                spreadsheet_id,
                view,
                sheet_id,
                range,
            } => {
                bounded(spreadsheet, 100)
                    && bounded(spreadsheet_id, 256)
                    && bounded(view, 100)
                    && *sheet_id >= 0
                    && range.len() <= 64
                    && parse_range(range).is_some()
            }
            Self::Document {
                document,
                document_id,
                tab_id,
            } => {
                bounded(document, 100)
                    && bounded(document_id, 256)
                    && super::documents::valid_tab(tab_id)
            }
        }
    }

    fn permits(&self, p: &Preview) -> bool {
        match self {
            Self::Grid {
                spreadsheet,
                spreadsheet_id,
                view,
                sheet_id,
                range,
            } => {
                p.resource_kind == ResourceKind::SheetsGrid
                    && p.action == "update"
                    && p.spreadsheet == *spreadsheet
                    && p.view == *view
                    && p.record_id == format!("{spreadsheet_id}:{sheet_id}")
                    && p.fields["range"]
                        .as_str()
                        .is_some_and(|r| contains(range, r))
            }
            Self::Document {
                document,
                document_id,
                tab_id,
            } => {
                p.resource_kind == ResourceKind::GoogleDocs
                    && matches!(p.action.as_str(), "replace_text" | "replace_text_batch")
                    && p.spreadsheet == *document
                    && p.record_id == *document_id
                    && p.view == *tab_id
            }
        }
    }
}

/// Bounded scope reviewed by the owner once; the final edit list may grow within it.
///
/// Objective text records intent. Only typed targets, mapping and limits can be
/// enforced mechanically; the ledger does not pretend to interpret natural language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskScope {
    /// Owner-facing task objective, not source content or an authority assertion.
    pub objective: String,
    /// Exact registry generation approved for these identities/permissions.
    pub mapping_version: String,
    /// One to eight explicitly authorized target/action bounds.
    pub targets: Vec<WriteTarget>,
    /// Finite admitted operation count, including failed/discarded attempts.
    pub max_operations: u32,
}

impl TaskScope {
    /// Validate the scope independently of any provider or owner decision.
    ///
    /// # Errors
    /// Rejects malformed, duplicate or unbounded scope.
    pub fn validate(&self) -> Result<(), CoreError> {
        if !bounded(&self.objective, 2048)
            || !bounded(&self.mapping_version, 128)
            || self.targets.is_empty()
            || self.targets.len() > 8
            || !(1..=1000).contains(&self.max_operations)
            || self
                .targets
                .iter()
                .enumerate()
                .any(|(i, t)| !t.valid() || self.targets[..i].contains(t))
        {
            return Err(CoreError(ErrorKind::InvalidInput, "write_task_scope"));
        }
        Ok(())
    }

    /// Enforce exact owner, mapping, supported inverse and declared target bounds.
    ///
    /// Current registry/source checks must also run during capture and final apply.
    ///
    /// # Errors
    /// Returns `write_task_scope` outside the grant, or `undo_unavailable` when the
    /// captured change lacks the paired inverse contract.
    pub fn permits(&self, p: &Preview, owner: &str) -> Result<(), CoreError> {
        self.validate()?;
        if p.actor != owner
            || p.mapping_version != self.mapping_version
            || !self.targets.iter().any(|target| target.permits(p))
        {
            return Err(CoreError(ErrorKind::Forbidden, "write_task_scope"));
        }
        validate_reversible(p)
    }
}

/// Authenticated owner's explicit decision about the bounded task scope.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDecision {
    /// Authorize this scope; supported later edits do not need another prompt.
    Approve,
    /// Close the proposal without granting authority.
    Reject,
}

/// End permission while retaining exact operation evidence and spent identities.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEnd {
    /// The agent reports completion; unresolved/prepared members prevent this.
    /// Actual operation outcomes remain independent of that task-level report.
    Completed,
    /// Revoke future execution even when dispatched outcomes remain unresolved.
    Cancelled,
}

/// Fresh reversible source capture, selected before durable admission.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskCapture {
    /// Registered literal rectangle.
    Grid(super::write_model::Change),
    /// Registered unique plain-text replacement.
    Document(super::documents::DocChange),
}

impl TaskScope {
    /// Validate current registered identities/permissions without provider IO.
    ///
    /// # Errors
    /// Rejects malformed scope, changed mapping or a target outside registration.
    pub fn registered(&self, registry: &super::registry::Registry) -> Result<(), CoreError> {
        self.validate()?;
        if self.mapping_version != registry.version {
            return Err(CoreError(ErrorKind::Conflict, "write_task_registry"));
        }
        for target in &self.targets {
            let valid = match target {
                WriteTarget::Grid {
                    spreadsheet,
                    spreadsheet_id,
                    view,
                    sheet_id,
                    range,
                } => {
                    let sheet = registry.lookup(spreadsheet)?;
                    sheet.readable
                        && sheet.id == *spreadsheet_id
                        && sheet.views.iter().any(|v| {
                            v.name == *view
                                && v.kind == super::registry::Kind::Grid
                                && v.sheet_id == *sheet_id
                                && v.writable_range
                                    .as_deref()
                                    .is_some_and(|r| contains(r, range))
                        })
                }
                WriteTarget::Document {
                    document,
                    document_id,
                    ..
                } => {
                    let doc = registry.document(document)?;
                    doc.readable && doc.writable && doc.id == *document_id
                }
            };
            if !valid {
                return Err(CoreError(ErrorKind::Forbidden, "write_task_registry"));
            }
        }
        Ok(())
    }

    /// Reject unsupported or out-of-scope intent before source access.
    ///
    /// Captured evidence must still pass `permits` before durable admission.
    /// # Errors
    /// Returns write_task_scope for a different resource/action/rectangle.
    pub fn permits_capture(&self, change: &TaskCapture) -> Result<(), CoreError> {
        let allowed = self.targets.iter().any(|target| match (target, change) {
            (
                WriteTarget::Grid {
                    spreadsheet,
                    view,
                    range,
                    ..
                },
                TaskCapture::Grid(c),
            ) => {
                c.spreadsheet == *spreadsheet
                    && c.view == *view
                    && c.action == "update"
                    && c.fields["range"]
                        .as_str()
                        .is_some_and(|r| contains(range, r))
            }
            (
                WriteTarget::Document {
                    document, tab_id, ..
                },
                TaskCapture::Document(c),
            ) => {
                c.document == *document
                    && c.tab_id == *tab_id
                    && matches!(c.action.as_str(), "replace_text" | "replace_text_batch")
            }
            _ => false,
        });
        if !allowed {
            return Err(CoreError(ErrorKind::Forbidden, "write_task_scope"));
        }
        Ok(())
    }
}
