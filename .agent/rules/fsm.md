# Durable operation FSM

An operation is one requested change to a registered Workspace resource, tracked
across approval, execution, verification and reconciliation. Reads are commands,
not mutation operations. This FSM already exists in
`services/secretary-core/src/domain/operation.rs`; preserve its semantics.

## Ownership and operation identity

- Domain decides `State + Event -> Result<State>`. Application coordinates guards,
  effects and outcome evidence; the journal persists decisions. Provider adapters
  classify responses. The FSM alone does not prove what happened in Google.
- Bind one operation ID to one immutable request/preview, owner, resource kind,
  registered target, mapping version, source revision, before/after and expiry.
  Same ID/same request returns stored intent/state; same ID/different request
  conflicts. Never reuse an ID to replace an approved intent.
- Approval applies to the exact preview digest. Content/target changes require a
  fresh preview and approval. Caller input or memory cannot manufacture authority.

## Execution order

1. Validate identity, registered permissions and mutation gates. Serialize mutation
   coordination through the shared writer mutex.
2. Read the bounded source, construct the immutable preview and persist it with
   `previewed` audit. No provider write occurs during preview.
3. Authenticate the owner decision through trusted approval routing; validate
   digest and expiry, then persist approved/rejected/expired.
4. On apply, validate stored actor/resource/action. An applied replay returns the
   stored result immediately. Commit `approved -> applying` before provider write.
5. Recheck registry/mapping, resource identity, revision and approved target.
   Conflict stops dispatch. Failed preflight does not authorize a different write.
6. Send the intended provider mutation at most once through this operation's
   dispatch path. Do not retry it automatically on ambiguous transport failure.
7. Classify the outcome; accepted responses still require source observation.
   Persist the outcome and return its actual state.

Google and SQLite do not share a transaction. This is not an external exactly-once
guarantee. Persisting applying before send deliberately permits conservative
reconciliation even when a crash occurred before the send.

## Complete legal transition contract

Creation persists `previewed`; it is not an OperationEvent. In the table,
`Finish(X)` means an outcome classified by a handler/observer, not by the caller.

| Current state | Event | Next state |
| --- | --- | --- |
| previewed | Approve | approved |
| approved | Approve | approved |
| previewed | Reject | rejected |
| rejected | Reject | rejected |
| previewed or approved | Expire | expired |
| approved | BeginApply | applying |
| applying | Recover | reconciliation |
| applying | Finish(Applied) | applied |
| applying | Finish(KnownFailure) | known_failure |
| applying | Finish(Uncertain) | uncertain |
| applying | Finish(Conflict) | conflict |
| applying | Finish(Reconciliation) | reconciliation |
| uncertain or reconciliation | Finish(Applied) | applied |
| uncertain or reconciliation | Finish(Conflict) | conflict |
| uncertain or reconciliation | Finish(Reconciliation) | reconciliation |

Every other state/event combination is rejected. Approve/Reject also have digest
and expiry guards: expired previewed/approved operations take Expire instead.
BeginApply is emitted only after the expiry decision; expired approval cannot apply.

`applied`, `known_failure`, `conflict`, `expired` and `rejected` are terminal for
execution. Repeated Reject on rejected is the explicit idempotent exception.
An applied API replay is an early return, not an extra FSM transition.

## Outcome and recovery rules

| State | Required interpretation | Next action |
| --- | --- | --- |
| applied | Observation satisfies the operation's exact evidence contract | Return success; do not send again |
| known_failure | Write was not sent or was definitively rejected | Report failure; do not reopen the operation |
| uncertain | Provider write may have happened | Observe/reconcile; no resend |
| conflict | Source/target or observed result contradicts approved intent | Report conflict; no rewrite of this intent |
| reconciliation | Evidence is insufficient or execution was interrupted | Observe again; no resend |

- A timeout after dispatch is not proof of rejection. Missing evidence cannot
  change uncertain/reconciliation to known_failure.
- Startup moves persisted applying operations to reconciliation without sending.
  Ordinary state changes and their audit entries are atomic SQLite transactions.
  Current startup recovery uses a bulk state update and separate global audit;
  do not claim per-operation atomic recovery audit that is not implemented.
- Reconciliation calls only resource observers, never provider mutation methods.
  It may resolve applied/conflict or remain reconciliation. Applying is accepted
  by the reconciliation entry point for interrupted execution.
- Never create a replacement operation to bypass an unresolved earlier write.
  A genuinely new attempt requires resolved prior outcome and fresh preview/approval.
- Test all allowed and rejected transitions, expiry/digest guards, repeated
  decisions, changed-request replay, terminal immutability, restart and accepted
  write/lost response. Assert provider-send count, not just returned status.

For each new mutation, specify its event, guards, persisted intent, provider effect,
outcome evidence and recovery path before implementation. The table documents
existing semantics; exhaustive matrix coverage remains required adoption work.
