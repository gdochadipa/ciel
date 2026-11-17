# Controlled tasks and shared undo — implementation contract

Status: scoped writes, same-tab compound capture, native lifecycle and selected-
original provider undo have supported local Linux/SDK/simulated-Google evidence.
Evidence-unavailable storage/read semantics are implemented; owner-operated
cleanup execution has supported local Linux/SDK/simulated-Google evidence below.
Live host qualification remains unfinished. No production
write, service, native job or delivery activation is enabled. See
[native transport](task-native-transport.md).

Earlier foundation evidence: inverse planning/linkage and native one-member
transport passed on 2026-11-14. Memory-aware dispatch passed SDK/Linux IO in
AfTS4Z4B and all 75 protected cases in YW7DQYg5 on 2026-11-15 after a fixture
timeout correction. Later sections retain the development contract and distinguish
those historical foundations from the current provider undo gate.

## Authority and lifetime

Owner clarification, 2026-11-12: authorize the task's objective and boundaries,
then let the agent work like a coding agent. This supersedes the earlier requirement
to enumerate and approve every exact edit before the task begins.

The configured owner authorizes the task through trusted OpenClaw approval. The
grant binds the objective, registered resource scope, permitted action classes,
constraints and current owner/conversation. The agent can discover, prepare and
perform supported edits incrementally within that scope. Clear implementation
decisions and additional iterations do not require another owner prompt. A model
boolean, source content or unrelated “yes” cannot create or widen the grant.

Ask a focused question when material ambiguity or a decision reserved to the owner
blocks progress. Explain the alternatives and consequences, with a recommendation
when available. Independent in-scope work can continue. Ask for expanded authority
when the required objective, target scope, action class or constraints exceed the
existing grant. Do not treat every adjustment of wording, range or execution plan
inside that grant as a change requiring approval. Confidence alone does not confer
new resource access or resolve an uncertain provider outcome.

Each concrete operation still gets exact immutable target/before/after evidence
before dispatch. Its digest binds the actual edit, and its authority derives from
the current task grant under an explicit Core-enforced policy. This is not another
per-operation human approval. Persist the grant revision, policy result and exact
operation linkage; never overwrite an already dispatched intent. Full proposed
diffs remain inspectable, and actual changes/outcomes must be reported.

There is no five-minute task countdown. Permission ends on completion, explicit
cancellation or new/reset conversation. Ordinary questions or an ordinary pause
do not revoke the unfinished task. There is one active write/undo task per owner
and trusted conversation incarnation. Undo requested after a task ends is a new
owner-authorized task; its routine supported steps follow the same autonomy rule.

The existing single-operation protocol retains its 300-second preview expiry and
60-second native prompt timeout. Do not remove these guards or create freshly
approved legacy previews to implement task lifetime. A versioned task execution
path must distinguish dialog lifetime, task permission and source freshness.
Immediately before each dispatch it must check current owner, conversation,
process incarnation, grant revision, cancellation and each operation's scope and
immutable intent linkage. Do not use an arbitrary model assertion as a policy
check. The versioned native transport now implements this through explicit trusted
composition; normal opt-in model task tools use this same versioned transport.

Core or Gateway restart suspends future task dispatch. Persisted applying members
enter the existing reconciliation path. Inspect outcomes without resending; obtain
fresh owner confirmation for exact remaining members. A reset/cancel cannot unsend
a request already dispatched. Such members must still be observed and reported.
No automatic rollback is part of cancellation or recovery.

## Implemented journal backend

`domain::write_task` defines strict scope/context shapes. A scope declares an
objective, mapping version, one to eight exact physical registered target/action
bounds and a finite limit of 1–1000 admitted operations. Objective text records
owner intent; it is not a machine-verified semantic policy. Current registry,
source, host and runtime authority still need application/adapter checks.

`journal::write_tasks::JournalWriteTasks` is an opt-in library backend. It is bound
once per Journal instance to one owner/deployment. Read-only-bound state refuses
attachment; read-only startup refuses adopting task-bound state. Existing legacy
records must belong to the same owner and retain their original digests/behavior.

