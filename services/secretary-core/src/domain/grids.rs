//! Literal rectangular edits; no arbitrary provider request or formula input.
use crate::domain::{
    error::{CoreError, ErrorKind},
    projection,
    registry::{Kind, View},
    write_model::Change,
};
use serde_json::{json, Value};
pub fn contains(outer: &str, inner: &str) -> bool {
    match (
        projection::parse_range(outer),
        projection::parse_range(inner),
    ) {
        (Some((r, c, rr, cc)), Some((a, b, aa, bb))) => a >= r && b >= c && aa <= rr && bb <= cc,
        _ => false,
    }
}
pub fn target<'a>(v: &View, c: &'a Change) -> Result<(&'a str, Value), CoreError> {
    let allowed = v
        .writable_range
        .as_deref()
        .filter(|_| v.kind == Kind::Grid)
        .ok_or(CoreError(ErrorKind::Forbidden, "view_read_only"))?;
    if c.action != "update"
        || c.record_id.is_some()
        || c.fields.as_object().is_none_or(|f| f.len() != 2)
    {
        return Err(CoreError::invalid());
    }
    let range = c.fields["range"].as_str().ok_or_else(CoreError::invalid)?;
    let (r, col, rr, cc) = projection::parse_range(range).ok_or_else(CoreError::invalid)?;
    if !contains(allowed, range) || (rr - r) * (cc - col) > 100 {
        return Err(CoreError::invalid());
    }
    let rows = c.fields["values"]
        .as_array()
        .filter(|rows| rows.len() == rr - r)
        .ok_or_else(CoreError::invalid)?;
    let mut entered = Vec::new();
    for row in rows {
        let row = row
            .as_array()
            .filter(|row| row.len() == cc - col)
            .ok_or_else(CoreError::invalid)?;
        let mut cells = Vec::new();
        for value in row {
            let raw = match value {
                Value::Null => Value::Null,
                Value::String(s) if s.len() <= 2048 => json!({"stringValue":s}),
                Value::Bool(b) => json!({"boolValue":b}),
                Value::Number(n)
                    if n.as_f64()
                        .is_some_and(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0) =>
                {
                    json!({"numberValue":n})
                }
                _ => return Err(CoreError::invalid()),
            };
            cells.push(raw);
        }
        entered.push(Value::Array(cells));
    }
    if serde_json::to_vec(&entered)
        .map_err(|_| CoreError::invalid())?
        .len()
        > 16384
    {
        return Err(CoreError::invalid());
    }
    Ok((range, Value::Array(entered)))
}
pub fn registered_targets(
    s: &crate::domain::registry::Spreadsheet,
    v: &View,
    range: &str,
) -> Result<(), CoreError> {
    let (r, c, rr, cc) = projection::parse_range(range).ok_or_else(CoreError::invalid)?;
    for other in s.views.iter().filter(|other| other.sheet_id == v.sheet_id) {
        if other.structured() {
            if let Some((a, b, aa, bb)) = projection::parse_range(&other.range) {
                if a < rr && aa > r && b < cc && bb > c {
                    return Err(CoreError(ErrorKind::Forbidden, "structured_target"));
                }
            }
        }
        for cell in other.headers.keys().chain(other.formulas.keys()) {
            if let Some((a, b)) = projection::coordinate(cell) {
                if a >= r && a < rr && b >= c && b < cc {
                    return Err(CoreError(ErrorKind::Forbidden, "protected_target"));
                }
            }
        }
    }
    Ok(())
}
pub fn snapshot(v: &View, book: &Value, range: &str) -> Result<Value, CoreError> {
    let (r, c, rr, cc) = projection::parse_range(range).ok_or_else(CoreError::invalid)?;
    let sheet = projection::sheet(book, v)?;
    if let Some(merges) = sheet["merges"].as_array() {
        for m in merges {
            let a = m["startRowIndex"].as_u64().unwrap_or(0) as usize;
            let b = m["startColumnIndex"].as_u64().unwrap_or(0) as usize;
            let aa = m["endRowIndex"].as_u64().ok_or_else(CoreError::drift)? as usize;
            let bb = m["endColumnIndex"].as_u64().ok_or_else(CoreError::drift)? as usize;
            if a < rr && aa > r && b < cc && bb > c {
                return Err(CoreError(ErrorKind::Forbidden, "merged_target"));
            }
        }
    }
    let cells = projection::cells(sheet);
    let mut result = Vec::new();
    for row in r..rr {
        let mut values = Vec::new();
        for col in c..cc {
            let mut x = col + 1;
            let mut name = String::new();
            while x > 0 {
                x -= 1;
                name.insert(0, (b'A' + (x % 26) as u8) as char);
                x /= 26;
            }
            name.push_str(&(row + 1).to_string());
            if v.headers.contains_key(&name) || v.formulas.contains_key(&name) {
                return Err(CoreError(ErrorKind::Forbidden, "protected_target"));
            }
            let raw = cells
                .get(&name)
                .map(|v| v["userEnteredValue"].clone())
                .unwrap_or(Value::Null);
            if !raw.is_null()
                && (!raw.is_object()
                    || raw.as_object().unwrap().len() != 1
                    || !(raw["stringValue"].is_string()
                        || raw["numberValue"].is_number()
                        || raw["boolValue"].is_boolean()))
            {
                return Err(CoreError(
                    ErrorKind::Forbidden,
                    "formula_or_unsupported_target",
                ));
            }
            values.push(raw);
        }
        result.push(Value::Array(values));
    }
    let result = Value::Array(result);
    if serde_json::to_vec(&result)
        .map_err(|_| CoreError::invalid())?
        .len()
        > 16384
    {
        return Err(CoreError::invalid());
    }
    Ok(result)
}
pub fn requests(p: &crate::domain::operation::Preview) -> Value {
    let (r, c, _, _) = projection::parse_range(p.fields["range"].as_str().unwrap()).unwrap();
    let rows=p.after.as_array().unwrap().iter().map(|row|json!({"values":row.as_array().unwrap().iter().map(|v|if v.is_null(){json!({})}else{json!({"userEnteredValue":v})}).collect::<Vec<_>>()})).collect::<Vec<_>>();
    json!({"requests":[{"updateCells":{"start":{"sheetId":p.record_id.rsplit_once(':').unwrap().1.parse::<i64>().unwrap(),"rowIndex":r,"columnIndex":c},"rows":rows,"fields":"userEnteredValue"}}]})
}
