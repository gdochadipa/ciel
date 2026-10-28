//! Bounded Google Docs text projection and immutable edit intent. No IO.
use super::{
    error::{CoreError, ErrorKind},
    operation::{Preview, ResourceKind},
    projection::fingerprint,
    registry::Document,
    write_model::valid_id,
};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocChange {
    pub operation_id: String,
    pub document: String,
    pub tab_id: String,
    pub action: String,
    pub fields: Value,
}
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub text: String,
    pub editable: bool,
}
pub struct Snapshot {
    pub id: String,
    pub title: String,
    pub revision: String,
    pub tabs: Vec<Tab>,
    pub fingerprint: String,
}
fn unsupported() -> CoreError {
    CoreError(ErrorKind::Conflict, "document_structure_unsupported")
}
fn plain_content(
    content: &Value,
    text: &mut String,
    editable: &mut bool,
    depth: usize,
) -> Result<(), CoreError> {
    if depth > 16 {
        return Err(unsupported());
    }
    let entries = content.as_array().ok_or_else(unsupported)?;
    if entries.len() > 10000 {
        return Err(unsupported());
    }
    for entry in entries {
        if entry.get("sectionBreak").is_some() {
            continue;
        }
        if let Some(paragraph) = entry.get("paragraph") {
            let elements = paragraph["elements"].as_array().ok_or_else(unsupported)?;
            for element in elements {
                if let Some(value) = element["textRun"]["content"].as_str() {
                    text.push_str(value);
                } else {
                    *editable = false;
                }
            }
        } else if let Some(table) = entry.get("table") {
            *editable = false;
            for row in table["tableRows"].as_array().ok_or_else(unsupported)? {
                for cell in row["tableCells"].as_array().ok_or_else(unsupported)? {
                    plain_content(&cell["content"], text, editable, depth + 1)?;
                }
            }
        } else {
            *editable = false;
        }
        if text.len() > 131072 {
            return Err(CoreError(
                ErrorKind::PayloadTooLarge,
                "document_text_too_large",
            ));
        }
    }
    Ok(())
}
fn has_suggestions(value: &Value, depth: usize) -> bool {
    if depth > 32 {
        return true;
    }
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key.starts_with("suggested")
                && !value.is_null()
                && value != &json!({})
                && value != &json!([]))
                || has_suggestions(value, depth + 1)
        }),
        Value::Array(values) => values.iter().any(|v| has_suggestions(v, depth + 1)),
        _ => false,
    }
}
fn collect_tabs(values: &Value, tabs: &mut Vec<Tab>, depth: usize) -> Result<(), CoreError> {
    if depth > 16 {
        return Err(unsupported());
    }
    for value in values.as_array().ok_or_else(unsupported)? {
        if tabs.len() >= 100 {
            return Err(unsupported());
        }
        let id = value["tabProperties"]["tabId"]
            .as_str()
            .filter(|s| valid_tab(s))
            .ok_or_else(unsupported)?
            .to_owned();
        if tabs.iter().any(|tab| tab.id == id) {
            return Err(unsupported());
        }
        let mut text = String::new();
        let mut editable = !has_suggestions(value, 0);
        for key in ["headers", "footers", "footnotes"] {
            if value["documentTab"]
                .get(key)
                .is_some_and(|v| !v.is_null() && v != &json!({}))
            {
                editable = false;
            }
        }
        plain_content(
            &value["documentTab"]["body"]["content"],
            &mut text,
            &mut editable,
            0,
        )?;
        if !text.ends_with('\n') {
            editable = false;
        }
        tabs.push(Tab {
            id,
            title: value["tabProperties"]["title"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            text,
            editable,
        });
        if let Some(children) = value.get("childTabs") {
            collect_tabs(children, tabs, depth + 1)?;
        }
    }
    Ok(())
}
fn valid_insert(text: &str) -> bool {
    text.chars().all(|c| {
        (!c.is_control() || matches!(c, '\n' | '\t'))
            && !matches!(c as u32,0xE000..=0xF8FF|0xF0000..=0xFFFFD|0x100000..=0x10FFFD)
    })
}
pub fn valid_tab(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub fn snapshot(doc: &Document, value: &Value) -> Result<Snapshot, CoreError> {
    if value["documentId"].as_str() != Some(doc.id.as_str()) {
        return Err(CoreError::drift());
    }
    let mut tabs = Vec::new();
    collect_tabs(&value["tabs"], &mut tabs, 0)?;
    if tabs.is_empty() || tabs.iter().map(|t| t.text.len()).sum::<usize>() > 131072 {
        return Err(unsupported());
    }
    Ok(Snapshot {
        id: doc.id.clone(),
        title: value["title"].as_str().unwrap_or("").to_owned(),
        revision: value["revisionId"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .unwrap_or("")
            .to_owned(),
        tabs,
        fingerprint: fingerprint(value),
    })
}
pub fn matches(p: &Preview, c: &DocChange, owner: &str) -> bool {
    p.resource_kind == ResourceKind::GoogleDocs
        && p.operation_id == c.operation_id
        && p.actor == owner
        && p.spreadsheet == c.document
        && p.view == c.tab_id
        && p.action == c.action
        && p.fields == c.fields
}
pub fn prepare(
    c: &DocChange,
    doc: &Document,
    snapshot: &Snapshot,
    owner: &str,
    version: &str,
    expires_at: i64,
) -> Result<Preview, CoreError> {
    if !valid_id(&c.operation_id) || !valid_tab(&c.tab_id) || !doc.writable {
        return Err(CoreError(ErrorKind::Forbidden, "document_write_disabled"));
    }
    let tab = snapshot
        .tabs
        .iter()
        .find(|t| t.id == c.tab_id)
        .ok_or(CoreError(ErrorKind::NotFound, "document_tab_not_found"))?;
    if !tab.editable || snapshot.revision.is_empty() {
        return Err(unsupported());
    }
    let fields = c.fields.as_object().ok_or_else(CoreError::invalid)?;
    let body = tab.text.strip_suffix('\n').ok_or_else(unsupported)?;
    let after = match c.action.as_str() {
        "append_text" => {
            if fields.len() != 1 {
                return Err(CoreError::invalid());
            }
            let text = fields
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 8192 && valid_insert(s))
                .ok_or_else(CoreError::invalid)?;
            format!("{body}{text}\n")
        }
        "replace_text" => {
            if fields.len() != 2 {
                return Err(CoreError::invalid());
            }
            let find = fields
                .get("find")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 2048 && !s.contains(['\n', '\r']))
                .ok_or_else(CoreError::invalid)?;
            let replacement = fields
                .get("replacement")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 8192 && valid_insert(s))
                .ok_or_else(CoreError::invalid)?;
            if find == replacement || body.matches(find).count() != 1 {
                return Err(CoreError(ErrorKind::Conflict, "document_match_not_unique"));
            }
            format!("{}\n", body.replacen(find, replacement, 1))
        }
        _ => return Err(CoreError::invalid()),
    };
    if after.len() > 131072 {
        return Err(CoreError(
            ErrorKind::PayloadTooLarge,
            "document_text_too_large",
        ));
    }
    Ok(Preview {
        resource_kind: ResourceKind::GoogleDocs,
        operation_id: c.operation_id.clone(),
        actor: owner.into(),
        spreadsheet: c.document.clone(),
        view: c.tab_id.clone(),
        action: c.action.clone(),
        record_id: doc.id.clone(),
        fields: c.fields.clone(),
        before: json!({"text":tab.text}),
        after: json!({"text":after}),
        mapping_version: version.into(),
        source_revision: snapshot.revision.clone(),
        expires_at,
        preview_digest: String::new(),
    })
}
pub fn requests(preview: &Preview) -> Result<Value, CoreError> {
    if preview.resource_kind != ResourceKind::GoogleDocs || !valid_tab(&preview.view) {
        return Err(CoreError::invalid());
    }
    let request = match preview.action.as_str() {
        "append_text" => {
            json!({"insertText":{"endOfSegmentLocation":{"tabId":preview.view},"text":preview.fields["text"]}})
        }
        "replace_text" => {
            json!({"replaceAllText":{"tabsCriteria":{"tabIds":[preview.view]},"containsText":{"text":preview.fields["find"],"matchCase":true,"searchByRegex":false},"replaceText":preview.fields["replacement"]}})
        }
        _ => return Err(CoreError::invalid()),
    };
    Ok(json!({"writeControl":{"requiredRevisionId":preview.source_revision},"requests":[request]}))
}

