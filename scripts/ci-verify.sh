#!/usr/bin/env bash
# Preserve the verification gate; expose only bounded compiler/test error summaries.
set -euo pipefail
ci_log=$(mktemp "${RUNNER_TEMP:-/tmp}/ciel-checks.XXXXXXXX")
trap 'rm -f "$ci_log"' EXIT
if bash scripts/verify.sh > "$ci_log" 2>&1; then
  cat "$ci_log"
else
  python3 - "$ci_log" <<'PY'
from pathlib import Path
import re
import sys
lines = Path(sys.argv[1]).read_text(errors='replace').splitlines()
selected = [line for line in lines if re.match(
    r'^(FAIL:|ERROR:|AssertionError:|TypeError:|ValueError:|PermissionError:|FileNotFoundError:|'
    r'subprocess.TimeoutExpired:|error(?:\[|:)|Diff in |FAILED|test .* \.\.\. (?:FAIL|ERROR))', line)]
for line in (selected[-20:] or ['Verification failed; inspect private job log.']):
    # Test fixture markers are not useful diagnostics and must not become annotations.
    line = re.sub(r'SYNTHETIC_[A-Za-z0-9_]+', '[fixture]', line)[:1200]
    line = line.replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A')
    print('::error title=Local verification::' + line)
PY
  exit 1
fi
