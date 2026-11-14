# Native task transport v1

Status: locally implemented trusted composition API. Production startup and the
26 model tools retain their existing behavior. They do not attach the task ledger
or call this client. Supported Linux/SDK and live qualification are separate gates.

## Composition and authentication

`Writes::with_task_ledger` uses the ledger's paired operation store, the existing
writer mutex and the same directly awaited command bus. Scoped composition rejects
legacy mutation/approval routes with `task_dispatch_required`, so a new legacy ID
cannot bypass task source fences. Legacy history and observation-only reconcile
remain available. Ordinary `Writes::new` retains the existing legacy profile. `JournalWriteTasks` refuses
read-only-bound state. Read-only bus policy rejects every task control/execute
command even with operator credentials. Disabled/ordinary coordinators return
`write_tasks_unavailable`; no environment flag or new daemon activates this path.

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
Memory-enabled task dispatch stays unavailable until the existing stale-note /
refresh fencing is integrated; the helper cannot silently bypass that policy.
The SDK model-tool metadata does not expose these credentials or methods.

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

Integration into normal model tool selection, native reset/new routing and summary
memory, broader same-source composition, formal provider undo and evidence cleanup
remain unfinished. Keep every live/approval/write activation gate disabled until
its own qualification and explicit activation authorization exist.
