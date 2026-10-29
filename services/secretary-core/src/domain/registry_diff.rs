//! Pure comparison of validated registry drafts; callers protect private report data.
use super::registry::Registry;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChangeKind {
    Added,
    Removed,
    Modified,
}
#[derive(Serialize)]
struct Delta<K> {
    key: K,
    change: ChangeKind,
    before: Option<Value>,
    after: Option<Value>,
}
#[derive(Default, Serialize)]
struct Counts {
    added: usize,
    removed: usize,
    modified: usize,
}
fn changes<K: Ord + Clone>(
    mut before: BTreeMap<K, Value>,
    mut after: BTreeMap<K, Value>,
) -> Vec<Delta<K>> {
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).cloned().collect();
    keys.into_iter()
        .filter_map(|key| {
            let before = before.remove(&key);
            let after = after.remove(&key);
            if before == after {
                return None;
            }
            let change = match (&before, &after) {
                (None, _) => ChangeKind::Added,
                (_, None) => ChangeKind::Removed,
                _ => ChangeKind::Modified,
            };
            Some(Delta {
                key,
                change,
                before,
                after,
            })
        })
        .collect()
}
fn counts<K>(changes: &[Delta<K>]) -> Counts {
    let mut result = Counts::default();
    for change in changes {
        match change.change {
            ChangeKind::Added => result.added += 1,
            ChangeKind::Removed => result.removed += 1,
            ChangeKind::Modified => result.modified += 1,
        }
    }
    result
}
fn spreadsheets(registry: &Registry) -> BTreeMap<String, Value> {
    registry
        .spreadsheets
        .iter()
        .map(|s| {
            let mut normalized = s.clone();
            normalized.views.sort_by(|a, b| a.name.cmp(&b.name));
            (s.alias.clone(), json!(normalized))
        })
        .collect()
}
fn documents(registry: &Registry) -> BTreeMap<String, Value> {
    registry
        .documents
        .iter()
        .map(|d| (d.alias.clone(), json!(d)))
        .collect()
}
fn views(registry: &Registry) -> BTreeMap<(String, String), Value> {
    registry
        .spreadsheets
        .iter()
        .flat_map(|s| {
            s.views
                .iter()
                .map(|v| ((s.alias.clone(), v.name.clone()), json!(v)))
        })
        .collect()
}
pub fn compare(before: &Registry, after: &Registry) -> Value {
    let spreadsheets = changes(spreadsheets(before), spreadsheets(after));
    let documents = changes(documents(before), documents(after));
    let views = changes(views(before), views(after));
    let mappings_changed = !spreadsheets.is_empty() || !documents.is_empty();
    let version_changed = before.version != after.version;
    let summary = json!({
        "spreadsheets":counts(&spreadsheets),"documents":counts(&documents),"views":counts(&views),
        "version_changed":version_changed,"mappings_changed":mappings_changed,
        "version_change_required":mappings_changed && !version_changed
    });
    json!({"format_version":1,"before_version":before.version,"after_version":after.version,
        "summary":summary,"changes":{"spreadsheets":spreadsheets,"documents":documents}})
}