Server-created proposal IDs/digests bind the exact scope and conversation
incarnation. Trusted owner decisions carry the captured grant revision; replay
cannot reactivate suspended/closed authority or authorize another identical-scope
task. Initial proposal approval expires after the native 60-second dialog window.
Active task authority has no five-minute countdown. Restart suspends authority;
fresh confirmation rejects pending old captures and requires resolving interrupted
writes before accepting new intent. Counts never reset on confirmation.

Admission validates actor, mapping, physical target/range, inverse coverage, capture
size/freshness, predecessor state and cumulative count in one transaction. Concrete
captures retain a maximum five-minute freshness window without limiting task
lifetime. Discarding an unsent preview uses the existing Reject transition, retains
its spent ID/evidence/count, and permits a fresh in-scope approach. Altering the
existing intent under the same ID conflicts.

Claim rechecks live grant/membership, exact digest, source fences and freshness,
then derives Approved and persists Applying atomically through the existing FSM.
Its audit distinguishes task-derived authority. Existing legacy approve/begin-apply
entry points reject linked task operations. Non-applied outcomes suspend the task
within the same transaction that records the operation outcome. Cancellation
revokes permission while keeping already-dispatched outcomes independently visible.
New IDs/tasks cannot bypass unresolved original source writes; legacy Sheets
classes without retained physical identity conservatively fence Sheets admission.

Trusted lifecycle invalidation matches the complete ended context, including its
Gateway and session incarnation. A delayed old notification cannot cancel a newer
task or resumed Gateway grant. Compact history returns at most 20 scopes and actual
operation-state counts from one SQLite snapshot, without before/after payloads.
Task-reported completion is separate from individual attempt outcomes.

These backend methods trust application/host callers. `TaskContext` strings are
descriptors, not native proof. The optional application port pairs this ledger to
its exact journal and the shared writer. Native routes require the distinct
operator bearer; the built trusted client captures actual v2 assertion/owner and
cancellation signal. A post-preflight challenge and atomic one-use accepted-member
receipt fence provider dispatch. Core rechecks permission, immutable intent and
source/inverse fences after confirmation. Ordinary legacy mutation/approval
routes are blocked in scoped composition; history/reconcile remain observation.
Supported SDK/HTTP/Core tests are required before qualification. Normal model tools select this transport only in explicitly qualified task
composition; production defaults remain disabled.

## Persisted original/inverse references

`JournalWriteTasks::admit_inverse` loads the authoritative original from the shared
owner-bound journal and validates the fresh capture against `Inverse` before
committing anything. The existing admission transaction retains both preview
digests and original/inverse IDs together with task membership, quota and audit.
Exact replay must retain the same original link; a normal `admit` replay cannot
silently strip that association. Claim checks the original/evidence/link again.

For task members the original task must be closed. Applied members are undone in
reverse admission order, across all resources in that task, and an earlier member
waits for later applied members to have an applied inverse. Pending, applying,
uncertain, reconciliation, conflict or applied inverse attempts block another
inverse ID. Known-failure, expired and rejected attempts retain references while
allowing a freshly captured attempt with current task authority. A definite failure
still suspends task execution through the existing outcome/FSM rules. Uncertainty
requires observation; it is never converted to permission by selecting a new ID.

This initial inverse path does not compensate an inverse (redo). Supported legacy
applied originals can be referenced without changing or adopting their original
identity, preview or approval. New task authority is always required; a completed
original's approval does not authorize undo. The trusted library methods remain available. Public undo capture uses an
explicit original selection and the inverse-admission path described below;
objective text alone does not select originals or derive inverse content.

`inverse_history` returns at most twenty references per page from one SQLite
snapshot, with task/revision/digest/outcome metadata and no before/after contents.
Pages use immutable inverse IDs so outcome changes do not reorder references.
Restart retains links but does not restore old task authority; fresh confirmation
retires unsent old captures while preserving their IDs and references. Evidence cleanup and live provider/host qualification remain unfinished;
local provider round-trip evidence is recorded below.

