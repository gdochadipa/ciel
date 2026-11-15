//! Exact inverse candidates for retained applied intent. Planning grants no authority.
use super::{
    documents::DocChange,
    error::{CoreError, ErrorKind},
    operation::{digest, Operation, OperationState, Preview, ResourceKind},
    projection::parse_range,
    write_model::{valid_id, Change},
};
use serde_json::{json, Value};

/// An inverse request that must still pass current-source preview and new approval.
#[derive(Clone)]
pub enum InverseChange {
    /// Restore exact entered values within the original rectangle.
    Grid(Change),
    /// Reverse one unambiguous plain-text replacement in the original tab.
    Document(DocChange),
}

/// Immutable original/inverse binding; not an execution grant or journal record.
///
/// Load the original through the authoritative owner-bound store. After preparing
/// the returned request against current registrations and source, validate that
/// fresh preview before offering it for a separate owner approval. This type does
/// not qualify provider behavior or create a public undo route.
///
/// # Examples
/// ```no_run
/// # fn example(original: &secretary_core::domain::operation::Operation,
/// # fresh_preview: &secretary_core::domain::operation::Preview)
/// # -> Result<(), secretary_core::CoreError> {
/// use secretary_core::domain::compensation::Inverse;
/// let inverse = Inverse::from_applied(original, "new-undo-id", "owner")?;
/// // Prepare inverse.change() through the normal current-source preview path.
/// inverse.validate_preview(fresh_preview)?;
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Inverse {
    original: Preview,
    change: InverseChange,
}

fn unavailable() -> CoreError {
    CoreError(ErrorKind::Conflict, "undo_unavailable")
}

/// Check inverse coverage of captured intent before offering forward approval.
///
/// This pure check does not assert that a preview is approved, that its source is
/// current, or that provider structures and permissions were validated. Those
/// remain responsibilities of normal preparation and the task coordinator.
///
/// # Examples
/// ```no_run
/// # fn check(preview: &secretary_core::domain::operation::Preview)
/// # -> Result<(), secretary_core::CoreError> {
/// secretary_core::domain::compensation::validate_reversible(preview)?;
/// # Ok(()) }
/// ```
/// # Errors
/// Rejects unsupported classes, insufficient capture or ambiguous inverse text.
pub fn validate_reversible(preview: &Preview) -> Result<(), CoreError> {
    inverse_change(preview, "inverse-support-check").map(|_| ())
}

fn inverse_change(preview: &Preview, operation_id: &str) -> Result<InverseChange, CoreError> {
    match preview.resource_kind {
        ResourceKind::SheetsGrid => grid(preview, operation_id).map(InverseChange::Grid),
        ResourceKind::GoogleDocs => document(preview, operation_id).map(InverseChange::Document),
        _ => Err(unavailable()),
    }
}

impl Inverse {
    /// Plan a supported exact inverse from an authoritative applied operation.
    ///
    /// Original approval expiry is deliberately irrelevant to retained evidence.
    /// The candidate has a distinct operation ID and carries no original approval.
    ///
    /// # Errors
    /// Rejects wrong owner, unresolved/failed original, invalid identity/digest,
    /// unsupported class, malformed evidence or an ambiguous inverse.
    pub fn from_applied(
        original: &Operation,
        operation_id: &str,
        owner: &str,
    ) -> Result<Self, CoreError> {
        let p = &original.preview;
        if original.state != OperationState::Applied
            || owner.is_empty()
            || owner.len() > 256
            || owner.chars().any(char::is_control)
            || p.actor != owner
            || !valid_id(&p.operation_id)
            || !valid_id(operation_id)
            || p.operation_id == operation_id
            || p.spreadsheet.is_empty()
            || p.view.is_empty()
            || p.record_id.is_empty()
            || p.mapping_version.is_empty()
            || p.source_revision.is_empty()
        {
            return Err(unavailable());
        }
        let mut sealed = p.clone();
        sealed.preview_digest.clear();
        if p.preview_digest != digest(&serde_json::to_value(&sealed).map_err(|_| unavailable())?) {
            return Err(unavailable());
        }
        let change = inverse_change(p, operation_id)?;
        Ok(Self {
            original: p.clone(),
            change,
        })
    }

    /// Read the candidate request. It has no approval or dispatch authority.
    pub fn change(&self) -> &InverseChange {
        &self.change
    }

    /// Read the retained original's stable operation ID for compensation linkage.
    pub fn original_id(&self) -> &str {
        &self.original.operation_id
    }

    /// Read the original preview digest that must be retained with that linkage.
    pub fn original_digest(&self) -> &str {
        &self.original.preview_digest
    }

    /// Bind a freshly prepared inverse to the exact original target and evidence.
    ///
    /// Source revision and expiry come from fresh preparation, not from the old
    /// approval. Normal preview/apply code must still enforce current grants,
    /// structure, revision, expiry and new owner approval before any provider send.
    ///
    /// # Errors
    /// Returns `undo_conflict` for any changed request, target, mapping or affected
    /// content. Even an unrelated text edit inside the captured Docs tab conflicts.
    pub fn validate_preview(&self, prepared: &Preview) -> Result<(), CoreError> {
        let original = &self.original;
        let request_matches = match &self.change {
            InverseChange::Grid(c) => {
                prepared.operation_id == c.operation_id
                    && prepared.action == c.action
                    && prepared.fields == c.fields
            }
            InverseChange::Document(c) => {
                prepared.operation_id == c.operation_id
                    && prepared.action == c.action
                    && prepared.fields == c.fields
            }
        };
        if !request_matches
            || prepared.actor != original.actor
            || prepared.resource_kind != original.resource_kind
            || prepared.spreadsheet != original.spreadsheet
            || prepared.view != original.view
            || prepared.record_id != original.record_id
            || prepared.mapping_version != original.mapping_version
            || prepared.source_revision.is_empty()
            || prepared.before != original.after
            || prepared.after != original.before
        {
            return Err(CoreError(ErrorKind::Conflict, "undo_conflict"));
        }
        Ok(())
    }
}

