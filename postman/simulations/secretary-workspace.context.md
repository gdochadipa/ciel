# secretary-workspace local simulation context

Created 2026-10-28 from official Sheets spreadsheets.get and Docs documents.get/batchUpdate text/revision subsets; references in T023.

## Purpose

Synthetic generic grid reads, bounded tab creation, controlled Docs text edits and explicit selected-Docs/Sheets-view search and Docs summary source contexts. Sheets and Docs are the only members; actual bridge, Rust Core and SQLite journal run outside the simulation.

## Execution

Run `bash scripts/acceptance.sh workspace` in the qualified environment. The launcher builds the SDK entry and Core, derives both actual listening member addresses, invokes public tools and cleans owned process groups. Inspect private member traffic and caller results.

## Assumptions

- Existing Postman CLI1.75/native scratch `/tmp` and separate persistent log-root requirements still apply. (observed 2026-10-27, previous scenarios)
- Docs fixture models plain-text tabs and strict required revision control, not the whole editor or live auth semantics. (reasoned 2026-10-28, handler)

## Observations

<!-- begin:observations -->
2026-10-29 (T036): Both members served traffic (55 Docs/48 Sheets events,42 primary-Docs paths). Actual SDK summary_context returned structured citations/coverage alongside legacy reads; existing write counters remained expected. Only source preparation/transport was exercised: no live model, generated-summary quality or host skill installation is represented.
2026-10-29 (T035, after review correction): Both members served traffic (53 Docs/48 Sheets events), including5 independent reference-spreadsheet GET path events. Actual built-SDK selected-view global pagination and target-order cursor rejection passed. Reference Sheet is a separate static read-only seed; whole-page optional-profile projection errors are additionally exercised by the public HTTP regression, not modeled as a new Google endpoint.
2026-10-29 (T033): Both members served traffic (53 Docs/38 Sheets request events); Docs included five reference-document path events alongside the mutable primary. Real bridge/Core and built-SDK selected-doc global pagination passed. The second Doc is an independent static read-only seed; no atomic snapshot or live provider behavior is modeled.
2026-10-28: Both Google Sheets and Google Docs members served traffic. Actual bridge/Core generic values, Docs preview/approval/append/replace, stale revision rejection, uncertain-response reconciliation and built-entry callback assertions passed; other tabs remained unchanged.
<!-- end:observations -->

2026-10-28 (T025): Both members served traffic again. Sheets now models metadata-only catalog reads and atomic addSheet/updateCells, preserving first-row literal headers. Real bridge/Core and built SDK tab factory/hook pass approval, replay, name collision, no automatic view grant, optional empty headers and accepted-create/malformed-response reconciliation. Three tab creates (including SDK fixture), no duplicate sends. Native HTTP tests additionally cover ID collision and journal restart. Mock state uses the namespaced `google-sheets-grid:create-state` key; intercepted JSON comes from `req.body`, as in the other member. Earlier use of native stream-only parsing returned 400; corrected before acceptance. No live Google/host qualification is established.

T027: workspace also exercises explicit registered grid writes through the real bridge/Core/journal and actual SDK update factory/hook, including literal/clear, approval, replay and accepted-write/lost-response observation-only reconciliation. The Sheets member supports a bounded updateCells subset on the synthetic original grid; no existing tab-create or Docs mutation is repurposed as live evidence. Both members served requests in the targeted local pass. Final all-scenario results are recorded in T027.


## Approval transport applicability — 2026-11-13

The acknowledgement-order investigation and its supported/unknown claims are
recorded in secretary-writes.context.md. The changed bridge covers all four
existing commit classes, including grid, Docs and tab creation. Local controlled
Core-response tests, four actual SDK handler/built-entry ordering cases and a
native-memory refusal case pass in the supported Linux gate on 2026-11-13. All
seven Google-only scenarios, including Workspace, pass against the frozen revision.
Registration/Core responses in the ordering cases remain fixtures; live owner
routing and real native v2 continuation authority are not qualified. The new
journal inverse-reference methods have SQLite tests only and are not exposed in
these provider scenarios; do not infer provider undo round-trip coverage.


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


## Lifecycle fixture registration — supported retry passed, 2026-11-15

- id: native-memory-fixture-hook-surface
- status: contradicted
- claim: The native-memory acceptance host fixture only needs to accept
  before_tool_call when registering the built plugin.