/// Literal, nonoverlapping matches in projected body text; offsets are Unicode scalars.
pub fn search(
    snapshot: &Snapshot,
    query: &str,
    tab_id: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<(Vec<Value>, bool), CoreError> {
    if tab_id.is_some_and(|id| !snapshot.tabs.iter().any(|tab| tab.id == id)) {
        return Err(CoreError(ErrorKind::NotFound, "document_tab_not_found"));
    }
    let mut matches = Vec::new();
    let mut seen = 0;
    let query_chars = query.chars().count();
    for tab in snapshot
        .tabs
        .iter()
        .filter(|tab| tab_id.is_none_or(|id| tab.id == id))
    {
        let chars: Vec<_> = tab.text.chars().collect();
        for (byte, _) in tab.text.match_indices(query) {
            if seen < offset {
                seen += 1;
                continue;
            }
            if matches.len() == limit {
                return Ok((matches, true));
            }
            let start = tab.text[..byte].chars().count();
            let end = start + query_chars;
            let snippet_start = start.saturating_sub(80);
            let snippet_end = (end + 80).min(chars.len());
            matches.push(json!({"tab_id":tab.id,"tab_title":tab.title.chars().take(200).collect::<String>(),"start_character":start,"end_character":end,"snippet_start_character":snippet_start,"snippet_end_character":snippet_end,"text":chars[snippet_start..snippet_end].iter().collect::<String>()}));
            seen += 1;
        }
    }
    if offset > 0 && matches.is_empty() {
        return Err(CoreError::invalid());
    }
    Ok((matches, false))
}
