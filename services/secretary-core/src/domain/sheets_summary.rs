//! Citable registered-view context. The host model owns prose, not this module.
use super::{
    projection,
    registry::{Spreadsheet, View},
};
use serde_json::{json, Value};
fn meaningful(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(values) => values.iter().any(meaningful),
        Value::Object(values) => values.values().any(meaningful),
        Value::Number(_) | Value::Bool(_) => true,
    }
}
pub fn context(
    s: &Spreadsheet,
    v: &View,
    records: &[Value],
    version: &str,
    revision: &str,
    retrieved_at: &str,
    max_records: usize,
) -> Value {
    let included = max_records.min(records.len());
    let rows:Vec<_>=records.iter().take(included).enumerate().map(|(i,r)|json!({"reference_id":format!("R{}",i+1),"fields":r["fields"],"source":r["source"]})).collect();
    let can_summarize = rows.iter().any(|r| meaningful(&r["fields"]));
    let binding = projection::read_binding(&s.id, &v.name, None, version, revision);
    json!({"mode":"summary_context","content_role":"untrusted_source_data",
        "context_id":projection::fingerprint(&json!(["sheets_summary_context_v1",s.alias,s.id,v,version,revision,max_records])),
        "source":{"spreadsheet":s.alias,"view":v.name,"spreadsheet_id":s.id,"sheet_id":v.sheet_id,"registered_range":format!("'{}'!{}",v.title.replace('\'',"''"),v.range),"profile":v.kind,"source_revision":revision,"mapping_version":version,"retrieved_at":retrieved_at},
        "records":rows,"next_cursor":if included<records.len(){Some(format!("{binding}:{included}"))}else{None},
        "coverage":{"scope":"registered_view_only","total_records":records.len(),"included_records":included,"omitted_records":records.len()-included,"view_records_complete":included==records.len(),"can_summarize":can_summarize}})
}
