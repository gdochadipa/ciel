# secretary-writes local simulation context

Created 2026-10-26 against T004 public registry/provider contract and the official Google Sheets values append/batchUpdate references linked in postman/README.md.

## Purpose

Stateful Google-only dependency for real Core previews, operator approvals, append/update, replay protection, stale source conflicts and accepted-write/lost-response reconciliation. Core, bridge, SQLite journal and operator routes run for real outside the simulation.

## Execution

Run `bash scripts/acceptance.sh writes`. The launcher selects the seven-view synthetic registry, a fresh disposable state directory and distinct synthetic Core/operator tokens. Restart simulation for independent runs; the runner resets only local fixture state using explicit fixture controls.

The first valid append approval now uses the real bridge approval callback against Core, checks that its description includes the entire captured preview and rejects callback replay. Later operator calls still exercise conflicts and reconciliation. This runner change has been bundled but not executed; it supplies no actual host approval UI/routing evidence.

## Assumptions

- Existing AI_Tasks/AI_Payments sheets have exact headers. Append models bounded owned tables, not general Google table inference. (reasoned 2026-10-26, handler)
- Local Postman async pm.state persistence and native socket response loss require an actual run. Namespaced keys avoid collisions. Generated CLI sample confirms get/set syntax. (observed source 2026-10-26)
- Fixture reset/lose-next-response controls are test setup, not Google API endpoints. No live host approval transport or delivery proof. (reasoned 2026-10-26, runner)

## Observations

<!-- begin:observations -->
<!-- @todo: execute on socket-capable host and record Google traffic and real Core outcomes. -->
<!-- end:observations -->

## What didn't work

- 2026-10-26: earlier read simulation failed `listen EPERM: operation not permitted 127.0.0.1`; stateful extension remains unexecuted, no network success claimed.
