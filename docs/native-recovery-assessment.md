# Native recovery assessment — pinned OpenClaw 2026.10.7

Observed 2026-10-28 for T007. This note verifies supported commands and storage semantics from the **installed package's own documentation and implementation**. It does not qualify a live host, scheduler persistence, restored native jobs or channel delivery. The source-inspection phase performed no backup/restore. A subsequent isolated synthetic archive/staging exercise is recorded below; no Gateway, native database/job or credential was used.

## Inspected primary sources

The sanitized qualified checkout is `/private/tmp/secretary-qualified-3xjcv7r_/project`. Inspection used image `secretary-verification:2026.10.7` with external networking disabled and only that checkout mounted; no host home, credentials or workbook was mounted. `node_modules/openclaw/package.json` reports version `2026.10.7` and Node `>=24.22.1 <25 || >=26.12.1`.

Paths below are relative to that installed `node_modules/openclaw/` package. The published pinned links identify the corresponding primary sources; this assessment read the installed copies:

- `docs/cli/backup.md`: [pinned Backup CLI](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/cli/backup.md). SHA-256 `25ac35ad73713d20d062f553b85af968e4e33af8a0678ee1f6a1d5069df1a5f7`.
- `docs/install/backups.md`: [pinned backup/recovery guide](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/install/backups.md). SHA-256 `ce4fdadb6374907246b15db0fa0e2ea17cd7af8a614aa58d61b4107021019396`.
- `dist/register.backup-DYWz6tPb.mjs`, region `src/cli/program/register.backup.ts`: [pinned CLI registration](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/cli/program/register.backup.ts).
- `dist/backup-restore-0C1VMKO7.mjs`, region `src/commands/backup-restore.ts`: [pinned restore implementation](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/commands/backup-restore.ts).
- `dist/backup-create-Cjh7Hrjy.mjs`, region `src/commands/backup-create.ts`, and `dist/openclaw-state-snapshot-sanitizer-CB5a7od0.mjs`, region `src/state/openclaw-state-snapshot-sanitizer.ts`: [pinned archive creation](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/commands/backup-create.ts), [pinned snapshot sanitizer](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/state/openclaw-state-snapshot-sanitizer.ts).

## Supported local archive flow

The pinned CLI registers these commands; no third-party backup service is required:

```sh
openclaw backup create --dry-run --json
openclaw backup create --output <new-archive.tar.gz> --verify --json
openclaw backup verify <archive.tar.gz> --json
openclaw backup restore <archive.tar.gz> --target <fresh-staging-directory> --json
```

Full archive creation captures state, config and required include dependencies, credentials, configured agent roots and optionally workspaces. It uses managed SQLite online snapshots and offline compaction, including committed WAL content without copying live sidecars. `--no-include-workspace` retains state, credentials and configured agent databases. `--only-config` exports only the root config file and **omits its include dependencies and native databases**; it cannot qualify scheduler recovery. Output inside backed-up source trees or an existing archive filename is refused. Archives are sensitive plaintext unless separately protected. [Backup CLI](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/cli/backup.md).

Verification checks manifest/layout and captured canonical SQLite inventory, integrity and ownership; legacy archives without complete inventory are distinguished. It does not authenticate an untrusted archive. Restore verifies before extraction, refuses nonempty targets and targets within live state/configured agent roots, creates fresh staging only, and cleans incomplete extraction. There is no in-place/force restore. Activation is a separate operator action: stop all relevant writers, preserve current state, use manifest-recorded asset/agent paths, activate offline and run Doctor before restarting. Plugin dependencies and generated skill links need reinstall/rebuild through supported commands. Different-version recovery requires target-release database preflight; older builds refuse newer schemas. [Restore implementation](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/commands/backup-restore.ts), [recovery guide](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/install/backups.md), [schema contract](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/reference/database-schemas.md).

## Scheduler and delivery boundary

