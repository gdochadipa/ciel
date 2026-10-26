# OpenClaw Personal Secretary

A local implementation of a personal secretary using one OpenClaw agent, five bounded domain Skills, a TypeScript bridge and a loopback Rust Core. Google Sheets owns planning records; OpenClaw's native scheduler owns reminders. Existing workbook tabs remain read-only initially.

Start with [architecture](docs/architecture.md), [interface notes](contracts/interface-notes.md), [workbook assessment](docs/spreadsheet-assessment.md) and [security boundaries](docs/security.md). The original [specification](docs/specs/openclaw-personal-secretary.md) defines the intended behavior. Progress and acceptance evidence live in [local tickets](.scratch/secretary-mvp/index.md).

See [current implementation status](docs/implementation-status.md) for what is coded, which checks actually passed, and why full acceptance remains open. This is not a production-ready or fully accepted implementation of the entire specification.

Run `bash scripts/verify.sh` once both build manifests are present. It checks local builds, actual host plugin validation and tests (including loopback HTTP checks); no production secret is needed for local fixtures. Rust checks use cached dependencies offline. Missing required crates (including Reqwest when unavailable) remain explicit dependency blockers. The repository's CI definition is not evidence that hosted CI has passed.

Dependency resolution is currently blocked by shell DNS restrictions. Production `Cargo.lock` and Bun lockfiles have not been fabricated. CI deliberately stops until both locks are generated, validated, and committed in a network-enabled environment (`cargo generate-lockfile` and `bun install`), then uses locked installs. Installed OpenClaw and Node 24.16+ are required for the full plugin packaging/activation checks. The adapter-only Rust harness and `bun run test` provide narrower local checks and do not replace those gates.

Local HTTP stand-ins and disposable fixtures demonstrate only their exercised behavior. They do not establish actual Google write preservation, installed OpenClaw SDK activation, trusted WhatsApp approvals, native restart persistence or channel delivery. [Deployment guidance](docs/deployment.md) keeps startup gated on an operator's version-specific host qualification. No production connection, workbook modification or deployment is implied by repository files.

Personal memory is delegated to OpenClaw by owner decision. The secretary plugin registers no memory tools or preference store; native memory lifecycle setup is outside plugin scope. The host's memory permissions and privacy policy remain operator responsibilities. Sources, ambiguous amounts, shared costs and unknown dates remain visible rather than being filled from conversation guesses. A payment record and its reminder have separate operation outcomes.

Before live activation, supply and qualify the actual VPS/runtime, approved Google spreadsheet/registry, service account, owner WhatsApp account and outbound recipient policy, trusted full diff approval, model entitlement, native scheduler delivery, and backup/restore. Keep `SECRETARY_WRITES_ENABLED=false` throughout initial qualification. The five guides under `skills/` grant no tools or additional authority. A low-effort review found two P2 defects; both have local regression fixes. Full runtime qualification remains open.


The Rust Core now uses domain/application/infrastructure/presentation layers with a typed in-process command bus and an explicit mutation FSM. See [the architecture](docs/architecture.md) for dependencies, legal transitions and preserved contracts.
