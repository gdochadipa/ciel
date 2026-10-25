#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
scenario="${1:-read}"
case "$scenario" in read|error|slow|drift|retry) ;; *) echo 'Use read, error, slow, drift or retry' >&2; exit 2;; esac
log_dir="$(mktemp -d "${TMPDIR:-/tmp}/secretary-acceptance.XXXXXX")"
sim_pid=''; core_pid=''
cleanup() {
 if [ -n "$core_pid" ]; then kill "$core_pid" 2>/dev/null || true; fi
 if [ -n "$sim_pid" ]; then kill "$sim_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT INT TERM
postman simulation run "postman/simulations/secretary-${scenario}.sim.yaml" --port 4900 --output ndjson --no-history > "$log_dir/sim.ndjson" 2> "$log_dir/sim.err" &
sim_pid=$!
ready=false
for _ in {1..50}; do
 if rg -q '"listening"' "$log_dir/sim.ndjson"; then ready=true; break; fi
 if ! kill -0 "$sim_pid" 2>/dev/null; then break; fi
 sleep 0.2
done
if [ "$ready" != true ]; then
 echo "Simulation failed before listening; no service acceptance ran. Logs: $log_dir" >&2
 cat "$log_dir/sim.err" >&2
 exit 1
fi
# Derive the provider address from the actual listening event, never a guessed port.
provider_base="$(bun scripts/acceptance.ts --simulation-base "$log_dir/sim.ndjson")"
cat "$log_dir/sim.ndjson"
SECRETARY_CONFIG="$PWD/postman/fixtures/registry.json" \
SECRETARY_CORE_TOKEN=synthetic-acceptance-core-token SECRETARY_OWNER=+15555550123 \
SECRETARY_MOCK_MODE=true SECRETARY_GOOGLE_BASE_URL="$provider_base" \
SECRETARY_GOOGLE_TOKEN=synthetic-provider-token \
cargo run --offline --locked -p secretary-core > "$log_dir/core.log" 2>&1 &
core_pid=$!
if ! bun scripts/acceptance.ts "$scenario"; then
 cat "$log_dir/core.log" >&2
 echo "Acceptance failed; disposable logs: $log_dir" >&2
 exit 1
fi
# Injected conditions short-circuit before a member request event (Postman CLI behavior).
if [ "$scenario" = error ] || [ "$scenario" = slow ]; then
 echo 'Condition case: member request events are unavailable; assess actual caller result with the configured injected failure.'
elif ! rg -q '"request"' "$log_dir/sim.ndjson"; then
 echo "No Google request event found; no dependency-path proof. Logs: $log_dir" >&2
 exit 1
fi
echo "Acceptance finished; inspect dependency traffic in $log_dir/sim.ndjson"
