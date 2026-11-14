//! One request owns preflight/confirmation/effect; no detached executor or daemon.
use super::http::{authorized, parse, AppState};
use crate::{
    application::{
        commands::Command,
        ports::PortFuture,
        task_authority::{DispatchConfirmation, TaskMember},
    },
    domain::error::{CoreError, ErrorKind},
};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
const CONFIRMATION_WAIT: Duration = Duration::from_secs(8);
const MAX_CHECKPOINTS: usize = 32;
fn denied() -> CoreError {
    CoreError(ErrorKind::Conflict, "native_confirmation_unavailable")
}

#[derive(Clone, Default)]
pub(super) struct Checkpoints(Arc<Mutex<HashMap<String, Pending>>>);
struct Pending {
    challenge: Challenge,
    reply: oneshot::Sender<()>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    member: TaskMember,
    preview_digest: String,
    nonce: String,
}
struct PendingGuard {
    checkpoints: Checkpoints,
    nonce: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        // Drop must never panic; poisoned state already denies every confirmation.
        if let Ok(mut pending) = self.checkpoints.0.lock() {
            pending.remove(&self.nonce);
        }
    }
}
struct Confirmation {
    owner: Arc<str>,
    checkpoints: Checkpoints,
    ready: mpsc::Sender<Value>,
}
impl DispatchConfirmation for Confirmation {
    fn confirm<'a>(
        &'a self,
        member: &'a TaskMember,
        preview_digest: &'a str,
    ) -> PortFuture<'a, Result<(), CoreError>> {
        Box::pin(async move {
            let challenge = Challenge {
                member: member.clone(),
                preview_digest: preview_digest.into(),
                nonce: uuid::Uuid::new_v4().to_string(),
            };
            let (reply, receiver) = oneshot::channel();
            {
                let mut pending = self.checkpoints.0.lock().map_err(|_| denied())?;
                if pending.len() >= MAX_CHECKPOINTS {
                    return Err(denied());
                }
                pending.insert(
                    challenge.nonce.clone(),
                    Pending {
                        challenge: challenge.clone(),
                        reply,
                    },
                );
            }
            let _guard = PendingGuard {
                checkpoints: self.checkpoints.clone(),
                nonce: challenge.nonce.clone(),
            };
            tokio::time::timeout(CONFIRMATION_WAIT, async {
                self.ready
                    .send(json!({"type":"ready","owner":self.owner.as_ref(),"challenge":challenge}))
                    .await
                    .map_err(|_| denied())?;
                receiver.await.map_err(|_| denied())
            })
            .await
            .map_err(|_| CoreError(ErrorKind::Unavailable, "native_confirmation_timeout"))?
        })
    }
}
fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), CoreError> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !authorized(token, &state.approval_token) {
        return Err(CoreError(ErrorKind::Unauthorized, "unauthorized"));
    }
    state.bus.task_writes()?;
    Ok(())
}
pub(super) async fn control(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    authenticate(&state, &headers)?;
    let command = parse(
        serde_json::from_slice(&body.map_err(|_| CoreError::invalid())?)
            .map_err(|_| CoreError::invalid())?,
    )?;
    let result = state.bus.execute(Command::TaskControl(command)).await?;
    Ok(Json(
        json!({"ok":true,"owner":state.bus.owner.as_ref(),"data":result}),
    ))
}
pub(super) async fn confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<Value>, CoreError> {
    authenticate(&state, &headers)?;
    let challenge: Challenge = serde_json::from_slice(&body.map_err(|_| CoreError::invalid())?)
        .map_err(|_| CoreError::invalid())?;
    let mut pending = state.checkpoints.0.lock().map_err(|_| denied())?;
    if !pending
        .get(&challenge.nonce)
        .is_some_and(|entry| entry.challenge == challenge)
    {
        return Err(denied());
    }
    let entry = pending.remove(&challenge.nonce).ok_or_else(denied)?;
    entry.reply.send(()).map_err(|_| denied())?;
    Ok(Json(
        json!({"ok":true,"owner":state.bus.owner.as_ref(),"data":{"confirmed":true}}),
    ))
}
pub(super) async fn execute(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Response, CoreError> {
    authenticate(&state, &headers)?;
    let member: TaskMember = serde_json::from_slice(&body.map_err(|_| CoreError::invalid())?)
        .map_err(|_| CoreError::invalid())?;
    let (ready, receiver) = mpsc::channel(1);
    let owner = state.bus.owner.clone();
    let confirmation = Arc::new(Confirmation {
        owner: owner.clone(),
        checkpoints: state.checkpoints.clone(),
        ready,
    });
    let computation = Box::pin(async move {
        state
            .bus
            .execute(Command::TaskExecute {
                member,
                confirmation,
            })
            .await
    });
    // Poll the awaited bus command as part of the HTTP body. Dropping the body
    // drops work and its checkpoint; persisted applying is observation-only.
    let stream = futures_util::stream::unfold(
        (computation, receiver, false, owner),
        |(mut computation, mut receiver, finished, owner)| async move {
            if finished {
                return None;
            }
            let (event, finished) = tokio::select! {
                Some(event)=receiver.recv()=>(event,false),
                result=&mut computation=> (match result {
                    Ok(data)=>json!({"type":"result","owner":owner.as_ref(),"ok":true,"data":data}),
                    Err(error)=>json!({"type":"result","owner":owner.as_ref(),"ok":false,"error":{"code":error.1}}),
                },true),
            };
            let bytes = Bytes::from(event.to_string() + "\n");
            Some((
                Ok::<_, std::convert::Infallible>(bytes),
                (computation, receiver, finished, owner),
            ))
        },
    );
    Ok((
        [
            (
                "content-type",
                "application/vnd.ciel.task-dispatch+ndjson;version=1",
            ),
            ("cache-control", "no-store"),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
