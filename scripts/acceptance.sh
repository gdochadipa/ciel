#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
scenario="${1:-read}"
case "$scenario" in read|error|slow|drift|retry|writes|workspace) ;; *) echo 'Use read, error, slow, drift, retry, writes or workspace' >&2; exit 2;; esac
log_dir="$(mktemp -d "${SECRETARY_ACCEPTANCE_LOG_ROOT:-${TMPDIR:-/tmp}}/secretary-acceptance.XXXXXX")"
sim_pid=''; core_pid=''; stage=core_build
cleanup() {
 local result=$? simulation_exit=''
 if [ -n "$sim_pid" ] && ! kill -0 "$sim_pid" 2>/dev/null; then
  if wait "$sim_pid"; then simulation_exit=0; else simulation_exit=$?; fi
 fi
 if [ -n "$core_pid" ]; then kill "$core_pid" 2>/dev/null || true; fi
 # Postman's npm wrapper spawns CLI/dataset children. Signal only this run's
 # dedicated process group, then bound the shutdown grace period.
 if [ -n "$sim_pid" ]; then
  kill -TERM -- "-$sim_pid" 2>/dev/null || true
  for _ in {1..30}; do
   if ! kill -0 -- "-$sim_pid" 2>/dev/null; then break; fi
   sleep 0.1
  done
  kill -KILL -- "-$sim_pid" 2>/dev/null || true
 fi
 if [ -n "$core_pid" ]; then wait "$core_pid" 2>/dev/null || true; fi
 if [ -n "$sim_pid" ]; then wait "$sim_pid" 2>/dev/null || true; fi
 if [ "$result" -ne 0 ]; then
  local diagnostic_args=(--log-dir "$log_dir" --scenario "$scenario" --stage "$stage" --exit-code "$result")
  if [ -n "$simulation_exit" ]; then diagnostic_args+=(--simulation-exit-code "$simulation_exit"); fi
  python3 scripts/acceptance_diagnostics.py "${diagnostic_args[@]}" >&2 || \
   echo 'Acceptance failure summary could not be written.' >&2
  echo "Acceptance failed; private disposable logs: $log_dir" >&2
 fi
 return "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# The write scenario also exercises the actual SDK's registered distribution entry.
if [ "$scenario" = writes ] || [ "$scenario" = workspace ]; then
 stage=sdk_build
 bun run build > "$log_dir/sdk.log" 2>&1
fi
stage=core_build
# Compile before the readiness deadline. Launch the emitted binary directly so
# cleanup owns the actual service PID rather than a cargo wrapper process.
cargo build --offline --locked -p secretary-core --message-format=json > "$log_dir/build.ndjson" 2> "$log_dir/build.err"
stage=core_artifact
core_binary="$(bun -e '
 const events=(await Bun.file(process.argv[1]).text()).trim().split("\n").map(line=>JSON.parse(line));
 const binaries=events.filter(event=>event.reason==="compiler-artifact" && event.target?.name==="secretary-core" && event.target.kind.includes("bin") && typeof event.executable==="string");
 if(binaries.length!==1)throw Error("Cargo did not report exactly one Core executable");
 console.log(binaries[0].executable);
' "$log_dir/build.ndjson" 2>> "$log_dir/build.err")"
stage=simulation_start
: > "$log_dir/sim.ndjson"
python3 -c 'import os, sys; os.setsid(); os.execvp(sys.argv[1], sys.argv[1:])' \
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
 exit 1
fi
# Derive the provider address from the actual listening event, never a guessed port.
stage=provider_address
provider_base="$(bun scripts/acceptance.ts --simulation-base "$log_dir/sim.ndjson" 2> "$log_dir/parser.err")"
docs_base="$provider_base"
if [ "$scenario" = workspace ]; then docs_base="$(bun scripts/acceptance.ts --simulation-docs-base "$log_dir/sim.ndjson" 2>> "$log_dir/parser.err")"; fi
registry="$PWD/postman/fixtures/registry.json"
writes_enabled=false
if [ "$scenario" = writes ]; then registry="$PWD/postman/fixtures/registry-writes.json"; writes_enabled=true; fi
if [ "$scenario" = workspace ]; then registry="$PWD/postman/fixtures/registry-workspace.json"; writes_enabled=true; fi
SECRETARY_CONFIG="$registry" SECRETARY_WRITES_ENABLED="$writes_enabled" \
SECRETARY_STATE_DIR="$log_dir/state" SECRETARY_APPROVAL_TOKEN=synthetic-separate-operator-token \
SECRETARY_CORE_TOKEN=synthetic-acceptance-core-token SECRETARY_OWNER=+15555550123 \
SECRETARY_MOCK_MODE=true SECRETARY_GOOGLE_BASE_URL="$provider_base" \
SECRETARY_GOOGLE_TOKEN=synthetic-provider-token SECRETARY_GOOGLE_DOCS_BASE_URL="$docs_base" \
"$core_binary" > "$log_dir/core.log" 2>&1 &
core_pid=$!
stage=client
if SECRETARY_ACCEPTANCE_PROVIDER_URL="$provider_base" SECRETARY_ACCEPTANCE_DOCS_URL="$docs_base" bun scripts/acceptance.ts "$scenario" > "$log_dir/client.log" 2>&1; then
 cat "$log_dir/client.log"
else
 exit "$?"
fi
stage=traffic
# Injected conditions short-circuit before a member request event (Postman CLI behavior).
if [ "$scenario" = error ] || [ "$scenario" = slow ]; then
 echo 'Condition case: member request events are unavailable; assess actual caller result with the configured injected failure.'
elif ! rg -q '"request"' "$log_dir/sim.ndjson"; then
 echo "No Google request event found; no dependency-path proof. Logs: $log_dir" >&2
 exit 1
fi
echo "Acceptance finished; inspect dependency traffic in $log_dir/sim.ndjson"
