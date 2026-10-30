# Local verification with the real SDK

Verified 2026-10-28 using Linux arm64: Node 24.22.1, Bun 1.2.21, Rust 1.95.0, OpenClaw 2026.10.7, TypeScript 7.0.2 and Postman CLI 1.75.0. The actual SDK engine range is `>=24.22.1 <25 || >=26.12.1`. Other platforms require their own run.

Use a normal Git checkout containing this increment. In this session the retained checkout is `/private/tmp/openclaw-secretary-implementation.on4Zt7`; the working source root has no Git metadata. The commands below copy only committed files into a disposable directory. They exclude the untracked workbook, operator `.env` files, credentials and personal OpenClaw configuration. No host home directory or Docker socket is mounted into the verification container.

Build the tool image using only the `deploy` context. Its ignore file excludes everything except the verification Dockerfile. Image base digests are pinned; tool installation needs registry access. Postman lifecycle scripts are disabled because verification needs the CLI, not automatic agent instruction setup.

```sh
docker build -f deploy/Dockerfile.verify -t secretary-verification:2026.10.7 deploy
secretary_check_dir="$(mktemp -d /tmp/secretary-verification.XXXXXX)"
mkdir -p "$secretary_check_dir/project" "$secretary_check_dir/cache" "$secretary_check_dir/home" "$secretary_check_dir/runs"
git archive HEAD | tar -x -C "$secretary_check_dir/project"
```

Define a helper in that shell. Network access is an explicit argument. Only the disposable checkout, dependency cache, empty home and private run directory are mounted; the caller's UID/GID owns outputs. Builds need writable checkout/cache space. Loopback is available even when external networking is disabled.

```sh
secretary_verify_run() {
  secretary_network="$1"
  shift
  docker run --rm --pull never --network "$secretary_network" \
    --user "$(id -u):$(id -g)" --cap-drop ALL --security-opt no-new-privileges \
    --mount "type=bind,source=$secretary_check_dir/project,target=/workspace" \
    --mount "type=bind,source=$secretary_check_dir/cache,target=/cache" \
    --mount "type=bind,source=$secretary_check_dir/home,target=/home/runner" \
    --mount "type=bind,source=$secretary_check_dir/runs,target=/runs" \
    --env HOME=/home/runner --env CARGO_HOME=/cache/cargo \
    --env CARGO_TARGET_DIR=/cache/target --env CARGO_BUILD_JOBS=4 \
    --env BUN_INSTALL_CACHE_DIR=/cache/bun --env TMPDIR=/tmp \
    --env SECRETARY_ACCEPTANCE_LOG_ROOT=/runs \
    --env PATH=/workspace/node_modules/.bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    --workdir /workspace --entrypoint /bin/sh secretary-verification:2026.10.7 "$@"
}
secretary_verify_run bridge -c 'cargo fetch --locked && bun install --frozen-lockfile'
secretary_verify_run none -c 'bash scripts/verify.sh'
```

The locked OpenClaw dependency is explicitly trusted for its reviewed local package postinstall. Optional untrusted dependency lifecycle scripts need not be enabled for these authoring checks. Do not generate substitute SDK declarations or silently replace production dependencies if installation fails.

Run dependency scenarios separately, with no external network. The fail-fast suite runs all seven scenarios sequentially and waits for each owned service to stop. The writes and workspace launchers rebuild the current plugin and includes actual SDK/built-entry factory and approval-hook integration using a host registration fixture. Core and the bridge run for real; Only Google is simulated; workspace uses separate Sheets and Docs members. Ports belong to each disposable container, avoiding host-port publication.

```sh
secretary_verify_run none -c 'bash scripts/acceptance-all.sh'
secretary_verify_run none -c 'bun run --cwd plugins/secretary-tools pack'
```

Inspect each private `runs/secretary-acceptance.*/sim.ndjson` for Google traffic. Injected persistent-error and latency conditions intercept before member request events; their actual caller assertions are the evidence. Leave `TMPDIR` on native container scratch: redirecting it to a host-mounted directory broke Postman dataset support. The separate log-root setting persists evidence without moving scratch. Logs can include bodies and should remain private and uncommitted.