fn literals(raw: &Value, rows: usize, columns: usize) -> Result<Value, CoreError> {
    if serde_json::to_vec(raw).map_err(|_| unavailable())?.len() > 16384 {
        return Err(unavailable());
    }
    let raw = raw
        .as_array()
        .filter(|values| values.len() == rows)
        .ok_or_else(unavailable)?;
    raw.iter()
        .map(|row| {
            row.as_array()
                .filter(|cells| cells.len() == columns)
                .ok_or_else(unavailable)?
                .iter()
                .map(|cell| {
                    if cell.is_null() {
                        return Ok(Value::Null);
                    }
                    let object = cell
                        .as_object()
                        .filter(|object| object.len() == 1)
                        .ok_or_else(unavailable)?;
                    match object.iter().next() {
                        Some((key, Value::String(s)))
                            if key == "stringValue" && s.len() <= 2048 =>
                        {
                            Ok(Value::String(s.clone()))
                        }
                        Some((key, Value::Bool(b))) if key == "boolValue" => Ok(json!(b)),
                        Some((key, Value::Number(n)))
                            if key == "numberValue"
                                && n.as_f64().is_some_and(|n| {
                                    n.is_finite() && n.abs() <= 9_007_199_254_740_991.0
                                }) =>
                        {
                            Ok(Value::Number(n.clone()))
                        }
                        _ => Err(unavailable()),
                    }
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn grid(p: &Preview, operation_id: &str) -> Result<Change, CoreError> {
    if p.action != "update" || p.fields.as_object().is_none_or(|fields| fields.len() != 2) {
        return Err(unavailable());
    }
    let range = p.fields["range"].as_str().ok_or_else(unavailable)?;
    let (r, c, rr, cc) = parse_range(range).ok_or_else(unavailable)?;
    let rows = rr
        .checked_sub(r)
        .filter(|n| *n > 0)
        .ok_or_else(unavailable)?;
    let columns = cc
        .checked_sub(c)
        .filter(|n| *n > 0)
        .ok_or_else(unavailable)?;
    if rows.checked_mul(columns).is_none_or(|n| n > 100) {
        return Err(unavailable());
    }
    let before = literals(&p.before, rows, columns)?;
    let after = literals(&p.after, rows, columns)?;
    if p.fields["values"] != after || before == after {
        return Err(unavailable());
    }
    Ok(Change {
        operation_id: operation_id.into(),
        spreadsheet: p.spreadsheet.clone(),
        view: p.view.clone(),
        action: "update".into(),
        record_id: None,
        fields: json!({"range":range,"values":before}),
    })
}

fn single_line(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 2048
        && text.chars().all(|c| {
            !c.is_control()
                && !matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
        })
}

fn body(value: &Value) -> Result<&str, CoreError> {
    if value.as_object().is_none_or(|object| object.len() != 1) {
        return Err(unavailable());
    }
    value["text"]
        .as_str()
        .filter(|text| text.len() <= 131072)
        .and_then(|text| text.strip_suffix('\n'))
        .ok_or_else(unavailable)
}

fn document(p: &Preview, operation_id: &str) -> Result<DocChange, CoreError> {
    if p.action == "replace_text_batch" {
        return compound_document(p, operation_id);
    }
    if p.action != "replace_text"
        || !super::documents::valid_tab(&p.view)
        || p.fields.as_object().is_none_or(|fields| fields.len() != 2)
    {
        return Err(unavailable());
    }
    let find = p.fields["find"]
        .as_str()
        .filter(|s| single_line(s))
        .ok_or_else(unavailable)?;
    let replacement = p.fields["replacement"]
        .as_str()
        .filter(|s| single_line(s))
        .ok_or_else(unavailable)?;
    let before = body(&p.before)?;
    let after = body(&p.after)?;
    // The provider uses replaceAllText. Both directions must match exactly once;
    // before/after swapping alone would silently replace unrelated occurrences.
    if find == replacement
        || before.matches(find).count() != 1
        || before.replacen(find, replacement, 1) != after
        || after.matches(replacement).count() != 1
        || after.replacen(replacement, find, 1) != before
    {
        return Err(unavailable());
    }
    Ok(DocChange {
        operation_id: operation_id.into(),
        document: p.spreadsheet.clone(),
        tab_id: p.view.clone(),
        action: "replace_text".into(),
        fields: json!({"find":replacement,"replacement":find}),
    })
}

fn compound_document(p: &Preview, operation_id: &str) -> Result<DocChange, CoreError> {
    use super::documents::{compound, valid_tab};
    if !valid_tab(&p.view) {
        return Err(unavailable());
    }
    let edits = compound::parse(&p.fields).map_err(|_| unavailable())?;
    let before = body(&p.before)?;
    let after = body(&p.after)?;
    if compound::prepare(before, &edits).map_err(|_| unavailable())? != after {
        return Err(unavailable());
    }
    Ok(DocChange {
        operation_id: operation_id.into(),
        document: p.spreadsheet.clone(),
        tab_id: p.view.clone(),
        action: "replace_text_batch".into(),
        fields: json!({"replacements": compound::inverse(&edits)}),
    })
}

#[cfg(test)]
mod tests;
