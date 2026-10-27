# secretary-workspace local simulation context

Created 2026-10-28 from official Sheets spreadsheets.get and Docs documents.get/batchUpdate text/revision subsets; references in T023.

## Purpose

Synthetic generic grid reads and controlled Docs text edits. Sheets and Docs are the only members; actual bridge, Rust Core and SQLite journal run outside the simulation.

## Execution

Run `bash scripts/acceptance.sh workspace` in the qualified environment. The launcher builds the SDK entry and Core, derives both actual listening member addresses, invokes public tools and cleans owned process groups. Inspect private member traffic and caller results.

## Assumptions

- Existing Postman CLI1.75/native scratch `/tmp` and separate persistent log-root requirements still apply. (observed 2026-10-27, previous scenarios)
- Docs fixture models plain-text tabs and strict required revision control, not the whole editor or live auth semantics. (reasoned 2026-10-28, handler)

## Observations

<!-- begin:observations -->
2026-10-28: Both Google Sheets and Google Docs members served traffic. Actual bridge/Core generic values, Docs preview/approval/append/replace, stale revision rejection, uncertain-response reconciliation and built-entry callback assertions passed; other tabs remained unchanged.
<!-- end:observations -->
