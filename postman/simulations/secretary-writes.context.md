# secretary-writes local simulation context

Created 2026-10-26 from the bounded Google Sheets contract and synthetic handlers linked in `postman/README.md`.

## Purpose

Stateful approved writes and uncertain-outcome reconciliation. Google is the sole member; bridge, Rust Core and its SQLite journal run for real outside the simulation.

## Execution

Run `bash scripts/acceptance.sh writes` in the qualified environment from `docs/local-verification.md`. The launcher first builds the actual SDK entry, then waits for the listening event, derives the actual Google address, starts Core, invokes public tools and stops only its spawned processes. Inspect private `sim.ndjson` alongside the caller result.

## Assumptions

- Socket-capable Linux and cached locked dependencies are needed; the qualified container supplies both. (observed 2026-10-26, T017)
- Keep container `TMPDIR=/tmp`; use `SECRETARY_ACCEPTANCE_LOG_ROOT=/runs` for persistent private logs. Host-mounted scratch broke Postman dataset startup. (observed 2026-10-26, T017)
- Synthetic contract behavior does not establish live Google authentication/semantics, host approval routing, native Gateway persistence or WhatsApp delivery. (reasoned 2026-10-26, bounded mock handlers)

## Observations

<!-- begin:observations -->
2026-10-27 (T021): Google served the stateful and actual built-entry factory/hook workflows; the sequential suite passed approval, conflict, reconciliation and single-record assertions.
2026-10-26 (T020): Google again served the stateful and built-entry workflows after preview-cache bounds were introduced; approval, reconciliation and single-record assertions passed. Host routing/UI remain unqualified.
2026-10-26 (T019): Google served the existing stateful workflow plus the built entry’s registered factory/hook append. SDK metadata, lazy construction, API-instance preview isolation and exactly-once append assertions passed; the host registry remains a boundary fixture.
2026-10-26: Google served stateful reads, append and updates; actual bridge/Core approval, replay, conflict and accepted-write/lost-response reconciliation passed with one payment.
<!-- end:observations -->
