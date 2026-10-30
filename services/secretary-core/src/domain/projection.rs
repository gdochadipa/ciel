use crate::{
    domain::error::CoreError,
    domain::registry::{Kind, Spreadsheet, View},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub fn fingerprint(book: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(book).unwrap_or_default())
    )
}
pub fn read_binding(
    id: &str,
    view: &str,
    query: Option<&str>,
    version: &str,
    revision: &str,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}|{}|{}|{}",
                id,
                view,
                query.unwrap_or(""),
                version,
                revision
            )
            .as_bytes()
        )
    )
}
pub fn coordinate(s: &str) -> Option<(usize, usize)> {
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (letters, row) = s.split_at(split);
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let col = letters.bytes().try_fold(0usize, |n, b| {
        n.checked_mul(26)?.checked_add((b - b'A' + 1) as usize)
    })?;
    let row: usize = row.parse().ok()?;
    if row == 0 || row > 10000 || col > 100 {
        return None;
    }
    Some((row - 1, col - 1))
}
pub fn parse_range(s: &str) -> Option<(usize, usize, usize, usize)> {
    let (a, b) = s.split_once(':')?;
    let (r, c) = coordinate(a)?;
    let (rr, cc) = coordinate(b)?;
    if r > rr || c > cc {
        return None;
    }
    Some((r, c, rr + 1, cc + 1))
}
fn address(row: usize, col: usize) -> String {
    let mut n = col + 1;
    let mut name = String::new();
    while n > 0 {
        n -= 1;
        name.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26
    }
    format!("{name}{}", row + 1)
}
pub(crate) fn sheet<'a>(book: &'a Value, v: &View) -> Result<&'a Value, CoreError> {
    book["sheets"]
        .as_array()
        .and_then(|s| {
            s.iter().find(|s| {
                s["properties"]["sheetId"].as_i64() == Some(v.sheet_id)
                    && s["properties"]["title"].as_str() == Some(&v.title)
            })
        })
        .ok_or_else(CoreError::drift)
}
pub(crate) fn cells(sheet: &Value) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    if let Some(grids) = sheet["data"].as_array() {
        for grid in grids {
            let row0 = grid["startRow"].as_u64().unwrap_or(0) as usize;
            let col0 = grid["startColumn"].as_u64().unwrap_or(0) as usize;
            if let Some(rows) = grid["rowData"].as_array() {
                for (r, row) in rows.iter().enumerate() {
                    if let Some(cols) = row["values"].as_array() {
                        for (c, cell) in cols.iter().enumerate() {
                            result.insert(address(row0 + r, col0 + c), cell.clone());
                        }
                    }
                }
            }
        }
    }
    result
}
fn value(cell: Option<&Value>) -> Value {
    cell.map(|c| {
        let e = &c["effectiveValue"];
        if let Some(n) = e.get("numberValue") {
            n.clone()
        } else if let Some(t) = e.get("stringValue") {
            t.clone()
        } else if let Some(b) = e.get("boolValue") {
            b.clone()
        } else {
            Value::Null
        }
    })
    .unwrap_or(Value::Null)
}
pub(crate) fn at(c: &BTreeMap<String, Value>, a: &str) -> Value {
    value(c.get(a))
}
fn text(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| !s.trim().is_empty())
}
fn integer(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| {
        v.as_f64()
            .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0)
            .map(|n| n as i64)
    })
}
fn cost(v: Value) -> Value {
    if let Some(n) = integer(&v).filter(|n| *n >= 0) {
        json!({"state":"known","amount":n})
    } else {
        json!({"state":"unknown","raw":v})
    }
}
// Only the configured, validated SUM reference grammar is interpreted. No formula execution.
fn recompute(cells: &BTreeMap<String, Value>, cell: &str, depth: usize) -> Option<i64> {
    if depth > 16 {
        return None;
    }
    let Some(formula) = cells
        .get(cell)
        .and_then(|v| v["userEnteredValue"]["formulaValue"].as_str())
    else {
        let value = at(cells, cell);
        return if value.is_number() {
            integer(&value)
        } else {
            Some(0)
        };
    };
    let args = formula.strip_prefix("=SUM(")?.strip_suffix(')')?;
    let mut sum = 0i64;
    for arg in args.split(',') {
        if arg.contains(':') {
            let (r, c, rr, cc) = parse_range(arg)?;
            if (rr - r).checked_mul(cc - c)? > 1000 {
                return None;
            }
            for row in r..rr {
                for col in c..cc {
                    sum = sum.checked_add(recompute(cells, &address(row, col), depth + 1)?)?
                }
            }
        } else {
            coordinate(arg)?;
            sum = sum.checked_add(recompute(cells, arg, depth + 1)?)?;
        }
    }
    Some(sum)
}
fn merged_anchor(v: &View, cell: &str) -> String {
    let Some((r, c)) = coordinate(cell) else {
        return cell.into();
    };
    for m in &v.merges {
        if let Some((r0, c0, r1, c1)) = parse_range(m) {
            if r >= r0 && r < r1 && c >= c0 && c < c1 {
                return address(r0, c0);
            }
        }
    }
    cell.into()
}
fn inherited(cells: &BTreeMap<String, Value>, v: &View, cell: &str) -> Value {
    at(cells, &merged_anchor(v, cell))
}
pub fn validate(s: &Spreadsheet, book: &Value) -> Result<(), CoreError> {
    if book["spreadsheetId"].as_str() != Some(s.id.as_str()) {
        return Err(CoreError::drift());
    }
    for v in &s.views {
        let sh = sheet(book, v)?;
        let c = cells(sh);
        for (a, expected) in &v.headers {
            if at(&c, a).as_str() != Some(expected) {
                return Err(CoreError::drift());
            }
        }
        // Generic grids project dynamic data rather than a frozen domain template.
        if v.kind == Kind::Grid {
            continue;
        }
        let observed: BTreeMap<String, String> = c
            .iter()
            .filter_map(|(a, c)| {
                c["userEnteredValue"]["formulaValue"]
                    .as_str()
                    .map(|f| (a.clone(), f.into()))
            })
            .collect();
        if observed != v.formulas {
            return Err(CoreError::drift());
        }
        if c.values()
            .any(|v| v["effectiveValue"].get("errorValue").is_some())
        {
            return Err(CoreError::drift());
        }
        let expected: BTreeSet<_> = v.merges.iter().filter_map(|m| parse_range(m)).collect();
        let mut observed = BTreeSet::new();
        if let Some(merges) = sh["merges"].as_array() {
            for m in merges {
                let r0 = m["startRowIndex"].as_u64().unwrap_or(0) as usize;
                let c0 = m["startColumnIndex"].as_u64().unwrap_or(0) as usize;
                let r1 = m["endRowIndex"].as_u64().ok_or_else(CoreError::drift)? as usize;
                let c1 = m["endColumnIndex"].as_u64().ok_or_else(CoreError::drift)? as usize;
                let (vr, vc, vrr, vcc) = parse_range(&v.range).ok_or_else(CoreError::drift)?;
                if r0 < vrr && r1 > vr && c0 < vcc && c1 > vc {
                    observed.insert((r0, c0, r1, c1));
                }
            }
        }
        if expected != observed {
            return Err(CoreError::drift());
        }
    }
    Ok(())
}
fn record(
    s: &Spreadsheet,
    v: &View,
    range: String,
    fields: Value,
    version: &str,
    revision: &str,
    now: &str,
) -> Value {
    json!({"fields":fields,"source":{"spreadsheet_id":s.id,"sheet_id":v.sheet_id,"range":format!("'{}'!{}",v.title.replace('\'',"''"),range),"record_id":null,"retrieved_at":now,"mapping_version":version,"source_revision":revision}})
}
pub fn project(
    s: &Spreadsheet,
    v: &View,
    book: &Value,
    version: &str,
) -> Result<Vec<Value>, CoreError> {
    let c = cells(sheet(book, v)?);
    let rev = fingerprint(book);
    let now = chrono::Utc::now().to_rfc3339();
    let mut out = Vec::new();
    let mut push =
        |range: String, fields: Value| out.push(record(s, v, range, fields, version, &rev, &now));
    match v.kind {
        Kind::Grid => {
            let (r, c0, rr, cc) = parse_range(&v.range).ok_or_else(CoreError::invalid)?;
            for row in r..rr {
                let mut fields = serde_json::Map::new();
                for col in c0..cc {
                    let addr = address(row, col);
                    let key = addr
                        .trim_end_matches(|c: char| c.is_ascii_digit())
                        .to_owned();
                    let cell = c.get(&addr);
                    let mut value = at(&c, &addr);
                    if let Some(error) = cell.and_then(|x| x["effectiveValue"].get("errorValue")) {
                        value = json!({"error":error.get("type").and_then(Value::as_str).unwrap_or("CELL_ERROR")});
                    }
                    fields.insert(key, value);
                }
                push(
                    format!("{}:{}", address(row, c0), address(row, cc - 1)),
                    Value::Object(fields),
                );
            }
        }
        Kind::WeddingCosts => {
            for (section, lo, hi) in [
                ("initial", 2, 8),
                ("prewedding", 12, 17),
                ("mesedek", 20, 26),
                ("ngidih", 29, 44),
                ("wedding_day", 50, 59),
            ] {
                for r in lo..=hi {
                    let description = at(&c, &format!("{}{}", if r == 17 { "C" } else { "B" }, r));
                    // Missing mandatory item position indicates structural drift rather than silent omission.
                    if text(&description).is_none() {
                        return Err(CoreError::drift());
                    }
                    let anchor = merged_anchor(v, &format!("D{r}"));
                    let observed = at(&c, &anchor);
                    push(
                        format!("A{r}:G{r}"),
                        json!({"description":description,"section":section,"vendor_note":if r==17 {Value::Null}else{inherited(&c,v,&format!("C{r}"))},"funding_note":inherited(&c,v,&format!("F{r}")),"cost_group":format!("{}:{}",v.sheet_id,anchor),"cost_anchor":anchor,"cost":if anchor==format!("D{r}"){cost(observed)}else{json!({"state":"shared","anchor":anchor})},"status":null,"unmapped_amount":at(&c,&format!("E{r}"))}),
                    );
                }
            }
        }
        Kind::GuestEstimates => {
            for (side, label, count) in [("left", "A", "B"), ("right", "C", "D")] {
                for r in 2..=10 {
                    let raw = at(&c, &format!("{count}{r}"));
                    let label = at(&c, &format!("{label}{r}"));
                    if raw.is_null() && label.is_null() {
                        continue;
                    }
                    let declared = if raw.is_u64() {
                        raw.clone()
                    } else if let Some(t) = raw.as_str() {
                        let first = t.split_whitespace().next().unwrap_or("");
                        if first.bytes().all(|b| b.is_ascii_digit()) {
                            first
                                .parse::<u64>()
                                .ok()
                                .map(Value::from)
                                .unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        }
                    } else {
                        Value::Null
                    };
                    push(
                        format!("A{r}:F{r}"),
                        json!({"side":side,"label":label,"raw_count":raw,"declared_count":declared,"notes":[at(&c,&format!("E{r}")),at(&c,&format!("F{r}"))],"confirmed_attendance":null}),
                    );
                }
            }
            for a in ["B12", "D12", "B13", "D13", "E12"] {
                let raw = at(&c, a);
                if !raw.is_null() {
                    push(
                        a.into(),
                        json!({"kind":"summary_or_scenario","raw":raw,"formula":c.get(a).and_then(|x|x["userEnteredValue"].get("formulaValue"))}),
                    );
                }
            }
        }
        Kind::PlanningNotes => {
            for r in 5..=19 {
                for col in ["B", "C"] {
                    let addr = format!("{col}{r}");
                    let raw = at(&c, &addr);
                    if let Some(t) = text(&raw) {
                        let candidates: Vec<_> = t
                            .lines()
                            .map(|x| x.trim().trim_start_matches(['-', '•']).trim())
                            .filter(|x| !x.is_empty())
                            .collect();
                        if candidates.is_empty() {
                            continue;
                        }
                        push(
                            addr,
                            json!({"month":inherited(&c,v,&format!("A{r}")),"owner_label":at(&c,&format!("{col}4")),"raw":raw,"task_candidates":candidates,"year":null,"due_at":null,"status":null}),
                        );
                    }
                }
            }
            for a in ["A26", "B26", "C26"] {
                let raw = at(&c, a);
                if !raw.is_null() {
                    push(
                        a.into(),
                        json!({"kind":"unlinked_note","raw":raw,"year":null,"due_at":null}),
                    );
                }
            }
        }
        Kind::PreweddingOptions => {
            for col in ["B", "C"] {
                let mut attrs = serde_json::Map::new();
                for r in 4..=10 {
                    attrs.insert(
                        format!("row_{r}"),
                        json!({"label":at(&c,&format!("A{r}")),"raw":at(&c,&format!("{col}{r}")),"parsed_explicit_thousands":parse_thousands(&at(&c,&format!("{col}{r}")))}),
                    );
                }
                push(
                    format!("{col}3:{col}10"),
                    json!({"name":at(&c,&format!("{col}3")),"base_cost":cost(at(&c,&format!("{col}4"))),"attributes":attrs,"selected":null,"quote_date":null}),
                );
            }
        }
        Kind::StructuredTasks | Kind::StructuredPayments => {
            for row in crate::domain::write_model::rows(s, v, book)? {
                let mut record = record(
                    s,
                    v,
                    format!("A{}:Z{}", row.row, row.row),
                    row.fields,
                    version,
                    &rev,
                    &now,
                );
                record["source"]["record_id"] = json!(row.record_id);
                out.push(record);
            }
        }
        Kind::HouseholdItems => {
            for r in 3..=16 {
                let label = at(&c, &format!("A{r}"));
                if text(&label).is_none() {
                    return Err(CoreError::drift());
                }
                push(
                    format!("A{r}:D{r}"),
                    json!({"description":label,"cost":cost(at(&c,&format!("B{r}"))),"unmapped_annotation":at(&c,&format!("D{r}")),"settled":if at(&c,&format!("C{r}")).as_str()==Some("YA"){json!(true)}else{Value::Null}}),
                );
            }
            // The unlabeled amount is separate from the fourteen item records.
        }
    }
    Ok(out)
}
pub fn report(s: &Spreadsheet, book: &Value, version: &str) -> Result<Value, CoreError> {
    let v = s
        .views
        .iter()
        .find(|v| v.kind == Kind::WeddingCosts)
        .ok_or_else(CoreError::invalid)?;
    let records = project(s, v, book, version)?;
    let mut sum = 0i64;
    let mut known = 0;
    let mut unknown = 0;
    let mut shared = 0;
    for r in &records {
        match r["fields"]["cost"]["state"].as_str() {
            Some("known") => {
                sum = sum
                    .checked_add(
                        r["fields"]["cost"]["amount"]
                            .as_i64()
                            .ok_or_else(CoreError::drift)?,
                    )
                    .ok_or_else(CoreError::drift)?;
                known += 1
            }
            Some("shared") => shared += 1,
            _ => unknown += 1,
        }
    }
    let c = cells(sheet(book, v)?);
    let summaries:Vec<_>=v.formulas.iter().map(|(a,f)|json!({"cell":a,"formula":f,"cached_value":at(&c,a),"recomputed_numeric_inputs":recompute(&c,a,0),"cache_matches":recompute(&c,a,0).zip(integer(&at(&c,a))).map(|(a,b)|a==b),"source":record(s,v,a.clone(),Value::Null,version,&fingerprint(book),&chrono::Utc::now().to_rfc3339())["source"]})).collect();
    Ok(
        json!({"known_cost_total":sum,"known_cost_groups":known,"unknown_cost_groups":unknown,"shared_child_positions":shared,"currency":null,"currency_status":"unconfirmed","paid_total":null,"outstanding_balance":null,"completion_percent":null,"coverage":"incomplete","caveats":["Dash costs remain unknown; funding labels are not payments.","Source summary formulas are cached values, not a remaining balance.","Household costs and guest estimates are excluded.","Monthly notes have no established year or deadlines."],"formula_summaries":summaries,"records":records,"source_revision":fingerprint(book),"mapping_version":version,"retrieved_at":chrono::Utc::now().to_rfc3339()}),
    )
}

fn parse_thousands(value: &Value) -> Vec<Value> {
    let Some(text) = value.as_str() else {
        return Vec::new();
    };
    text.split_whitespace()
        .filter_map(|token| {
            let raw = token.trim_matches(|c: char| matches!(c, ',' | ';' | ':' | '(' | ')'));
            let digits = raw.strip_suffix('k').or_else(|| raw.strip_suffix('K'))?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let amount = digits.parse::<i64>().ok()?.checked_mul(1000)?;
            Some(json!({"raw_token":raw,"amount":amount}))
        })
        .collect()
}