## Scoped provider undo integration

A new undo task uses the existing `tasks_propose` owner approval with a typed
`scope.undo_of` selection: `{kind:"task", task_id}` for a closed original task,
or `{kind:"operation", operation_id}` for one retained applied original. Typed
registered targets, mapping and finite operation count remain required. The
native prompt labels the request as an undo task and displays this exact scope.
Ordinary scopes omit undo_of; omission retains their historical JSON and digest.
Existing trusted library inverse admission without this optional field retains
its earlier contract; the new public inverse route cannot use that compatibility
path. Public forward captures are rejected inside an undo task.

Use `tasks_capture` with `change: {kind:"inverse", operation_id, original_id}`.
The caller supplies no inverse fields, before/after content, target or approval.
Under the shared writer, the Core validates current undo selection, loads the
owner-bound sealed applied original and checks closure, reverse order, earlier
inverse attempts and declared physical/mapping bounds before provider source IO.
It derives the inverse, reads current source, validates exact retained reversal
and atomically admits through the original/inverse journal path. Replay retains
the same original association, preview digest and spent count without recapture.

`tasks_execute` supplies the existing one-member checkpoint and dispatch receipt;
claim and final permit recheck inverse linkage and original selection. Unknown
outcomes only reconcile by observation, never resend. A later unresolved original
on any resource also blocks earlier undo; completed inverses permit the next
original in reverse order. Original operation states/evidence remain unchanged.
Docs single/compound and literal Sheets rectangle coverage uses the same provider
adapters. Unrelated current content outside the captured target is preserved.
Supported legacy applied originals may be selected individually; redo is refused.

For undo scopes, `tasks_inspect` additionally returns `undo_next`: the next
remaining original's ID/state and at most one blocking inverse ID/state, or null
when none remains. It exposes no before/after bodies and grants no authority.
Uncertain/conflicting originals are reported rather than skipped. Capture may
still refuse a displayed original; current registry/source/authority checks are
mandatory. Metadata comes from the same bounded SQLite snapshot as scope review.
Supported evidence in agent-vbznb0a7 covers actual locked SDK build/validation,
42 Python, 227 Rust/37 doctests, 108 Bun, 56 actual SDK Node, 7 script cases, all 7
Google scenarios and 75 protected cases. The normal Google fixture observes 4
scope decisions (two forward, two undo), 10 Docs sends containing 14 replacements,
2 grid sends, untouched other-tab content and lost inverse response recovery by
observation. Both initial/corrected snapshots retain 223 unchanged source hashes.
The initial full stage failed only a new fixture's model-call total: applied
history observation refreshes memory, so two observations move 10->11->12. Exact
boundary assertions reproduced this; only the fixture was corrected. The full
verifier and six Google cases remain accepted; SDK/build/registration/Workspace
and protected acceptance were rerun successfully. No production,adapter,lock or
SDK source was changed by that correction. Evidence cleanup and live host
qualification remain separate work.

## SDK evidence and remaining host qualification

Runtime factories now register explicit version-2 descriptors while retaining
the SDK authoring metadata and existing tool schemas. Each execution captures
the host assertion, preserves its live owner getter and checks the tool's abort
signal. The native approval hook resolves exactly one live factory binding by
tool/agent/session key/incarnation, checks the trusted hook requester and captures
its abort signal. Missing fields, multiple live candidates or capacity exhaustion
deny approval. The lookup holds at most 64 live bindings per API, grants nothing
and does not expire durable task permission on a timer. A retained callback keeps
its original assertion; reassembling tools or changing model input cannot replace it.

Each Core HTTP dispatch rechecks authority after encoding in the same synchronous
segment as fetch. This establishes the plugin request boundary only. A dispatched
request is never retried on cancellation; it may already have reached Core. It does
not establish current host authority during Core's asynchronous source preflight
or at its provider send. The new optional native transport adds that boundary:
Core fresh preflight -> captured native confirmation -> revision-bound durable
accepted-member receipt -> one existing adapter send/observe. It remains unexposed
to normal model tools; supported SDK/HTTP verification passed on 2026-11-14.

