# secretary-writes local simulation context

Created 2026-10-26 from the bounded Google Sheets contract and synthetic handlers linked in `postman/README.md`.

## Purpose

Stateful approved writes and uncertain-outcome reconciliation. Google is the sole member; bridge, Rust Core and its SQLite journal run for real outside the simulation.

## Execution

Run `bash scripts/acceptance.sh writes` in the qualified environment from `docs/local-verification.md`. The launcher first builds the actual SDK entry, then waits for the listening event, derives the actual Google address, starts Core, invokes public tools and stops only its spawned processes. Inspect private `sim.ndjson` alongside the caller result.

## Assumptions

- Socket-capable Linux and cached locked dependencies are needed; the qualified container supplies both. (observed 2026-10-26, T017)
- Keep container `TMPDIR=/tmp`; use `SECRETARY_ACCEPTANCE_LOG_ROOT=/runs` for persistent private logs. Host-mounted scratch broke Postman dataset startup. (observed 2026-10-26, T017)
- Synthetic contract behavior does not establish live Google authentication/semantics, host approval routing, native Gateway persistence or WhatsApp delivery. (reasoned 2026-10-26, bounded mock handlers)

## Observations

<!-- begin:observations -->
2026-10-28 (T023): Google served the existing workflow successfully after generic Workspace capabilities were added; the seven-case suite passed.
2026-10-27 (T022): Google served the stateful and built-entry workflows after the Core ownership-lock fix; approval/reconciliation/single-record assertions passed. No live host UI/routing evidence is claimed.
2026-10-27 (T021): Google served the stateful and actual built-entry factory/hook workflows; the sequential suite passed approval, conflict, reconciliation and single-record assertions.
2026-10-26 (T020): Google again served the stateful and built-entry workflows after preview-cache bounds were introduced; approval, reconciliation and single-record assertions passed. Host routing/UI remain unqualified.
2026-10-26 (T019): Google served the existing stateful workflow plus the built entry’s registered factory/hook append. SDK metadata, lazy construction, API-instance preview isolation and exactly-once append assertions passed; the host registry remains a boundary fixture.
<!-- end:observations -->


## Approval acknowledgement ordering — 2026-11-13

- id: bridge-waits-for-core-approval-ack
- status: supported
- claim: With a locally prepared native approval, the bridge withholds apply until
  Core acknowledges the expected approved state; refusal and revoked invocation
  authority do not send apply in the controlled delayed-response cases exercised.
- reason: Notification of a native allow decision is distinct from durable Core
  approval. A pristine pre-change bridge sent apply before the held Core response;
  the new ordering barrier prevents that request and preserves unknown outcomes.
- verification: Supported Linux full gate on 2026-11-13 passes 65 Bun tests,
  18 actual-SDK Node tests and all seven Google-only scenarios. The four actual
  SDK handler/built-entry cases use synthetic Core replies to assert ordering;
  existing provider scenarios use real Core with Google simulated. These do not
  qualify live owner routing, native v2 continuation authority or Google writes.
- source: plugins/secretary-tools/src/approval-ack.ts;
  plugins/secretary-tools/src/bridge.ts;
  plugins/secretary-tools/test/approval-ack.test.ts
- updated_at: 2026-11-13

- id: actual-native-handler-approval-order
- status: supported
- claim: The built plugin's acknowledgement barrier also holds when the actual
  locked SDK host returns allow before its onResolution callback completes.
- reason: Four supported-runtime tests exercise the actual locked SDK deferred
  approval handler and built entry. Native allow returns before callback completion;
  delayed/refused acknowledgements, revoked invocation and native denial have the
  expected apply-request counts. A fifth memory case leaves note/history untouched
  when Core refuses approval.
- verification: All cases pass in Node 24.22.1 with OpenClaw 2026.10.7, genuine
  completed package lifecycle and unchanged frozen source inputs. Registration
  remains a boundary fixture and Core replies are synthetic in the ordering cases;
  live owner routing and real v2 continuation issuance remain unqualified.
- source: plugins/secretary-tools/test/approval-order.node.mjs;
  plugins/secretary-tools/src/bridge.ts
- updated_at: 2026-11-13


## Native one-member transport applicability — 2026-11-14

Status: supported regression passed on 2026-11-14 for all seven existing Google-only
scenarios and the 205-input native task revision. The earlier V6Dxsrsc run covered
the 198-input plugin-v2 revision. New code
refactors shared grid/Docs capture/apply to insert an optional post-preflight
native confirmation for task members. Legacy composition keeps existing behavior;
scoped composition blocks legacy mutation/approval bypasses and retains observers.
The Google HTTP adapters/mocks are unchanged. The new task router/bus/SQLite tests
use a controlled WorkspacePort and assert zero sends before confirmation, one-use
binding, cancellation, drift, timeout/drop, receipt rollback and uncertainty.
An actual-SDK/real-Core HTTP fixture adds a single owner decision with incremental
members, but its provider remains a controlled source. It is not Google HTTP or
live-host proof. This existing scenario passed in the full frozen Linux gate to
regress the shared adapter path; do not treat it as native task provider round-trip
or undo qualification. Owning sources: application/task_authority.rs, grids.rs,
documents.rs, presentation/task_native.rs and tests/task_native.rs.


## Native task memory coupling — supported verifier and protected gate passed, 2026-11-15

- id: task-member-native-memory-fence
- status: supported
- claim: The optional memory-aware native task client persists a per-source stale
  fence before dispatch, rereads after applied, and never resends to repair memory.
- reason: It reuses the existing native safe-root lock and refresh workflow. Core
  history supplies exact operation/physical-source identity; memory audience is
  checked again at post-preflight confirmation. The default model tools and Google
  adapters/mocks remain unchanged.
- verification: AfTS4Z4B full supported verifier passes37 SDK Node cases,
  including all12 memory-aware native IO cases, plus all seven existing Google-only
  regressions and88 Bun cases. All206 input hashes match the frozen receipt and
  then-current source. Two protected CLI fixture subprocesses timed out after15s;
  cleanup raised EPERM across the dropped-UID boundary. That run emitted no final
  source-check. YW7DQYg5 on 2026-11-15 passes all75 protected cases, including the
  real dropped-UID child-reaping regression, official SDK build/validation and a
  final206-input unchanged check after the fixture-only supervisor/budget correction.
  Compared with the successful main verifier, only two fixture Python files and
  verification-status Markdown changed. The supported memory claim is limited to the new
  controlled Core/source/model cases with actual SDK safe-root IO. Existing Google
  scenarios exercise legacy native memory, not the optional native task/member
  protocol. No actual host routing, live Google or model-quality qualification.
- source: plugins/secretary-tools/src/task-native.ts;
  plugins/secretary-tools/src/summary-memory.ts;
  plugins/secretary-tools/test/task-memory.node.mjs
- updated_at: 2026-11-15
