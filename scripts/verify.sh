#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -c 'import ast, pathlib; [ast.parse(pathlib.Path(p).read_text()) for p in ("scripts/preflight.py", "scripts/backup.py", "scripts/install.py")]'
python3 -B -m unittest discover -s scripts -p 'test_*.py' -v
if [[ ! -f Cargo.toml || ! -f package.json ]]; then
  echo 'Required Cargo and package manifests are missing.' >&2
  exit 1
fi
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo test --offline --locked --workspace
bun run build
bun run validate
SECRETARY_HTTP_TESTS=1 bun run test
bun test scripts/acceptance.test.ts
