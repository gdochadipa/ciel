use crate::domain::{
    error::CoreError,
    operation::{Operation, Outcome, Preview},
    registry::{Spreadsheet, View},
};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type PortFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WriteOutcome {
    Accepted,
    Rejected,
    Uncertain,
}

/// Source access: implementations own protocol, authentication and bounded IO.
pub trait SpreadsheetPort: Send + Sync {
    fn read<'a>(&'a self, sheet: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>>;
    fn write_values<'a>(
        &'a self,
        sheet: &'a Spreadsheet,
        view: &'a View,
        preview: &'a Preview,
        row: Option<usize>,
    ) -> PortFuture<'a, WriteOutcome>;
}

/// Durable intent and atomic lifecycle persistence; no mutation is sent without it.
pub trait OperationStore: Send + Sync {
    fn audit<'a>(
        &'a self,
        id: Option<&'a str>,
        event: &'a str,
    ) -> PortFuture<'a, Result<(), CoreError>>;
    fn find<'a>(&'a self, id: &'a str) -> PortFuture<'a, Result<Option<Operation>, CoreError>>;
    fn get<'a>(&'a self, id: &'a str) -> PortFuture<'a, Result<Operation, CoreError>>;
    fn create(&self, preview: Preview) -> PortFuture<'_, Result<Preview, CoreError>>;
    fn approve<'a>(
        &'a self,
        id: &'a str,
        digest: &'a str,
        decision: &'a str,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    fn begin_apply<'a>(&'a self, id: &'a str) -> PortFuture<'a, Result<Operation, CoreError>>;
    fn finish<'a>(
        &'a self,
        id: &'a str,
        outcome: Outcome,
    ) -> PortFuture<'a, Result<Operation, CoreError>>;
    fn history<'a>(
        &'a self,
        id: Option<&'a str>,
        limit: usize,
        cursor: usize,
    ) -> PortFuture<'a, Result<Value, CoreError>>;
}
