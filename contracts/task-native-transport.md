# Native task transport v1

Status: trusted transport plus opt-in model-task tools and normal startup
composition. All task/qualification defaults remain false. Supported Linux/SDK
verification for the new composition passes below; live qualification is separate.

## Composition and authentication

`Writes::with_task_ledger` uses the ledger's paired operation store, the existing
writer mutex and the same directly awaited command bus. Scoped composition rejects
legacy mutation/approval routes with `task_dispatch_required`, so a new legacy ID
cannot bypass task source fences. Legacy history and observation-only reconcile
remain available. Ordinary `Writes::new` retains the existing legacy profile. `JournalWriteTasks` refuses
read-only-bound state. Read-only bus policy rejects every task control/execute
command even with operator credentials. Disabled/ordinary coordinators return
`write_tasks_unavailable`; the ordinary runtime keeps this profile until explicitly selected as described below; no new daemon is added.

All three endpoints require the existing distinct operator bearer, never the
model/Core bearer. Authentication precedes JSON decoding and source access.
Request bodies are strict and at most 64 KiB. The ledger owner must match the
configured bus owner. Successful native replies and stream frames identify that
Core owner; the client checks it before authorizing or confirming any effect. There is no caller owner field;
the owner/deployment was bound at ledger composition. Native context descriptors
are accepted only from the trusted adapter. Strings alone do not prove host
identity. The adapter must use the actual v2 assertion, current owner predicate,
agent/session/incarnation and its process-minted Gateway identity.

The built plugin exports `createNativeTaskClient` for trusted composition. It
requires explicit transport qualification, a real hook/tool cancellation signal
and `summaryMemoryEnabled:false`.
Memory-enabled trusted composition instead uses `createMemoryNativeTaskClient`
with `summaryMemoryEnabled:true`, the host API and a distinct model/Core bearer
(`coreToken`) for history and fresh reads. The API must have the same configured
owner, explicit summary-memory opt-in and no read-only profile. Both clients require
explicit transport qualification; neither is activated by default startup.
The SDK model-tool metadata does not expose credentials or native context.

## Native summary-memory coupling

The memory-aware client waits for the captured task decision acknowledgement
before any history lookup or note IO. It resolves the exact member from Core
history, checks operation ID, registered physical Docs identity and mapping, and
uses the existing per-source native lock. It persists stale/pending-operation
evidence before starting the one-member execute request. Lock/fence failure stops
that request. A grid member creates no Docs note.

Current owner-private memory audience and no-promotion policy are rechecked by
the native assertion immediately before every Core request, including confirmation
after source preflight. Revocation withholds that confirmation and retains stale
evidence; it does not erase an already accepted member. Applied results reread the
source and replace the same seven-day note under its lock. Refresh failure is
reported as `summary_memory.status:refresh_failed` beside the applied result,
never as a reason to resend. Other outcomes leave the fence stale. Only observed
terminal history for the exact pending operation and physical source clears a
fence through the ordinary discovery/history path. Memory status is separate from
the Core operation state.

Verification status: AfTS4Z4B on 2026-11-14 passed the full supported SDK
verifier, including all12 memory-aware safe-root IO cases, plus all seven existing
Google-only regressions. YW7DQYg5 on 2026-11-15 passed all75 protected cases,
including same-identity timeout cleanup, official SDK build/validation and a final
206-input unchanged source check. These results do not establish live host routing, model quality, memory
indexing or native task-tool/lifecycle activation.

## Owner scope and lifecycle

`POST /v1/write-tasks/native` accepts exactly one action:

- `propose`: native context and bounded `TaskScope`; Core checks current registry,
  physical resource identities and permitted target/action bounds before proposal.
- `review`: task ID/context; returns retained scope, counts and whether this caller
  needs fresh confirmation. Inspection never revokes or renews a grant. A delayed
  old review cannot suspend a replacement Gateway grant.
- `decide`: exact task ID, context, scope digest, expected revision and
  `approve`/`reject`, derived only from the captured native owner dialog. Current
  registry is checked before approval. Rebinding a Gateway requires a fresh owner
  decision at the current revision, retires unsent captures and preserves counts.
- `capture`: task ID/context and one tagged grid/Docs change. Check active context
  and exact request bounds before source IO; capture under the shared writer.
  Exact replay returns stored evidence/state without rereading or replacing it.
  Fresh source capture and quota/membership admission remain separate from sending.
- `discard`: task ID/context/operation ID; reject unsent intent, retain evidence,
  spent ID and count, and keep permission for another in-scope approach.
- `end`: task ID/exact context/current grant revision and `completed`/`cancelled`;
  stale requests cannot close a replacement grant. Completion refuses unresolved
  members. Cancellation retains dispatched outcomes independently.
- `invalidate`: exact ended context for trusted lifecycle notifications. Such
  notifications are supplemental; they are not the synchronous send fence.

