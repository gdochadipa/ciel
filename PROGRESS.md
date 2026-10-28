# OpenClaw Secretary — Progress and continuation

Updated: 2026-10-28 · Asia/Makassar

## Current position

[T025](.scratch/secretary-mvp/T025.md) implements bounded tab creation inside an existing registered spreadsheet through the shared preview/approval/command bus/FSM/journal. It adds `sheets_preview_create_tab` and `sheets_create_tab`, bringing the plugin to 25 tools. Creation requires explicit `create_tabs` permission (default false) and global writes. The resulting tab stays unregistered/readable=false/writable=false until separate operator configuration. T023 generic grid reads and controlled Docs edits remain supported; wedding profiles are optional and personal memory belongs to OpenClaw.

The owner retained a lightweight single in-process command bus + durable SQLite operation FSM. No CQRS buses, broker, replicated read store, projection worker or extra daemon. [ADR0001](docs/adr/0001-lightweight-command-bus-and-operation-fsm.md) and [ADR0002](docs/adr/0002-workspace-capabilities-and-optional-domain-profiles.md) record these decisions. Docs shares the existing owner/approval/journal/coordinator; legacy Sheets preview JSON and request digests remain compatible.

**Live activation remains T007 and is not accepted.** `approvalTransportQualified` and `schedulerQualified` remain false; production writes remain disabled. No deployment, live Google mutation, real native job or WhatsApp send occurred. No additional paid service, billing activation or quota upgrade is authorized.

## Current verification evidence

[T024](.scratch/secretary-mvp/T024.md) closes an operator-gate gap after T023: any readable Docs registration requires separate `google_disposable_docs` evidence, even when writes are disabled. Missing/blank/non-string evidence and malformed flags fail closed. Sheets-only/unreadable Docs registries preserve the old contract. Thirteen operator CLI tests pass on macOS and qualified Linux. This checks private local attestations, not live provider behavior; T007 remains open.

- T023 baseline `bash scripts/verify.sh` passed: 30 Rust tests, 41 TypeScript checks (37 plugin + four acceptance configuration, all four HTTP tests enabled), ten Python operator checks, formatting/Clippy with warnings denied and actual SDK strict types/build/validation (`valid: true`, no errors).
- All seven Google-only dependency simulations passed sequentially: read, writes, retry, drift, error, slow and workspace. Real bridge/Core/journal executed; workspace served both Sheets and Docs (three Sheets and 31 Docs request events). SDK-built entry factories/approval hooks execute through a host registration fixture, not a qualified live host UI.
- Docs coverage includes explicit tab targeting, pagination/source evidence, full captured preview, separate approval, exact replacement, append replay without another send, external revision conflict, other-tab preservation, accepted-write/lost-response uncertainty and observation-only reconciliation. These are bounded synthetic-provider results, not live Google semantics.
- Six workspace Rust tests passed again after adding a known legacy Sheets digest assertion. T022's explicit last-Journal-owner unlock and clone-exclusion regressions remain included. T018's real Core WAL backup/restore and ten operator checks remain included.
- Previous T023 SDK pack produced a three-runtime-file archive, 36400 bytes, SHA-256 `ce65b2245846abd2491aebe785ecc9350c1b2d308bc0919963aab6d5d24b1d4d`, at ignored `plugins/secretary-tools/dist/secretary-tools.tgz`. No activation receipt/install/reload/publish was executed.

Qualified runtime: Linux arm64, Node 24.22.1, Bun 1.2.21, Rust 1.95.0, OpenClaw 2026.10.7, TypeScript 7.0.2 and Postman CLI 1.75.0. The actual SDK requires Node `>=24.22.1 <25 || >=26.12.1`. Actual Cargo/Bun locks and full production dependencies are retained; never substitute SDK declarations or dependency graphs. Hosted CI is configured for the seven-scenario gate but remains unobserved without a remote/runner.

## Read these first

