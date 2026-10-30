# OpenClaw Secretary project instructions

Read [PROGRESS.md](PROGRESS.md) before continuing work. It records current implementation status, pending features, verification evidence and the workspace/integration Git arrangement. Keep changing status there; keep durable project decisions here.

## Architecture and ownership

- Keep one Rust Core process, one in-process command bus and a durable operation FSM backed by a SQLite operation journal. Preserve the domain/application/infrastructure/presentation boundaries. Do not add separate CQRS buses, brokers, projection workers or extra daemons without an explicit revised architecture decision.
- OpenClaw owns personal memory. The plugin's SQLite journal stores operation and approval state, not personal notes or a second knowledge store.
- The owner authorizes short initial per-Doc discovery summaries in native OpenClaw memory with seven-day logical expiry. After confirmed edits through OpenClaw, reread source and refresh the same note; uncertain outcomes keep it stale/unverified. Follow [summary memory](docs/docs-summary-memory.md). Native storage/expiry remains host-qualified; do not claim a successful save, enforced deletion or token savings from source guidance alone.
- Support general registered Workspace resources. The wedding spreadsheet is an optional domain profile, not a universal schema or template.
- Reuse immutable previews, owner approval, digest binding, the shared journal and FSM for new mutations. Handle replay and uncertain outcomes without blindly resending provider writes.

For architecture changes, read [ADR0001](docs/adr/0001-lightweight-command-bus-and-operation-fsm.md) and [ADR0002](docs/adr/0002-workspace-capabilities-and-optional-domain-profiles.md). For Sheets/Docs capability changes, read [Workspace capabilities](docs/specs/workspace-capabilities.md).

## Operational boundaries

- Keep work local unless the user explicitly requests target-specific live actions. Deployment, live Google mutations, real native jobs and WhatsApp sends require that authorization and the applicable qualification gates. General requests to continue development do not authorize those actions.
- Keep production writes and approval/scheduler qualification flags disabled until the corresponding live evidence and explicit activation authorization exist. Follow [T007](.scratch/secretary-mvp/T007.md) and [deployment guidance](docs/deployment.md) for live qualification.
- Avoid additional paid services, billing activation and paid quota upgrades unless the user explicitly revises the cost constraint.
- Keep credentials, tokens, private target IDs, personal workbook contents and raw provider logs out of Git and agent output. Preserve the existing workbook unchanged.

For verification, use [local verification](docs/local-verification.md), actual SDK/dependency locks and checks appropriate to the change. Report local, simulated-provider and live evidence separately; local success does not qualify the live host.