New tests use the actual locked registry, factory-context issuer, retained callback
wrapper and run-scope fence. Their requester identity and incarnation change are
synthetic fixture inputs. They do not run the host reset/new command or prove live
continuation provenance. The full supported Linux run on 2026-11-14 passes the
native-task transport scenario, all seven existing Google-only regressions and
74 protected acceptance cases. Those host qualification claims remain separate;
the new transport fixture uses a controlled WorkspacePort, not Google HTTP.

Inspected the actual locked OpenClaw 2026.10.7 package locally. Its version-2 tool
factory requires `assertInvocationCurrent`; the host context exposes optional
`agentId`, `sessionKey` and a `sessionId` regenerated by new/reset. Required absent
fields must deny task execution. Host fields must not be copied from tool input.
The before-tool approval hook exposes requester and session fields but does not
itself expose the version-2 invocation assertion. The bridge must bind approval
callbacks to the same scoped task grant and current invocation authority.

The actual reset implementation launches `before_reset` asynchronously, and
`session_end` is detached work. Those notifications alone are not a synchronous
dispatch fence. Before wiring execution, exercise the actual SDK's version-2
factory, retained callback rejection, reset/abort and final-effect assertion.
The Core must enforce its own durable grant/membership checks on each member.
Never let one long Core request continue sending remaining members after the
host loses authority. Fail unavailable until this transport contract is proved;
actual target routing and owner approval remain separate live qualification.

## Compound source and execution rules

Use the existing awaited bus, shared writer mutex, operation FSM and journal.
Task records bind immutable operation intent to the current scoped grant; they do
not create another durable workflow engine. The task's final operation list need
not be known before owner authorization. Each subsequent operation requires fresh
bounded source capture, inverse support and durable admission under that grant.

Related edits prepared together for one source can use a bounded provider batch
with exact aggregate before/after evidence and an initial revision/fingerprint
guard. Reject unsupported overlaps or ambiguous text replacements. For later
iterations, track verified earlier effects and expected source transitions rather
than comparing against the task's original pre-write revision. Do not silently
refresh legacy approved previews or modify an existing operation's digest.

Several batches or resources require explicit dependency and reverse-order
semantics within the authorized target scope. The previous single-batch proposal
is an optional optimization, not the task authorization model. Google and SQLite
do not share a transaction. Sheets still has a read/write race after preflight;
do not claim atomic compare-and-swap or unconditional undo.

Persist each member's task-authorized intent and `applying` before dispatch. Send once,
observe accepted responses, and stop remaining sends on failure, conflict or
uncertainty. Reconciliation only observes. Report partial outcomes independently.
The undo plan includes only resolved applied members whose evidence is retained.
An unresolved original or inverse cannot be bypassed by a new operation ID.

### Same-tab compound capture

Scoped `tasks_capture` also accepts a document `replace_text_batch` with
`fields: {replacements: [{find, replacement}, ...]}`. It contains 2..16 unique,
case-sensitive, nonempty single-line literal replacements, each side at most 2048
UTF-8 bytes. All belong to one registered editable tab. Original match intervals
and reverse intervals must be disjoint; ordered projection must equal independent
source-span projection and reverse exactly. Overlaps, cascades, ambiguous reverse
matches, unsupported structures and no-op aggregates are rejected.

The batch is one immutable member and consumes one max_operations unit. It sends
one `batchUpdate` with ordered replaceAllText requests and the captured
requiredRevisionId. Exact aggregate before/after evidence uses the existing FSM,
source fences, owner grant, native dispatch checkpoint and send receipt. It never
rebases an existing preview. Later batches capture fresh revisions after earlier
verified effects. Drift stops dispatch; uncertainty blocks later sends and permits
only observation. Task history retains partial outcomes independently. Legacy
model tools retain their existing action set. This does not add cross-tab/resource
batching, a dependency scheduler, provider undo execution or cleanup authority.

