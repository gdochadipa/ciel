# secretary-workspace local simulation context

Created 2026-10-28 from official Sheets spreadsheets.get and Docs documents.get/batchUpdate text/revision subsets; references in T023.

## Purpose

Synthetic generic grid reads, bounded tab creation, controlled Docs text edits and explicit selected-Docs search. Sheets and Docs are the only members; actual bridge, Rust Core and SQLite journal run outside the simulation.

## Execution

Run `bash scripts/acceptance.sh workspace` in the qualified environment. The launcher builds the SDK entry and Core, derives both actual listening member addresses, invokes public tools and cleans owned process groups. Inspect private member traffic and caller results.

## Assumptions

- Existing Postman CLI1.75/native scratch `/tmp` and separate persistent log-root requirements still apply. (observed 2026-10-27, previous scenarios)
- Docs fixture models plain-text tabs and strict required revision control, not the whole editor or live auth semantics. (reasoned 2026-10-28, handler)

## Observations

<!-- begin:observations -->
2026-10-29 (T033): Both members served traffic (53 Docs/38 Sheets request events); Docs included five reference-document path events alongside the mutable primary. Real bridge/Core and built-SDK selected-doc global pagination passed. The second Doc is an independent static read-only seed; no atomic snapshot or live provider behavior is modeled.
2026-10-28: Both Google Sheets and Google Docs members served traffic. Actual bridge/Core generic values, Docs preview/approval/append/replace, stale revision rejection, uncertain-response reconciliation and built-entry callback assertions passed; other tabs remained unchanged.
<!-- end:observations -->

2026-10-28 (T025): Both members served traffic again. Sheets now models metadata-only catalog reads and atomic addSheet/updateCells, preserving first-row literal headers. Real bridge/Core and built SDK tab factory/hook pass approval, replay, name collision, no automatic view grant, optional empty headers and accepted-create/malformed-response reconciliation. Three tab creates (including SDK fixture), no duplicate sends. Native HTTP tests additionally cover ID collision and journal restart. Mock state uses the namespaced `google-sheets-grid:create-state` key; intercepted JSON comes from `req.body`, as in the other member. Earlier use of native stream-only parsing returned 400; corrected before acceptance. No live Google/host qualification is established.

T027: workspace also exercises explicit registered grid writes through the real bridge/Core/journal and actual SDK update factory/hook, including literal/clear, approval, replay and accepted-write/lost-response observation-only reconciliation. The Sheets member supports a bounded updateCells subset on the synthetic original grid; no existing tab-create or Docs mutation is repurposed as live evidence. Both members served requests in the targeted local pass. Final all-scenario results are recorded in T027.