The SDK archive is under the disposable checkout's `plugins/secretary-tools/dist/secretary-tools.tgz`. Its three runtime files differ from npm dry-run's four-file selection, which also includes README. Packing does not install, enable, reload or publish the plugin.

This session's approved direct Docker commands worked, while Docker invoked from a sandboxed shell wrapper could not reach the daemon. A user terminal with Docker access can use the helper above; agents may need equivalent direct Docker invocations. Native macOS registry DNS/loopback restrictions were not changed.

The verification script includes 29 Python CLI tests (13 operator + ten installer + six launcher); the Rust suite includes a real Core WAL backup/restore check. These use disposable local files and never require live credentials.

Full local checks passed; live VPS/Google OAuth, trusted owner approval UI, native scheduler persistence, WhatsApp routing, backup/restore and hosted CI remain separate gates in [implementation status](implementation-status.md). Memory remains owned by OpenClaw and all live capability flags remain off.

The repository's full CI job now installs Postman CLI1.75.0 with lifecycle scripts disabled and runs the same `scripts/acceptance-all.sh` gate after local verification. It requires no Postman login, API key or live credentials, and publishes nothing. The acceptance step has a ten-minute ceiling; the job has a thirty-minute ceiling. Local sequential execution passed in T023; hosted CI on published 30d4dfa passed local behavior but failed the simulation step (exit 1; detailed cause open).

T026 adds a real installer exercise using a disposable clone of the retained history, the same actual frozen dependency cache and no external network. Staging explicitly selects the Rust host target, then native file installation runs only under a disposable `--root`; config/state preservation, hash verification and replay pass. The installed host/Systemd/Google are not qualified. See [installation](installation.md) and [T026](../.scratch/secretary-mvp/T026.md) for exact CLI boundaries and evidence.

## Acceptance failure diagnostics

On failure, the launcher retains private raw logs in its disposable run directory and writes diagnostic.json. Console output and the GitHub job summary contain allowlisted metadata only: scenario, failed stage, original exit status, naturally exited simulation-wrapper status when observed, bounded event counts and fixed error categories. Each known log contributes at most its final 64 KiB; counts cover retained tails and error categories are hints. Empty categories do not imply absence of an error. Provider JSON/body/header content is not echoed; inspect private files locally if more detail is required. No raw-log artifact is uploaded.

Six external-command CLI regressions cover startup stdout errors, build/parser/client failures, exit preservation, owned-process cleanup, successful output and malformed/oversized log handling. They use synthetic canaries to check that raw content does not enter the safe summary; actual Core/Google behavior still requires the separate seven-scenario suite. These diagnostics improve future evidence and do not establish why the earlier hosted simulation step failed.

T030 adds seven Rust integration checks against the actual emitted Core executable and disposable files. They cover offline/no-runtime-env behavior, sanitized output, the shared validator, configured-grant summaries, inactive Unix 0600 drafts, no-overwrite/symlink/FIFO/byte bounds and safe argument routing. Final full local verification passes 57 Rust, 29 Python and 46 TypeScript checks; all seven service scenarios pass after the no-argument startup preservation change. This is Linux CLI/service evidence, not a new native release or VPS installation receipt. Use the [registry guide](registry-setup.md) only with a reviewed T030-or-later Core build.

## Preparing a separate amd64 verification environment

The reported VPS is x86_64 Ubuntu 24.04.5 LTS/glibc 2.39, while the retained executed build/test/staging evidence above is Linux ARM64. On2026-10-29 Docker inspection confirmed that all three immutable base indexes in deploy/Dockerfile.verify include linux/amd64. Local buildx advertises amd64 support, but the previous secretary-verification:2026.10.7 image is absent from the current daemon. Manifest inspection is not an image build or a Core/SDK platform pass.

To prepare the architecture-specific tool image, use the existing pinned Dockerfile and isolated deploy-only context:

