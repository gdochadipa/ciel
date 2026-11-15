//! Pure capture/request/inverse contracts; no provider or owner authority is implied.
use secretary_core::domain::{
    compensation::{validate_reversible, Inverse, InverseChange},
    documents::{self, DocChange},
    operation::{Operation, OperationState, Preview},
    registry::Document,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn prepare(text: &str, edits: Value) -> Result<Preview, secretary_core::CoreError> {
    let doc = Document {
        alias: "notes".into(),
        id: "doc".into(),
        readable: true,
        writable: true,
    };
    let snapshot = documents::snapshot(
        &doc,
        &json!({"documentId":"doc","revisionId":"rev1","tabs":[{"tabProperties":{"tabId":"t.0"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":text}}]}}]}}}]}),
    )?;
    documents::prepare(
        &DocChange {
            operation_id: "batch".into(),
            document: "notes".into(),
            tab_id: "t.0".into(),
            action: "replace_text_batch".into(),
            fields: edits,
        },
        &doc,
        &snapshot,
        "owner",
        "v1",
        1,
    )
}
fn edits() -> Value {
    json!({"replacements":[{"find":"old","replacement":"新🙂"},{"find":"note","replacement":"record"}]})
}

#[test]
fn independent_unicode_edits_seal_one_aggregate_and_reverse_in_request_order() {
    let mut p = prepare("prefix old note suffix\n", edits()).unwrap();
    assert_eq!(p.before, json!({"text":"prefix old note suffix\n"}));
    assert_eq!(p.after, json!({"text":"prefix 新🙂 record suffix\n"}));
    validate_reversible(&p).unwrap();
    let request = documents::requests(&p).unwrap();
    assert_eq!(
        request["writeControl"],
        json!({"requiredRevisionId":"rev1"})
    );
    assert_eq!(request["requests"].as_array().unwrap().len(), 2);
    for (index, find) in ["old", "note"].iter().enumerate() {
        assert_eq!(
            request["requests"][index]["replaceAllText"]["containsText"],
            json!({"text":find,"matchCase":true,"searchByRegex":false})
        );
        assert_eq!(
            request["requests"][index]["replaceAllText"]["tabsCriteria"],
            json!({"tabIds":["t.0"]})
        );
    }
    p.preview_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::to_value(&p).unwrap()).unwrap())
    );
    let original = Operation {
        preview: p.clone(),
        state: OperationState::Applied,
    };
    let inverse = Inverse::from_applied(&original, "undo", "owner").unwrap();
    let InverseChange::Document(change) = inverse.change() else {
        panic!("document inverse")
    };
    assert_eq!(
        change.fields,
        json!({"replacements":[{"find":"record","replacement":"note"},{"find":"新🙂","replacement":"old"}]})
    );
    let mut fresh = prepare(p.after["text"].as_str().unwrap(), change.fields.clone()).unwrap();
    fresh.operation_id = "undo".into();
    inverse.validate_preview(&fresh).unwrap();
    assert_eq!(fresh.after, p.before);
    let mut forged = p;
    forged.after["text"] = json!("forged\n");
    assert_eq!(
        validate_reversible(&forged).unwrap_err().1,
        "undo_unavailable"
    );
}

#[test]
fn overlapping_cascading_boundary_and_ambiguous_inverse_edits_are_refused() {
    for (text, pairs) in [
        ("old note\n", vec![("old", "new"), ("ld", "x")]),
        ("old note\n", vec![("old", "new"), ("new", "next")]),
        ("old note\n", vec![("old", "note"), ("note", "record")]),
        ("a bcd\n", vec![("a", "bc"), ("bcd", "z")]),
        ("old note new\n", vec![("old", "new"), ("note", "record")]),
        ("old old note\n", vec![("old", "new"), ("note", "record")]),
        ("old note\n", vec![("old", "new"), ("old", "next")]),
    ] {
        let fields = json!({"replacements": pairs.iter().map(|(find,replacement)| json!({"find":find,"replacement":replacement})).collect::<Vec<_>>()});
        assert_eq!(
            prepare(text, fields).err().unwrap().1,
            "document_compound_conflict",
            "{text} {pairs:?}"
        );
    }
}

#[test]
fn strict_fields_and_finite_literal_limits_apply_before_admission() {
    for fields in [
        json!({}),
        json!({"replacements":[]}),
        json!({"replacements":[{"find":"old","replacement":"new"}]}),
        json!({"replacements":vec![json!({"find":"old","replacement":"new"});17]}),
        json!({"replacements":[],"actor":"owner"}),
    ] {
        assert_eq!(
            prepare("old note\n", fields).err().unwrap().1,
            "invalid_input"
        );
    }
    for value in [
        "".into(),
        "old".into(),
        "x\ny".into(),
        "x\ty".into(),
        "\u{e000}".into(),
        "🙂".repeat(513),
    ] {
        let mut fields = edits();
        fields["replacements"][0]["replacement"] = json!(value);
        assert_eq!(
            prepare("old note\n", fields).err().unwrap().1,
            "invalid_input"
        );
    }
    let mut extra = edits();
    extra["replacements"][0]["authority"] = json!(true);
    assert_eq!(
        prepare("old note\n", extra).err().unwrap().1,
        "invalid_input"
    );
    let pairs: Vec<_> = (0..16)
        .map(|i| json!({"find":format!("[{i}]"),"replacement":format!("<{i}>")}))
        .collect();
    let text = format!(
        "{}\n",
        (0..16)
            .map(|i| format!("[{i}]"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    prepare(&text, json!({"replacements":pairs})).unwrap();
    let mut fields = edits();
    fields["replacements"][0]["replacement"] = json!("🙂".repeat(512));
    prepare("old note\n", fields).unwrap();
}