The native proposal callback captures immutable scope/ID/digest/revision, accepts
only allow-once/deny, registers acknowledgement before returning and awaits Core
before returning a grant acknowledgement. At most 64 ordering barriers are shared
across client recreation; they are not write authority or a second task store.
Pending/failed acknowledgement blocks member requests. Explicit inspection can
recover only the matching Core-authorized active scope/current incarnation after
a lost grant acknowledgement; it never resends the decision, resets counts or
resolves an uncertain member. A proposal exposes its server task ID for this
observation. Duplicate callbacks cannot renew permission or reset usage. The dialog lasts at most 60 seconds. There is no five-minute task TTL.
The existing five-minute capture freshness is separate. Ordinary questions and
pauses do not change persisted permission. Closed/new/reset incarnations cannot
reuse a prior grant. Uncertain members must be observed before reconfirmation.

## One-member dispatch and its acceptance boundary

`POST /v1/write-tasks/execute` selects one `TaskMember`: task ID, exact native
context, grant revision and operation ID. It never accepts a batch, preview,
`approved` boolean or remaining task list. The response media type is
`application/vnd.ciel.task-dispatch+ndjson;version=1`, with no-store caching.

1. Under the shared writer, Core atomically claims the exact member and persists
   the existing `applying` state before source work.
2. Recheck registrations, source identity/revision and exact before/after evidence
   using the existing grid/Docs preflight. Drift/failure returns its actual outcome
   without a ready message or provider mutation.
3. Recheck active durable context and emit one `ready` frame with a random nonce,
   complete member binding and exact preview digest. Hold the writer while waiting
   at most eight seconds for confirmation. This is a dispatch handshake deadline,
   not a task permission lifetime. At most 32 pending confirmations exist per API.
4. The client validates the entire challenge and rechecks its captured native
   assertion/owner/cancellation after preflight and after request encoding, in the
   same synchronous segment as sending `POST /v1/write-tasks/confirm`. It never
   asks the owner again for this in-scope member. The endpoint authenticates and
   matches the full one-use challenge before waking the awaiting bus command.
5. Core rechecks active grant/revision/membership, sealed intent, source fences,
   inverse references and freshness in a writer transaction. It inserts one
   immutable `write_task_dispatches` receipt and audit before the provider effect.
   Cancellation ordered before this acceptance wins. An already accepted member
   may finish after cancellation/reset; those events cannot retract its committed
   dispatch. No subsequent member inherits this acceptance. A confirmation HTTP
   acknowledgement alone is not proof that this transaction or provider succeeded.
6. Send through the existing adapter once, observe when accepted and atomically
   finish through the original journal/FSM. Return one `result` frame with the
   actual operation state. `ok:true` does not mean applied. The client validates
   framing, operation identity and bounded output (512 KiB) before disclosure.

Google and SQLite are not transactional together. Sheets still has its existing
read/write race; this is not provider compare-and-swap or external exactly-once.
Docs keeps requiredRevisionId. After native confirmation, a reset can race with
network delivery; the selected one-member acceptance boundary is deliberately
irrevocable. No long Core loop can continue other members under an ended host.

No detached executor is spawned: the HTTP body polls the awaited bus command.
Dropping it removes its pending checkpoint and cancels that future. A persisted
applying member remains conservative observation-only recovery evidence, including
when the request may already have reached the provider. Timeout/refusal before
acceptance produces known failure without a send; the existing non-applied outcome
suspension rule applies. A lost confirmation/result, cancellation after dispatch,
crash or transport error never authorizes resend or a replacement operation ID.
Inspect/reconcile existing evidence. Already spent receipt IDs remain spent.

## Evidence and remaining integration

Router/bus/real SQLite tests use a controlled stateful WorkspacePort and assert
source reads, exact sends, cancellation ordering, context/revision failure,
checkpoint replay/drop/timeout, accepted-receipt rollback, discarded intent and
Gateway reconfirmation. They do not establish Google HTTP behavior. A Node suite
uses the built client, actual SDK v2 registry/context/broker and a temporary real
Core HTTP fixture, including incremental edits, one owner decision and uncertainty.
Its requester/source are synthetic. The full supported Linux run on 2026-11-14
passes 212 Rust tests, 37 doctests, 88 Bun tests, 25 actual-SDK Node tests, seven
script cases, all seven Google-only scenarios and 74 protected acceptance cases.
The SDK/Core fixture proves incremental members after one owner decision and no
resend after uncertainty. The existing Google scenarios regress the shared apply
refactor; they do not automatically exercise the new task protocol.

## Native lifecycle revocation

Default v2 model factories register `before_reset` and `session_end` hooks and bind
their assertions to a per-host-API revocation set. Each hook retires only the exact
agent/session-key/session-ID tuple; delayed old-session hooks never select a
successor or a task ID. Hooks make no Core requests, approve nothing and do not
change durable grants or usage. An incomplete or contradictory hook identity
cannot select an incarnation. The host's current invocation assertion remains
mandatory because SDK lifecycle notifications are detached and may arrive late.

