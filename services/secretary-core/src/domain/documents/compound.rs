//! Bounded independent replacements, with exact forward and reverse text evidence.
use crate::domain::error::{CoreError, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Replacement {
    find: String,
    replacement: String,
}

fn conflict() -> CoreError {
    CoreError(ErrorKind::Conflict, "document_compound_conflict")
}

fn literal(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 2048
        && text.chars().all(|c| {
            !c.is_control()
                && !matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
        })
}

pub(crate) fn parse(fields: &Value) -> Result<Vec<Replacement>, CoreError> {
    if fields.as_object().is_none_or(|fields| fields.len() != 1) {
        return Err(CoreError::invalid());
    }
    let values = fields["replacements"]
        .as_array()
        .filter(|values| (2..=16).contains(&values.len()))
        .ok_or_else(CoreError::invalid)?;
    values
        .iter()
        .map(|value| {
            let edit: Replacement =
                serde_json::from_value(value.clone()).map_err(|_| CoreError::invalid())?;
            if !literal(&edit.find) || !literal(&edit.replacement) || edit.find == edit.replacement
            {
                return Err(CoreError::invalid());
            }
            Ok(edit)
        })
        .collect()
}

fn project(body: &str, edits: &[Replacement]) -> Result<String, CoreError> {
    let mut spans = edits
        .iter()
        .map(|edit| {
            let mut matches = body.match_indices(&edit.find);
            let (start, _) = matches.next().ok_or_else(conflict)?;
            if matches.next().is_some() {
                return Err(conflict());
            }
            Ok((start, start + edit.find.len(), edit.replacement.as_str()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    spans.sort_unstable_by_key(|span| span.0);
    if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(conflict());
    }
    let mut expected = body.to_owned();
    for (start, end, replacement) in spans.into_iter().rev() {
        expected.replace_range(start..end, replacement);
    }
    // Google executes the ordered requests against intermediate text. Comparing
    // with independent source spans rejects cascades and boundary-created matches.
    let mut sequential = body.to_owned();
    for edit in edits {
        if sequential.matches(&edit.find).count() != 1 {
            return Err(conflict());
        }
        sequential = sequential.replacen(&edit.find, &edit.replacement, 1);
    }
    if sequential != expected || expected.len() >= 131072 {
        return Err(conflict());
    }
    Ok(expected)
}

pub(crate) fn inverse(edits: &[Replacement]) -> Vec<Replacement> {
    edits
        .iter()
        .rev()
        .map(|edit| Replacement {
            find: edit.replacement.clone(),
            replacement: edit.find.clone(),
        })
        .collect()
}

pub(crate) fn prepare(body: &str, edits: &[Replacement]) -> Result<String, CoreError> {
    let after = project(body, edits)?;
    if after == body || project(&after, &inverse(edits))? != body {
        return Err(conflict());
    }
    Ok(after)
}

pub(crate) fn requests(fields: &Value, tab: &str) -> Result<Vec<Value>, CoreError> {
    Ok(parse(fields)?
        .iter()
        .map(|edit| super::replacement_request(tab, &edit.find, &edit.replacement))
        .collect())
}
