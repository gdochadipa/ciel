# secretary-retry local simulation context

Created 2026-10-25 from the bounded Google Sheets contract and synthetic handlers linked in `postman/README.md`.

## Purpose

Transient provider failures and recovery. Google is the sole member; bridge, Rust Core and its SQLite journal run for real outside the simulation.

## Execution

Run `bash scripts/acceptance.sh retry` in the qualified environment from `docs/local-verification.md`. The launcher waits for the listening event, derives the actual Google address, starts Core, invokes public tools and stops only its spawned processes. Inspect private `sim.ndjson` alongside the caller result.

## Assumptions

- Socket-capable Linux and cached locked dependencies are needed; the qualified container supplies both. (observed 2026-10-26, T017)
- Keep container `TMPDIR=/tmp`; use `SECRETARY_ACCEPTANCE_LOG_ROOT=/runs` for persistent private logs. Host-mounted scratch broke Postman dataset startup. (observed 2026-10-26, T017)
- Synthetic contract behavior does not establish live Google authentication/semantics, host approval routing, native Gateway persistence or WhatsApp delivery. (reasoned 2026-10-26, bounded mock handlers)

## Observations

<!-- begin:observations -->
2026-10-27 (T021): Google served transient failures then synthetic grids; real Core recovery passed within the sequential suite.
2026-10-26: Google served transient failures followed by synthetic grids; actual Core retry and bridge assertions passed.
<!-- end:observations -->
