# OpenClaw Personal Secretary

A local implementation of a personal secretary using one OpenClaw agent, five bounded domain Skills, a TypeScript bridge and a loopback Rust Core. Google Sheets owns planning records; OpenClaw's native scheduler owns reminders. Existing workbook tabs remain read-only initially.

Start with [architecture](docs/architecture.md), [interface notes](contracts/interface-notes.md), [workbook assessment](docs/spreadsheet-assessment.md) and [security boundaries](docs/security.md). The original [specification](docs/specs/openclaw-personal-secretary.md) defines the intended behavior. Progress and acceptance evidence live in [local tickets](.scratch/secretary-mvp/index.md).

Run `bash scripts/verify.sh` once both build manifests are present. It checks local builds and tests; no production secret is needed for local fixtures. Rust checks use cached dependencies offline. Missing required crates (including Reqwest when unavailable) remain explicit dependency blockers. The repository's CI definition is not evidence that hosted CI has passed.

Local HTTP stand-ins and disposable fixtures demonstrate only their exercised behavior. They do not establish actual Google write preservation, installed OpenClaw SDK activation, trusted WhatsApp approvals, native restart persistence or channel delivery. [Deployment guidance](docs/deployment.md) keeps startup gated on an operator's version-specific host qualification. No production connection, workbook modification or deployment is implied by repository files.

Native memory save/update/archive/history and prevention of automatic saving remain unresolved; memory mutation must stay unavailable until qualified. Sources, ambiguous amounts, shared costs and unknown dates remain visible rather than being filled from conversation guesses. A payment record and its reminder have separate operation outcomes.

Before live activation, supply and qualify the actual VPS/runtime, approved Google spreadsheet/registry, service account, owner WhatsApp account and outbound recipient policy, trusted full diff approval, model entitlement, native scheduler delivery, and backup/restore. Keep `SECRETARY_WRITES_ENABLED=false` throughout initial qualification. The five guides under `skills/` grant no tools or additional authority. Code review is explicitly deferred by the user; no review completion is claimed.
