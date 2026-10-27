# OpenClaw Secretary — Progress and continuation

Updated: 2026-10-28 · Asia/Makassar

## Current position

[T023](.scratch/secretary-mvp/T023.md) is implemented and accepted locally: generic registered Google Sheets grid reads and registered Google Docs read/preview/controlled append/exact replacement. Wedding tables and reports remain optional domain profiles, not the universal template. The plugin now registers 23 tools. Personal memory belongs to OpenClaw; no second personal-memory store is introduced.

The owner retained a lightweight single in-process command bus + durable SQLite operation FSM. No CQRS buses, broker, replicated read store, projection worker or extra daemon. [ADR0001](docs/adr/0001-lightweight-command-bus-and-operation-fsm.md) and [ADR0002](docs/adr/0002-workspace-capabilities-and-optional-domain-profiles.md) record these decisions. Docs shares the existing owner/approval/journal/coordinator; legacy Sheets preview JSON and request digests remain compatible.

**Live activation remains T007 and is not accepted.** `approvalTransportQualified` and `schedulerQualified` remain false; production writes remain disabled. No deployment, live Google mutation, real native job or WhatsApp send occurred. No additional paid service, billing activation or quota upgrade is authorized.

## Current verification evidence

[T024](.scratch/secretary-mvp/T024.md) closes an operator-gate gap after T023: any readable Docs registration requires separate `google_disposable_docs` evidence, even when writes are disabled. Missing/blank/non-string evidence and malformed flags fail closed. Sheets-only/unreadable Docs registries preserve the old contract. Thirteen operator CLI tests pass on macOS and qualified Linux. This checks private local attestations, not live provider behavior; T007 remains open.

- Full `bash scripts/verify.sh` passed: 30 Rust tests, 41 TypeScript checks (37 plugin + four acceptance configuration, all four HTTP tests enabled), ten Python operator checks, formatting/Clippy with warnings denied and actual SDK strict types/build/validation (`valid: true`, no errors).
- All seven Google-only dependency simulations passed sequentially: read, writes, retry, drift, error, slow and workspace. Real bridge/Core/journal executed; workspace served both Sheets and Docs (three Sheets and 31 Docs request events). SDK-built entry factories/approval hooks execute through a host registration fixture, not a qualified live host UI.
- Docs coverage includes explicit tab targeting, pagination/source evidence, full captured preview, separate approval, exact replacement, append replay without another send, external revision conflict, other-tab preservation, accepted-write/lost-response uncertainty and observation-only reconciliation. These are bounded synthetic-provider results, not live Google semantics.
- Six workspace Rust tests passed again after adding a known legacy Sheets digest assertion. T022's explicit last-Journal-owner unlock and clone-exclusion regressions remain included. T018's real Core WAL backup/restore and ten operator checks remain included.
- Actual SDK pack produced a three-runtime-file archive, 36400 bytes, SHA-256 `ce65b2245846abd2491aebe785ecc9350c1b2d308bc0919963aab6d5d24b1d4d`, at ignored `plugins/secretary-tools/dist/secretary-tools.tgz`. No activation receipt/install/reload/publish was executed.

Qualified runtime: Linux arm64, Node 24.22.1, Bun 1.2.21, Rust 1.95.0, OpenClaw 2026.10.7, TypeScript 7.0.2 and Postman CLI 1.75.0. The actual SDK requires Node `>=24.22.1 <25 || >=26.12.1`. Actual Cargo/Bun locks and full production dependencies are retained; never substitute SDK declarations or dependency graphs. Hosted CI is configured for the seven-scenario gate but remains unobserved without a remote/runner.

## Read these first

- [Implementation status](docs/implementation-status.md), [Workspace capability bounds](docs/specs/workspace-capabilities.md), [plugin README](plugins/secretary-tools/README.md).
- [Local verification](docs/local-verification.md), [local ticket graph](.scratch/secretary-mvp/index.md), [T007 live checklist](.scratch/secretary-mvp/T007.md).
- [Architecture](docs/architecture.md), [specification](docs/specs/openclaw-personal-secretary.md), [security](docs/security.md), [deployment](docs/deployment.md), [Core contract](contracts/openapi.yaml).

## Operator-reported Google evidence