## Paired inverse foundation

`domain::compensation::validate_reversible` checks captured forward evidence
before approval. `domain::compensation::Inverse` prepares a candidate inverse from a sealed,
authoritative applied operation. It grants no approval, reads no provider and
does not persist or dispatch anything. Consumers must load the original from the
owner-bound journal, prepare a fresh preview against current registrations/source,
and call `validate_preview` before admitting it under the new undo task. That check binds
the fresh request to the original physical target and exact reversed before/after.
Expiry of the original approval does not delete undo evidence or authorize undo.

| Class | Captured evidence | Candidate inverse | Refusal conditions |
| --- | --- | --- | --- |
| Sheets literal rectangle | Exact entered types and blanks, bounded range, source identity and mapping | Same rectangle with original literal values; null clears, empty string remains a string | Formula/unknown entered type, invalid dimensions, >100 cells, >16 KiB snapshot, >2048-byte cell text, unsafe number, changed registration/target/affected values |
| Docs one-match plain text replacement | Exact selected-tab before/after text and original find/replacement | Reverse one unique nonempty single-line replacement | Append, deletion/empty inverse needle, ambiguous reverse match, multiline/control/private-use replacement, >2048-byte find or replacement, changed tab/body/registration |
| Docs same-tab compound | Exact aggregate body before/after and 2..16 independent replacements | Reverse request order, with unique disjoint reverse intervals and exact text restoration | Overlap/cascade, ambiguous reverse match, unsupported structure, changed tab/body/registration or evidence mismatch |
| Append, structured rows, inserted rows, new tabs, rich structure | Insufficient for these inverse contracts | Unavailable | Refuse admission to the new controlled task profile |

Docs coverage is conditional text restoration, with normal current-source and
required-revision validation. It does not promise restoration of formatting,
layout, styles, tables or suggestions. Future composition must preserve untouched
source content and reject unsupported structures at capture time. Existing legacy
write behavior is not newly admitted or qualified by the inverse planner.

## Persistence, history and cleanup obligations

Keep legacy operation JSON, request/preview digests and FSM semantics unchanged.
Add versioned task membership and original/compensating references in the same
journal. Atomic transactions must bind task authority, grant revision and membership;
claiming a member and persisting `applying` must not admit a stale grant.
One active conversation task needs a durable uniqueness constraint.

Retain evidence until explicit owner-approved cleanup. Before implementing purge,
introduce a distinct evidence-unavailable representation retaining spent operation
IDs, request/preview digests, owner, target identity, original/inverse links,
outcomes and audit. Old preview/approval/apply/reconcile requests must not recreate
authority or refill removed evidence. Never modify existing previews in place to
fake tombstones or delete operation rows to free their IDs.

The storage upgrade separates immutable preview JSON into `operation_evidence`.
Permanent `operations` rows retain owner/registered and physical target identity,
request/preview digests, outcome and membership/compensation/audit references.
Legacy previews move byte for byte within one SQLite transaction; a failed
migration rolls back schema and data. This is a forward schema upgrade: an older
binary cannot read the upgraded layout. Use a matched pre-upgrade backup for a
rollback; do not run an old binary against the upgraded journal.

Missing payload alone is a storage error. A distinct, versioned cleanup receipt
and an eligible resolved outcome are required to represent unavailable evidence.
History then returns the retained identity/outcome plus
`evidence: {status: "unavailable", cleanup_id, removed_at}` without source bodies.
Retained-preview history is unchanged. Same-ID replay cannot repopulate evidence;
preview/approval/apply/reconciliation/inverse capture refuse with
`operation_evidence_unavailable` (or an earlier authority denial), without source
access. This is availability metadata, not a new FSM state or proof of failure.
Task history reports `unavailable_evidence` counts; inverse-reference history keeps
original/attempt outcomes, digests and evidence status. `undo_next.evidence` reports
`retained` or `unavailable`; an unavailable member is not silently skipped.

