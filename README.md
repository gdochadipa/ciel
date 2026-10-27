# OpenClaw Personal Secretary

A local implementation of a personal secretary using one OpenClaw agent, five bounded domain Skills, a TypeScript bridge and a loopback Rust Core. Google Sheets owns planning records; OpenClaw's native scheduler owns reminders. Existing workbook tabs remain read-only initially.

Start with [architecture](docs/architecture.md), [interface notes](contracts/interface-notes.md), [workbook assessment](docs/spreadsheet-assessment.md) and [security boundaries](docs/security.md). The original [specification](docs/specs/openclaw-personal-secretary.md) defines the intended behavior. Progress and acceptance evidence live in [local tickets](.scratch/secretary-mvp/index.md).

See [current implementation status](docs/implementation-status.md) for what is coded, which checks actually passed, and why full acceptance remains open. This is not a production-ready or fully accepted implementation of the entire specification.

The full local verification now passes with actual dependency locks and the installed OpenClaw 2026.10.7 SDK: 24 Rust tests and ten Python operator checks, plus the current evidence of 37 TypeScript checks, strict SDK type checking, host build/validation/pack and six real bridge/Core scenarios against simulated Google. See [the reproducible Docker setup](docs/local-verification.md). The verified runtime uses Node 24.22.1, Bun 1.2.21 and Rust 1.95.0; the SDK engine range is `>=24.22.1 <25 || >=26.12.1`. The full CI job also runs `bash scripts/acceptance-all.sh` with pinned Postman CLI and synthetic local targets; all six cases pass sequentially locally. Hosted CI remains unexecuted.

Local HTTP stand-ins and disposable fixtures demonstrate only their exercised behavior. They do not establish actual Google write preservation, installed OpenClaw SDK activation, trusted WhatsApp approvals, native restart persistence or channel delivery. [Deployment guidance](docs/deployment.md) keeps startup gated on an operator's version-specific host qualification. No production connection, workbook modification or deployment is implied by repository files.

Personal memory is delegated to OpenClaw by owner decision. The secretary plugin registers no memory tools or preference store; native memory lifecycle setup is outside plugin scope. The host's memory permissions and privacy policy remain operator responsibilities. Sources, ambiguous amounts, shared costs and unknown dates remain visible rather than being filled from conversation guesses. A payment record and its reminder have separate operation outcomes.

Before live activation, supply and qualify the actual VPS/runtime, approved Google spreadsheet/registry, service account, owner WhatsApp account and outbound recipient policy, trusted full diff approval, model entitlement, native scheduler delivery, and backup/restore. Keep `SECRETARY_WRITES_ENABLED=false` throughout initial qualification. The five guides under `skills/` grant no tools or additional authority. A low-effort review found two P2 defects; both have local regression fixes. Full runtime qualification remains open.


The Rust Core now uses domain/application/infrastructure/presentation layers with a typed in-process command bus and an explicit mutation FSM. See [the architecture](docs/architecture.md) for dependencies, legal transitions and preserved contracts.