Trusted composition outside the default entry must call
`registerNativeSessionLifecycle(api)` once during registration and use
`bindNativeSessionInvocation(api, context, signal)` for the raw task client.
The memory-aware client automatically uses that same per-API binding. A delivered
reset/end invalidates retained callbacks, in-flight post-await checks and final
post-preflight confirmation. A previously accepted provider effect can still
finish; inspect existing operation evidence rather than resending it.

Revocations never expire or evict older incarnations. At 4096 distinct ended
incarnations per API, another distinct ending saturates the guard and blocks all
subsequent invocation checks with `NATIVE_LIFECYCLE_CAPACITY`. Restarting the
plugin clears local bookkeeping; genuine new host authority and the existing Core
Gateway-restart reconfirmation rules still apply. This is a memory bound, not a
task permission countdown.

Supported Node24 cases use the actual pinned SDK Gateway before-reset/session-end
emitters, registry, v2 resolver and hook runner with synthetic session/requester
identities and Core responses. They cover new/reset revocation, retained approval
and delayed old hooks. A separate memory-aware task case proves hook revocation
during preflight prevents confirmation/sends. The sra4HWst supported Linux verifier
passes official SDK build/validation,212 Rust/37 doctests,92 Bun and41 SDK Node
cases plus six Google-only scenarios. Its Workspace fixture rejected a newly
registered hook before native-memory execution. After correcting only fixture
registration/test wiring, WJbq61zd passes the official build,two new registration
regressions,Workspace/native memory and all75 protected cases; the210-input final
source check is unchanged. These emitter and simulated-provider integration tests
do not establish actual live reset commands or production owner routing.

Normal model task-tool/startup composition below passes its supported Linux gate. Broader same-source composition, formal provider undo and
evidence cleanup remain unfinished. Keep
every live/approval/write activation gate disabled until
its own qualification and explicit activation authorization exist.


## Model tools and normal startup composition

The manifest declares 33 tools: the existing 26 plus `tasks_propose`,
`tasks_resume`, `tasks_inspect`, `tasks_capture`, `tasks_execute`, `tasks_discard`
and `tasks_end`. Task tools reject before Core traffic unless `taskToolsEnabled`,
`taskTransportQualified` and `approvalTransportQualified` are explicitly true.
Read-only runtime rejects all task tools before reading operator credentials.
Model arguments never contain owner/context/credentials or a caller-approved bit.

Only the two scope tools request native approval in `before_tool_call`. The hook
joins the exact current v2 factory and trusted requester, snapshots the input,
proposes/reviews through Core and binds that proposal to its original task ID,
scope digest and revision. A bounded 64-entry per-API map holds only pending
proposal ordering, not permissions. Execute cannot manufacture a proposal;
changed input cannot consume another prompt. The tool explicitly awaits the
native client's Core grant ACK because the actual SDK callback is detached.
Failed ACK returns the retained task ID for inspection, never an assumed grant.
No additional owner prompt is requested for capture or one-member execute within
the approved scope. Inspect returns scope/phase/revision/counts; operation history
provides member outcomes. Scope expansion or unresolved intent needs owner review.

Each task-tool invocation selects the existing raw or memory-aware client from
current trusted configuration, using the distinct operator bearer and the model
bearer for history/reads only. Memory-enabled execution cannot select the raw
client to bypass stale fencing. Delivered reset, host expiry, owner loss and
cancellation still withhold final confirmation after preflight. An already
accepted effect may finish; uncertain results remain observation-only.

Normal Core startup retains the legacy profile unless
`SECRETARY_WRITE_TASKS_ENABLED=true`. The opt-in also requires
`SECRETARY_WRITES_ENABLED=true`, a journal directory and an explicit stable
`SECRETARY_DEPLOYMENT_ID` (1–128 ASCII letters/digits/dot/underscore/hyphen,
starting with a letter/digit). The existing journal is bound to owner/deployment;
opening the ledger retains counts and suspends old grants. Legacy mutation and
approval paths then reject `task_dispatch_required`, while history/reconciliation
remain observation-only. Invalid profile values or missing deployment fail startup.
This is local composition, not a qualification receipt or activation authorization.
Do not set these flags for a live target until its separate gates and owner
activation authorization exist. Existing shipped templates remain inactive.


Supported composition evidence (2026-11-15): after canonicalizing the manifest's
JSON key order to the actual SDK generator, agent-i3hl29kj passes official SDK
lifecycle/typecheck/build/validation, 42 Python, 214 Rust/37 doctests, 99 Bun,
51 Node/SDK cases, seven script cases, all seven Google scenarios and all75
protected Linux cases. Final source-check records215 inputs, unchanged=true;
owned source/targets and the unused immutable test image are cleaned. The full
and protected stages use approved direct Docker CLI calls with the same pinned
image/locks/network/capability boundaries as the local runner. The normal-startup
Google task case observes one scope decision, three Docs sends and one grid send,
including uncertain-result fencing and observation-only reconciliation. Workspace
traffic records89 Docs and57 Sheets requests. Owner/context/model/provider inputs
remain synthetic; this is not live host/approval UI/Google qualification.