```sh
docker buildx build --builder default --platform linux/amd64 --load \
  -f deploy/Dockerfile.verify -t secretary-verification:2026.10.7-amd64 deploy
```

This preparation command has not been executed/qualified in the current step. Use a fresh sanitized disposable checkout and separate node_modules, Cargo target/cache and home/run directories for the amd64 suite; do not reuse ARM-native optional JavaScript packages or binaries. Run containers with explicit --platform linux/amd64 and the new image tag, retaining the existing no-home/no-credential/no-workbook mounting rules. Populate real locked dependencies, then run the documented verification and seven simulations; record emulation versus native execution and inspect the resulting native target/ELF/library requirements. The pinned Bookworm lab is not the exact Ubuntu host.

Resource qualification must separately measure OS/OpenClaw/Core and build/runtime peaks against the reported 2-GB allocation; the source suite's earlier build-job setting is not a proven fit for this server. Do not infer VPS readiness, model capacity, free disk or ABI compatibility from manifest support or existing ARM64 receipts. No image build, hosted CI, target deployment or live provider action was performed to document this preparation path.

## T031 offline resource additions

The same public executable/disposable-file seam now includes five new regressions (twelve CLI tests total). `bash scripts/verify.sh` passed with actual locked dependencies on network-disabled Linux ARM64: 62 Rust, 29 Python and 46 TypeScript checks, formatting/Clippy and SDK build/validation. Both Docs/Sheets success tests first failed against the prior CLI. Inputs/output permissions, preserved grants, collisions, versions, bounds and no-overwrite are covered; no credential, provider or active host state is used. The current daemon again provides the existing ARM64 tool image; the earlier absence described in the amd64 preparation inventory is historical. No amd64 environment was built/qualified.

No provider code, no-argument startup or plugin tools changed; the seven Google scenarios were not rerun for T031. Their latest evidence and native staging/installed init/check receipt remain T030. The new add command needs a reviewed T031-or-later build and has no installed-release/target-host receipt yet. See [registry setup](registry-setup.md).

## T032 existing-spreadsheet draft views

Four additional real-executable/disposable-file regressions bring the registry CLI to sixteen tests. Primary success/identity-conflict slices failed before implementation and passed after. Final network-disabled Linux ARM64 scripts/verify.sh passes with actual locked SDK/dependencies: 66 Rust, 29 Python and 46 TypeScript, format/Clippy with warnings denied and SDK build/validation. New views preserve parent permissions, inherit its declared readability and add no write grant; tests exercise both readable and inactive parents, consistent existing-tab ranges, identity conflicts, view limits/strict fields and replay/no-overwrite.

No provider/no-argument startup/plugin code changed; seven Postman scenarios were not repeated. Their last evidence and native installed init/check receipt remain T030. A reviewed T032-or-later Core is required for grid_view input; no new native release/target/VPS/live receipt is claimed.

## T033 selected Docs search

Five new public HTTP/actual Google adapter tests and one bridge HTTP regression pass, including whole-page failure on provider errors/identity drift and four-second aggregate timeout. Null/selector/type/permission errors, Unicode folding, global pages and all-source cursor invalidation are covered. Final full network-disabled Linux ARM64 verification passes with actual locks/SDK: 71 Rust, 29 Python, 47 TypeScript, format/Clippy and SDK build/validation. The first full attempt stopped at unsynchronized formatting; the final source was formatted/copied back before the passing run.

All seven sequential Postman scenarios pass again, including selected Docs through the actual built SDK factory. Workspace logged both Google members (53 Docs/38 Sheets request events), including five reference-document path events; only dependencies were simulated. Actual SDK pack/installer archive contract/MIT checks pass: 26 tools, three runtime files, 38495 bytes, SHA-256 50a3910b9ea1276ff5ad36af259a8eb32b30aa9581c0a0483a0ab65020c2d1db. Private logs/receipt under qualified runs/t033-*. Existing native staging/installed CLI receipt remains T030; no target/VPS/live qualification is inferred.

## T034 private registry comparison