At the foundation gate below no runtime removal API existed; unavailable records
were synthetic tests. The subsequent owner-operated cleanup protocol is specified
in the final section. Development authorization never authorizes operator purge,
physical database/WAL shrink or backups/native-memory deletion.

The foundation gate on 2026-11-16 passes the actual locked SDK build/validation,
42 Python, 236 Rust/37 doctests, 108 Bun, 56 SDK Node, 7 script contract cases,
all seven Google scenarios and 75 protected Linux cases. All 225 frozen inputs
remain unchanged. Migration tests preserve exact legacy bytes/digests/foreign
keys across restart and rollback, cover multiple bounded batches and injected
payload-storage failure. Disposable unavailable records preserve history/links,
spend IDs and refuse native inverse capture/reconciliation with no source IO.
The Google scenarios regress retained-evidence paths: their existing Workspace
fixture still observes 4 scope decisions, 10 Docs sends/14 replacements and 2 grid
sends, including lost-response observation without resend. They do not exercise
an actual cleanup command or qualify a live host. That historical foundation did
not expose a runtime removal API.

Cleanup preview must select closed tasks with resolved outcomes/age ranges and disclose affected undo
coverage and estimated reusable bytes. Under the shared writer boundary, check
live task/grant/dialog/inverse/recovery/native-memory references, then atomically
commit approved removal and audit only if the selection and references still
match. Protect unresolved or dependent evidence. A task-history response must
distinguish unavailable undo from a nonexistent original task.

Pause new writes when capture cannot be retained safely. Report reusable database
pages separately from physical file shrinkage, WAL size and backup copies. Cleanup
of operational evidence does not clean OpenClaw personal memory or backups.

## Required acceptance before calling item 5 complete

1. Actual built SDK to authenticated Core/bus/journal and simulated Google: scoped
   task authorization, several incrementally prepared edits, exact per-operation
   capture, no repeated prompts for clear in-scope work, and focused clarification
   for unresolved owner decisions or expanded authority.
2. Wrong/missing owner/incarnation, retained approval callback, duplicate approval,
   out-of-scope action/target, new/reset/cancel, ordinary pause and restart confirmation.
3. Same-source compound edits without self-conflict, human drift, partial outcome,
   crash around every persistence boundary and exact no-duplicate send counts.
4. Sheets/Docs paired round trips, original history retained, unrelated content
   preserved, new task authority for undo, later-edit conflict and uncertain inverse
   observed without resend. A pure planner test is not provider round-trip proof.
5. Compatible retained legacy records, bounded history, cleanup/tombstone replay
   protection, concurrent undo selection, low disk and interrupted cleanup.
6. Formatting, warnings-denied Clippy, full verifier, relevant Google simulations
   and protected admission regressions with actual locks/SDK. Keep all target/live
   approval, service and write activation evidence separately qualified.

## Owner-operated evidence cleanup

`POST /v1/evidence-cleanup` uses the distinct configured operator bearer,
authenticates before decoding, and dispatches directly through the existing bus
and shared writer. It is intentionally absent from the 33 model-facing tools.
The ordinary Core bearer and caller approval booleans cannot authorize cleanup.
An attached owner/deployment-bound task ledger is required. The application port
can run with provider writes disabled; it cannot elevate the read-only profile.
Normal binary task-profile startup retains its existing opt-in qualification gates.

The operator first posts:

```json
{"action":"preview","selection":{"task_ids":["11111111-1111-4111-8111-111111111111"],"operation_ids":[],"completed_before":1794873600}}
```

Use actual selected IDs and an elapsed Unix cutoff, not the illustrative values.
At most20 selectors and200 retained operations are accepted; an oversized selection
is refused rather than truncated. Task selections include all members of a closed, resolved task, including a
cancelled/retired task whose individual outcomes are resolved. Its closure reason
is disclosed; cancellation alone never establishes provider failure. Standalone operation selectors are for legacy records without
membership. Applied, known-failure, rejected and expired evidence is eligible;
conflicts and uncertain/interrupted work remain protected. The preview discloses
objectives/targets/outcomes/digests, affected IDs, source-payload byte estimates,
applied originals losing conditional undo evidence and inverse payload counts.
It exposes no before/after source bodies. All preview contents bind its digest.

