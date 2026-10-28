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
