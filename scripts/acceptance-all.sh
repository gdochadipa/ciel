#!/usr/bin/env bash
# All targets are disposable local fixtures. A failure stops the gate immediately.
set -euo pipefail
cd "$(dirname "$0")/.."
for scenario in read writes retry drift error slow workspace; do
  bash scripts/acceptance.sh "$scenario"
done
