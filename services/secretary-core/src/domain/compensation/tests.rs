use super::*;

fn sealed(mut preview: Preview) -> Operation {
    preview.preview_digest.clear();
    preview.preview_digest = digest(&serde_json::to_value(&preview).unwrap());
    Operation {
        preview,
        state: OperationState::Applied,
    }
}

fn grid_original() -> Operation {
    sealed(Preview {
        resource_kind: ResourceKind::SheetsGrid,
        operation_id: "original".into(),
        actor: "owner".into(),
        spreadsheet: "sheet".into(),
        view: "grid".into(),
        action: "update".into(),
        record_id: "physical:7".into(),
        fields: json!({"range":"A1:E1","values":[["changed",1,true,null,"new"]]}),
        before: json!([[{"stringValue":""},{"numberValue":0},{"boolValue":false},null,{"stringValue":"=literal"}]]),
        after: json!([[{"stringValue":"changed"},{"numberValue":1},{"boolValue":true},null,{"stringValue":"new"}]]),
        mapping_version: "v1".into(),
        source_revision: "source".into(),
        expires_at: 1,
        preview_digest: String::new(),
    })
}

fn doc_original() -> Operation {
    let mut preview = grid_original().preview;
    preview.resource_kind = ResourceKind::GoogleDocs;
    preview.view = "t.0".into();
    preview.action = "replace_text".into();
    preview.fields = json!({"find":"old","replacement":"新🙂"});
    preview.before = json!({"text":"prefix old suffix\n"});
    preview.after = json!({"text":"prefix 新🙂 suffix\n"});
    sealed(preview)
}

#[test]
fn exact_types_and_expired_original_are_retained_without_inheriting_approval() {
    let original = grid_original();
    let retained = serde_json::to_value(&original.preview).unwrap();
    let inverse = Inverse::from_applied(&original, "undo", "owner").unwrap();
    let InverseChange::Grid(change) = inverse.change() else {
        panic!("expected grid")
    };
    assert_eq!(
        change.fields,
        json!({"range":"A1:E1","values":[["",0,false,null,"=literal"]]})
    );
    assert_eq!(inverse.original_id(), "original");
    assert_eq!(inverse.original_digest(), original.preview.preview_digest);
    assert_eq!(serde_json::to_value(&original.preview).unwrap(), retained);
}

#[test]
fn original_authority_and_resolved_outcome_are_required() {
    let original = grid_original();
    for state in [
        OperationState::Previewed,
        OperationState::Approved,
        OperationState::Applying,
        OperationState::Uncertain,
        OperationState::Reconciliation,
        OperationState::Conflict,
        OperationState::KnownFailure,
        OperationState::Rejected,
        OperationState::Expired,
    ] {
        let mut changed = original.clone();
        changed.state = state;
        assert_eq!(
            Inverse::from_applied(&changed, "undo", "owner")
                .err()
                .unwrap()
                .1,
            "undo_unavailable"
        );
    }
    for (id, owner) in [
        ("original", "owner"),
        ("invalid/id", "owner"),
        ("undo", "other"),
        ("undo", ""),
    ] {
        assert!(Inverse::from_applied(&original, id, owner).is_err());
    }
    let mut tampered = original.clone();
    tampered.preview.before[0][0] = json!({"stringValue":"invented"});
    assert!(Inverse::from_applied(&tampered, "undo", "owner").is_err());
}

#[test]
fn grid_refuses_unsupported_or_unbounded_inverse_evidence() {
    for bad in [
        json!({"formulaValue":"=1+1"}),
        json!({}),
        json!({"stringValue":"a","boolValue":false}),
        json!({"numberValue":9007199254740992u64}),
        json!({"stringValue":"x".repeat(2049)}),
    ] {
        let mut p = grid_original().preview;
        p.before[0][0] = bad;
        assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    }
    for range in ["A1:F1", "A1:A101", "invalid", "E1:A1"] {
        let mut p = grid_original().preview;
        p.fields["range"] = json!(range);
        assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    }
    let mut p = grid_original().preview;
    p.fields["values"][0][0] = json!("different");
    assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    let mut p = grid_original().preview;
    p.before = json!(vec![vec![json!({"stringValue":"x".repeat(2000)}); 5]; 2]);
    p.after = json!(vec![vec![json!({"stringValue":"after"}); 5]; 2]);
    p.fields = json!({"range":"A1:E2","values":vec![vec!["after";5];2]});
    assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
}

#[test]
fn docs_refuses_ambiguous_deletion_multiline_and_unsupported_forward_classes() {
    let original = doc_original();
    let inverse = Inverse::from_applied(&original, "undo", "owner").unwrap();
    let InverseChange::Document(change) = inverse.change() else {
        panic!("expected document")
    };
    assert_eq!(change.fields, json!({"find":"新🙂","replacement":"old"}));
    for replacement in ["", "old", "prefix", "x\ny", "x\ty", "\u{e000}"] {
        let mut p = original.preview.clone();
        p.fields["replacement"] = json!(replacement);
        p.after = json!({"text":format!("prefix {replacement} suffix\n")});
        assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    }
    let mut p = original.preview.clone();
    p.before = json!({"text":"old old\n"});
    p.after = json!({"text":"新🙂 old\n"});
    assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    for kind in [ResourceKind::Sheets, ResourceKind::SheetsTab] {
        let mut p = original.preview.clone();
        p.resource_kind = kind;
        assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
    }
    let mut p = original.preview;
    p.action = "append_text".into();
    assert!(Inverse::from_applied(&sealed(p), "undo", "owner").is_err());
}

#[test]
fn inverse_preview_requires_exact_reversal_and_original_target() {
    for original in [grid_original(), doc_original()] {
        let inverse = Inverse::from_applied(&original, "undo", "owner").unwrap();
        let mut fresh = original.preview.clone();
        fresh.operation_id = "undo".into();
        fresh.fields = match inverse.change() {
            InverseChange::Grid(c) => c.fields.clone(),
            InverseChange::Document(c) => c.fields.clone(),
        };
        fresh.before = original.preview.after.clone();
        fresh.after = original.preview.before.clone();
        fresh.source_revision = "current-source".into();
        fresh.expires_at = 99999;
        inverse.validate_preview(&fresh).unwrap();
        for field in [
            "actor",
            "resource_kind",
            "operation_id",
            "spreadsheet",
            "view",
            "record_id",
            "mapping_version",
            "source_revision",
            "before",
            "after",
            "fields",
            "action",
        ] {
            let mut changed = serde_json::to_value(&fresh).unwrap();
            changed[field] = if field == "resource_kind" {
                json!("sheets_tab")
            } else {
                json!("")
            };
            let changed = serde_json::from_value(changed).unwrap();
            assert_eq!(
                inverse.validate_preview(&changed).unwrap_err().1,
                "undo_conflict",
                "{field}"
            );
        }
    }
}
