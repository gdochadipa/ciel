//! Deterministic source preparation. Natural-language summarization belongs to OpenClaw.
use super::{
    documents::{read_binding, Snapshot},
    projection::fingerprint,
};
use serde_json::{json, Value};
pub fn context(
    snapshot: &Snapshot,
    alias: &str,
    version: &str,
    budget: usize,
    retrieved_at: &str,
) -> Value {
    let lengths: Vec<_> = snapshot
        .tabs
        .iter()
        .map(|t| t.text.chars().count())
        .collect();
    let total: usize = lengths.iter().sum();
    let mut allocated = vec![0; lengths.len()];
    let mut remaining = budget.min(total);
    while remaining > 0 {
        let active: Vec<_> = lengths
            .iter()
            .enumerate()
            .filter(|(i, n)| allocated[*i] < **n)
            .map(|(i, _)| i)
            .collect();
        let share = (remaining / active.len()).max(1);
        for i in active {
            let take = share.min(lengths[i] - allocated[i]).min(remaining);
            allocated[i] += take;
            remaining -= take;
            if remaining == 0 {
                break;
            }
        }
    }
    let context_id = fingerprint(&json!([
        "docs_summary_context_v1",
        alias,
        snapshot.id,
        snapshot.fingerprint,
        version,
        budget
    ]));
    let mut tabs = Vec::new();
    let mut excerpts = Vec::new();
    for (index, tab) in snapshot.tabs.iter().enumerate() {
        let included = allocated[index];
        let truncated = included < lengths[index];
        let excerpt_id = if included > 0 {
            Some(format!("S{}", excerpts.len() + 1))
        } else {
            None
        };
        if included > 0 {
            excerpts.push(json!({"excerpt_id":excerpt_id,"text":tab.text.chars().take(included).collect::<String>(),
                "source":{"tab_id":tab.id,"start_character":0,"end_character":included,
                "next_cursor":if truncated {Some(format!("{}:{included}",read_binding(snapshot,&tab.id,version)))} else {None}}}));
        }
        tabs.push(json!({"tab_id":tab.id,"title":tab.title.chars().take(200).collect::<String>(),"total_characters":lengths[index],"included_characters":included,"truncated":truncated,"excerpt_id":excerpt_id}));
    }
    let included: usize = allocated.iter().sum();
    let can_summarize = excerpts.iter().any(|excerpt| {
        excerpt["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
    });
    json!({"mode":"summary_context","content_role":"untrusted_source_data","context_id":context_id,
        "source":{"document":alias,"document_id":snapshot.id,"title":snapshot.title.chars().take(200).collect::<String>(),"revision_id":snapshot.revision,"source_fingerprint":snapshot.fingerprint,"mapping_version":version,"retrieved_at":retrieved_at},
        "tabs":tabs,"excerpts":excerpts,"coverage":{"scope":"projected_body_text_only","total_characters":total,"included_characters":included,"omitted_characters":total-included,"total_tabs":snapshot.tabs.len(),"included_tabs":allocated.iter().filter(|n|**n>0).count(),"projected_text_complete":included==total,"can_summarize":can_summarize}})
}