- reason: The default plugin also registers before_reset and session_end.
  A fixture asserting a single hook fails before native-memory source/model/IO
  behavior can run; this does not establish a Google or lifecycle guard defect.
- verification: Operator run sra4HWst matches all208 frozen lifecycle inputs.
  Official SDK lifecycle/typecheck/build/validation,212 Rust/37 doctests,
  92 Bun/41 actual-SDK Node cases and seven script cases pass. Six earlier Google
  scenarios pass. Workspace reaches three built-entry cases, then the native
  memory fixture rejects before_reset at registration. Protected acceptance and
  final source-check are not reached; cleanup removes the unused test image.
  Extracted the unchanged restrictive registration boundary into a shared helper:
  Node24 with the actual built plugin reproduces the same assertion, then passes
  after accepting exactly the three registered hooks. Unknown/duplicate hooks
  still reject and the approval callback cannot be replaced. The read-budget
  fixture had the same stale assertion and now uses that same helper. WJbq61zd
  reruns official SDK lifecycle/build/typecheck/validation,the two new registration
  regressions,Workspace/native-summary-memory and all75 protected cases: all pass.
  Its exact scope excludes the full verifier and six completed Google scenarios.
  All210 frozen hashes match then-current source; final source-check reports
  unchanged=true and image cleanup succeeds. Google member traffic contains70
  Docs and51 Sheets request events. Only fixture/test wiring and this context
  changed from sra4HWst; production sources and locks are unchanged. No live
  host/model/Google or native-task provider qualification is inferred.
- source: scripts/acceptance-hooks.mjs; scripts/acceptance-native-memory.mjs;
  scripts/acceptance-read-budget.mjs;
  plugins/secretary-tools/test/acceptance-registration.node.mjs;
  sra4HWst/simulations/secretary-acceptance.rJ8J5A/client.log;
  WJbq61zd/simulations/secretary-acceptance.PFiOLx/client.log;
  WJbq61zd/acceptance.log; WJbq61zd/source-check.json
- updated_at: 2026-11-15


## Model task tools and normal startup — supported simulation, 2026-11-15

- id: model-task-startup-composition
- status: supported
- claim: The built default tasks_* tools can perform sequential bounded Docs/grid
  edits after one scope decision through the normal Core binary's opt-in profile,
  while keeping uncertain Docs outcomes fenced and preserving native note refresh.
- reason: agent-i3hl29kj runs the built tools through the normal Core binary's
  separate fresh task journal against both Google stand-ins. Actual outcomes,
  send counters and member traffic support this bounded simulated path. Existing
  mocks/provider adapters/locks are unchanged; no live target authority is inferred.
- verification: Full Linux gate on 2026-11-15 passes the official SDK build,
  51 Node/SDK cases (including model-tool/real-Core and actual v2 issuance), all
  seven Google scenarios and75 protected cases. The new Google task case observes
  one scope decision, two applied Docs replacements, one grid write and an
  accepted/lost Docs result; exact three Docs/one grid send counters prove no
  resend. Native stale fencing, reread/refresh, observation-only reconciliation
  and task closure pass. Workspace traffic includes89 google-docs and57 google
  requests. Final215-input source check is unchanged and owned cleanup succeeds.
  Earlier svzHLA8M functional stages passed but its final integrity check failed
  because the SDK reordered manifest keys. The ordering-only correction is
  semantically identical JSON and the actual build/source-hash repro turns green.
  The supported rerun uses approved direct Docker calls with the same isolated
  runner stages; real host requester/approval UI, Google and model remain unqualified.
- source: scripts/acceptance.sh; scripts/acceptance-task-tools.mjs;
  plugins/secretary-tools/src/task-tools.ts; plugins/secretary-tools/src/index.ts;
  services/secretary-core/src/main.rs;
  plugins/secretary-tools/test/task-tools.node.mjs;
  plugins/secretary-tools/test/native-authority.node.mjs;
  plugins/secretary-tools/test/task-native-core.node.mjs.
- updated_at: 2026-11-15

Evidence for the model-task claim: agent-i3hl29kj/verification.log;
agent-i3hl29kj/simulations/secretary-acceptance.lQIAP0/{sim.ndjson,task-client.log};
agent-i3hl29kj/acceptance.log; agent-i3hl29kj/source-check.json.