- [AGENTS.md](AGENTS.md): persistent project decisions and agent instructions.
- [Implementation status](docs/implementation-status.md), [Workspace capability bounds](docs/specs/workspace-capabilities.md), [plugin README](plugins/secretary-tools/README.md).
- [Local verification](docs/local-verification.md), [local ticket graph](.scratch/secretary-mvp/index.md), [T007 live checklist](.scratch/secretary-mvp/T007.md).
- [Architecture](docs/architecture.md), [specification](docs/specs/openclaw-personal-secretary.md), [security](docs/security.md), [deployment](docs/deployment.md), [Core contract](contracts/openapi.yaml).

## Operator-reported Google evidence

The owner identified `/home/ubuntu/test_sheets.py`, whose AST shows service-account loading/refresh through `google-auth`. The existing credential path is `/home/ubuntu/.config/openclaw-memory/service-account.json`. Operator-reported results: `AUTH_REFRESH: OK`, `SHEETS_READ: OK`, metadata HTTP 200 with five tab IDs/titles, credential file mode 0600. The checks used Sheets read-only scope and printed no token/private key/cells. Core already supports this auth family; do not add user OAuth or request a replacement key solely because Python uses `google-auth`.

The real target ID/path-only evidence is retained privately at `/private/tmp/secretary-live-readonly-evidence.json` (mode 0600), never Git; target SHA-256 prefix `26cc56536755`. Existing tabs are Sheet1–Sheet4 and RUMAH, without AI_Tasks/AI_Payments. This target is an example, not the universal schema or a confirmed disposable write resource. No wedding-only private registry was generated. Select intended resources/bounded ranges rather than forcing this workbook to match a template.

This is operator-reported Python evidence, not agent remote execution or a live Rust Core pass. Host/connection and effective service-user access, API/ACL/scopes, standard quota settings and private Core registry/wiring still need verification. No live Docs ID has been supplied; Sheets access does not prove Docs access. Neither the remote credential nor virtualenv is present locally; no secret was copied. Existing hosting/model costs are separate; no new paid services are allowed.

## T025 — tab creation implemented locally

The owner-requested creation of tabs inside registered spreadsheets is implemented; no new spreadsheet file endpoint exists. Preview binds target spreadsheet, generated 31-bit sheet ID, exact title, bounded dimensions and optional literal headers. Bounds: 1–100 title characters, 1–1000 rows, 1–26 columns, <=10000 cells; <=256 UTF-8 bytes per header. Explicit `create_tabs` defaults false and is independent of structured-row permissions. Result stays unregistered and receives no automatic view/read/write grant.

One Google batch contains addSheet and optional row-zero updateCells using stringValue. Registered and current-provider title/ID collisions fail closed; commit rechecks collisions before one send. Replay uses persisted intent/state. Accepted-write/lost-response uncertainty survives journal reopen and reconciles by observing exact metadata/header state, without resending. See [Workspace capabilities](docs/specs/workspace-capabilities.md) and T025 for bounds, concurrent-editor limits and operator registration policy.

Local evidence: four new public HTTP/Core/real Reqwest tests and two plugin approval tests; real SDK type/build/validation; Workspace Postman acceptance with 22 Sheets and 31 Docs request events, including exactly three intended tab-create sends (ordinary, lost response, built-entry fixture), no duplicates. Both Google members served traffic; Core, bridge and SQLite journal ran for real. The actual SDK registers 25 tools and exercises the new factory/approval hook through the host registry fixture. Full `bash scripts/verify.sh` passed in the qualified network-disabled Linux container: 34 Rust tests, 43 TypeScript checks (39 plugin + four acceptance configuration; all HTTP checks enabled), 13 Python operator checks, formatting/Clippy with warnings denied and actual SDK strict type/build/validation (`valid: true`, no errors). All seven `scripts/acceptance-all.sh` scenarios passed sequentially on T025 (read, writes, retry, drift, error, slow, workspace). Low-effort review of the single T025 commit: Standards found no material issues; Spec identified the missing new-tool host example allowlist entries (P2), now fixed with activation flags still off. Expiry generation was moved to application to preserve a deterministic domain. Both axes rechecked the amended code commit against `f375476` and found no remaining material issues. Post-fix formatting, Clippy with warnings denied and all four tab HTTP tests passed; a JSON check confirms all 25 tools are allowed by the example host policy. This does not qualify live Google or the host UI.

## T007 preparation continued locally

