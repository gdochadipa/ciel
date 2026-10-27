# Implementation status

Updated 2026-10-27. The local plugin/Core verification milestone is complete; T018 adds tested operator gates and Core journal recovery; T019 adds built-entry factory/hook integration; T020 bounds transient preview state; T021 adds a sequential acceptance CI gate and reliable launcher cleanup. **Live activation remains unqualified (T007)**; this is not a production acceptance claim.

## Implemented behavior

The owner explicitly retained FSM + one in-process command bus for lightweight operation. [ADR0001](adr/0001-lightweight-command-bus-and-operation-fsm.md) records the decision. Read and write handlers share dispatch; there is no separate query bus, event broker, replicated read store, projection worker or event-sourced model.

The plugin exposes 17 spreadsheet, report and reminder tools. Rust Core retains domain/application/infrastructure/presentation layers, typed command dispatch and a durable SQLite operation FSM. Transient preview state is limited to 64 entries and 4 MiB serialized JSON per host API, pruned on preview/approval activity without background timers. Evicted previews need fetching again; already-open dialogs retain their immutable snapshot. Core operation history is unaffected. Controlled writes use immutable previews, separate operator approval, owner/digest/action binding, single-use commits, conflict checks and explicit reconciliation after uncertain provider outcomes. Reminder operations use OpenClaw's native Gateway and retain canceled/completed jobs for history.

Personal memory is owned by OpenClaw. The plugin has no memory tools or preference store. The host template requests FTS-only retrieval and three primary-memory results; these are resource-conscious defaults, not measured savings or a hard token cap. Core's SQLite journal stores operation state, not personal memory.

## Executed local evidence

All checks below used synthetic data in isolated Linux. Actual dependency locks were generated and frozen installs validated. Runtime: OpenClaw 2026.10.7, Node 24.22.1, Bun 1.2.21, Rust 1.95.0, TypeScript 7.0.2 and Postman CLI 1.75.0.

| Check | Result and boundary |
| --- | --- |
| Full Rust dependency graph | Formatting, Clippy with warnings denied and 24 tests passed offline/locked, including the real Reqwest HTTP adapter and accepted-write/lost-response reconciliation. Google OAuth/JWT code compiled; live credential exchange through Rust Core remains unverified. The owner separately reported successful Python service-account refresh (see below). |
| Operator CLI and restore | Ten Python checks pass on native macOS and qualified Linux. Actual Core WAL journal backup/restore preserves approval/digest, recovers interrupted writes to reconciliation and rejects replay. Temporary backup sidecars are cleaned; existing snapshots remain untouched (T018). |
| TypeScript checks | 34 plugin tests plus three acceptance-configuration tests passed, with all four HTTP tests enabled and no skips. Native Gateway behavior remains a boundary fixture. |
| Actual SDK | Strict type checking against the installed public SDK passed. OpenClaw build and validation passed (`valid: true`, no errors); metadata import requires no credentials. |
| Built entry integration | T019/T020: actual SDK recognizes bundle metadata and all 17 factory registrations match the manifest. Exercised tool factories and approval hook execute through a host registration fixture. Preview/hook sharing, API-instance isolation, owner/revocation guards and one-shot approval → real Core → one synthetic Google append passed (T019). Live host UI/policy remains unqualified. |
| Distribution | Actual host pack produced a local archive with three runtime files: package metadata, manifest and bundled entry. npm dry-run separately verifies its four-file selection, including README, and private-file exclusion. No install/enable/reload/publish occurred. |
| Full verification script | `bash scripts/verify.sh` exited successfully on the current T022 baseline with actual locks: 24 Rust, 37 TypeScript and ten Python checks, formatting/Clippy and SDK type/build/validation. CI now also installs pinned Postman CLI and runs the fail-fast six-scenario suite without secrets/login/publication. Hosted CI has not run. |
| Real bridge → Rust Core → simulated Google | All six Postman scenarios passed together sequentially in T021: read, writes (including built-entry factory/hook exercise), retry, drift, persistent error and slow response. Core and its journal ran for real; Google was the sole simulated dependency. |

The write scenario exercised approval callback replay, append/update, owner/bearer/digest rejection, changed payload, stale external edits, uncertain-write rejection and reconciliation to exactly one synthetic payment. Retry served two failures before recovery. Drift failed closed. Injected error/latency conditions short-circuit before Google member request events; caller error/timeout assertions establish those outcomes. This is bounded Google contract evidence, not live Google semantics or host approval UI qualification.

The real SDK requires Node `>=24.22.1 <25 || >=26.12.1`; the earlier 24.16 assumption was corrected. `defineToolPlugin` now supplies actual host authoring metadata and context factories, preserving the native approval hook. Bridge construction and credential access are lazy. See [T017](../.scratch/secretary-mvp/T017.md) for fixes and run evidence and [local verification](local-verification.md) for reproduction.

The T022 full baseline exposed and fixed a journal lock handoff race: explicit RAII unlock on last Journal owner prevents inherited descriptors from prolonging ownership. A live Journal clone still excludes another owner. Two deterministic regressions and 50 iterations of the original journal suite passed, with no retry/sleep workaround. The full write/SDK-entry scenario also passed again with the updated Core.

On 2026-10-27 the owner reported `AUTH_REFRESH: OK` from a Python check using the existing host service-account file, the Google token endpoint and Sheets read-only scope. This is operator-reported live auth evidence, not a Core integration or spreadsheet-access pass. An approved spreadsheet ID/link has been requested for metadata-only validation; Core wiring, host permissions, Sheets ACL/API enablement and remaining live gates are still open. No credentials or tokens were obtained/committed and no live cells were changed.

## Remaining acceptance gates

- Qualify the actual VPS, pinned OpenClaw activation and model entitlement.
- Exercise disposable live Sheets with restricted service-account OAuth and verify preservation against real provider semantics.
- Prove owner-only approval routing, complete preview rendering without truncation, denial/expiry/replay and callback-error blocking in the real host.
- Prove native scheduler persistence after restart, concurrent-edit/declaration convergence, run history and owner-only WhatsApp delivery/outbound policy. Offset discovery is not a transactional snapshot.
- Exercise a supported native scheduler backup/restore and complete target-host recovery on disposable resources. Core journal snapshot/restore now passes locally; native scheduler/full-host recovery is still open.
- Verify native memory privacy/permissions separately. Custom plugin memory lifecycle is outside scope.
- Run hosted CI when a remote/runner is configured. No comprehensive new security audit is claimed.

`approvalTransportQualified` and `schedulerQualified` remain false; production writes remain disabled. No production configuration, live Google record, native job or WhatsApp message was changed. Local build success does not authorize deployment.

## Preserved source

The workbook `docs/Persiapan Nikah.xlsx` is unchanged and excluded from Git. SHA-256: `b24351dbda4c69a7cbf5f14d9b18d7fbbc983deff406e8dad3607f6bcd196de3`.

The native macOS shell still lacks the qualified Node/SDK environment and permits neither registry DNS nor loopback bind. The qualified Docker workflow resolves those local verification blockers without changing the user profile. History is retained in `/private/tmp/openclaw-secretary-implementation.on4Zt7`, branch `integration/secretary-mvp`; source is synchronized with the workspace. See [PROGRESS.md](../PROGRESS.md).
