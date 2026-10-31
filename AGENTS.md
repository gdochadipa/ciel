# OpenClaw Secretary project instructions

Read local `PROGRESS.md` before continuing work when present. It records implementation status, pending features, verification evidence and the workspace/integration Git arrangement. Keep status there; keep durable rules here and in `.agent/rules/`. Fresh checkouts use versioned instructions without requiring local notes.

## Rules and local documentation

- Repository rules in `.agent/rules/` and this `AGENTS.md` are versioned project instructions and may be committed. Keep them self-contained so a checkout does not need unpublished design notes to follow them.
- Keep operational README files, executable Skill instructions and API contracts versioned because shipped workflows depend on them.
- Keep working documentation local: `docs/`, `PROGRESS.md`, `GLOSSARY.md`, planning records and interview notes. General code/rules commit or push authorization excludes these documents. Select code/config/test/rule paths explicitly during integration sync and staging; inspect the staged diff before committing.
- Already tracked working documents require the same exclusion; ignore patterns do not protect them. Preserve published files and history. Publishing named working documents, deleting published docs or rewriting history requires separate explicit owner authorization.

Read [code/comment rules](.agent/rules/code.md) for code changes, [FSM rules](.agent/rules/fsm.md) for preview/approval/write/recovery changes, [memory rules](.agent/rules/memory.md) for native summary-memory changes, and [processing rules](.agent/rules/processing.md) for request handling and development workflow. These files are explicit instructions reached through this entry point; do not assume the directory is automatically loaded by every agent tool.

## Architecture and ownership

- Keep one Rust Core process, one in-process command bus and a durable operation FSM backed by a SQLite operation journal. Preserve the domain/application/infrastructure/presentation boundaries. Do not add separate CQRS buses, brokers, projection workers or extra daemons without an explicit revised architecture decision.
- OpenClaw owns personal memory. The plugin's SQLite journal stores operation and approval state, not personal notes or a second knowledge store.
- The owner authorizes short initial per-Doc discovery summaries in native OpenClaw memory with seven-day logical expiry. After confirmed edits through OpenClaw, reread source and refresh the same note; uncertain outcomes keep it stale/unverified. Follow [memory rules](.agent/rules/memory.md). T038 implements opt-in native file persistence and discovery expiry locally; live host/model/index behavior remains unqualified. Require current owner-private local authority and explicit no-promotion host policy. Do not claim physical deletion, host activation or measured token savings from local tests.
- Support general registered Workspace resources. The wedding spreadsheet is an optional domain profile, not a universal schema or template.
- Reuse immutable previews, owner approval, digest binding, the shared journal and FSM for new mutations. Handle replay and uncertain outcomes without blindly resending provider writes.

For architecture changes, follow [code ownership](.agent/rules/code.md) and [FSM rules](.agent/rules/fsm.md). For Sheets/Docs changes, inspect the [API contract](contracts/openapi.yaml), registered permissions and actual adapters. Local ADR/spec notes may supply additional context when available.

## Operational boundaries

- Keep work local unless the user explicitly requests target-specific live actions. Deployment, live Google mutations, real native jobs and WhatsApp sends require that authorization and the applicable qualification gates. General requests to continue development do not authorize those actions.
- Keep production writes and approval/scheduler qualification flags disabled until the corresponding live evidence and explicit activation authorization exist. Follow the [installation boundaries](README.md#installation) and shipped preflight/qualification configuration. Local T007 notes may supply additional evidence when available.
- Avoid additional paid services, billing activation and paid quota upgrades unless the user explicitly revises the cost constraint.
- Keep credentials, tokens, private target IDs, personal workbook contents and raw provider logs out of Git and agent output. Preserve the existing workbook unchanged.

For verification, use `bash scripts/verify.sh`, actual SDK/dependency locks and checks appropriate to the change. Provider changes also require relevant Google-only simulations through `scripts/acceptance-all.sh`. Report local, simulated-provider and live evidence separately; local success does not qualify the live host.
