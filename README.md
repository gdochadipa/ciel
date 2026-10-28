# Ciel

Ciel is a lightweight personal secretary for OpenClaw, using one OpenClaw agent, optional domain Skills, an owner-only TypeScript plugin and a loopback Rust Core. Registered Google Sheets grids and Google Docs are general capabilities, including bounded read-only Docs text search with an optional case-insensitive mode; the wedding workbook is an optional domain profile. Native OpenClaw owns reminders and personal memory. Controlled writes share one command bus, approval flow, journal and FSM.

Start with [architecture](docs/architecture.md), [interface notes](contracts/interface-notes.md), [workbook assessment](docs/spreadsheet-assessment.md) and [security boundaries](docs/security.md). The original [specification](docs/specs/openclaw-personal-secretary.md) defines the intended behavior. Progress and acceptance evidence live in [local tickets](.scratch/secretary-mvp/index.md).

See [current implementation status](docs/implementation-status.md) for what is coded, which checks actually passed, and why full acceptance remains open. This is not a production-ready or fully accepted implementation of the entire specification.

The full local verification now passes with actual dependency locks and the installed OpenClaw 2026.10.7 SDK: 57 Rust tests, 29 Python CLI checks and 46 TypeScript checks, strict SDK type checking, host build/validation/pack and seven real bridge/Core scenarios against simulated Google Sheets/Docs. See [the reproducible Docker setup](docs/local-verification.md). The verified runtime uses Node 24.22.1, Bun 1.2.21 and Rust 1.95.0; the SDK engine range is `>=24.22.1 <25 || >=26.12.1`. The full CI job also runs `bash scripts/acceptance-all.sh` with pinned Postman CLI and synthetic local targets; all seven cases pass sequentially locally. The prior hosted simulation step failed; further GitHub Actions and VPS work are deferred by the owner.

Local HTTP stand-ins and disposable fixtures demonstrate only their exercised behavior. They do not establish actual Google write preservation, installed OpenClaw SDK activation, trusted WhatsApp approvals, native restart persistence or channel delivery. [Deployment guidance](docs/deployment.md) keeps startup gated on an operator's version-specific host qualification. No production connection, workbook modification or deployment is implied by repository files.

Personal memory is delegated to OpenClaw by owner decision. The secretary plugin registers no memory tools or preference store; native memory lifecycle setup is outside plugin scope. The host's memory permissions and privacy policy remain operator responsibilities. Sources, ambiguous amounts, shared costs and unknown dates remain visible rather than being filled from conversation guesses. A payment record and its reminder have separate operation outcomes.

Before live activation, supply and qualify the actual VPS/runtime, approved Google spreadsheet/registry, service account, owner WhatsApp account and outbound recipient policy, trusted full diff approval, model entitlement, native scheduler delivery, and backup/restore. Keep `SECRETARY_WRITES_ENABLED=false` throughout initial qualification. The five guides under `skills/` grant no tools or additional authority. A low-effort review found two P2 defects; both have local regression fixes. Full runtime qualification remains open.


The Rust Core now uses domain/application/infrastructure/presentation layers with a typed in-process command bus and an explicit mutation FSM. See [the architecture](docs/architecture.md) for dependencies, legal transitions and preserved contracts.

Use [the generic workspace example](config/workspace.example.json) for registered grids and Docs. The first Docs edit capabilities are approved append text and one-match text replacement on a selected supported tab. [Capability notes](docs/specs/workspace-capabilities.md) document bounds, compatibility and live qualification still required.

T025 adds preview/approval-based tab creation inside registered spreadsheets (`create_tabs` defaults false). New tabs remain unregistered for reads/writes until the operator explicitly configures bounded access. No new spreadsheet files are created; see [Workspace capabilities](docs/specs/workspace-capabilities.md).

For the owner-selected GitHub → VPS checkout → installer flow, see [installation](docs/installation.md). The CLI stages a reviewed native release and installs inactive files separately; it never starts services, activates plugins or changes production qualification flags.

Licensed under the [MIT License](LICENSE).

Offline registry preparation (T030): the current Core supports registry init/check before runtime setup, with inactive non-overwriting drafts and sanitized validation summaries from the same Rust rules. See [registry setup](docs/registry-setup.md) for commands and required source-version checks. No Google credential/access or service activation is involved.
