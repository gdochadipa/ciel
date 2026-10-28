# GitHub checkout → VPS installation

The owner-selected delivery flow is local development → reviewed GitHub commit → clone that revision on the chosen VPS → run the repository installer. T026 provides `scripts/install.py` for commit-bound staging, verification and explicit file installation. No command in it starts services or imports/activates plugins; this document is not a target installation or activation receipt. T007 qualification remains open.

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


## Installer CLI

Set `SECRETARY_REPO_URL` to the owner-selected GitHub URL and `SECRETARY_REVISION` to the **full reviewed 40-character commit**, then clone and check out that revision on the chosen VPS. These are operator inputs; no actual destination or token belongs in this file. Use Git's normal credential helper/SSH agent for private repositories, never credentials embedded in a URL or shell history.

```sh
git clone "$SECRETARY_REPO_URL" secretary
cd secretary
git checkout --detach "$SECRETARY_REVISION"
python3 scripts/install.py plan --revision "$SECRETARY_REVISION"
```

The source must be a clean checkout at exactly that commit. Plan only inspects Git/lock prerequisites; staging additionally requires Python >=3.11, Node in the package engine range, Bun 1.2.21, Rust/Cargo 1.95.0 and OpenClaw 2026.10.7. Install these through a separately reviewed operator procedure if absent; the installer does not provision global tools or download a bootstrap script. Build/stage as the unprivileged operator, not root. The build uses actual SDK strict type/build/validate, Cargo `--locked --release --target <rust-host>` and Bun `--frozen-lockfile`, preserving both lockfiles. Normal first-time staging can download those locked dependencies; it does not activate any paid service or billing upgrade.

```sh
secretary_release_parent="$(mktemp -d)"
python3 scripts/install.py stage --revision "$SECRETARY_REVISION" \
  --output "$secretary_release_parent/release"
python3 scripts/install.py verify --revision "$SECRETARY_REVISION" \
  --release "$secretary_release_parent/release"
```

The release parent must be a private, operator-owned directory, outside the checkout. `--offline` skips JavaScript installation and requires that this checkout already had a successful `bun install --frozen-lockfile`; it also adds Cargo `--offline`. Use that mode only with qualified cached dependencies. Build commands use isolated HOME/config paths and filter inherited Core/Google environment secrets. No credential, live environment, registry, journal or workbook is copied. Build failure removes its private temporary release and publishes nothing. A same-revision, same-host output is reused after hash verification; conflicting outputs or locks fail closed.

The fixed release contents are: native Core executable; a plugin archive with exactly three regular runtime files/25 tools and inactive flags; preflight/backup utilities; service/registry/OpenClaw/qualification/environment **examples**; this guide; and `release.json`. The receipt binds commit, native OS/architecture/libc, tool versions, both lock hashes and every payload hash. Treat the release directory as trusted local operator input: these hashes detect drift, not authenticity of an attacker-authored receipt. Verify against the independently selected reviewed commit, not a commit copied from an untrusted receipt.

After **explicit target file-installation authorization**, the following command places the release at `/opt/secretary` and the unit template at `/etc/systemd/system/secretary-core.service`:

```sh
sudo python3 scripts/install.py install-files --revision "$SECRETARY_REVISION" \
  --release "$secretary_release_parent/release"
```

It requires Linux/root for the real filesystem. `--root <private-disposable-root>` exercises the same file layout without root or host changes. A release must match the native OS, CPU architecture and libc; build on the target rather than transferring an incompatible binary. Conflicting installations/units and symlinked target paths are refused, and an identical replay is verified/reused. First installation refuses an existing named service unit in /etc, /run or vendor locations, existing enablement links, or unresolved/active/enabled effective system-level state (queried read-only with systemctl on the real filesystem), including an identical unmanaged unit, so it cannot silently introduce files into an existing restart loop. No existing `/etc/secretary` configuration or `/var/lib/secretary` state is overwritten.

`install-files` does **not** create service accounts, write active secrets/registry/qualification, run mutating `systemctl` commands (including daemon-reload), register the plugin, start a Gateway or enable a unit. Thus it installs reviewable inactive files; the service is not yet runnable and the plugin is not yet registered. Use the [deployment prerequisites](deployment.md) to prepare the service account/private configuration, qualify disposable target settings and review effective host policies. Register the local plugin archive and reload/start only under separately authorized, qualified instructions. Do not treat `--no-enable` as disabling an existing plugin entry, and do not use a guessed marketplace package.

A failed unit publication can leave the fully verified release installed but the unit absent; inspect the result and retry the identical command after resolving the filesystem error. It never starts anything to compensate. Automatic upgrades, overwrites, uninstall, rollback activation and live qualification are deliberately not implemented by this first-install command. Preserve old files/state and use an explicit reviewed upgrade procedure.


## Executed local evidence

T026's CLI is accepted locally. Ten public installer tests pass on macOS and qualified Linux with only external build tools simulated. The full real-dependency suite passes 23 Python, 34 Rust and 43 TypeScript checks, formatting/Clippy and actual SDK type/build/validation. A separate real disposable Git clone built/staged the native Core and actual 25-tool SDK archive, verified/reused the release, installed/replayed files under a disposable root, preserved existing synthetic config/journal bytes and stayed Git-clean. Details, tested code revision, artifact hashes and review fixes are recorded in [T026](../.scratch/secretary-mvp/T026.md). No host installation, plugin activation, provider write or native job/message is established by these local checks. GitHub destination and T007 target steps remain separate.