Before preview and again inside the approval transaction, require a quiescent
owner/deployment: no proposed/active/suspended task or unresolved operation. Both
ends of every compensation edge must belong to the exact selection; select whole
original/inverse tasks and any standalone legacy original together. This first
maintenance profile deliberately refuses unrelated unfinished work too. Closing
a task does not resolve an uncertain member. No provider read or send is used.

Native memory's pending-operation reference is preserved, not removed or asserted
absent. Cleanup validates the permanent history projection required by the shipped
native observer: operation ID, document/tab, physical ID, mapping and terminal
outcome remain identical. Payload-dependent undo/recovery is protected separately.
This does not enumerate host indexes, delete notes, claim physical expiry or grant
native-memory authority. Native repair still holds its normal lock/policy and reads
current source; it never reconstructs removed journal evidence or resends a write.

After owner review, post exactly one captured decision:

```json
{"action":"decide","cleanup_id":"SERVER_RETURNED_ID","preview_digest":"SERVER_RETURNED_DIGEST","decision":"approve"}
```

`reject` retains every payload. Approve recomputes the selection/references under
the shared writer and SQLite writer lock. It atomically deletes only payload rows,
sets versioned cleanup references on permanent records, records per-operation
removal and owner-decision audit, and stores the committed result. Failure rolls
back the entire transaction. IDs, digests, parent rows, outcomes and links survive.
No additional human prompt is required per selected operation.

Pending previews belong to the current Core process; after restart make and review
a fresh preview. There is no task permission countdown. Use
`{"action":"inspect","cleanup_id":"SERVER_RETURNED_ID"}` after an unknown response.
Matching approved replay returns the persisted result, including after restart,
without another removal; a conflicting decision/digest is refused. No blind retry
with a new selection or cleanup ID may substitute for observing the original.

Results report removed payload bytes, SQLite page size and reusable pages before/
after the transaction. These are separate from physical database/WAL shrinkage and
backup copies. This endpoint performs no VACUUM, explicit WAL truncation, backup
removal or native personal-memory deletion. Preview/receipt/audit metadata remains
operational history. Low storage prevents new intents from being safely persisted;
cleanup storage failures do not authorize provider writes or partial deletion.


Cleanup qualification, 2026-11-17: the frozen229-input agent-xg7_yq_o gate passes
actual locked SDK lifecycle/build/validation,42 Python,248 Rust/37 doctests,
108 Bun,57 SDK Node,7 script cases,all7 Google scenarios and75 protected cases.
No source or fixture changes occur during these gates. Pre-gate review corrected
an unknown-tool fixture status/duplicate JS name and aligned the copied operator
response schema; all final code/schema/test bytes were frozen before the full run.
New tests cover exact/full-preview digest binding, owner/model credential
separation, read-only refusal, disabled-provider composition, complete inverse
references, concurrent undo and duplicate decisions, pending restart rejection,
committed receipt replay and injected per-operation/final-audit rollback. An actual
SQLite max_page_count bound produces SQLITE_FULL code13; this is engine quota
failure evidence, not host-volume exhaustion. A test-only child is killed before
and after commit, then reopened: removal is atomic and lost-ACK observation works.

The actual built SDK/Core/Google Workspace fixture explicitly approves one cleanup
of two completed linked payloads. It checks receipt replay, unchanged native-note
bytes during cleanup and successful native repair from permanent history without
another Google write. Source counters remain10 Docs sends/14 replacement requests
and2 grid sends; scope decisions4,cleanup owner decisions1,synthetic model calls13.
Member logs contain131 Docs/63 Sheets request events, including fixture controls.
Controlled-source Core tests separately assert no provider IO for cleanup itself.
This establishes local/simulated behavior, not real owner UI,host/Google/model,
physical erasure or operator-journal purge. Production qualification stays separate.