The T025 runtime was rebuilt and packed with the actual SDK: 25 tools, three regular runtime files, 36977 bytes, SHA-256 `cc4d95f67207ba456589f2c79961ea07738a9366ee74eac1435f21b9d7b46f44`. The new archive is retained privately at `/private/tmp/secretary-qualified-3xjcv7r_/runs/secretary-tools-58ecb33.tgz`; receipt is in `runs/secretary-t007-local/`. The previous archive was preserved. No install/activation/reload/publish occurred. The actual OpenClaw config schema validator accepted the committed host example (`valid: true`, no warnings) with isolated state and no Gateway; this does not qualify effective host policies.

[Native recovery assessment](docs/native-recovery-assessment.md) identifies supported pinned native archive create/verify/staged-restore commands. A real local synthetic config/opaque-file archive exercise passed, preserving bytes and rejecting nonempty/live-state subtree targets. Its native SQLite inventory was empty: scheduler/native-database/full-host recovery remains open. Native backup sanitizes delivery queue work and ownership/idempotency receipts; restoring does not establish safe automatic delivery continuation.

The owner supplied a session SSH target for read-only inventory and requires dynamic operator selection. No target IP/user/key is written into source or runtime configuration. Plugin → Core stays loopback on the installed VPS (`coreUrl` configurable); SSH is only operator transport. The agent's SSH attempt was blocked locally (`Operation not permitted`) before authentication, so no host command ran. Git remote/hosted runner is still absent. T007 remains open; no live capability flag changed. Low-effort review of this documentation/evidence commit passed both Standards and Spec with no material findings. No runtime code changed; verification used the actual CLI pack/schema/archive exercises above plus local-link/JSON/whitespace and artifact-content checks, rather than rerunning unchanged application suites.

## Remaining live gates

1. Qualify the actual VPS, pinned plugin activation (25 tools), model entitlement and disposable Sheets/Docs scopes/API/ACL/read/write preservation. Docs qualification must include explicit tabs, required revisions and reconciliation before activation.
2. Prove trusted owner-only approval routing, complete preview rendering, denial/expiry/replay and callback-error blocking in the real host. The SDK fixture does not establish host policy or requester provenance.
3. Qualify native Gateway declaration convergence/concurrent edits, restart persistence, history and owner-only WhatsApp delivery/outbound policy. Offset discovery is not transactional.
4. Test supported native scheduler backup/restore and complete target-host recovery. Core journal recovery already passes locally.
5. Verify native OpenClaw memory privacy/permissions separately. Markdown is native note source; OpenClaw owns its SQLite index, JSON is configuration/interchange and Core SQLite is operation state. FTS-only retrieval and three primary-memory results are defaults, not measured savings or a hard token ceiling.
6. Observe hosted CI when a remote/runner is configured.

Generic Sheets grids are read-only. Docs editing currently covers plain-text append and one exact case-sensitive replacement in a selected supported tab. New tab creation is T025; new spreadsheet/Docs file creation, rich formatting and arbitrary batch requests remain unsupported. Unsupported structures/suggestions fail closed for editing; see the capability spec for actual limits.

## Workspace and continuation

Working source: `/Users/ochadipa/my_project/openclaw_project/personal_ai_oc`. Actual history: `/private/tmp/openclaw-secretary-implementation.on4Zt7`, branch `integration/secretary-mvp`; source root has no usable Git metadata. Tracked-file hashes: `/private/tmp/openclaw-secretary-sync.json`. Preserve both before syncing; do not initialize replacement history.

Qualified disposable environment: `/private/tmp/secretary-qualified-3xjcv7r_` (project/cache/home/runs), image `secretary-verification:2026.10.7`. Mount only sanitized files, not host home, credentials or workbook. Native macOS Node/DNS/socket limits persist. Raw logs/generated packages are local artifacts, not tracked source.

Workbook `docs/Persiapan Nikah.xlsx` remains unchanged: SHA-256 `b24351dbda4c69a7cbf5f14d9b18d7fbbc983deff406e8dad3607f6bcd196de3`.

Continue from material unresolved gates using target-specific evidence. Do not invent custom memory work or treat SDK package validation as live activation. Current local capability implementation includes T025; target-host qualification remains T007.