Five new real-executable/disposable-file regressions bring registry CLI coverage to21 tests; the primary comparison test failed before implementation and passed after. They cover full private report content, terminal canaries/counts, permissions/identity/range changes, normalized ordering/defaults, version cases, rename classification, invalid/unsafe inputs, cap and exclusive-output/replay safety. Full network-disabled Linux ARM64 scripts/verify.sh passes with actual locks/SDK: 76 Rust, 29 Python and 47 TypeScript, format/Clippy -D warnings and actual SDK build/validation. Private log qualified runs/t034-verify.log. Workbook unchanged.

No provider/no-argument startup/plugin code changed; seven Google-only scenarios and SDK archive checks were not repeated (latest T033). Native staged/installed receipt remains T030, so a reviewed T034-or-later Core is required for diff; no target/host/live receipt is claimed. See [registry setup](registry-setup.md).

## T035 selected Sheets views

Six new actual Core HTTP/Google-adapter regressions and one bridge HTTP regression pass. The first success tests failed before implementation. Review then found one P2: a profile could fail projection outside the page even after workbook validation passed. Its public regression reproduced200 instead of409, then passed after every selected view was projected; valid profile rows remain supported. Both review axes rechecked the fix with no remaining material findings.

Final full network-disabled Linux ARM64 scripts/verify.sh passes with actual locks/SDK:82 Rust,29 Python,48 TypeScript, format/Clippy -D warnings and SDK build/validation. All seven Google-only scenarios were repeated and pass after the fix. Workspace run logged53 Docs/48 Sheets request events, including5 reference-spreadsheet path events; actual built-SDK global pages/cursor checks passed. Private final logs qualified runs/t035-verify-final.log and t035-acceptance-final.log. SDK pack/installer archive/MIT checks pass:26 tools,three runtime files,38901 bytes,SHA-256 36845054b1588a4b94062ffbefd520a6033820b4aee2c3550c3c7f3ef01ef768. Native staging/install receipt remains T030; no target/VPS/live qualification is inferred.

## T036 source-backed Docs summary context

Five new public HTTP/Core/real Google-adapter regressions plus one bridge HTTP regression pass. Success and whitespace-only cases first failed, then passed. Final network-disabled Linux ARM64 scripts/verify.sh:87 Rust,29 Python,49 TypeScript with actual locks, format/Clippy -D warnings and actual SDK build/validation. An initial all-target compile failure exposed old typed DocsRead constructors; the original struct/command was retained, old tests unchanged, and the final run passed. No code-review workflow or agents were used, per latest owner instruction.

All seven Google-only scenarios passed; Workspace logged55 Docs/48 Sheets request events, including42 primary-Docs path events. Actual built SDK executes summary_context/citation/coverage alongside legacy reads. Private logs qualified runs/t036-verify-final.log and t036-acceptance.log, Workspace secretary-acceptance.uIVKdB/. SDK pack/installer archive/MIT checks:26 tools,three runtime files,39266 bytes,SHA-256 bd1aa0ec6c0b1b13566df444c1db8956832c02dd6de5b08ad68c10f1ece2257b. No live model was called: generated-prose quality, citation grounding by the host model and effective skill loading remain target qualification. Native staging/installed receipt remains T030; no new target/live pass. See [Docs summaries](docs-summaries.md).

## T037 selected Docs summary context

Full network-disabled Linux ARM64 verification passed93 Rust,29 Python,51 TypeScript checks, formatting/Clippy -D warnings and actual locked SDK type/build/validation. Six new HTTP/Core/actual Google-adapter regressions and one bridge regression include red-before-green primary successes, global Unicode budgeting,300tabs, empty/changed/untrusted sources, continuations, pre-IO authorization/shape denial and whole provider/identity/projection/deadline/packet-size errors. Published OpenAPI validation covers disjoint legacy/single/multi success and malformed packets, with real bridge/Core responses validated during Workspace acceptance. Native summary memory is source workflow guidance only: local tests do not call a model or save native notes, enforce TTL/deletion, verify host tools/skills or measure token savings.

