//! Inverse planning through the real bus/FSM/SQLite, with a stateful in-process port.
//! These are local coordination tests, not Google protocol or native approval proof.
use secretary_core::{
    application::{
        commands::{Approval, Command, Commit, MutationCommand},
        ports::{PortFuture, WorkspacePort, WriteOutcome},
    },
    domain::{
        compensation::{validate_reversible, Inverse, InverseChange},
        documents::DocChange,
        operation::{Preview, ResourceKind},
        registry::{Document, Spreadsheet, View},
        write_model::Change,
    },
    journal::Journal,
    writes::Writes,
    CommandBus, CoreError, Registry,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone)]
struct Source {
    sheet: Arc<tokio::sync::Mutex<Value>>,
    document: Arc<tokio::sync::Mutex<Value>>,
    sends: Arc<AtomicUsize>,
    lose_next: Arc<AtomicBool>,
}

impl Source {
    fn new() -> Self {
        Self {
            sheet: Arc::new(tokio::sync::Mutex::new(
                json!({"spreadsheetId":"physical-sheet","sheets":[{"properties":{"sheetId":7,"title":"Grid"},"data":[{"rowData":[{"values":[{"userEnteredValue":{"stringValue":""}},{"userEnteredValue":{"numberValue":0}},{"userEnteredValue":{"boolValue":false}},{},{"userEnteredValue":{"stringValue":"=literal"}},{"userEnteredValue":{"stringValue":"untouched"}}]}]}]}]}),
            )),
            document: Arc::new(tokio::sync::Mutex::new(
                json!({"documentId":"physical-doc","title":"Notes","revisionId":"rev-0","tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"prefix old suffix\n"}}]}}]}}},{"tabProperties":{"tabId":"t.1","title":"Other"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"untouched\n"}}]}}]}}}]}),
            )),
            sends: Default::default(),
            lose_next: Default::default(),
        }
    }

    fn outcome(&self) -> WriteOutcome {
        if self.lose_next.swap(false, Ordering::SeqCst) {
            WriteOutcome::Uncertain
        } else {
            WriteOutcome::Accepted
        }
    }

    async fn human_edit(&self, document: bool) {
        if document {
            let mut doc = self.document.lock().await;
            doc["revisionId"] = json!("human-revision");
            doc["tabs"][0]["documentTab"]["body"]["content"][0]["paragraph"]["elements"][0]
                ["textRun"]["content"] = json!("prefix 新🙂 suffix plus human\n");
        } else {
            self.sheet.lock().await["sheets"][0]["data"][0]["rowData"][0]["values"][0] =
                json!({"userEnteredValue":{"stringValue":"human"}});
        }
    }
}

impl WorkspacePort for Source {
    fn read<'a>(&'a self, _s: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async { Ok(self.sheet.lock().await.clone()) })
    }
    fn write_values<'a>(
        &'a self,
        _s: &'a Spreadsheet,
        _v: &'a View,
        _p: &'a Preview,
        _row: Option<usize>,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async { panic!("inverse must never use the structured-row writer") })
    }
    fn write_grid<'a>(
        &'a self,
        _s: &'a Spreadsheet,
        p: &'a Preview,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async move {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let mut book = self.sheet.lock().await;
            for (index, value) in p.after[0].as_array().unwrap().iter().enumerate() {
                let cell = &mut book["sheets"][0]["data"][0]["rowData"][0]["values"][index];
                if value.is_null() {
                    cell.as_object_mut().unwrap().remove("userEnteredValue");
                } else {
                    cell["userEnteredValue"] = value.clone();
                }
            }
            self.outcome()
        })
    }
    fn read_document<'a>(&'a self, _d: &'a Document) -> PortFuture<'a, Result<Value, CoreError>> {
        Box::pin(async { Ok(self.document.lock().await.clone()) })
    }
    fn write_document<'a>(
        &'a self,
        _d: &'a Document,
        p: &'a Preview,
    ) -> PortFuture<'a, WriteOutcome> {
        Box::pin(async move {
            let mut doc = self.document.lock().await;
            if doc["revisionId"] != p.source_revision {
                return WriteOutcome::Rejected;
            }
            let count = self.sends.fetch_add(1, Ordering::SeqCst) + 1;
            let text = &mut doc["tabs"][0]["documentTab"]["body"]["content"][0]["paragraph"]
                ["elements"][0]["textRun"]["content"];
            let find = p.fields["find"].as_str().unwrap();
            let replacement = p.fields["replacement"].as_str().unwrap();
            *text = json!(text.as_str().unwrap().replace(find, replacement));
            doc["revisionId"] = json!(format!("rev-{count}"));
            self.outcome()
        })
    }
}