Native job definitions, runtime state and run history belong to OpenClaw's shared SQLite database. Therefore a managed shared-state snapshot is the supported recovery unit, rather than copying/editing historical `cron/jobs.json` or merging scheduler files. The global snapshot sanitizer deletes state/agent lease rows, all `delivery_queue_entries`, expiring plugin blobs and exec-approval socket tokens; it does not delete scheduler definitions in the inspected implementation. **Inference:** stored disabled/canceled job definitions should remain in a compatible full/shared snapshot, but their actual preservation and restart behavior still require disposable runtime evidence. [Pinned scheduler persistence](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/automation/cron-jobs/how-it-works.md), [snapshot sanitizer](https://github.com/openclaw/openclaw/blob/v2026.10.7/src/state/openclaw-state-snapshot-sanitizer.ts).

Delivery queue sanitization removes pending work and ownership/idempotency receipts. A restored portable snapshot is **not an exactly-once delivery continuation boundary**. Ordinary rollback also moves approvals and other retained state back in time; WhatsApp ratchet credentials can require relinking. Do not automatically replay writes, pending approvals or uncertain deliveries. Inspect/reconcile native identities and disabled/canceled state before any scheduler/channel activation. Legacy `cron/runs` logs matching volatile rules are excluded; they are not substitute recovery evidence. [Backup CLI sanitization and exclusions](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/cli/backup.md).

## Executed local archive staging and remaining qualification

On 2026-10-28, the real pinned CLI ran in the network-disabled verification container with `OPENCLAW_STATE_DIR` and `OPENCLAW_CONFIG_PATH` pointing to a new private disposable tree. The state contained only `{}` configuration and one synthetic marker file; there was no native SQLite database, job, credential, channel or Gateway.

The following native commands exited 0: `backup create --dry-run --json --no-include-workspace`, `backup create --verify --json --no-include-workspace`, independent `backup verify --json`, and `backup restore --target <fresh-stage> --json`. Restored configuration and marker bytes matched exactly. Restore to a nonempty target was refused while preserving its sentinel, and restore beneath the active synthetic state directory was refused without creating a target. Native command/assertion time was approximately 9.08 seconds; this is not a host recovery-time measurement.

The manifest's SQLite inventory was empty. Although verify/restore returned `sqliteInventoryVerified: true`, that establishes an empty inventory and **does not prove any database or scheduler recovery**. The archive was 630 bytes, SHA-256 `3307241b0f25f1d14d313142c076f8597a1781554e9ecff88586494677402734`. Private synthetic outputs/receipt are retained outside Git in `/private/tmp/secretary-qualified-3xjcv7r_/runs/secretary-t007-local/`; raw outputs, state and archives are not committed. This exercise used the state archive flow rather than `--only-config`, so it also preserved the opaque marker; its evidence boundary is still config/opaque-file staging only.

A repeat must use new private source/archive/staging paths and no host home/credentials/workbook mounts. Keep output outside source state, keep networking disabled, independently verify, compare bytes and test nonempty/live-subtree refusal; do not activate the restored tree. No narrow supported shared-database initialization command was verified: the inspected `dist/register.database-CZj66QTD.mjs` registers preflight and ownership operations, not initialization. Do not invent a database/schema or seed scheduler tables to claim native recovery.

For read-only host diagnosis use `openclaw doctor --json` or `--lint --json`. Ordinary Doctor, including `--non-interactive`, can migrate state and manage service maintenance; it is not a read-only inventory command. [Pinned Doctor postures](https://github.com/openclaw/openclaw/blob/v2026.10.7/docs/cli/doctor/running.md).

T007 remains open for an explicitly authorized disposable native scheduler snapshot/restore, native ID/cancellation/history preservation, crash/uncertain-delivery handling and separate owner-only receipt. Core journal restore remains independently tested in T018. Local archive staging does not qualify live Google, owner approval routing, native memory privacy, model entitlement or complete-host recovery; production writes and qualification flags must remain off.