T037 final all-seven sequential Google-only scenarios pass after an initial acceptance harness parse/schema-branch-count failure was corrected. Actual built SDK selected-context calls and published response validation pass in Workspace;59 Docs/48 Sheets request events. Final private logs t037-verify.log/t037-acceptance-final.log. SDK pack/installer archive/MIT checks pass:26tools,3runtimefiles,39673bytes,SHA256265ea880bfff29c1db4ec8589869e36a030a4822fb5421a161f84e092fab4e95; private runs/t037-secretary-tools.tgz and t037-pack-receipt.json. Medium Standards/Spec review and delta rechecks clear. Native receipt remains T030, live T007 open; native memory/model/token behaviors remain untested.

## T038 executable native summary memory

Native effects require the real SDK on its supported Node24 runtime. Bun1.2.21 cannot load the SDK foundation Undici dependency for these effects; do not patch the SDK or replace its declarations. The plugin test command now runs existing Bun bridge tests plus Node tests against the previously built distribution. Run bun run build before standalone plugin tests, as verify.sh already does. Full local verification passes93 Rust/29 Python/51 Bun checks plus13 Node24 actual SDK/native-file factory tests. Native tests use disposable workspaces and synthetic HTTP/model boundaries; no user home or Google credential is mounted. Fixed expiry, recreation, no-IO hits, policy/audience/lifetime, source identity, native containment/locks and applied/uncertain/failed-refresh paths are covered. Local completion output is synthetic, not real model quality or token measurement.

T038 final: review found and closed one Spec P2 (delayed pre-edit source packet overwriting refreshed memory) with a failing-then-passing actual SDK/native regression. Replacement publication source is fetched under the held per-source lock. The prebundle uses real esbuild from the locked SDK graph because Bun's module-location shim fails the artifact gate; dependency locks remain unchanged. Final full suite and all seven scenarios pass on that artifact, including native-memory acceptance in Workspace c9Pa1Z (70 Docs/48 Sheets request events). Final private logs t038-verify-packaged.log/t038-acceptance-packaged.log. SDK pack/installer archive/MIT checks:26tools,3runtimefiles,44440bytes,SHA256b5c0cd08ba5fa85ff23aea890cb7c24048bd9668e7bf233f3d16a261038d4a33; private t038-secretary-tools.tgz/t038-pack-receipt.json. Both review axes/build rechecks clear. No target/native-installed/live model/index/physical erasure or token measurement is claimed.

## T039 cited Sheets context

Full network-disabled Linux ARM64 verify passes99 Rust/29 Python/53 Bun plus13 Node24 actual SDK/native tests, formatting/Clippy -D warnings and locked SDK type/build/validation. Six new public Core HTTP/actual Google-adapter regressions cover typed/error/Unicode/blank fields, defaults/budget/view/source binding, legacy-compatible continuation (including a fixed old-format cursor), pre-IO shape/auth/registration denial and whole provider/identity/deadline/packet-size/late-profile failures. One bridge regression and a published OpenAPI response regression cover the new selector/shape alongside legacy output. Initial Core/bridge success tests failed before implementation. Medium Standards/Spec review found no material issues; all seven simulations/SDK archive checks pass after correcting the acceptance range assertion to the registered read scope. This tests source preparation, not live model prose/token savings or target readiness.

T039 final Workspace secretary-acceptance.E5iAEV/ logged70 Docs/51 Sheets request events, including actual built-SDK summary calls and legacy continuation; Google/model boundaries were synthetic. Actual SDK pack/installer archive/MIT checks pass:26tools,3files,44793bytes,SHA25650470ddddb17ca0d41d373da1689318f8b0fac5b53a5d7357352dc3e957dcd14. Private t039-verify.log/t039-acceptance-final.log, t039-secretary-tools.tgz and t039-pack-receipt.json. Earlier failed acceptance log is retained; runtime was unchanged by the read-vs-write-range assertion correction. Medium Standards/Spec and final harness rechecks clear. Native installed receipt remains T030 and live T007 remains open.