fn registry() -> Registry {
    serde_json::from_value(json!({"version":"mapping-v1","spreadsheets":[{"alias":"sheet","id":"physical-sheet","readable":true,"views":[{"name":"grid","sheet_id":7,"title":"Grid","kind":"grid","range":"A1:F1","writable_range":"A1:E1","headers":{},"merges":[],"formulas":{}}]}],"documents":[{"alias":"notes","id":"physical-doc","readable":true,"writable":true}]})).unwrap()
}

fn bus(journal: &Journal, source: &Source) -> CommandBus {
    CommandBus::new(registry(), "owner".into(), source.clone())
        .unwrap()
        .with_writes(Writes::new(journal.clone(), true))
}

fn forward(document: bool) -> MutationCommand {
    if document {
        MutationCommand::DocsPreview(DocChange {
            operation_id: "original".into(),
            document: "notes".into(),
            tab_id: "t.0".into(),
            action: "replace_text".into(),
            fields: json!({"find":"old","replacement":"新🙂"}),
        })
    } else {
        MutationCommand::Preview(Change {
            operation_id: "original".into(),
            spreadsheet: "sheet".into(),
            view: "grid".into(),
            action: "update".into(),
            record_id: None,
            fields: json!({"range":"A1:E1","values":[["after",12,true,"filled",null]]}),
        })
    }
}

async fn prepare_inverse(bus: &CommandBus, inverse: &Inverse) {
    let command = match inverse.change() {
        InverseChange::Grid(c) => MutationCommand::Preview(c.clone()),
        InverseChange::Document(c) => MutationCommand::DocsPreview(c.clone()),
    };
    bus.execute(Command::Mutation(command)).await.unwrap();
}

async fn approve(bus: &CommandBus, journal: &Journal, id: &str) {
    let operation = journal.get(id).await.unwrap();
    bus.execute(Command::Approve {
        operation_id: id.into(),
        approval: Approval {
            decision: "approve".into(),
            preview_digest: operation.preview.preview_digest,
        },
    })
    .await
    .unwrap();
}

async fn apply(bus: &CommandBus, document: bool, id: &str) -> Result<Value, CoreError> {
    let commit = Commit {
        operation_id: id.into(),
    };
    bus.execute(Command::Mutation(if document {
        MutationCommand::DocsApply(commit)
    } else {
        MutationCommand::Update(commit)
    }))
    .await
}

async fn applied_original(bus: &CommandBus, journal: &Journal, document: bool) -> Inverse {
    bus.execute(Command::Mutation(forward(document)))
        .await
        .unwrap();
    let captured = journal.get("original").await.unwrap();
    assert_eq!(captured.state.as_str(), "previewed");
    validate_reversible(&captured.preview).unwrap();
    approve(bus, journal, "original").await;
    assert_eq!(
        apply(bus, document, "original").await.unwrap()["state"],
        "applied"
    );
    Inverse::from_applied(&journal.get("original").await.unwrap(), "undo", "owner").unwrap()
}

