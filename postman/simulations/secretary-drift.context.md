# secretary-drift local simulation context

Created 2026-10-25 from the Google Sheets read subset documented in `postman/README.md`.

## Purpose

Exercises a required header changed to an unknown schema. Google is the sole member; actual Core and bridge are outside the simulation.

## Execution

Run `bash scripts/acceptance.sh drift`. The runner creates private temporary logs, waits for the simulation listening event, starts real Core, then invokes public bridge tools. See the script for exact disposable environment. Stop only spawned processes via its cleanup trap.

## Assumptions

- Socket-capable host and cached Rust dependencies/lockfile are required; otherwise no service path can run. (observed 2026-10-25, read attempt)
- Synthetic fixture proves only the modeled Sheets read shape; live auth, full Google semantics, native Gateway and WhatsApp delivery remain separate. (reasoned 2026-10-25, `postman/README.md`)

## Observations

<!-- begin:observations -->
<!-- @todo: execute this scenario in a socket-capable environment and record which members served traffic. -->
<!-- end:observations -->
