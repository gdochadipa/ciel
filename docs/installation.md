# GitHub checkout → VPS installation

The owner-selected delivery flow is local development → reviewed GitHub commit → clone that revision on the chosen VPS → run the repository installer. T026 provides `scripts/install.py` for commit-bound staging, verification and explicit file installation. No command in it starts services or imports/activates plugins; this document is not a target installation or activation receipt. T007 qualification remains open.

## Source and destination

Use the retained integration Git history (`integration/secretary-mvp`), not a replacement repository or a copy that contains untracked operator files. The owner selected [gdochadipa/ciel](https://github.com/gdochadipa/ciel), initialized with MIT LICENSE. Its initial commit is merged into the retained integration history. On the VPS, clone the selected repository and check out the reviewed commit. A branch name alone is not an immutable release identity. Verify the checkout before running installation code.

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

Set `SECRETARY_REPO_URL` to `https://github.com/gdochadipa/ciel.git` and `SECRETARY_REVISION` to the **full reviewed 40-character commit**, then clone and check out that revision on the chosen VPS. These are operator inputs; no actual destination or token belongs in this file. Use Git's normal credential helper/SSH agent for private repositories, never credentials embedded in a URL or shell history.

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

Receipt format 2 adds the repository MIT `LICENSE` as a required hashed payload; earlier format-1 staging is not accepted by the current verifier. The fixed release contents are: MIT LICENSE; native Core executable; a plugin archive with exactly three regular runtime files/26 tools and inactive flags; preflight/backup utilities; service/registry/OpenClaw/qualification/environment **examples**; this guide; and `release.json`. The plugin bundle also embeds the full MIT notice in its JavaScript banner while retaining the SDK three-file archive layout. The receipt binds commit, native OS/architecture/libc, tool versions, both lock hashes and every payload hash. Treat the release directory as trusted local operator input: these hashes detect drift, not authenticity of an attacker-authored receipt. Verify against the independently selected reviewed commit, not a commit copied from an untrusted receipt.

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


## First publication to the selected GitHub repository

The existing `main` seed contains only the owner's MIT LICENSE. It was fetched anonymously and merged with `--allow-unrelated-histories`; all retained local implementation commits and the seed are ancestors of the prepared integration branch. The selected repository is not a replacement for local history. No force push is required.

After local checks/review, publish the prepared branch and fast-forward main atomically from an authenticated operator terminal:

```sh
git -C /private/tmp/openclaw-secretary-implementation.on4Zt7 push --atomic origin \
  integration/secretary-mvp integration/secretary-mvp:main
```

That path is the retained integration checkout for this development workspace, not a VPS path or a runtime dependency. If remote main advanced, the normal non-force push must refuse; fetch and integrate that work before retrying. If branch protection requires a PR, publish only the integration branch and use the repository's PR workflow rather than weakening protections. The reviewed revision for VPS installation is the actual successful published commit, verified after push. GitHub Actions configuration already exists but its hosted results remain unobserved until publication.

Agent-native Git/gh DNS/connectivity and active gh authentication are currently unavailable. Anonymous remote inspection/fetch succeeded through the sanitized Docker network path without credentials. No private credential was mounted or printed. If normal terminal authentication is needed, use GitHub's normal browser/credential-helper login there; do not paste tokens into agent messages or URLs. Repository connection/preparation is not proof that a push succeeded.


Ciel publication preparation passed the full local suite (23 Python, 34 Rust, 43 TypeScript, formatting/Clippy and actual SDK type/build/validate). A real disposable clone of code `4e691b5` staged format 2 with eleven payload files, verified root MIT LICENSE bytes and the full MIT banner inside the three-file SDK archive, then verified/replayed a disposable-root file installation. Plugin archive: 37636 bytes, SHA-256 `ca59e0a466d47ee99a7f190910e2dae596f3cc95179a5f1f6dd39ac2be37eff7`. It remained Git-clean. No credential/token was mounted or printed.

The agent attempted the atomic non-force push and received `Could not resolve host: github.com`; no remote ref was updated by that attempt. Local native gh authentication is also unavailable in this session. The terminal command above is the remaining publication step; run it on the development computer containing the retained checkout, not on the VPS. After success, verify GitHub refs before selecting a reviewed clone revision or treating hosted CI as observed. No VPS command is part of publication.

## Current publication and T027 release status

The operator's publication of 30d4dfa is verified on GitHub main and integration/secretary-mvp. Its hosted source/local checks passed, but the simulation step failed; the detailed cause remains unavailable without job logs. Later local commits, including reviewed T027 code 5d6e135, are not yet verified as published. Use the existing non-force publication flow only after review; a local artifact is not proof of remote publication. VPS cloning remains deferred.

A clean clone of 5d6e135 passed actual offline/locked native Linux ARM64 staging/verification/reuse and disposable-root installation/replay, preserving synthetic config/state and MIT notices. Format 2 contains eleven hashed payloads and a three-file, 25-tool plugin archive (37889 bytes; SHA-256 acd1ccf1ab1304cbd8f526d8062bce45af39bd78c4136d1a5fd3623b1fb4959c). The private receipt binds that code revision; a later documentation-only commit does not change its provenance. This is local evidence only; build natively for the selected VPS and retain all T007 gates.

T028 adds read-only docs_search as the 26th registered tool. Current installer validation requires that tool/count and retains disabled approval/scheduler defaults. Historical25-tool receipts above remain bound to their tested code; they do not prove the new package.

## Current owner instruction: publish with Actions deferred

The owner authorized publication after T028 and continues to defer GitHub Actions. A separate documentation commit containing [skip ci] preserves that choice for the checked-in push/pull_request workflow without changing workflow or protection configuration. GitHub documents this mechanism in [Skipping workflow runs](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs). It does not claim CI success or bypass required branch checks; if a policy blocks the non-force update, use the repository's permitted review flow and retain its protections.

Native Git currently fails DNS resolution and gh authentication is invalid in the agent session. When publication cannot run there, use an already authenticated normal operator terminal on this development computer, with Git's normal credential helper; never paste tokens or private keys into a message. The prepared command is:

```sh
git -C /private/tmp/openclaw-secretary-implementation.on4Zt7 push -u origin --atomic \
  integration/secretary-mvp integration/secretary-mvp:main
```

Verify the actual remote SHAs after a successful push before treating the local revision as published. This command does not clone/install on the VPS or qualify live capabilities.

## Offline registry preparation

The native Core binary supports registry init --output PATH and registry check --config PATH before any runtime environment/credential loading. Init creates a new inactive draft (never replacing an existing file); check uses the same Rust registry rules as service startup and reports only aggregate counts. Neither command starts/activates/installs/reloads anything or verifies live Google access. Use [offline registry setup](registry-setup.md); preflight and T007 gates remain mandatory for a later authorized host installation.

## Verified local native release with offline CLI

A clean clone of 42f18d52705a8d2a285a0756f236f0895d7cc6c9 passed real locked/offline native Linux ARM64 staging/verify/reuse and disposable-root install/replay. Format 2 has eleven hashed payloads and 26 SDK tools. The installed Core was verified against its receipt before registry init/check exercised inactive 0600 drafts, no-overwrite/symlink/schema rejection and sanitized output, even with poisoned runtime environment. Synthetic config/state bytes/mode were preserved and the clone stayed clean.

Core is 10090640 bytes, SHA-256 e503aa79ab13f27f808af596ebd1d5c39d41c1c94028b480d9db8b995f92ad52, target aarch64-unknown-linux-gnu. Plugin is 38194 bytes, SHA-256 4b400a24c5fd02c8f9f3aa75e9af3ad0346e9d5396b948a335531705c8b0b3ce, with three regular runtime files and MIT notices preserved. Receipt/artifacts stay private under qualified runs/t030-release-phv4h5c8/. A later documentation-only commit does not change the tested code provenance.

This verifies local native staging and inactive file placement, not the VPS CPU/libc/runtime or production qualification. Build natively for the selected target under the separately authorized workflow; no activation/reload/start, credential transfer, paid service or automatic publication is implied.

## Current reported target platform

The owner reports Tencent Singapore with 2 vCPU/2 GB/40 GB and x86_64 Ubuntu 24.04.5 LTS/glibc 2.39. These are operator-provided command/specification results, not agent inventory or available-capacity measurements. Choose native x86_64-unknown-linux-gnu/linux/amd64; retained ARM64 artifacts do not establish compatibility. The repository installer already selects the actual build host target and binds platform/toolchain/artifact hashes in its receipt. It must still be built/tested for the selected platform before a release is considered target-compatible. No current x86 staging receipt exists. See [platform-specific local verification](local-verification.md); VPS cloning/installation and activation remain deferred.

## Download a native package without cloning

The **Downloadable Ubuntu x86_64 release** Actions workflow builds each main commit
(or a manually selected ref). Only main publishes a commit-specific prerelease at
[GitHub Releases](https://github.com/gdochadipa/ciel/releases). Assets contain the
full source commit in their names; there is no mutable latest-build installer.
Standard public Ubuntu 24.04 runners are used, with no paid runner or cache. Temporary
Actions artifacts expire after one day; Release assets remain downloadable.

Select a successful `build-<full-40-character-commit>` release, download its
`ciel-<commit>-linux-x86_64.tar.gz` and matching `.sha256` into a new operator-owned
private directory. Copy the exact commit and asset URLs from that Release. On Ubuntu:

```bash
# Replace this value with the selected Release's exact full commit.
ciel_revision=FULL_40_CHARACTER_COMMIT
ciel_archive="ciel-$ciel_revision-linux-x86_64.tar.gz"
sha256sum -c "$ciel_archive.sha256"
tar -xzf "$ciel_archive"
cd "ciel-$ciel_revision"
python3 install.py verify --revision "$ciel_revision" --release release
```

Download only from the trusted project Release. SHA256 detects download corruption;
it does not authenticate a malicious replacement of both package and checksum.
The inner `release/release.json` binds the source commit, locks, actual tool versions,
build host and every staged artifact. Installer and payload are bundled together;
Python 3 is sufficient for verification and file installation. No Git, Rust or Bun
is needed on the target for those steps. OpenClaw/Node remain runtime prerequisites.
The package targets Linux x86_64 with glibc 2.39; the installer rejects another native
host. ARM64 local verification packages must not be installed on the VPS.

After separate authorization for target file installation, the concrete command is:

```bash
sudo python3 install.py install-files --revision "$ciel_revision" --release release
```

This copies inactive files only. It preserves existing configuration/state and refuses
conflicting installations; it does not start/enable services, register/activate the
plugin, install personal registry data or enable any qualification/write/memory flag.
Follow the remaining manual configuration and T007 qualification steps in this guide.
An existing installation of another revision requires explicit upgrade preparation.

The workflow gates packaging on locked source checks, all seven simulated-provider
scenarios, native staging, clone-free verification and disposable-root installation.
`smoke.json` records an **offline registry CLI** run with two CPUs and a 2 GiB address
space limit. It proves that narrow binary/loader smoke only: it does not measure the
full OpenClaw/model memory load, request concurrency, disk growth or live VPS behavior.
No remote host address, SSH key, Google credential or deployment secret is required.
