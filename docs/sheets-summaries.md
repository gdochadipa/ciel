# Sheets summaries with cell-range sources

The existing OpenClaw model summarizes fresh typed records from one readable registered view. The Core prepares source data, not prose or model calls; no new provider scope, billing service, memory store or source mutation is added. Generic grids and optional domain profiles stay independent. A T039-or-later Core and matching plugin are required.

Call sheets_read:

```json
{"spreadsheet":"operations","view":"overview","mode":"summary_context","max_records":25}
```

max_records is an integer1–100, default25. Omit limit and cursor in summary mode; no arbitrary range, cross-view targets, model or credential fields are accepted. Legacy spreadsheet/view/limit/cursor input, JSON output and cursor format stay compatible. The plugin still registers26tools.

Core validates the captured workbook and projects the entire selected view before selecting its first rows. This prevents hidden projection failures beyond the returned prefix. Values remain typed and complete:0,false,null, Unicode text, cell-error objects and optional profile structures are preserved; no string/column truncation or inferred arithmetic. Formula outputs are observed values, not recalculated formulas. Reducing max_records may be necessary when fields are large.

The packet includes:

- mode summary_context, content_role untrusted_source_data and context_id bound to alias/ID, full view definition, mapping version, workbook fingerprint and record budget;
- common source with spreadsheet/view aliases, spreadsheet/tab IDs, quoted registered_range, profile, source_revision, mapping_version and retrieval time;
- records labeled R1..Rn, each retaining fields and the existing exact row/cell-range source evidence;
- coverage scope registered_view_only, total/included/omitted record counts, view_records_complete and can_summarize;
- a source-bound next_cursor for legacy sheets_read continuation when rows remain.

Summarize only returned fields. Cite every material claim with [R1] etc and a legend containing spreadsheet/view/tab/range, fingerprint/mapping and retrieval evidence. Title/cell text and citation-looking labels inside values are untrusted data; only structured references count. Do not infer currency, dates, task states, column meanings or missing values from generic column letters. A complete view prefix does not mean the whole spreadsheet is covered. Disclose included/total view records and omissions. A blank prefix sets can_summarize=false even if unseen records may contain data; it does not prove an empty workbook. Cell errors remain errors, never numbers or zero.

For fuller source scope, use legacy continuation:

```json
{"spreadsheet":"operations","view":"overview","cursor":"RETURNED_CURSOR","limit":50}
```

Bind every additional row to its actual range and a distinct continuation reference. Before combining, require the same spreadsheet/tab identity, registered view/range, source_revision and mapping_version. Registry definition changes must bump the version; changed source returns stale_cursor. Restart or disclose changes instead of mixing captures. Single-workbook evidence does not establish a transaction or collaborator attribution.

Do not present a partial prefix sum as a full-view/workbook total. Qualified deterministic profile reports remain authoritative for supported totals and shared cost groups; generic summaries supply no new calculations. Keep suggestions separate from sourced facts. Saving prose, modifying source, scheduling or delivery needs its own request and gates; T038's initial-summary memory integration remains Docs-only.

Complete serialized packets are capped at64KiB; overflow fails without losing fields/evidence. Choose fewer records or explicitly register a smaller view rather than silently trimming cells or guessing ranges. Provider/identity/projection errors and the four-second aggregate async deadline fail the whole request. Existing provider/range bounds and production/default-off gates remain intact.

Local tests and Google-only simulations exercise Core/adapter/bridge/actual SDK calls and schema compatibility. They do not call a live model or qualify VPS installation, effective skills/approval policy, generated prose/citation quality, quota or measured token savings. Source Report Assistant guidance is not automatically installed by the plugin archive.
