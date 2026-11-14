#!/usr/bin/env bash
# Local native Linux evidence only; source/cache mounts are read-only.
set -euo pipefail
cd "$(dirname "$0")/.."
service_check_image=${CIEL_VERIFICATION_IMAGE_ID:-secretary-verification:2026.10.7}
if [[ -n ${CIEL_VERIFICATION_IMAGE_ID:-} && ! "$service_check_image" =~ ^sha256:[a-f0-9]{64}$ ]]; then
  printf 'Verification image override must be an inspected immutable image ID.\n' >&2
  exit 1
fi
service_check_sdk=${CIEL_VERIFICATION_NODE_MODULES:-$PWD/node_modules}
if [[ ! -f "$service_check_sdk/openclaw/package.json" || ! -f plugins/secretary-tools/dist/index.js ]]; then
  printf 'Build with the actual locked SDK before protected acceptance; no SDK gate is skipped.\n' >&2
  exit 1
fi
service_check_sdk=$(cd "$service_check_sdk" && pwd -P)
service_check_target=$(mktemp -d "${TMPDIR:-/tmp}/ciel-service-linux.XXXXXXXX")
trap 'rm -rf "$service_check_target"' EXIT
chmod 755 "$service_check_target"
docker run --rm --pull never --network none --interactive --cap-drop ALL \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER --cap-add MKNOD \
  --cap-add SETUID --cap-add SETGID \
  --cap-add SETPCAP \
  --security-opt no-new-privileges \
  --mount "type=bind,source=$PWD,target=/workspace,readonly" \
  --mount "type=bind,source=$service_check_sdk,target=/workspace/node_modules,readonly" \
  --mount "type=bind,source=${CARGO_HOME:-$HOME/.cargo}/registry,target=/usr/local/cargo/registry,readonly" \
  --mount "type=bind,source=$service_check_target,target=/target" \
  -e CARGO_TARGET_DIR=/target \
  "$service_check_image" bash -s <<'SH'
    set -euo pipefail
    test "$(node --version)" = v24.22.1
    python3 -c 'import json; from pathlib import Path; assert json.loads(Path("node_modules/openclaw/package.json").read_text())["version"] == "2026.10.7"'
    cargo fmt --all -- --check
    rustfmt --edition 2021 --check services/secretary-core/tests/support/launcher_probe.rs services/secretary-core/tests/support/gateway_payload.rs
    cargo clippy --offline --locked --workspace --all-targets -- -D warnings
    cargo test --offline --locked --workspace
    node node_modules/openclaw/openclaw.mjs plugins pack --root /workspace/plugins/secretary-tools --out /target/debug/readonly-plugin.tgz --json
    python3 -B scripts/service_check_linux.py /target/debug/secretary-core
SH
