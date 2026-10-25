use secretary_core::{projection, Registry};
use serde_json::{json, Value};
pub fn cell(value: Value) -> Value {
    if value.is_number() {
        json!({"effectiveValue":{"numberValue":value}})
    } else {
        json!({"effectiveValue":{"stringValue":value}})
    }
}
pub fn fixture() -> (Registry, Value) {
    let registry: Registry =
        serde_json::from_str(include_str!("../../../../config/spreadsheets.example.json")).unwrap();
    let s = &registry.spreadsheets[0];
    let mut sheets = Vec::new();
    for v in &s.views {
        let mut rows = vec![json!({"values":vec![json!({});7]}); 63];
        let mut put = |addr: &str, value: Value| {
            let (r, c) = projection::coordinate(addr).unwrap();
            rows[r]["values"][c] = value;
        };
        for (a, t) in &v.headers {
            put(a, cell(json!(t)))
        }
        for (a, f) in &v.formulas {
            put(
                a,
                json!({"userEnteredValue":{"formulaValue":f},"effectiveValue":{"numberValue":0}}),
            )
        }
        match v.kind {
            secretary_core::registry::Kind::WeddingCosts => {
                for (lo, hi) in [(2, 8), (12, 17), (20, 26), (29, 44), (50, 59)] {
                    for r in lo..=hi {
                        put(
                            &format!("{}{}", if r == 17 { "C" } else { "B" }, r),
                            cell(json!(format!("Demo item {r}"))),
                        );
                        put(&format!("D{r}"), cell(json!("-")));
                    }
                }
                for (a, n) in [("D2", 100), ("D12", 600), ("D50", 800), ("D53", 700)] {
                    put(a, cell(json!(n)))
                }
                for a in ["D13", "D51", "D52", "D54"] {
                    put(a, json!({}))
                }
            }
            secretary_core::registry::Kind::HouseholdItems => {
                for r in 3..=16 {
                    put(&format!("A{r}"), cell(json!(format!("Demo household {r}"))))
                }
            }
            secretary_core::registry::Kind::PlanningNotes => {
                put("A5", cell(json!("October")));
                put(
                    "B5",
                    cell(json!("- Call sample vendor\n-\n• Compare sample quote")),
                );
            }
            secretary_core::registry::Kind::PreweddingOptions => {
                put("B3", cell(json!("Option A")));
                put("C3", cell(json!("Option B")));
            }
            _ => {}
        }
        let merges: Vec<_> = v
            .merges
            .iter()
            .map(|m| {
                let (r, c, rr, cc) = projection::parse_range(m).unwrap();
                json!({"startRowIndex":r,"startColumnIndex":c,"endRowIndex":rr,"endColumnIndex":cc})
            })
            .collect();
        sheets.push(json!({"properties":{"sheetId":v.sheet_id,"title":v.title},"data":[{"rowData":rows}],"merges":merges}));
    }
    let book = json!({"spreadsheetId":s.id,"sheets":sheets});
    (registry, book)
}
