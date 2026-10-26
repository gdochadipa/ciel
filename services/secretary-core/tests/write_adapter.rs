mod common;
use common::{cell, fixture};
use secretary_core::{
    projection,
    write_model::{prepare as domain_prepare, rows, Change, PreparationContext},
};
use serde_json::json;
fn prepare(
    change: &Change,
    owner: &str,
    s: &secretary_core::registry::Spreadsheet,
    v: &secretary_core::registry::View,
    book: &serde_json::Value,
    version: &str,
) -> Result<secretary_core::domain::operation::Preview, secretary_core::CoreError> {
    domain_prepare(
        change,
        owner,
        s,
        v,
        book,
        version,
        &PreparationContext {
            new_record_id: "fixture-record",
            expires_at: 2_000_000_000,
        },
    )
}
fn change() -> Change {
    serde_json::from_value(json!({"operation_id":"sample-operation","spreadsheet":"wedding-demo","view":"tasks","action":"append","fields":{"title":"=literal text stays RAW","status":"todo"}})).unwrap()
}
#[test]
fn structured_preview_has_exact_fields_and_legacy_targets_stay_read_only() {
    let (registry, book) = fixture();
    let s = &registry.spreadsheets[0];
    let v = &s.views[5];
    registry.validate().unwrap();
    projection::validate(s, &book).unwrap();
    let p = prepare(
        &change(),
        "configured-owner",
        s,
        v,
        &book,
        &registry.version,
    )
    .unwrap();
    assert_eq!(p.actor, "configured-owner");
    assert_eq!(
        p.after,
        json!({"title":"=literal text stays RAW","status":"todo","due_date":null})
    );
    assert!(p.before.is_null());
    assert!(!p.record_id.is_empty());
    assert!(prepare(
        &change(),
        "configured-owner",
        s,
        &s.views[0],
        &book,
        &registry.version
    )
    .is_err());
    let mut invalid = change();
    invalid.fields["approved"] = json!(true);
    assert!(prepare(&invalid, "configured-owner", s, v, &book, &registry.version).is_err());
}
#[test]
fn updates_resolve_stable_identity_and_duplicate_ids_fail_closed() {
    let (registry, mut book) = fixture();
    let s = &registry.spreadsheets[0];
    let v = &s.views[5];
    for (row, id, op, title) in [
        (6, "record-a", "initial-op-a", "Existing task"),
        (10, "record-b", "initial-op-b", "Other task"),
    ] {
        for (col, val) in [
            (0, json!(id)),
            (1, json!(op)),
            (2, json!(title)),
            (3, json!("todo")),
        ] {
            book["sheets"][5]["data"][0]["rowData"][row]["values"][col] = cell(val);
        }
    }
    let c:Change=serde_json::from_value(json!({"operation_id":"update-op","spreadsheet":"wedding-demo","view":"tasks","action":"update","record_id":"record-a","fields":{"status":"done"}})).unwrap();
    let p = prepare(&c, "configured-owner", s, v, &book, &registry.version).unwrap();
    assert_eq!(p.before["title"], "Existing task");
    assert_eq!(p.after["title"], "Existing task");
    assert_eq!(p.after["status"], "done");
    assert_eq!(rows(s, v, &book).unwrap()[0].row, 7);
    book["sheets"][5]["data"][0]["rowData"][10]["values"][0] = cell(json!("record-a"));
    assert!(rows(s, v, &book).is_err());
}
#[test]
fn payments_require_explicit_positive_currency_date_and_a_registered_cost_group() {
    let (registry, book) = fixture();
    let s = &registry.spreadsheets[0];
    let v = &s.views[6];
    let mut c:Change=serde_json::from_value(json!({"operation_id":"payment-op","spreadsheet":"wedding-demo","view":"payments","action":"append","fields":{"amount":500,"currency":"IDR","payment_date":"2026-10-25","cost_group":"101:D2"}})).unwrap();
    assert!(prepare(&c, "owner", s, v, &book, &registry.version).is_ok());
    c.fields["payment_date"] = json!("October");
    assert!(prepare(&c, "owner", s, v, &book, &registry.version).is_err());
    c.fields["payment_date"] = json!("2026-10-25");
    c.fields["cost_group"] = json!("invented-group");
    assert!(prepare(&c, "owner", s, v, &book, &registry.version).is_err());
}
