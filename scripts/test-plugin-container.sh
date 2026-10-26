#!/usr/bin/env bash
# Source/HTTP checks only; uses a preinstalled local runtime, never installs a host SDK.
set -euo pipefail
if [[ $# != 1 || ! $1 =~ ^[A-Za-z0-9][A-Za-z0-9._/:@-]*$ ]]; then
  echo 'Usage: bash scripts/test-plugin-container.sh <cached-image-with-bun-node-npm-and-sh>' >&2
  exit 2
fi
secretary_test_image=$1
secretary_test_root=$(cd "$(dirname "$0")/.." && pwd -P)
docker run --rm --pull never --network none --read-only \
  --user 65534:65534 --cap-drop ALL --security-opt no-new-privileges \
  --tmpfs /tmp:rw,nosuid,nodev,size=256m \
  --mount "type=bind,source=$secretary_test_root/plugins,target=/workspace/plugins,readonly" \
  --mount "type=bind,source=$secretary_test_root/scripts,target=/workspace/scripts,readonly" \
  --mount "type=bind,source=$secretary_test_root/package.json,target=/workspace/package.json,readonly" \
  --workdir /workspace --env HOME=/tmp --env SECRETARY_HTTP_TESTS=1 \
  --entrypoint /bin/sh "$secretary_test_image" -c '
    set -eu
    node --version
    bun --version
    npm --version
    bun run check:plugin
    bun run test
    bun test scripts/acceptance.test.ts
  '