#[tokio::test]
async fn paired_round_trips_require_new_approval_and_preserve_types_history_and_unrelated_content()
{
    for document in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let source = Source::new();
        let bus = bus(&journal, &source);
        let before_sheet = source.sheet.lock().await.clone();
        let before_doc = source.document.lock().await.clone();
        let inverse = applied_original(&bus, &journal, document).await;
        let original = journal.get("original").await.unwrap();
        let original_json = serde_json::to_value(&original.preview).unwrap();
        assert_eq!(
            apply(&bus, document, "original").await.unwrap()["state"],
            "applied"
        );
        prepare_inverse(&bus, &inverse).await;
        let prepared = journal.get("undo").await.unwrap();
        inverse.validate_preview(&prepared.preview).unwrap();
        assert_eq!(prepared.state.as_str(), "previewed");
        assert!(apply(&bus, document, "undo").await.is_err());
        assert_eq!(source.sends.load(Ordering::SeqCst), 1);
        approve(&bus, &journal, "undo").await;
        for _ in 0..2 {
            assert_eq!(
                apply(&bus, document, "undo").await.unwrap()["state"],
                "applied"
            );
        }
        assert_eq!(source.sends.load(Ordering::SeqCst), 2);
        assert_eq!(*source.sheet.lock().await, before_sheet);
        let mut after_doc = source.document.lock().await.clone();
        after_doc["revisionId"] = before_doc["revisionId"].clone();
        assert_eq!(after_doc, before_doc);
        let retained = journal.get("original").await.unwrap();
        assert_eq!(retained.state.as_str(), "applied");
        assert_eq!(
            serde_json::to_value(retained.preview).unwrap(),
            original_json
        );
    }
}

#[tokio::test]
async fn edits_before_undo_preparation_conflict_with_exact_inverse_evidence() {
    for document in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let source = Source::new();
        let bus = bus(&journal, &source);
        let inverse = applied_original(&bus, &journal, document).await;
        source.human_edit(document).await;
        let command = match inverse.change() {
            InverseChange::Grid(c) => MutationCommand::Preview(c.clone()),
            InverseChange::Document(c) => MutationCommand::DocsPreview(c.clone()),
        };
        bus.execute(Command::Mutation(command)).await.unwrap();
        assert_eq!(
            inverse
                .validate_preview(&journal.get("undo").await.unwrap().preview)
                .unwrap_err()
                .1,
            "undo_conflict"
        );
        assert_eq!(source.sends.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn human_edits_after_fresh_inverse_preview_stop_normal_dispatch() {
    for document in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let source = Source::new();
        let bus = bus(&journal, &source);
        let inverse = applied_original(&bus, &journal, document).await;
        prepare_inverse(&bus, &inverse).await;
        inverse
            .validate_preview(&journal.get("undo").await.unwrap().preview)
            .unwrap();
        approve(&bus, &journal, "undo").await;
        source.human_edit(document).await;
        assert_eq!(
            apply(&bus, document, "undo").await.unwrap()["state"],
            "conflict"
        );
        assert_eq!(source.sends.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn accepted_inverse_with_lost_response_is_observed_after_restart_without_resend() {
    for document in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(directory.path()).await.unwrap();
        let source = Source::new();
        let initial_bus = bus(&journal, &source);
        let inverse = applied_original(&initial_bus, &journal, document).await;
        prepare_inverse(&initial_bus, &inverse).await;
        inverse
            .validate_preview(&journal.get("undo").await.unwrap().preview)
            .unwrap();
        approve(&initial_bus, &journal, "undo").await;
        source.lose_next.store(true, Ordering::SeqCst);
        assert_eq!(
            apply(&initial_bus, document, "undo").await.unwrap()["state"],
            "uncertain"
        );
        assert!(apply(&initial_bus, document, "undo").await.is_err());
        drop(initial_bus);
        drop(journal);
        let journal = Journal::open(directory.path()).await.unwrap();
        let restarted_bus = bus(&journal, &source);
        assert_eq!(
            restarted_bus
                .execute(Command::Reconcile {
                    operation_id: "undo".into()
                })
                .await
                .unwrap()["state"],
            "applied"
        );
        assert_eq!(
            apply(&restarted_bus, document, "undo").await.unwrap()["state"],
            "applied"
        );
        assert_eq!(source.sends.load(Ordering::SeqCst), 2);
        assert_eq!(
            journal.get("original").await.unwrap().preview.resource_kind,
            if document {
                ResourceKind::GoogleDocs
            } else {
                ResourceKind::SheetsGrid
            }
        );
    }
}
