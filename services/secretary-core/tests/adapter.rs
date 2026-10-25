mod common;
use common::{cell, fixture};
use secretary_core::projection;
use serde_json::json;
#[test]
fn merged_costs_are_counted_once_and_dash_costs_remain_unknown() {
    let (registry, book) = fixture();
    let s = &registry.spreadsheets[0];
    projection::validate(s, &book).unwrap();
    let result = projection::report(s, &book, &registry.version).unwrap();
    assert_eq!(result["known_cost_total"], 2200);
    assert_eq!(result["known_cost_groups"], 4);
    assert_eq!(result["unknown_cost_groups"], 38);
    assert_eq!(result["shared_child_positions"], 4);
    assert_eq!(result["records"].as_array().unwrap().len(), 46);
    assert!(result["outstanding_balance"].is_null());
    assert!(result["currency"].is_null());
}
#[test]
fn household_projects_fourteen_positions_and_notes_do_not_invent_a_year() {
    let (registry, book) = fixture();
    let s = &registry.spreadsheets[0];
    let household = projection::project(s, &s.views[4], &book, &registry.version).unwrap();
    assert_eq!(household.len(), 14);
    assert!(household[0]["fields"]["settled"].is_null());
    let notes = projection::project(s, &s.views[2], &book, &registry.version).unwrap();
    assert_eq!(
        notes[0]["fields"]["task_candidates"],
        json!(["Call sample vendor", "Compare sample quote"])
    );
    assert!(notes[0]["fields"]["year"].is_null());
    assert!(notes[0]["fields"]["due_at"].is_null());
    assert_eq!(notes[0]["source"]["sheet_id"], 103);
    assert_eq!(notes[0]["source"]["range"], "'Sheet3'!B5");
}
#[test]
fn changed_headers_formulas_or_merges_suspend_reads() {
    let (registry, mut book) = fixture();
    let s = &registry.spreadsheets[0];
    book["sheets"][0]["data"][0]["rowData"][0]["values"][1] = cell(json!("MOVED HEADER"));
    assert!(projection::validate(s, &book).is_err());
    let (_, mut book) = fixture();
    book["sheets"][0]["merges"] = json!([]);
    assert!(projection::validate(s, &book).is_err());
    let (_, mut book) = fixture();
    book["sheets"][0]["data"][0]["rowData"][8]["values"][3]["userEnteredValue"]["formulaValue"] =
        json!("=SUM(D2:D7)");
    assert!(projection::validate(s, &book).is_err());
}

#[test]
fn source_sum_caches_are_checked_without_becoming_a_paid_balance() {
    let (registry, book) = fixture();
    let result = projection::report(&registry.spreadsheets[0], &book, &registry.version).unwrap();
    let summaries = result["formula_summaries"].as_array().unwrap();
    let initial = summaries.iter().find(|s| s["cell"] == "D9").unwrap();
    assert_eq!(initial["recomputed_numeric_inputs"], 100);
    assert_eq!(initial["cache_matches"], false);
}

#[test]
fn unlabeled_amounts_and_explicit_thousands_text_stay_reversible() {
    let (registry, mut book) = fixture();
    book["sheets"][0]["data"][0]["rowData"][3]["values"][4] = cell(json!(321));
    book["sheets"][4]["data"][0]["rowData"][4]["values"][3] = cell(json!(-123));
    book["sheets"][3]["data"][0]["rowData"][8]["values"][1] =
        cell(json!("Location 700k\nTransport 500K"));
    let s = &registry.spreadsheets[0];
    let costs = projection::project(s, &s.views[0], &book, &registry.version).unwrap();
    assert_eq!(costs[2]["fields"]["unmapped_amount"], 321);
    let home = projection::project(s, &s.views[4], &book, &registry.version).unwrap();
    assert_eq!(home[2]["fields"]["unmapped_annotation"], -123);
    let quotes = projection::project(s, &s.views[3], &book, &registry.version).unwrap();
    assert_eq!(
        quotes[0]["fields"]["attributes"]["row_9"]["parsed_explicit_thousands"],
        json!([{"raw_token":"700k","amount":700000},{"raw_token":"500K","amount":500000}])
    );
}
