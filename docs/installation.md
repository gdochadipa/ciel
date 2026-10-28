# GitHub checkout → VPS installation

The owner-selected delivery flow is local development → reviewed GitHub commit → clone that revision on the chosen VPS → run the repository installer. This document records the intended flow while T026's installer interface is being prepared; it is not an installation or activation receipt. T007 qualification remains open.

## Source and destination

Use the retained integration Git history (`integration/secretary-mvp`), not a replacement repository or a copy that contains untracked operator files. The GitHub URL and visibility must be selected by the owner before publication. On the VPS, clone the selected repository and check out the reviewed commit. A branch name alone is not an immutable release identity. Verify the checkout before running installation code.

SSH is optional operator transport for reaching the selected VPS. No IP is embedded in the plugin or installer. Once installed, the plugin talks to Core on that same VPS using the configured loopback `coreUrl` (default `http://127.0.0.1:8787`); it does not clone repositories or connect back to the development computer at runtime.

## Stages

1. **Local review and publication:** complete relevant checks, review each commit, and push only to the selected GitHub destination. Never push the personal workbook, credentials, private registry/target IDs, journal, raw logs or generated packages. No new paid service or billing/quota activation is required.
2. **Target checkout and build:** clone the reviewed revision, check prerequisites, retain actual dependency locks and build with the pinned SDK. Build Rust natively for that VPS; the existing local ARM64 debug binary is not a portable release. Installer preparation must not download/execute an unreviewed one-line bootstrap or change global toolchains without explicit direction.
3. **Release staging:** produce the Core binary, plugin archive, source revision/hash receipt, preflight/backup utilities, service template and inactive configuration examples. Preserve existing configuration/state and leave all secrets and private registrations operator-managed outside the release.
4. **Explicit target installation:** after review and target-specific authorization, place reviewed files and register an inactive service. This must not bypass the production preflight or start/enable services. Plugin registration needs a separately checked effective policy: `plugins install --no-enable` preserves policy and can still apply an existing enabled entry through a running Gateway. Do not treat the flag as a disabled-install guarantee.
5. **Qualification and activation:** execute separately authorized disposable Core/Google/approval/scheduler/recovery checks and record their evidence. Production write, approval and scheduler flags remain false until the corresponding gates and explicit activation authorization are satisfied. Activation/reload/start is distinct from cloning or staging.

The supplied systemd unit requires a complete private T007 attestation in `ExecStartPre`. Initial disposable qualification therefore needs separately reviewed test settings; the installer must not manufacture a successful attestation or weaken that production gate. See [deployment](deployment.md), [T007](../.scratch/secretary-mvp/T007.md) and [T026](../.scratch/secretary-mvp/T026.md).

## Current target evidence

The owner reports compatible Node/OpenClaw versions and successful active-config schema validation. Cold plugin inspection returns `Plugin not found: secretary-tools`; the checked system unit returns `not-found/inactive/dead`. This explains why secretary-specific host qualification cannot yet run through that installation. It does not prove that no alternate process/profile/install exists, and it does not authorize deployment. The agent environment cannot open SSH to the target; operator execution or a permitted connection is still necessary for host steps.