The owner identified `/home/ubuntu/test_sheets.py`, whose AST shows service-account loading/refresh through `google-auth`. The existing credential path is `/home/ubuntu/.config/openclaw-memory/service-account.json`. Operator-reported results: `AUTH_REFRESH: OK`, `SHEETS_READ: OK`, metadata HTTP 200 with five tab IDs/titles, credential file mode 0600. The checks used Sheets read-only scope and printed no token/private key/cells. Core already supports this auth family; do not add user OAuth or request a replacement key solely because Python uses `google-auth`.

The real target ID/path-only evidence is retained privately at `/private/tmp/secretary-live-readonly-evidence.json` (mode 0600), never Git; target SHA-256 prefix `26cc56536755`. Existing tabs are Sheet1–Sheet4 and RUMAH, without AI_Tasks/AI_Payments. This target is an example, not the universal schema or a confirmed disposable write resource. No wedding-only private registry was generated. Select intended resources/bounded ranges rather than forcing this workbook to match a template.

This is operator-reported Python evidence, not agent remote execution or a live Rust Core pass. Host/connection and effective service-user access, API/ACL/scopes, standard quota settings and private Core registry/wiring still need verification. No live Docs ID has been supplied; Sheets access does not prove Docs access. Neither the remote credential nor virtualenv is present locally; no secret was copied. Existing hosting/model costs are separate; no new paid services are allowed.

## Next requested capability — create a tab in an existing spreadsheet

Owner discussion 2026-10-28: support creating a new sheet/tab inside an already registered Google spreadsheet. Status: recorded for implementation; not implemented or tested yet. This is distinct from creating a new spreadsheet file.

Use the existing command bus, immutable preview, owner approval, durable journal and FSM. The preview should identify the target spreadsheet, requested tab title, bounded initial dimensions and optional headers. Implementation must handle title/ID collisions, replay and uncertain provider outcomes without blindly creating duplicate tabs. Define explicit registration/read/write permissions for the resulting tab; creating a tab must not automatically grant unrestricted access. Google supports tab creation through [AddSheetRequest](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/request#AddSheetRequest). No live tab creation is authorized by this progress entry.

## Remaining live gates

1. Qualify the actual VPS, pinned plugin activation (23 tools), model entitlement and disposable Sheets/Docs scopes/API/ACL/read/write preservation. Docs qualification must include explicit tabs, required revisions and reconciliation before activation.
2. Prove trusted owner-only approval routing, complete preview rendering, denial/expiry/replay and callback-error blocking in the real host. The SDK fixture does not establish host policy or requester provenance.
3. Qualify native Gateway declaration convergence/concurrent edits, restart persistence, history and owner-only WhatsApp delivery/outbound policy. Offset discovery is not transactional.
4. Test supported native scheduler backup/restore and complete target-host recovery. Core journal recovery already passes locally.
5. Verify native OpenClaw memory privacy/permissions separately. Markdown is native note source; OpenClaw owns its SQLite index, JSON is configuration/interchange and Core SQLite is operation state. FTS-only retrieval and three primary-memory results are defaults, not measured savings or a hard token ceiling.
6. Observe hosted CI when a remote/runner is configured.

Generic Sheets grids are read-only. Docs editing currently covers plain-text append and one exact case-sensitive replacement in a selected supported tab. Creation, rich formatting and arbitrary batch requests are outside T023. Unsupported structures/suggestions fail closed for editing; see the capability spec for actual limits.

## Workspace and continuation

Working source: `/Users/ochadipa/my_project/openclaw_project/personal_ai_oc`. Actual history: `/private/tmp/openclaw-secretary-implementation.on4Zt7`, branch `integration/secretary-mvp`; source root has no usable Git metadata. Tracked-file hashes: `/private/tmp/openclaw-secretary-sync.json`. Preserve both before syncing; do not initialize replacement history.

Qualified disposable environment: `/private/tmp/secretary-qualified-3xjcv7r_` (project/cache/home/runs), image `secretary-verification:2026.10.7`. Mount only sanitized files, not host home, credentials or workbook. Native macOS Node/DNS/socket limits persist. Raw logs/generated packages are local artifacts, not tracked source.

Workbook `docs/Persiapan Nikah.xlsx` remains unchanged: SHA-256 `b24351dbda4c69a7cbf5f14d9b18d7fbbc983deff406e8dad3607f6bcd196de3`.

Continue from material unresolved gates using target-specific evidence. Do not invent custom memory work or treat SDK package validation as live activation. Current local implementation is complete for T023; target-host qualification remains T007.
